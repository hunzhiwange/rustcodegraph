use rustcodegraph::{
    CodeGraph, IndexOptions,
    mcp::{
        explore_session_state::ExploreSessionState,
        tools::{ToolHandler, get_explore_output_budget},
    },
};
use serde_json::json;

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

struct Fixture {
    root: std::path::PathBuf,
    graph: CodeGraph,
    handler: ToolHandler,
}
impl Fixture {
    fn new(fanout: usize, unicode: bool, huge: bool) -> Self {
        let root = std::env::temp_dir().join(format!(
            "rcg-flow-budget-{}-{}-{}",
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("entry.ts"),
            "import { middle } from './middle';\nexport function entry() { middle(); }\n",
        )
        .unwrap();
        let payload = if huge {
            "x".repeat(20000)
        } else {
            "中文🙂".repeat(if unicode { 180 } else { 1 })
        };
        std::fs::write(root.join("middle.ts"), format!("import {{ finish }} from './finish';\nexport function middle() {{\n{} finish('{}');\n}}\n", " // padding padding\n".repeat(700), payload)).unwrap();
        std::fs::write(
            root.join("finish.ts"),
            "export function finish(value: string) {\n return 'END_EVIDENCE';\n}\n",
        )
        .unwrap();
        std::fs::write(root.join("noise.ts"), (0..fanout).map(|i| format!("import {{ entry }} from './entry';\nexport function entryNoise{i}() {{ entry(); }}\n")).collect::<String>()).unwrap();
        let mut graph = CodeGraph::init_sync(&root).unwrap();
        assert!(graph.index_all(IndexOptions::default()).success);
        let mut handler = ToolHandler::new(true);
        handler.set_default_code_graph(&graph);
        Self {
            root,
            graph,
            handler,
        }
    }
    fn explore(&mut self, session: &mut ExploreSessionState) -> String {
        let result = self.handler.execute_in_session(
            "rustcodegraph_explore",
            json!({"query":"entry middle finish entryNoise"})
                .as_object()
                .unwrap(),
            session,
        );
        assert!(!result.is_error.unwrap_or(false));
        result.content.into_iter().map(|c| c.text).collect()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.handler.close_all();
        self.graph.close();
        std::fs::remove_dir_all(&self.root).unwrap();
    }
}
fn evidence(text: &str) {
    for needle in [
        "2\texport function entry() { middle(); }",
        " finish('",
        "return 'END_EVIDENCE'",
    ] {
        assert!(text.contains(needle), "missing {needle}: {text}");
    }
    assert!(text.len() <= get_explore_output_budget(4).max_output_chars);
    eprintln!("response bytes={}", text.len());
}
#[test]
fn cross_file_anchors_are_present() {
    evidence(&Fixture::new(0, false, false).explore(&mut ExploreSessionState::default()));
}
#[test]
fn fanout_cannot_displace_main_evidence() {
    evidence(&Fixture::new(160, false, false).explore(&mut ExploreSessionState::default()));
}
#[test]
fn unicode_sections_obey_file_and_response_caps() {
    let text = Fixture::new(100, true, false).explore(&mut ExploreSessionState::default());
    evidence(&text);
    for section in text.split("#### ").skip(1) {
        let section = section.split("### Blast").next().unwrap();
        assert!(
            section.len() <= 3800,
            "section bytes={}: {section}",
            section.len()
        );
    }
}
#[test]
fn oversized_statement_reports_exact_gap_and_next_tool() {
    let text = Fixture::new(0, false, true).explore(&mut ExploreSessionState::default());
    assert!(text.contains("uncovered anchor lines:"), "{text}");
    assert!(
        text.contains("rustcodegraph_explore") && text.contains("rustcodegraph_node"),
        "{text}"
    );
}
#[test]
fn repeat_and_changed_file_resend_evidence() {
    let mut f = Fixture::new(80, false, false);
    let mut s = ExploreSessionState::default();
    evidence(&f.explore(&mut s));
    evidence(&f.explore(&mut s));
    let path = f.root.join("finish.ts");
    let source = std::fs::read_to_string(&path)
        .unwrap()
        .replace("END_EVIDENCE", "CHANGED_EVIDENCE");
    std::fs::write(path, source).unwrap();
    let text = f.explore(&mut s);
    assert!(text.contains("return 'CHANGED_EVIDENCE'"), "{text}");
}

#[test]
fn substantial_ranges_deduplicate_and_changed_files_are_resent() {
    let mut f = Fixture::new(80, false, false);
    let path = f.root.join("finish.ts");
    let source = format!(
        "export function finish(value: string) {{\n{} return 'END_EVIDENCE';\n}}\n",
        " // useful context\n".repeat(16)
    );
    std::fs::write(&path, &source).unwrap();
    assert!(f.graph.index_all(IndexOptions::default()).success);
    let mut s = ExploreSessionState::default();
    let first = f.explore(&mut s);
    let second = f.explore(&mut s);
    assert!(
        second.contains("Source already sent in this session: finish.ts:"),
        "{second}"
    );
    assert!(
        second.contains("2\texport function entry() { middle(); }"),
        "{second}"
    );
    assert!(second.len() < first.len());
    std::fs::write(&path, source.replace("END_EVIDENCE", "CHANGED_EVIDENCE")).unwrap();
    let third = f.explore(&mut s);
    assert!(third.contains("return 'CHANGED_EVIDENCE'"), "{third}");
    assert!(!third.contains("Source already sent in this session: finish.ts:"));
    let ranges = &s.projects.front().unwrap().calls.front().unwrap().files;
    assert!(
        ranges
            .iter()
            .any(|f| f.path == "finish.ts" && f.ranges.contains(&(1, 19)))
    );
    eprintln!(
        "dedup response bytes: {} -> {} -> {}; finish.ts range=1-19",
        first.len(),
        second.len(),
        third.len()
    );
}

#[test]
fn unavailable_source_reports_anchor_gaps_as_success() {
    let mut f = Fixture::new(0, false, false);
    std::fs::remove_file(f.root.join("middle.ts")).unwrap();
    let text = f.explore(&mut ExploreSessionState::default());
    assert!(text.contains("uncovered anchor lines: 2, 703"), "{text}");
    assert!(text.len() <= get_explore_output_budget(4).max_output_chars);
}
