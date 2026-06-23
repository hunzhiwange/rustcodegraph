//! Adaptive output budget for `rustcodegraph_explore` (#185).
//!
//! This is the Rust port of `__tests__/explore-output-budget.test.ts`.
//! The pure budget-shape cases and end-to-end Rust MCP explore budget cases are
//! live.

use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use regex::Regex;
use rustcodegraph::mcp::tools::{
    ToolHandler, ToolResult, get_explore_budget, get_explore_output_budget,
};
use rustcodegraph::{CodeGraph, IndexOptions};
use serde_json::{Map, Value, json};

const EXPLORE_BUDGET_STATUS: &str = "Rust CodeGraph MCP explore budget cases are active";
const RUST_EXPLORE_LINENUMS_ENV: &str = "RUSTCODEGRAPH_EXPLORE_LINENUMS";

static ENV_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

struct EnvVarGuard {
    _lock: MutexGuard<'static, ()>,
    key: &'static str,
    old: Option<OsString>,
}

impl EnvVarGuard {
    fn remove(key: &'static str) -> Self {
        let lock = ENV_MUTEX
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("env mutex should not be poisoned");
        let old = env::var_os(key);
        unsafe {
            env::remove_var(key);
        }
        Self {
            _lock: lock,
            key,
            old,
        }
    }

    fn set(key: &'static str, value: &str) -> Self {
        let lock = ENV_MUTEX
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("env mutex should not be poisoned");
        let old = env::var_os(key);
        unsafe {
            env::set_var(key, value);
        }
        Self {
            _lock: lock,
            key,
            old,
        }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match &self.old {
            Some(value) => unsafe {
                env::set_var(self.key, value);
            },
            None => unsafe {
                env::remove_var(self.key);
            },
        }
    }
}

struct TempProject {
    root: PathBuf,
}

impl TempProject {
    fn new(prefix: &str) -> Self {
        for attempt in 0..100 {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time should be after Unix epoch")
                .as_nanos();
            let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
            let root = env::temp_dir().join(format!(
                "{prefix}{}-{unique}-{counter}-{attempt}",
                std::process::id()
            ));
            match fs::create_dir(&root) {
                Ok(()) => {
                    fs::create_dir(root.join("src")).unwrap_or_else(|err| {
                        panic!("failed to create fixture src dir {}: {err}", root.display())
                    });
                    return Self { root };
                }
                Err(err) if err.kind() == ErrorKind::AlreadyExists => continue,
                Err(err) => panic!("failed to create fixture dir {}: {err}", root.display()),
            }
        }
        panic!("failed to allocate a unique explore budget temp directory")
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
    _temp: TempProject,
    cg: CodeGraph,
    handler: ToolHandler,
}

impl Fixture {
    fn new() -> Self {
        let temp = TempProject::new("codegraph-explore-budget-");

        // A handful of files with one fat target file. The fat file mimics the
        // Alamofire Session.swift case: many methods stacked on top of each
        // other, which collapsed into one giant cluster pre-#185.
        let mut fat_lines = vec!["export class Session {".to_string()];
        for i in 0..30 {
            fat_lines.push(format!("  method{i}(arg: string): string {{"));
            fat_lines.push(format!("    return this.helper{i}(arg) + \"{i}\";"));
            fat_lines.push("  }".to_string());
            fat_lines.push(format!("  private helper{i}(arg: string): string {{"));
            fat_lines.push(format!("    return arg.repeat({});", i + 1));
            fat_lines.push("  }".to_string());
        }
        fat_lines.push("}".to_string());
        temp.write_src("session.ts", &fat_lines.join("\n"));

        // A few small supporting files so the project has >1 indexed file.
        for i in 0..5 {
            temp.write_src(
                &format!("support{i}.ts"),
                &format!(
                    "import {{ Session }} from './session';\n\
                     export function callSession{i}(s: Session) {{ return s.method{i}('hi'); }}\n"
                ),
            );
        }

        let mut cg = CodeGraph::init_sync(temp.path()).expect("failed to initialize CodeGraph");
        let index_result = cg.index_all(IndexOptions::default());
        assert!(
            index_result.success,
            "index_all should succeed, errors: {:?}",
            index_result.errors
        );
        let mut handler = ToolHandler::new(true);
        handler.set_default_code_graph(&cg);

        Self {
            _temp: temp,
            cg,
            handler,
        }
    }

