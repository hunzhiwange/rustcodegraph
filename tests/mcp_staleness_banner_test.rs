//! Per-file staleness banner on MCP tool responses (issue #403).
//!
//! The watcher tracks every file event since the last successful sync; the
//! tool dispatcher intersects "files referenced in this response" with that
//! pending set and prepends a banner plus an optional footer.
//!
//! This is the Rust port of `__tests__/mcp-staleness-banner.test.ts`. The
//! handler-level behavioral cases exercise the Rust `CodeGraph::watch` facade
//! and MCP stale/degraded notices directly.

use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use regex::Regex;
use rustcodegraph::mcp::tools::{ToolHandler, ToolResult};
use rustcodegraph::sync::watcher::{
    __emit_watch_event_for_tests, __set_fs_watch_for_tests, WatchStartError,
};
use rustcodegraph::{CodeGraph, IndexOptions, WatchOptions};
use serde_json::{Map, Value, json};

static TEST_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn wait_for<F>(mut condition: F, timeout_ms: u64, interval_ms: u64)
where
    F: FnMut() -> bool,
{
    let start = Instant::now();
    let timeout = Duration::from_millis(timeout_ms);
    loop {
        if condition() {
            return;
        }
        assert!(start.elapsed() <= timeout, "waitFor timed out");
        thread::sleep(Duration::from_millis(interval_ms));
    }
}

struct TestGuard {
    _lock: MutexGuard<'static, ()>,
    old_node_env: Option<OsString>,
}

impl TestGuard {
    fn new() -> Self {
        let lock = TEST_MUTEX
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let old_node_env = env::var_os("NODE_ENV");
        unsafe {
            env::set_var("NODE_ENV", "test");
        }
        __set_fs_watch_for_tests(None);
        Self {
            _lock: lock,
            old_node_env,
        }
    }
}

impl Drop for TestGuard {
    fn drop(&mut self) {
        __set_fs_watch_for_tests(None);
        match &self.old_node_env {
            Some(value) => unsafe {
                env::set_var("NODE_ENV", value);
            },
            None => unsafe {
                env::remove_var("NODE_ENV");
            },
        }
    }
}

struct TempProject {
    root: PathBuf,
}

impl TempProject {
    fn new() -> Self {
        for attempt in 0..100 {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time should be after Unix epoch")
                .as_nanos();
            let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
            let root = env::temp_dir().join(format!(
                "codegraph-stale-banner-{}-{unique}-{counter}-{attempt}",
                std::process::id()
            ));
            match fs::create_dir(&root) {
                Ok(()) => {
                    fs::create_dir(root.join("src")).unwrap_or_else(|err| {
                        panic!(
                            "failed to create fixture src directory {}: {err}",
                            root.display()
                        )
                    });
                    return Self { root };
                }
                Err(err) if err.kind() == ErrorKind::AlreadyExists => continue,
                Err(err) => panic!("failed to create fixture dir {}: {err}", root.display()),
            }
        }
        panic!("failed to allocate a unique staleness-banner temp directory")
    }

    fn path(&self) -> &Path {
        &self.root
    }

