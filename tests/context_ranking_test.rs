//! Context ranking: common-word precision + low-confidence handoff.
//!
//! This is the Rust port of `__tests__/context-ranking.test.ts`.

use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use rustcodegraph::context::index::{BuildContextResult, ContextBuilder, LOW_CONFIDENCE_MARKER};
use rustcodegraph::db::index::DatabaseConnection;
use rustcodegraph::db::queries::QueryBuilder;
use rustcodegraph::search::query_utils::{
    derive_project_name_tokens, is_distinctive_identifier, score_path_relevance,
};
use rustcodegraph::types::{BuildContextOptions, Confidence, ContextFormat, TaskInput};
use rustcodegraph::{CodeGraph, IndexOptions};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn create_temp_dir(prefix: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after the Unix epoch")
        .as_nanos();
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = env::temp_dir().join(format!("{prefix}{}-{unique}-{counter}", std::process::id()));
    fs::create_dir_all(&dir)
        .unwrap_or_else(|err| panic!("failed to create temp dir {}: {err}", dir.display()));
    dir
}

fn remove_temp_dir(dir: &Path) {
    if dir.exists() {
        let _ = fs::remove_dir_all(dir);
    }
}

struct TempDir {
    root: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Self {
        Self {
            root: create_temp_dir(prefix),
        }
    }

    fn path(&self) -> &Path {
        &self.root
    }

    fn write(&self, relative_path: &str, contents: &str) {
        let path = self.root.join(relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .unwrap_or_else(|err| panic!("failed to create {}: {err}", parent.display()));
        }
        fs::write(&path, contents)
            .unwrap_or_else(|err| panic!("failed to write {}: {err}", path.display()));
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        remove_temp_dir(&self.root);
    }
}

struct ContextRankingFixture {
    temp: TempDir,
    cg: CodeGraph,
}

impl ContextRankingFixture {
    fn new() -> Self {
        let temp = TempDir::new("codegraph-ctxrank-");

        // The corroborated target: a capture-flow screen whose NAME alone
        // matches three query terms (capture + intro + screen), and which
        // lives under a matching directory.
        temp.write(
            "src/app/capture/intro.tsx",
            "export function CaptureIntroScreen() {\n\
             \x20\x20// Onboarding screen shown before the user selects flat or standing object capture.\n\
             \x20\x20return null;\n\
             }\n",
        );

        // The trap: an unrelated constant literally named FLAT, in a totally
        // different area. "flat" in a prose query exact-matches it.
        temp.write(
            "scripts/dataset/download.ts",
            "export const FLAT = 'freiburg_flat_dataset';\n\
             export function downloadDataset(name: string): string { return name; }\n",
        );

        let mut cg = CodeGraph::init_sync(temp.path()).expect("CodeGraph should initialize");
        let result = cg.index_all(IndexOptions::default());
        assert!(result.success, "indexing failed: {:?}", result.errors);

        Self { temp, cg }
    }

    fn path(&self) -> &Path {
        self.temp.path()
    }
}

impl Drop for ContextRankingFixture {
    fn drop(&mut self) {
        self.cg.destroy();
    }
}

fn database_path(project_root: &Path) -> PathBuf {
    project_root.join(".rustcodegraph").join("rustcodegraph.db")
}

fn with_context_builder<R>(
    project_root: &Path,
    f: impl FnOnce(&mut ContextBuilder<'_, '_>) -> R,
) -> R {
    let mut db = DatabaseConnection::open(database_path(project_root))
        .expect("failed to open fixture CodeGraph database");
    let result = {
        let mut queries = QueryBuilder::new(db.get_db());
        let mut builder = ContextBuilder::new(project_root, &mut queries);
        f(&mut builder)
    };
    db.close()
        .expect("failed to close fixture CodeGraph database");
    result
}

fn project_tokens(values: &[&str]) -> HashSet<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

