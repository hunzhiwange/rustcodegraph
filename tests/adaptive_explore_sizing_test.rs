//! Regression tests for adaptive `rustcodegraph_explore` sizing: sibling
//! skeletonization.
//!
//! This is the Rust port of `__tests__/adaptive-explore-sizing.test.ts`.
//! The cases exercise the Rust `CodeGraph` facade and MCP tool handler directly
//! so adaptive sibling skeletonization stays active in the Rust path.

use std::env;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use rustcodegraph::mcp::tools::{ToolHandler, ToolResult};
use rustcodegraph::types::{EdgeKind, NodeKind};
use rustcodegraph::{CodeGraph, IndexOptions};
use serde_json::{Map, Value, json};

const SKELETON_MARK: &str = "· skeleton (signatures only";

const QUERY: &str = "dispatch proceed handleLogging LoggingInterceptor BridgeInterceptor CacheInterceptor RetryInterceptor ResponseFormatter";

const SPARE_QUERY: &str = "dispatch proceed handleLogging LoggingInterceptor BridgeInterceptor CacheInterceptor RetryInterceptor ResponseFormatter authenticate encode AuthInterceptor Codec JsonCodec";

const ADAPTIVE_EXPLORE_STATUS: &str = "Rust CodeGraph adaptive explore sizing cases are active";
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);
static ADAPTIVE_ENV_LOCK: Mutex<()> = Mutex::new(());

fn section_for(text: &str, basename: &str) -> String {
    let lines = text.lines().collect::<Vec<_>>();
    let Some(start) = lines
        .iter()
        .position(|line| line.starts_with("#### ") && line.contains(basename))
    else {
        return String::new();
    };

    let end = lines
        .iter()
        .enumerate()
        .skip(start + 1)
        .find_map(|(i, line)| (line.starts_with("### ") || line.starts_with("#### ")).then_some(i))
        .unwrap_or(lines.len());

    lines[start..end].join("\n")
}

struct TempProject {
    root: PathBuf,
}

impl TempProject {
    fn new(prefix: &str) -> Self {
        for attempt in 0..100 {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time should be after the Unix epoch")
                .as_nanos();
            let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
            let root = env::temp_dir().join(format!(
                "{prefix}-{}-{nanos}-{counter}-{attempt}",
                std::process::id()
            ));
            match fs::create_dir(&root) {
                Ok(()) => {
                    fs::create_dir(root.join("src")).expect("failed to create fixture src dir");
                    return Self { root };
                }
                Err(err) if err.kind() == ErrorKind::AlreadyExists => continue,
                Err(err) => panic!("failed to create temp dir {}: {err}", root.display()),
            }
        }
        panic!("failed to allocate a unique adaptive explore temp directory")
    }

    fn path(&self) -> &Path {
        &self.root
    }