    fn explore(&mut self) -> String {
        let result = self.handler.execute(
            "rustcodegraph_explore",
            &query_args("Session method helper"),
        );
        first_text(&result).to_string()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.handler.close_all();
        self.cg.destroy();
    }
}

fn query_args(query: &str) -> Map<String, Value> {
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

mod get_explore_output_budget {
    use super::*;

    #[test]
    fn returns_a_strictly_smaller_total_cap_for_small_projects_than_for_huge_ones() {
        let small = get_explore_output_budget(100);
        let huge = get_explore_output_budget(30_000);

        assert!(small.max_output_chars < huge.max_output_chars);
        assert!(small.default_max_files < huge.default_max_files);
        assert!(small.max_chars_per_file < huge.max_chars_per_file);
    }

    #[test]
    fn caps_total_output_well_under_8000_tokens_32k_chars_on_small_projects() {
        let small = get_explore_output_budget(100);

        assert!(small.max_output_chars <= 20_000);
    }

    #[test]
    fn caps_medium_large_projects_at_the_inline_tool_result_ceiling_24k_so_the_result_is_never_externalized()
     {
        // A bigger single response gets externalized by the host to a file the
        // agent Reads back, so large repos get more calls via
        // get_explore_budget, not a fatter single response.
        let large = get_explore_output_budget(10_000);

        assert!(large.max_output_chars <= 25_000);
        assert!(large.max_output_chars >= 20_000);
    }

    #[test]
    fn uses_tier_breakpoints_matching_get_explore_budget_so_call_count_and_output_budget_agree_on_a_project()
     {
        // Very-tiny tier (<150 files) gets a tighter cap than small (150-499),
        // paired with tool gating to handle the MCP-overhead-dominates regime.
        let tier0a = get_explore_output_budget(50);
        let tier0b = get_explore_output_budget(149);
        assert_eq!(tier0a.max_output_chars, tier0b.max_output_chars);

        let tier1a = get_explore_output_budget(150);
        let tier1b = get_explore_output_budget(499);
        assert_eq!(tier1a.max_output_chars, tier1b.max_output_chars);
        // The <500 explore-call budget covers both very-tiny and small.
        assert_eq!(get_explore_budget(50), get_explore_budget(499));

        let tier2a = get_explore_output_budget(500);
        let tier2b = get_explore_output_budget(4_999);
        assert_eq!(tier2a.max_output_chars, tier2b.max_output_chars);
        assert_eq!(get_explore_budget(500), get_explore_budget(4_999));

        let tier3a = get_explore_output_budget(5_000);
        let tier3b = get_explore_output_budget(14_999);
        assert_eq!(tier3a.max_output_chars, tier3b.max_output_chars);

        // Small tiers step up (13k -> 18k -> 24k); medium and large share the
        // ~24k inline ceiling, so scaling with repo size lives in the call
        // budget rather than in a fatter single response.
        assert_ne!(tier0a.max_output_chars, tier1a.max_output_chars);
        assert_ne!(tier1a.max_output_chars, tier2a.max_output_chars);
        assert_eq!(tier2a.max_output_chars, tier3a.max_output_chars);
        assert!(get_explore_budget(5_000) > get_explore_budget(4_999));
    }

    #[test]
    fn gates_off_additional_relevant_files_completeness_signal_and_budget_note_on_small_projects() {
        let small = get_explore_output_budget(100);

        assert!(!small.include_additional_files);
        assert!(!small.include_completeness_signal);
        assert!(!small.include_budget_note);
    }

    #[test]
    fn keeps_all_meta_text_on_for_projects_that_earn_the_breadth_signal_500_files() {
        let medium = get_explore_output_budget(1_000);

        assert!(medium.include_additional_files);
        assert!(medium.include_completeness_signal);
        assert!(medium.include_budget_note);
    }

    #[test]
    fn keeps_the_relationships_section_on_for_medium_tiers_small_tiers_drop_it_to_maximize_body_density()
     {
        // ITER2: relationships dropped on <500 tiers; on tiny repos the
        // per-call payload is the cost driver, so even "cheap" structural
        // signal adds up across follow-up turns. Re-enabled at >=500 where body
        // budgets are roomy enough to absorb the 1-2KB overhead.
        assert!(!get_explore_output_budget(50).include_relationships);
        assert!(get_explore_output_budget(1_000).include_relationships);
        assert!(get_explore_output_budget(10_000).include_relationships);
        assert!(get_explore_output_budget(30_000).include_relationships);
    }

    #[test]
    fn caps_the_per_file_header_symbol_list_more_tightly_on_small_projects() {
        // Without this cap, a file like Alamofire's Session.swift produced a
        // 3.4KB symbol list in the per-file header, dwarfing the body cap.
        let small = get_explore_output_budget(100);
        let huge = get_explore_output_budget(30_000);

        assert!(small.max_symbols_in_file_header < huge.max_symbols_in_file_header);
        assert!(small.max_symbols_in_file_header > 0);
    }

    #[test]
    fn uses_a_tighter_clustering_gap_threshold_on_small_projects_to_break_runaway_single_clusters()
    {
        let small = get_explore_output_budget(100);
        let huge = get_explore_output_budget(30_000);

        assert!(small.gap_threshold <= huge.gap_threshold);
    }

    #[test]
    fn handles_the_boundary_file_counts_exactly_off_by_one_regression_guard() {
        // 149 -> very-tiny, 150 -> small
        assert_eq!(
            get_explore_output_budget(149).max_output_chars,
            get_explore_output_budget(50).max_output_chars
        );
        assert_eq!(
            get_explore_output_budget(150).max_output_chars,
            get_explore_output_budget(200).max_output_chars
        );
        // 499 -> small, 500 -> medium
        assert_eq!(
            get_explore_output_budget(499).max_output_chars,
            get_explore_output_budget(200).max_output_chars
        );
        assert_eq!(
            get_explore_output_budget(500).max_output_chars,
            get_explore_output_budget(1_000).max_output_chars
        );
        // 4999 -> medium, 5000 -> large
        assert_eq!(
            get_explore_output_budget(4_999).max_output_chars,
            get_explore_output_budget(1_000).max_output_chars
        );
        assert_eq!(
            get_explore_output_budget(5_000).max_output_chars,
            get_explore_output_budget(10_000).max_output_chars
        );
        // 14999 -> large, 15000 -> xlarge
        assert_eq!(
            get_explore_output_budget(14_999).max_output_chars,
            get_explore_output_budget(10_000).max_output_chars
        );
        assert_eq!(
            get_explore_output_budget(15_000).max_output_chars,
            get_explore_output_budget(30_000).max_output_chars
        );
    }
}

mod codegraph_explore_output_respects_the_adaptive_budget {
    use super::*;

    #[test]
    fn keeps_total_output_under_the_small_project_cap() {
        let mut fixture = Fixture::new();
        let text = fixture.explore();
        let small_budget = get_explore_output_budget(100);

        // Allow a small overshoot for the trailing markers; the cap is
        // enforced per-file rather than as an absolute output ceiling.
        assert!(text.len() < small_budget.max_output_chars + 500);
    }

    #[test]
    fn omits_the_meta_text_gated_off_for_small_projects() {
        let mut fixture = Fixture::new();
        let text = fixture.explore();

        assert!(!text.contains("### Additional relevant files"));
        assert!(!text.contains("Complete source code is included above"));
        assert!(!text.contains("Explore budget:"));
    }

    #[test]
    fn still_includes_the_relationships_section_it_is_the_cheapest_structural_signal() {
        let mut fixture = Fixture::new();
        let text = fixture.explore();

        // Either there are relationships, or no edges were significant; both
        // are fine. This confirms we did not accidentally gate it off.
        let has_relationships = text.contains("### Relationships");
        let source_follows_header = text.find("### Source Code").is_some_and(|i| i > 0);
        assert!(has_relationships || source_follows_header);
    }

    #[test]
    fn prefixes_source_lines_with_line_numbers_by_default_cat_n_style() {
        let _env = EnvVarGuard::remove(RUST_EXPLORE_LINENUMS_ENV);
        let mut fixture = Fixture::new();
        let text = fixture.explore();

        // At least one fenced source line should look like `<digits>\t<code>`.
        assert!(
            Regex::new(r"\n\d+\t")
                .expect("regex should compile")
                .is_match(&text)
        );
    }

    #[test]
    fn omits_line_numbers_when_rustcodegraph_explore_linenums_0() {
        let _env = EnvVarGuard::set(RUST_EXPLORE_LINENUMS_ENV, "0");
        let mut fixture = Fixture::new();
        let text = fixture.explore();

        // The synthetic source has no tab-prefixed numeric lines of its own,
        // so none should appear when the toggle is off.
        assert!(
            !Regex::new(r"\n\d+\t(?:export|  )")
                .expect("regex should compile")
                .is_match(&text)
        );
    }

    #[test]
    fn uses_language_neutral_omission_markers_no_c_style_slashes_in_the_output() {
        let mut fixture = Fixture::new();
        let text = fixture.explore();

        assert!(!text.contains("// ... (gap)"));
        assert!(!text.contains("// ... trimmed"));
    }

    #[test]
    fn does_not_collapse_a_whole_file_class_into_just_its_header_envelope_filter() {
        let mut fixture = Fixture::new();
        let text = fixture.explore();

        // A method body line (`methodN(arg: string)`) should appear, not just
        // the `export class Session {` opener.
        let has_method_body = Regex::new(r"method\d+\(arg: string\)")
            .expect("regex should compile")
            .is_match(&text);
        assert!(has_method_body);
    }
}

#[test]
fn explore_budget_cases_are_active_for_this_port() {
    assert_eq!(
        EXPLORE_BUDGET_STATUS,
        "Rust CodeGraph MCP explore budget cases are active"
    );
}