fn markdown_context(builder: &mut ContextBuilder<'_, '_>, query: &str) -> String {
    match builder
        .build_context(
            TaskInput::Query(query.to_owned()),
            Some(BuildContextOptions {
                max_nodes: None,
                max_code_blocks: None,
                max_code_block_size: None,
                include_code: None,
                format: Some(ContextFormat::Markdown),
                search_limit: None,
                traversal_depth: None,
                min_score: None,
            }),
        )
        .expect("context should build")
    {
        BuildContextResult::Formatted(text) => text,
        BuildContextResult::Context(_) => panic!("markdown context should be formatted text"),
    }
}

mod is_distinctive_identifier_tests {
    use super::*;

    #[test]
    fn treats_plain_dictionary_words_as_non_distinctive() {
        for word in ["flat", "object", "screen", "standing", "capture"] {
            assert!(!is_distinctive_identifier(word));
        }
    }

    #[test]
    fn treats_leading_capital_only_words_proper_nouns_sentence_start_as_non_distinctive() {
        assert!(!is_distinctive_identifier("Screen"));
        assert!(!is_distinctive_identifier("Zustand"));
    }

    #[test]
    fn treats_camel_case_pascal_case_snake_case_acronyms_digits_as_distinctive() {
        assert!(is_distinctive_identifier("setLastEmail"));
        assert!(is_distinctive_identifier("OrgUserStore"));
        assert!(is_distinctive_identifier("user_store"));
        assert!(is_distinctive_identifier("REST"));
        assert!(is_distinctive_identifier("v2"));
    }
}

// A single PascalCase query word (notably a project name a user naturally
// includes) splits into sub-tokens that all match the SAME path segment; summed
// per sub-token it boosted that path 4x, burying the rest of the query's stack
// (#720). Path relevance must count each original WORD once per level, while
// still splitting it for cross-convention matching.
mod score_path_relevance_per_word_scoring_720 {
    use super::*;

    #[test]
    fn counts_a_single_pascal_case_word_once_per_path_level_not_once_per_sub_token() {
        // "SuperBizAgent" -> super/biz/agent/superbizagent all hit the dir,
        // but it's one concept: +5 (dir) once, not +20.
        assert_eq!(
            score_path_relevance("SuperBizAgentFrontend/app.js", "SuperBizAgent", None),
            5.0
        );
    }

    #[test]
    fn still_splits_a_word_so_it_matches_across_naming_conventions() {
        // getUserName must still match a snake_case path via its sub-tokens.
        assert!(
            score_path_relevance("get_user_name.go", "getUserName", None) >= 10.0,
            "getUserName should match get_user_name.go"
        );
    }

    #[test]
    fn still_credits_distinct_query_words_matching_different_path_segments() {
        // auth (dir) and handler (filename) are separate concepts -- each
        // counts.
        assert!(
            score_path_relevance("src/auth/login_handler.go", "auth handler", None)
                > score_path_relevance("src/auth/login_handler.go", "auth", None)
        );
    }
}

// The project name is context, not a discriminator: dropping it from path
// scoring stops every file under a `<ProjectName>.../` tree from winning on the
// name alone, so the rest of the query decides the ranking (#720).
mod project_name_down_weighting_in_path_relevance_720 {
    use super::*;