    fn write_src(&self, name: &str, body: &str) {
        fs::write(self.root.join("src").join(name), body.trim_start())
            .expect("failed to write fixture source file");
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
    _temp: TempProject,
    cg: CodeGraph,
    handler: ToolHandler,
}

impl Fixture {
    fn new() -> Self {
        let temp = TempProject::new("rustcodegraph-adaptive-explore");

        // The interchangeable contract: 4 implementers below => sibling family.
        temp.write_src(
            "interceptor.ts",
            r#"
export interface Interceptor {
  intercept(request: string): string;
}
"#,
        );

        // The mechanism + the spine: dispatch -> proceed -> handleLogging.
        temp.write_src(
            "dispatcher.ts",
            r#"
import { LoggingInterceptor } from './logging-interceptor';

export class RequestDispatcher {
  dispatch(): string {
    const chain = new InterceptorChain();
    return chain.proceed();
  }
}

export class InterceptorChain {
  proceed(): string {
    const exemplar = new LoggingInterceptor();
    return exemplar.handleLogging();
  }
}
"#,
        );

        // On-spine exemplar: full source must be preserved.
        temp.write_src(
            "logging-interceptor.ts",
            r#"
import { Interceptor } from './interceptor';

export class LoggingInterceptor implements Interceptor {
  handleLogging(): string {
    const tag = 'LOGGING_BODY_MARKER';
    return this.intercept(tag);
  }
  intercept(request: string): string {
    return 'logged:' + request;
  }
}
"#,
        );

        // Off-spine siblings: signatures survive, bodies are elided.
        temp.write_src(
            "bridge-interceptor.ts",
            r#"
import { Interceptor } from './interceptor';

export class BridgeInterceptor implements Interceptor {
  intercept(request: string): string {
    const detail = 'BRIDGE_BODY_MARKER';
    return 'bridged:' + request + detail;
  }
}
"#,
        );
        temp.write_src(
            "cache-interceptor.ts",
            r#"
import { Interceptor } from './interceptor';

export class CacheInterceptor implements Interceptor {
  intercept(request: string): string {
    const detail = 'CACHE_BODY_MARKER';
    return 'cached:' + request + detail;
  }
}
"#,
        );
        temp.write_src(
            "retry-interceptor.ts",
            r#"
import { Interceptor } from './interceptor';

export class RetryInterceptor implements Interceptor {
  intercept(request: string): string {
    const detail = 'RETRY_BODY_MARKER';
    return 'retried:' + request + detail;
  }
}
"#,
        );

        // A 1:1 interface->impl pair: off-spine, but not a sibling family.
        temp.write_src(
            "formatter.ts",
            r#"
export interface Formatter {
  format(input: string): string;
}
"#,
        );
        temp.write_src(
            "response-formatter.ts",
            r#"
import { Formatter } from './formatter';
import { JsonCodec } from './codec';

export class ResponseFormatter implements Formatter {
  format(input: string): string {
    const detail = 'FORMATTER_BODY_MARKER';
    return new JsonCodec().encode(input) + detail;
  }
}
"#,
        );

        // Off-spine sibling with a uniquely named callable.
        temp.write_src(
            "auth-interceptor.ts",
            r#"
import { Interceptor } from './interceptor';

export class AuthInterceptor implements Interceptor {
  authenticate(token: string): string {
    const detail = 'AUTH_BODY_MARKER';
    return 'auth:' + token + detail;
  }
  intercept(request: string): string {
    return this.authenticate(request);
  }
}
"#,
        );

        // Base + subclasses in one file: focused view keeps the named base body.
        temp.write_src(
            "codec.ts",
            r#"
export class Codec {
  encode(input: string): string {
    const detail = 'CODEC_BASE_MARKER';
    return input + detail;
  }
}
export class JsonCodec extends Codec {
  encode(input: string): string { return '{' + input + '}'; }
}
export class XmlCodec extends Codec {
  encode(input: string): string {
    const detail = 'XML_BODY_MARKER';
    return '<' + input + detail + '>';
  }
}
export class YamlCodec extends Codec {
  encode(input: string): string { return '- ' + input; }
}
"#,
        );

        let mut cg = CodeGraph::init_sync(temp.path()).expect("failed to initialize CodeGraph");
        let _ = cg.index_all(IndexOptions::default());
        let mut handler = ToolHandler::new(true);
        handler.set_default_code_graph(&cg);

        Self {
            _temp: temp,
            cg,
            handler,
        }
    }

