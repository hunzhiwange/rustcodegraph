//! Git worktree index-mismatch detection (issue #155).
//!
//! A CodeGraph index is resolved by walking up to the nearest `.rustcodegraph/`.
//! When a worktree is nested inside the main checkout, that walk reaches the
//! MAIN checkout's index and a query silently returns the main branch's code
//! instead of the worktree's. `detect_worktree_index_mismatch` spots exactly
//! this case so callers can warn.
//!
//! These tests drive real `git` against real temp worktrees - no mocking - so
//! they exercise the same `git rev-parse --show-toplevel` behavior production
//! relies on.

use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use rustcodegraph::mcp::tools::{ToolHandler, ToolResult};
use rustcodegraph::sync::worktree::{
    detect_worktree_index_mismatch, git_worktree_root, worktree_mismatch_warning,
};
use rustcodegraph::{CodeGraph, IndexOptions};
use serde_json::{Map, Value, json};

static TEST_LOCK: Mutex<()> = Mutex::new(());
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn test_lock() -> MutexGuard<'static, ()> {
    TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn git<I, S>(cwd: &Path, args: I)
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let args = args
        .into_iter()
        .map(|arg| arg.as_ref().to_os_string())
        .collect::<Vec<_>>();
    let output = Command::new("git")
        .args(&args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .unwrap_or_else(|err| panic!("failed to run git {args:?} in {}: {err}", cwd.display()));

    assert!(
        output.status.success(),
        "git {args:?} failed in {}\nstatus: {}\nstderr:\n{}",
        cwd.display(),
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
}

fn try_git<I, S>(cwd: &Path, args: I)
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let args = args
        .into_iter()
        .map(|arg| arg.as_ref().to_os_string())
        .collect::<Vec<_>>();
    let _ = Command::new("git")
        .args(&args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// realpath so macOS /var -> /private/var symlinking does not break equality.
fn real(path: impl AsRef<Path>) -> String {
    let resolved = absolutize(path.as_ref());
    fs::canonicalize(&resolved)
        .unwrap_or(resolved)
        .to_string_lossy()
        .into_owned()
}

fn absolutize(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    }
}

fn temp_dir(prefix: &str) -> PathBuf {
    for _ in 0..100 {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after Unix epoch")
            .as_nanos();
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::SeqCst);
        let path =
            env::temp_dir().join(format!("{prefix}-{}-{nanos}-{counter}", std::process::id()));
        if fs::create_dir(&path).is_ok() {
            return path;
        }
    }
    panic!("failed to create unique temp dir for {prefix}");
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn init_git_repo(main_repo: &Path) {
    git(main_repo, ["init", "-q"]);
    git(main_repo, ["config", "user.email", "test@example.com"]);
    git(main_repo, ["config", "user.name", "Test"]);
    git(main_repo, ["config", "commit.gpgsign", "false"]);
}

fn add_feature_worktree(main_repo: &Path, worktree: &Path) {
    git(
        main_repo,
        [
            OsString::from("worktree"),
            OsString::from("add"),
            OsString::from("-q"),
            OsString::from("-b"),
            OsString::from("feature"),
            worktree.as_os_str().to_os_string(),
        ],
    );
}

struct DetectionFixture {
    main_repo: PathBuf,
    worktree: PathBuf,
    non_git: PathBuf,
}

impl DetectionFixture {
    fn new() -> Self {
        let main_repo = temp_dir("cg-wt-main");
        let non_git = temp_dir("cg-wt-plain");

        init_git_repo(&main_repo);
        fs::write(main_repo.join("README.md"), "# main\n")
            .expect("README fixture should be written");
        git(&main_repo, ["add", "."]);
        git(&main_repo, ["commit", "-q", "-m", "init"]);

        // Nest the worktree under the main checkout, mirroring tools that place
        // worktrees in (gitignored) subpaths like `.claude/worktrees/<name>/`.
        let worktree = main_repo.join("wt");
        add_feature_worktree(&main_repo, &worktree);

        Self {
            main_repo,
            worktree,
            non_git,
        }
    }
}

impl Drop for DetectionFixture {
    fn drop(&mut self) {
        try_git(
            &self.main_repo,
            [
                OsString::from("worktree"),
                OsString::from("remove"),
                OsString::from("--force"),
                self.worktree.as_os_str().to_os_string(),
            ],
        );
        let _ = fs::remove_dir_all(&self.main_repo);
        let _ = fs::remove_dir_all(&self.non_git);
    }
}

struct ToolFixture {
    main_repo: PathBuf,
    worktree: PathBuf,
    cg: Option<CodeGraph>,
    handler: ToolHandler,
}

impl ToolFixture {
    fn new() -> Self {
        let main_repo = temp_dir("cg-wt-tool");

        init_git_repo(&main_repo);
        fs::create_dir(main_repo.join("src")).expect("src fixture directory should be created");
        fs::write(
            main_repo.join("src").join("a.ts"),
            "export function mainOnly() { return 1; }\n",
        )
        .expect("a.ts fixture should be written");
        git(&main_repo, ["add", "."]);
        git(&main_repo, ["commit", "-q", "-m", "init"]);

        // The index lives in the MAIN checkout.
        let mut cg = CodeGraph::init_sync(&main_repo).expect("CodeGraph should initialize");
        let _ = cg.index_all(IndexOptions::default());

        // Nested worktree, mirroring tools that place them under
        // .claude/worktrees/<name>/.
        let worktree = main_repo.join("wt");
        add_feature_worktree(&main_repo, &worktree);

        let mut handler = ToolHandler::new(true);
        handler.set_default_code_graph(&cg);

        Self {
            main_repo,
            worktree,
            cg: Some(cg),
            handler,
        }
    }
}

impl Drop for ToolFixture {
    fn drop(&mut self) {
        if let Some(mut cg) = self.cg.take() {
            cg.destroy();
        }
        try_git(
            &self.main_repo,
            [
                OsString::from("worktree"),
                OsString::from("remove"),
                OsString::from("--force"),
                self.worktree.as_os_str().to_os_string(),
            ],
        );
        let _ = fs::remove_dir_all(&self.main_repo);
    }
}

struct PathEnvGuard {
    saved: Option<OsString>,
}

impl PathEnvGuard {
    fn empty() -> Self {
        let saved = env::var_os("PATH");
        // Rust 2024 treats process-wide environment mutation as unsafe because
        // other threads may read it concurrently. Tests in this file hold
        // TEST_LOCK, matching the sequential behavior of the source Vitest file.
        unsafe {
            env::set_var("PATH", "");
        }
        Self { saved }
    }
}

impl Drop for PathEnvGuard {
    fn drop(&mut self) {
        unsafe {
            if let Some(saved) = self.saved.take() {
                env::set_var("PATH", saved);
            } else {
                env::remove_var("PATH");
            }
        }
    }
}

fn search_args(query: &str) -> Map<String, Value> {
    let mut args = Map::new();
    args.insert("query".to_string(), json!(query));
    args
}

fn first_text(result: &ToolResult) -> &str {
    result
        .content
        .first()
        .map(|content| content.text.as_str())
        .unwrap_or("")
}

mod detect_worktree_index_mismatch_issue_155 {
    use super::*;

    #[test]
    fn flags_a_worktree_borrowing_the_main_checkout_index() {
        let _guard = test_lock();
        let fixture = DetectionFixture::new();

        let m = detect_worktree_index_mismatch(&fixture.worktree, &fixture.main_repo)
            .expect("mismatch should be detected");
        assert_eq!(m.worktree_root, real(&fixture.worktree));
        assert_eq!(m.index_root, real(&fixture.main_repo));
    }

    #[test]
    fn returns_null_when_the_index_lives_in_the_same_working_tree() {
        let _guard = test_lock();
        let fixture = DetectionFixture::new();

        assert!(detect_worktree_index_mismatch(&fixture.main_repo, &fixture.main_repo).is_none());
        assert!(detect_worktree_index_mismatch(&fixture.worktree, &fixture.worktree).is_none());
    }

    #[test]
    fn returns_null_for_a_subdirectory_of_the_same_working_tree() {
        let _guard = test_lock();
        let fixture = DetectionFixture::new();
        let sub = fixture.main_repo.join("src");
        fs::create_dir(&sub).expect("subdirectory should be created");

        assert!(detect_worktree_index_mismatch(&sub, &fixture.main_repo).is_none());
    }

    #[test]
    fn returns_null_when_start_path_is_not_in_a_git_repo() {
        let _guard = test_lock();
        let fixture = DetectionFixture::new();

        assert!(detect_worktree_index_mismatch(&fixture.non_git, &fixture.main_repo).is_none());
    }

    #[test]
    fn returns_null_when_the_index_root_is_a_plain_non_worktree_directory() {
        let _guard = test_lock();
        let fixture = DetectionFixture::new();

        // startPath is a real worktree, but the index sits in an unrelated
        // non-git dir - that is "index in an ancestor", not "borrowed another
        // worktree".
        assert!(detect_worktree_index_mismatch(&fixture.worktree, &fixture.non_git).is_none());
    }

    #[test]
    fn git_worktree_root_reports_each_tree_distinctly() {
        let _guard = test_lock();
        let fixture = DetectionFixture::new();

        assert_eq!(
            git_worktree_root(&fixture.worktree),
            Some(real(&fixture.worktree))
        );
        assert_eq!(
            git_worktree_root(&fixture.main_repo),
            Some(real(&fixture.main_repo))
        );
        assert_eq!(git_worktree_root(&fixture.non_git), None);
    }

    #[test]
    fn warning_names_both_trees_and_the_fix() {
        let _guard = test_lock();
        let fixture = DetectionFixture::new();

        let msg = worktree_mismatch_warning(
            &detect_worktree_index_mismatch(&fixture.worktree, &fixture.main_repo)
                .expect("mismatch should be detected"),
        );
        assert!(msg.contains(&real(&fixture.worktree)), "{msg}");
        assert!(msg.contains(&real(&fixture.main_repo)), "{msg}");
        assert!(msg.contains("codegraph init"), "{msg}");
    }
}

/// The detection above only helps if it reaches the agent. Agents call the read
/// tools (search/context/trace/...), almost never status - so the mismatch
/// notice has to ride on every read tool's result, not just status. These tests
/// drive the real `ToolHandler::execute` chokepoint against a real index whose
/// default project resolves UP from a nested worktree to the main checkout.
mod worktree_mismatch_surfaces_on_hot_read_tools_issue_155 {
    use super::*;

    #[test]
    fn prefixes_a_compact_notice_on_codegraph_search_run_from_a_nested_worktree() {
        let _guard = test_lock();
        let mut fixture = ToolFixture::new();

        fixture
            .handler
            .set_default_project_hint(path_string(&fixture.worktree));
        let res = fixture
            .handler
            .execute("rustcodegraph_search", &search_args("mainOnly"));
        let text = first_text(&res);
        assert!(!res.is_error.unwrap_or(false));
        assert!(text.contains("different git worktree"), "{text}");
        assert!(text.contains(&real(&fixture.worktree)), "{text}");
        assert!(text.contains("codegraph init"), "{text}");
    }

    #[test]
    fn does_not_prefix_when_the_default_project_is_the_main_checkout_itself() {
        let _guard = test_lock();
        let mut fixture = ToolFixture::new();

        fixture
            .handler
            .set_default_project_hint(path_string(&fixture.main_repo));
        let res = fixture
            .handler
            .execute("rustcodegraph_search", &search_args("mainOnly"));
        assert!(
            !first_text(&res).contains("different git worktree"),
            "{}",
            first_text(&res)
        );
    }

    #[test]
    fn still_shows_the_verbose_warning_on_codegraph_status() {
        let _guard = test_lock();
        let mut fixture = ToolFixture::new();

        fixture
            .handler
            .set_default_project_hint(path_string(&fixture.worktree));
        let res = fixture.handler.execute("rustcodegraph_status", &Map::new());
        let text = first_text(&res);
        assert!(text.contains("different git working tree"), "{text}");
        assert!(text.contains(&real(&fixture.worktree)), "{text}");
    }

    #[test]
    fn caches_detection_a_later_tool_call_needs_no_further_git_spawn() {
        let _guard = test_lock();
        let mut fixture = ToolFixture::new();

        fixture
            .handler
            .set_default_project_hint(path_string(&fixture.worktree));
        // First call computes + caches the mismatch (this is the only git
        // spawn).
        let first = fixture
            .handler
            .execute("rustcodegraph_search", &search_args("mainOnly"));
        assert!(
            first_text(&first).contains("different git worktree"),
            "{}",
            first_text(&first)
        );

        // Make git unreachable. A fresh detection would now return null (no
        // notice); the notice still appearing on a different tool proves it
        // came from cache.
        let _path_guard = PathEnvGuard::empty();
        let second = fixture
            .handler
            .execute("rustcodegraph_explore", &search_args("mainOnly"));
        assert!(
            first_text(&second).contains("different git worktree"),
            "{}",
            first_text(&second)
        );
    }
}