    #[test]
    fn derives_the_project_name_from_go_mod_package_json_skipping_short_names() {
        let dir = TempDir::new("codegraph-projname-");
        dir.write("go.mod", "module example.com/SuperBizAgent\n\ngo 1.21\n");
        dir.write("package.json", r#"{"name":"@acme/superbizagent-web"}"#);

        let tokens = derive_project_name_tokens(&dir.path().to_string_lossy());
        assert!(tokens.contains("superbizagent"));
        assert!(tokens.contains("superbizagentweb"));
    }

    #[test]
    fn drops_a_project_name_query_word_from_path_scoring_when_other_words_remain() {
        let proj = project_tokens(&["superbizagent"]);
        // Without the project name dropped, the frontend path wins on it (+5).
        // With it dropped, only "backend" is left -- and it doesn't match this
        // path.
        let with_drop = score_path_relevance(
            "SuperBizAgentFrontend/app.js",
            "SuperBizAgent backend",
            Some(&proj),
        );
        let no_drop = score_path_relevance(
            "SuperBizAgentFrontend/app.js",
            "SuperBizAgent backend",
            None,
        );
        assert!(with_drop < no_drop);
        assert_eq!(with_drop, 0.0);
    }

    #[test]
    fn keeps_the_project_name_word_when_it_is_the_only_query_word_bare_query_still_scores() {
        let proj = project_tokens(&["superbizagent"]);
        assert_eq!(
            score_path_relevance("SuperBizAgentFrontend/app.js", "SuperBizAgent", Some(&proj)),
            5.0
        );
    }

    #[test]
    fn does_not_affect_a_query_that_omits_the_project_name() {
        let proj = project_tokens(&["superbizagent"]);
        let path = "internal/controller/chat/chat.go";
        assert_eq!(
            score_path_relevance(path, "controller chat", Some(&proj)),
            score_path_relevance(path, "controller chat", None)
        );
    }
}

mod context_ranking_common_word_precision_and_confidence {
    use super::*;

    #[test]
    fn does_not_let_a_common_word_exact_match_flat_outrank_a_corroborated_symbol() {
        let fixture = ContextRankingFixture::new();
        with_context_builder(fixture.path(), |builder| {
            let sg = builder
                .find_relevant_context("capture intro onboarding screen flat object", None)
                .expect("context should be found");
            let root_names = sg
                .roots
                .iter()
                .map(|id| sg.nodes.get(id).map(|node| node.name.clone()))
                .collect::<Vec<_>>();

            // The corroborated capture screen surfaces as an entry point...
            assert!(
                root_names
                    .iter()
                    .any(|name| name.as_deref() == Some("CaptureIntroScreen")),
                "expected CaptureIntroScreen in roots, got {root_names:?}"
            );
            // ...and the trap constant is never the lead result (the bug we
            // fixed).
            assert_ne!(
                root_names.first().and_then(|name| name.as_deref()),
                Some("FLAT")
            );

            let cap_idx = root_names
                .iter()
                .position(|name| name.as_deref() == Some("CaptureIntroScreen"));
            let flat_idx = root_names
                .iter()
                .position(|name| name.as_deref() == Some("FLAT"));
            if let (Some(cap_idx), Some(flat_idx)) = (cap_idx, flat_idx) {
                assert!(cap_idx < flat_idx);
            }

            // And it's confidently answered (we located a corroborated symbol).
            assert_eq!(sg.confidence, Some(Confidence::High));
        });
    }

    #[test]
    fn flags_low_confidence_and_emits_the_handoff_when_only_common_words_match() {
        let fixture = ContextRankingFixture::new();
        with_context_builder(fixture.path(), |builder| {
            let query = "flat object thing";
            let sg = builder
                .find_relevant_context(query, None)
                .expect("context should be found");
            assert_eq!(sg.confidence, Some(Confidence::Low));

            let md = markdown_context(builder, query);
            assert!(md.contains(LOW_CONFIDENCE_MARKER));
            // The handoff routes to the precise tools rather than claiming
            // completeness.
            assert!(md.contains("codegraph_explore"));
        });
    }

    #[test]
    fn does_not_emit_the_handoff_for_a_precise_distinctive_symbol_query() {
        let fixture = ContextRankingFixture::new();
        with_context_builder(fixture.path(), |builder| {
            let sg = builder
                .find_relevant_context("CaptureIntroScreen", None)
                .expect("context should be found");
            assert_eq!(sg.confidence, Some(Confidence::High));

            let md = markdown_context(builder, "CaptureIntroScreen");
            assert!(!md.contains(LOW_CONFIDENCE_MARKER));
        });
    }
}