    fn explore(&mut self, query: &str, max_files: usize) -> String {
        let result = self
            .handler
            .execute("rustcodegraph_explore", &args(query, max_files));
        first_text(&result).to_string()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.handler.close_all();
        self.cg.destroy();
    }
}

fn args(query: &str, max_files: usize) -> Map<String, Value> {
    let mut args = Map::new();
    args.insert("query".to_string(), json!(query));
    args.insert("maxFiles".to_string(), json!(max_files));
    args
}

fn first_text(result: &ToolResult) -> &str {
    result
        .content
        .first()
        .map(|content| content.text.as_str())
        .unwrap_or("")
}

fn reset_adaptive_env() {
    // Rust 2024 treats process-wide environment mutation as unsafe because
    // other threads may read it concurrently.
    unsafe {
        env::remove_var("RUSTCODEGRAPH_ADAPTIVE_EXPLORE");
    }
}

fn set_adaptive_env_disabled() {
    unsafe {
        env::set_var("RUSTCODEGRAPH_ADAPTIVE_EXPLORE", "0");
    }
}

fn lock_adaptive_env() -> MutexGuard<'static, ()> {
    match ADAPTIVE_ENV_LOCK.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

#[test]
fn section_for_returns_file_section_until_next_header() {
    let text = "\
### Files
#### src/a.ts · full
line a
#### src/b.ts · skeleton (signatures only)
line b
### Relationships
edge";

    assert_eq!(section_for(text, "a.ts"), "#### src/a.ts · full\nline a");
    assert_eq!(
        section_for(text, "b.ts"),
        "#### src/b.ts · skeleton (signatures only)\nline b"
    );
    assert_eq!(section_for(text, "missing.ts"), "");
}

mod adaptive_codegraph_explore_sizing_sibling_skeletonization {
    use super::*;

    #[test]
    fn fixture_sanity_interceptor_has_at_least_3_implementers_formatter_has_less_than_3() {
        let _guard = lock_adaptive_env();
        reset_adaptive_env();
        let mut fixture = Fixture::new();

        let find = |cg: &mut CodeGraph, name: &str, kind: NodeKind| {
            cg.search_nodes(name, None)
                .into_iter()
                .map(|result| result.node)
                .find(|node| node.name == name && node.kind == kind)
        };

        let interceptor = find(&mut fixture.cg, "Interceptor", NodeKind::Interface)
            .expect("Interceptor interface should be indexed");
        let formatter = find(&mut fixture.cg, "Formatter", NodeKind::Interface)
            .expect("Formatter interface should be indexed");

        let mut implementers = |id: &str| {
            fixture
                .cg
                .get_incoming_edges(id)
                .into_iter()
                .filter(|edge| matches!(edge.kind, EdgeKind::Implements | EdgeKind::Extends))
                .count()
        };

        assert!(
            implementers(&interceptor.id) >= 3,
            "Interceptor should produce the >=3 sibling-family signal"
        );
        assert!(
            implementers(&formatter.id) < 3,
            "Formatter should remain below the sibling-family threshold"
        );
    }

    #[test]
    fn skeletonizes_off_spine_polymorphic_siblings_bodies_elided_signatures_kept() {
        let _guard = lock_adaptive_env();
        reset_adaptive_env();
        let mut fixture = Fixture::new();
        let text = fixture.explore(QUERY, 12);

        assert!(
            text.contains("## Flow (call path among the symbols you queried)"),
            "the spine must form before sibling skeletonization can apply"
        );

        for (file, marker) in [
            ("bridge-interceptor.ts", "BRIDGE_BODY_MARKER"),
            ("cache-interceptor.ts", "CACHE_BODY_MARKER"),
            ("retry-interceptor.ts", "RETRY_BODY_MARKER"),
        ] {
            let section = section_for(&text, file);
            assert!(
                !section.is_empty(),
                "{file} should be present in explore output"
            );
            assert!(
                section.contains(SKELETON_MARK),
                "{file} should be skeletonized"
            );
            assert!(
                section.contains("intercept(request"),
                "{file} should keep the signature"
            );
            assert!(
                !section.contains(marker),
                "{file} body marker must not survive skeletonization"
            );
        }
    }

    #[test]
    fn keeps_the_on_spine_exemplar_full_even_though_it_is_a_sibling() {
        let _guard = lock_adaptive_env();
        reset_adaptive_env();
        let mut fixture = Fixture::new();
        let text = fixture.explore(QUERY, 12);

        let section = section_for(&text, "logging-interceptor.ts");
        assert!(
            !section.is_empty(),
            "logging-interceptor.ts should be present"
        );
        assert!(
            !section.contains(SKELETON_MARK),
            "on-spine exemplar must not be skeletonized"
        );
        assert!(section.contains("LOGGING_BODY_MARKER"));
    }

