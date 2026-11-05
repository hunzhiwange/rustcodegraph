use rustcodegraph::{CodeGraph, IndexOptions, mcp::tools::ToolHandler};
use serde_json::json;

static NEXT_TEMP: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
const FLOW_QUERY: &str =
    "mutateElement triggerUpdate triggerRender render StaticCanvas renderStaticScene";

fn explore(filler: &str, jsx: &str, ending: &str) -> (String, String) {
    explore_query(filler, jsx, ending, FLOW_QUERY)
}

fn explore_query(filler: &str, jsx: &str, ending: &str, query: &str) -> (String, String) {
    let root = std::env::temp_dir().join(format!(
        "rcg-snippets-{}-{}-{}",
        NEXT_TEMP.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let source = format!(
        r#"import {{ unrelated }} from './unused';
class Scene {{
 callbacks = new Set<() => void>();
 triggerUpdate() {{ for (const callback of this.callbacks) {{ callback(); }} }}
 onUpdate(cb: () => void) {{ this.callbacks.add(cb); }}
 mutateElement() {{ this.triggerUpdate(); }}
}}
function renderStaticScene() {{ return 32032; }}
function StaticCanvas() {{ renderStaticScene(); return null; }}
class App {{
 scene = new Scene();
 triggerRender = (
   force?: boolean,
 ) => {{ this.setState({{}}); }};
 setState(value: unknown) {{}}
 render() {{
{filler}
 this.scene.onUpdate(
   this.triggerRender,
 );
 return {jsx};
 }}
}}
{ending}
"#
    );
    std::fs::write(root.join("App.tsx"), &source).unwrap();
    let mut graph = CodeGraph::init_sync(&root).unwrap();
    assert!(graph.index_all(IndexOptions::default()).success);
    let mut handler = ToolHandler::new(true);
    handler.set_default_code_graph(&graph);
    let result = handler.execute(
        "rustcodegraph_explore",
        json!({"query":query}).as_object().unwrap(),
    );
    assert!(!result.is_error.unwrap_or(false));
    let text = result
        .content
        .into_iter()
        .map(|c| c.text)
        .collect::<String>();
    handler.close_all();
    graph.close();
    std::fs::remove_dir_all(root).unwrap();
    (text, source)
}

#[test]
fn long_render_keeps_wiring_jsx_and_short_method_instead_of_import() {
    let (text, _) = explore(
        &" // padding padding padding\n".repeat(500),
        "<StaticCanvas />",
        "",
    );
    assert!(text.contains("this.setState({});"), "{text}");
    assert!(text.contains("this.scene.onUpdate("), "{text}");
    assert!(text.contains("return <StaticCanvas />;"), "{text}");
    assert!(!text.contains("import { unrelated }"), "{text}");
    assert!(text.contains("flow excerpts (partial source)"), "{text}");
    eprintln!("long-method source evidence:\n{text}");
}

#[test]
fn multiline_props_and_original_lines_survive() {
    let (text, source) = explore(
        &" // padding\n".repeat(800),
        "<StaticCanvas\n title={'中文 ) {'}\n active={true}\n />",
        "",
    );
    for needle in ["title={'中文 ) {'}", "active={true}", " />;"] {
        let line = source.lines().position(|l| l.contains(needle)).unwrap() + 1;
        assert!(
            text.contains(&format!("{line}\t")),
            "missing {needle}: {text}"
        );
        assert!(
            text.contains(&format!(
                "{line}\t{}",
                source.lines().nth(line - 1).unwrap()
            )),
            "{text}"
        );
    }
}

#[test]
fn crlf_unicode_keeps_whole_lines() {
    let (text, _) = explore(
        &" // 中文 🙂\r\n".repeat(800),
        "<StaticCanvas title={'🙂中文'} />",
        "",
    );
    assert!(
        text.contains("<StaticCanvas title={'🙂中文'} />;"),
        "{text}"
    );
}

#[test]
fn oversized_statement_reports_uncovered_evidence() {
    let (text, _) = explore(
        &" // padding\n".repeat(800),
        &format!("<StaticCanvas title={{'{}'}} />", "x".repeat(20000)),
        "",
    );
    assert!(text.contains("uncovered anchor"), "{text}");
}

#[test]
fn short_definitions_remain_complete() {
    let (text, _) = explore("", "<StaticCanvas />", "");
    assert!(text.contains("force?: boolean,"), "{text}");
    assert!(text.contains(") => { this.setState({}); };"), "{text}");
}

#[test]
fn unavailable_project_and_missing_symbol_keep_success_shape() {
    let mut handler = ToolHandler::new(true);
    let result = handler.execute(
        "rustcodegraph_explore",
        json!({"query":"missingSymbol"}).as_object().unwrap(),
    );
    assert!(!result.is_error.unwrap_or(false));
    let (text, _) = explore_query("", "<StaticCanvas />", "", "missingSymbol");
    assert!(!text.is_empty());
}