    fn write_src(&self, name: &str, contents: &str) {
        let path = self.root.join("src").join(name);
        fs::write(&path, contents)
            .unwrap_or_else(|err| panic!("failed to write fixture {}: {err}", path.display()));
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        if self.root.exists() {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}

struct Fixture {
    _guard: TestGuard,
    temp: TempProject,
    cg: CodeGraph,
    handler: ToolHandler,
}

impl Fixture {
    fn new() -> Self {
        let guard = TestGuard::new();
        let temp = TempProject::new();

        // Three isolated files with no cross-references keep each test's
        // "which path does the response mention?" assertion unambiguous.
        temp.write_src(
            "alpha-only.ts",
            "export function alphaOnly() { return 1; }\n",
        );
        temp.write_src(
            "bravo-only.ts",
            "export function bravoOnly() { return 2; }\n",
        );
        temp.write_src(
            "charlie-only.ts",
            "export function charlieOnly() { return 3; }\n",
        );

        let mut cg = CodeGraph::init_sync(temp.path()).expect("failed to initialize CodeGraph");
        let result = cg.index_all(IndexOptions::default());
        assert!(
            result.success,
            "index_all should succeed, errors: {:?}",
            result.errors
        );
        let mut handler = ToolHandler::new(true);
        handler.set_default_code_graph(&cg);

        Self {
            _guard: guard,
            temp,
            cg,
            handler,
        }
    }

    fn degrade_watcher(&mut self) {
        // Force watch-resource exhaustion at startup so the real watcher
        // degrades deterministically on any platform.
        __set_fs_watch_for_tests(Some(Arc::new(|_dir| {
            Err(WatchStartError::resource_exhaustion("too many open files"))
        })));
        let started = self.cg.watch(WatchOptions {
            debounce_ms: Some(1000),
            ..WatchOptions::default()
        });
        assert!(!started);
        assert!(self.cg.is_watcher_degraded());
    }

    fn execute_text(&mut self, tool: &str, args: Map<String, Value>) -> (ToolResult, String) {
        let result = self.handler.execute(tool, &args);
        let text = first_text(&result).to_string();
        (result, text)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.handler.close_all();
        self.cg.unwatch();
        self.cg.close();
    }
}

fn first_text(result: &ToolResult) -> &str {
    result
        .content
        .first()
        .map(|content| content.text.as_str())
        .unwrap_or("")
}

fn search_args(query: &str) -> Map<String, Value> {
    let mut args = Map::new();
    args.insert("query".to_string(), json!(query));
    args
}

fn node_args(symbol: &str, include_code: bool) -> Map<String, Value> {
    let mut args = Map::new();
    args.insert("symbol".to_string(), json!(symbol));
    args.insert("includeCode".to_string(), json!(include_code));
    args
}

fn assert_batch_wait_notice_without_read_or_grep(text: &str) {
    assert!(
        Regex::new("(?i)batch sync").unwrap().is_match(text),
        "{text}"
    );
    assert!(Regex::new("(?i)waiting").unwrap().is_match(text), "{text}");
    assert!(
        !Regex::new(r"(?i)\b(Read|Grep)\b").unwrap().is_match(text),
        "{text}"
    );
}

mod mcp_staleness_banner {
    use super::*;

    #[test]
    fn prepends_a_stale_banner_when_the_response_references_a_pending_file() {
        let mut fixture = Fixture::new();

        // Long debounce so the edit lingers in pendingFiles while we query.
        fixture.cg.watch(WatchOptions {
            debounce_ms: Some(4000),
            ..WatchOptions::default()
        });
        fixture.cg.wait_until_watcher_ready(None);

        // Real disk write so a later sync sees the new content, plus a
        // synthesized event so the pendingFiles set updates immediately.
        fixture.temp.write_src(
            "alpha-only.ts",
            "export function alphaOnly() { return 99; }\n",
        );
        __emit_watch_event_for_tests(fixture.temp.path(), "src/alpha-only.ts");

        wait_for(
            || {
                fixture
                    .cg
                    .get_pending_files()
                    .iter()
                    .any(|pending| pending.path == "src/alpha-only.ts")
            },
            2000,
            25,
        );

        let (result, text) = fixture.execute_text("rustcodegraph_search", search_args("alphaOnly"));
        assert_ne!(result.is_error, Some(true));

        // Banner shape: warning glyph + filename + actionable instruction.
        assert!(text.starts_with('⚠'));
        assert!(text.contains("src/alpha-only.ts"));
        assert!(Regex::new(r"edited \d+ms ago").unwrap().is_match(&text));
        assert_batch_wait_notice_without_read_or_grep(&text);
        // The actual result must still follow the banner.
        assert!(Regex::new(r"alphaOnly").unwrap().is_match(&text));
    }

    #[test]
    fn node_symbol_lookup_uses_the_same_stale_banner_path() {
        let mut fixture = Fixture::new();
        fixture.cg.watch(WatchOptions {
            debounce_ms: Some(4000),
            ..WatchOptions::default()
        });
        fixture.cg.wait_until_watcher_ready(None);

        fixture.temp.write_src(
            "alpha-only.ts",
            "export function alphaOnly() { return 101; }\n",
        );
        __emit_watch_event_for_tests(fixture.temp.path(), "src/alpha-only.ts");
        wait_for(
            || {
                fixture
                    .cg
                    .get_pending_files()
                    .iter()
                    .any(|pending| pending.path == "src/alpha-only.ts")
            },
            2000,
            25,
        );

        let (result, text) =
            fixture.execute_text("rustcodegraph_node", node_args("alphaOnly", true));
        assert_ne!(result.is_error, Some(true));
        assert!(text.starts_with('⚠'), "{text}");
        assert!(text.contains("src/alpha-only.ts"), "{text}");
        assert_batch_wait_notice_without_read_or_grep(&text);
        assert!(text.contains("alphaOnly"), "{text}");
    }

    #[test]
    fn uses_the_footer_not_the_banner_when_pending_files_are_not_referenced() {
        let mut fixture = Fixture::new();
        fixture.cg.watch(WatchOptions {
            debounce_ms: Some(4000),
            ..WatchOptions::default()
        });
        fixture.cg.wait_until_watcher_ready(None);

        // Edit bravo-only.ts but search for alphaOnly, whose hit is only in
        // alpha-only.ts. The fixture files share no imports/calls.
        fixture.temp.write_src(
            "bravo-only.ts",
            "export function bravoOnly() { return 22; }\n",
        );
        __emit_watch_event_for_tests(fixture.temp.path(), "src/bravo-only.ts");
        wait_for(
            || {
                fixture
                    .cg
                    .get_pending_files()
                    .iter()
                    .any(|pending| pending.path == "src/bravo-only.ts")
            },
            2000,
            25,
        );

        let (_result, text) =
            fixture.execute_text("rustcodegraph_search", search_args("alphaOnly"));

        assert!(!text.starts_with('⚠'));
        assert!(
            Regex::new(r"elsewhere in this project are waiting for the next batch sync")
                .unwrap()
                .is_match(&text)
        );
        assert!(text.contains("src/bravo-only.ts"));
        assert_batch_wait_notice_without_read_or_grep(&text);
    }

    #[test]
    fn drops_the_banner_once_the_sync_completes_and_clears_the_pending_entry() {
        let mut fixture = Fixture::new();
        fixture.cg.watch(WatchOptions {
            debounce_ms: Some(200),
            ..WatchOptions::default()
        });
        fixture.cg.wait_until_watcher_ready(None);

        fixture.temp.write_src(
            "alpha-only.ts",
            "export function alphaOnly() { return 7; }\n",
        );
        __emit_watch_event_for_tests(fixture.temp.path(), "src/alpha-only.ts");
        // Wait through debounce (200ms) + sync; pendingFiles drains back to empty.
        wait_for(|| fixture.cg.get_pending_files().is_empty(), 3000, 25);

        let (_result, text) =
            fixture.execute_text("rustcodegraph_search", search_args("alphaOnly"));
        assert!(!text.starts_with('⚠'));
        assert!(
            !Regex::new(r"elsewhere in this project are waiting for the next batch sync")
                .unwrap()
                .is_match(&text)
        );
    }

    #[test]
    fn lists_pending_files_under_pending_sync_in_codegraph_status() {
        let mut fixture = Fixture::new();
        fixture.cg.watch(WatchOptions {
            debounce_ms: Some(4000),
            ..WatchOptions::default()
        });
        fixture.cg.wait_until_watcher_ready(None);

        fixture.temp.write_src(
            "charlie-only.ts",
            "export function charlieOnly() { return 33; }\n",
        );
        __emit_watch_event_for_tests(fixture.temp.path(), "src/charlie-only.ts");
        wait_for(
            || {
                fixture
                    .cg
                    .get_pending_files()
                    .iter()
                    .any(|pending| pending.path == "src/charlie-only.ts")
            },
            2000,
            25,
        );

        let (_result, text) = fixture.execute_text("rustcodegraph_status", Map::new());
        assert!(text.contains("### Pending sync:"));
        assert!(text.contains("src/charlie-only.ts"));
        assert_batch_wait_notice_without_read_or_grep(&text);
        // Status embeds the info first-class, so the auto-banner is suppressed.
        assert!(!text.starts_with('⚠'));
    }

    #[test]
    fn returns_zero_pending_files_when_no_watcher_is_active() {
        let fixture = Fixture::new();

        assert!(fixture.cg.get_pending_files().is_empty());
    }

    #[test]
    fn prepends_a_whole_index_degraded_banner_once_live_watching_has_permanently_stopped_876() {
        let mut fixture = Fixture::new();
        fixture.degrade_watcher();

        let (result, text) = fixture.execute_text("rustcodegraph_search", search_args("alphaOnly"));
        assert_ne!(result.is_error, Some(true));

        assert!(text.starts_with('⚠'));
        assert!(
            Regex::new("(?i)auto-sync is DISABLED")
                .unwrap()
                .is_match(&text)
        );
        assert!(
            Regex::new("(?i)Read files directly")
                .unwrap()
                .is_match(&text)
        );
        assert!(text.contains("OS watch/file limit exhausted"));
        // The real result still follows the banner.
        assert!(Regex::new(r"alphaOnly").unwrap().is_match(&text));
    }

    #[test]
    fn surfaces_the_degraded_state_as_its_own_section_in_codegraph_status_876() {
        let mut fixture = Fixture::new();
        fixture.degrade_watcher();

        let (_result, text) = fixture.execute_text("rustcodegraph_status", Map::new());
        assert!(text.contains("### Auto-sync disabled:"));
        assert!(text.contains("OS watch/file limit exhausted"));
        // Status renders the notice inline, so the auto-banner is not also prepended.
        assert!(!text.starts_with('⚠'));
    }
}