    #[test]
    fn keeps_a_distinct_step_full_off_spine_but_supertype_has_less_than_3_implementers() {
        let _guard = lock_adaptive_env();
        reset_adaptive_env();
        let mut fixture = Fixture::new();
        let text = fixture.explore(QUERY, 12);

        let section = section_for(&text, "response-formatter.ts");
        assert!(
            !section.is_empty(),
            "response-formatter.ts should be present"
        );
        assert!(
            !section.contains(SKELETON_MARK),
            "a 1:1 interface impl is not a sibling and must stay full"
        );
        assert!(section.contains("FORMATTER_BODY_MARKER"));
    }

    #[test]
    fn rustcodegraph_adaptive_explore_0_disables_skeletonization_siblings_render_full() {
        let _guard = lock_adaptive_env();
        reset_adaptive_env();
        set_adaptive_env_disabled();

        let result = std::panic::catch_unwind(|| {
            let mut fixture = Fixture::new();
            let text = fixture.explore(QUERY, 12);

            assert!(
                !text.contains(SKELETON_MARK),
                "no file should be skeletonized with the flag off"
            );
            let section = section_for(&text, "bridge-interceptor.ts");
            assert!(!section.is_empty());
            assert!(section.contains("BRIDGE_BODY_MARKER"));
        });

        reset_adaptive_env();
        if let Err(payload) = result {
            std::panic::resume_unwind(payload);
        }
    }

    #[test]
    fn spares_an_off_spine_sibling_when_the_agent_named_a_callable_in_it_realcall_fix() {
        let _guard = lock_adaptive_env();
        reset_adaptive_env();
        let mut fixture = Fixture::new();
        let text = fixture.explore(SPARE_QUERY, 15);

        assert!(text.contains("## Flow (call path among the symbols you queried)"));

        let auth = section_for(&text, "auth-interceptor.ts");
        assert!(!auth.is_empty(), "auth-interceptor.ts should be present");
        assert!(
            !auth.contains(SKELETON_MARK),
            "a file holding an agent-named callable must not be skeletonized"
        );
        assert!(auth.contains("AUTH_BODY_MARKER"));

        let bridge = section_for(&text, "bridge-interceptor.ts");
        assert!(
            bridge.contains(SKELETON_MARK),
            "a sibling named only by type still skeletonizes"
        );
        assert!(!bridge.contains("BRIDGE_BODY_MARKER"));
    }

    #[test]
    fn collapses_a_base_and_subclasses_family_file_to_a_focused_view_compiler_py() {
        let _guard = lock_adaptive_env();
        reset_adaptive_env();
        let mut fixture = Fixture::new();
        let text = fixture.explore(SPARE_QUERY, 15);

        let codec = section_for(&text, "codec.ts");
        assert!(!codec.is_empty(), "codec.ts should be present");
        assert!(
            codec.contains("· focused"),
            "a named family file collapses to a focused, not full, view"
        );
        assert!(
            codec.contains("CODEC_BASE_MARKER"),
            "the named base method body is kept"
        );
        assert!(
            !codec.contains("XML_BODY_MARKER"),
            "a non-named subclass body is elided to a signature"
        );
    }

    #[test]
    fn naming_a_shared_polymorphic_method_does_not_spare_the_siblings_uniqueness_aware() {
        let _guard = lock_adaptive_env();
        reset_adaptive_env();
        let mut fixture = Fixture::new();
        let text = fixture.explore(&format!("{QUERY} intercept"), 12);

        let bridge = section_for(&text, "bridge-interceptor.ts");
        assert!(
            bridge.contains(SKELETON_MARK),
            "a sibling named only via a shared method is not spared"
        );
        assert!(
            !bridge.contains("BRIDGE_BODY_MARKER"),
            "a shared method does not earn a body in a non-supertype leaf"
        );
    }
}

#[test]
fn adaptive_explore_cases_are_active_for_this_port() {
    assert_eq!(
        ADAPTIVE_EXPLORE_STATUS,
        "Rust CodeGraph adaptive explore sizing cases are active"
    );
}
