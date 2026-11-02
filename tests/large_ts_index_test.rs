use rustcodegraph::types::NodeKind;
use rustcodegraph::{CodeGraph, IndexOptions};

#[test]
fn large_tsx_uses_ast_members_and_preserves_multiline_arrow_bodies() {
    let root = std::env::temp_dir().join(format!("rcg-large-ts-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let code = "class App {\n  state: number = 0;\n  public triggerRender = (\n    force?: boolean,\n  ) => {\n    if (force) {\n      const local = { nested: true };\n      return renderStaticScene(local);\n    }\n    renderStaticScene(null);\n  };\n}\nfunction renderStaticScene(value: unknown) { return value; }\n";
    let source = format!(
        "{code}{}",
        format!("//{}\n{}", "x".repeat(131072), "\n".repeat(5001))
    );
    std::fs::write(root.join("App.tsx"), source).unwrap();
    let mut graph = CodeGraph::init_sync(&root).unwrap();
    assert!(graph.index_all(IndexOptions::default()).success);
    let nodes = graph.get_nodes_in_file("App.tsx");
    graph.close();
    std::fs::remove_dir_all(root).unwrap();
    assert!(
        !nodes
            .iter()
            .any(|n| matches!(n.name.as_str(), "if" | "return" | "local" | "nested")),
        "{nodes:?}"
    );
    let method = nodes.iter().find(|n| n.name == "triggerRender").unwrap();
    assert_eq!(method.kind, NodeKind::Method);
    assert_eq!((method.start_line, method.end_line), (3, 11));
    assert_eq!(
        nodes.iter().find(|n| n.name == "state").unwrap().kind,
        NodeKind::Property
    );
}

#[test]
fn large_javascript_keeps_arrow_fields_callable_and_data_fields_as_properties() {
    let root = std::env::temp_dir().join(format!("rcg-large-js-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let source = format!(
        "class Canvas {{\n state = 0;\n triggerRender = (\n force,\n ) => {{\n if (force) {{ return draw(); }}\n draw();\n }};\n}}\nfunction draw() {{}}\n{}",
        format!("//{}\n{}", "x".repeat(131072), "\n".repeat(5001))
    );
    std::fs::write(root.join("canvas.js"), source).unwrap();
    let mut graph = CodeGraph::init_sync(&root).unwrap();
    assert!(graph.index_all(IndexOptions::default()).success);
    let nodes = graph.get_nodes_in_file("canvas.js");
    graph.close();
    std::fs::remove_dir_all(root).unwrap();
    let method = nodes.iter().find(|n| n.name == "triggerRender").unwrap();
    assert_eq!(
        (method.kind, method.start_line, method.end_line),
        (NodeKind::Method, 3, 8)
    );
    assert_eq!(
        nodes.iter().find(|n| n.name == "state").unwrap().kind,
        NodeKind::Property
    );
    assert!(
        !nodes
            .iter()
            .any(|n| matches!(n.name.as_str(), "if" | "return"))
    );
}

#[test]
fn complete_flow_is_shown_before_shorter_overlapping_segments() {
    use rustcodegraph::mcp::tools::ToolHandler;
    use serde_json::json;
    let root = std::env::temp_dir().join(format!("rcg-large-flow-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let code = r#"class Scene {
 callbacks = new Set<() => void>();
 triggerUpdate() { for (const callback of this.callbacks) { callback(); } }
 onUpdate(cb: () => void) { this.callbacks.add(cb); }
 mutateElement() { this.triggerUpdate(); }
}
function renderStaticScene() { return 32032; }
function StaticCanvas() { renderStaticScene(); return null; }
class App {
 scene = new Scene();
 componentDidMount() { this.scene.onUpdate(this.triggerRender); }
 triggerRender = (
   force?: boolean,
 ) => { this.setState({}); };
 setState(value: unknown) {}
 render() { return <StaticCanvas />; }
}
"#;
    std::fs::write(
        root.join("App.tsx"),
        format!(
            "{code}{}",
            format!("//{}\n{}", "x".repeat(131072), "\n".repeat(5001))
        ),
    )
    .unwrap();
    let mut graph = CodeGraph::init_sync(&root).unwrap();
    assert!(graph.index_all(IndexOptions::default()).success);
    let mut handler = ToolHandler::new(true);
    handler.set_default_code_graph(&graph);
    let result=handler.execute("rustcodegraph_explore",json!({"query":"mutateElement triggerUpdate triggerRender render StaticCanvas renderStaticScene"}).as_object().unwrap());
    let text = result
        .content
        .into_iter()
        .map(|c| c.text)
        .collect::<String>();
    handler.close_all();
    assert!(graph.get_nodes_in_file("App.tsx").len() < 30);
    graph.close();
    std::fs::remove_dir_all(root).unwrap();
    let flow = text
        .split("## Flow")
        .nth(1)
        .unwrap()
        .split("### Blast radius")
        .next()
        .unwrap();
    let path = flow.lines().find(|line| line.starts_with("- ")).unwrap();
    for name in [
        "mutateElement",
        "triggerUpdate",
        "triggerRender",
        "render",
        "StaticCanvas",
        "renderStaticScene",
    ] {
        assert!(path.contains(name), "{flow}");
    }
    for mechanism in ["callback", "react-render", "jsx-render"] {
        assert!(path.contains(mechanism), "{flow}");
    }
    assert!(path.contains("registeredAt"), "{flow}");
}

#[test]
fn flow_source_survives_a_large_blast_radius_summary() {
    use rustcodegraph::mcp::tools::ToolHandler;
    use serde_json::json;
    let root = std::env::temp_dir().join(format!("rcg-flow-source-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let mut source = "export function target() { return 'ESSENTIAL_FLOW_SOURCE'; }\n".to_owned();
    for i in 0..250 {
        source.push_str(&format!(
            "export function caller_{i}_{}() {{ target(); }}\n",
            "long".repeat(30)
        ));
    }
    std::fs::write(root.join("many.ts"), source).unwrap();
    let mut graph = CodeGraph::init_sync(&root).unwrap();
    assert!(graph.index_all(IndexOptions::default()).success);
    let mut handler = ToolHandler::new(true);
    handler.set_default_code_graph(&graph);
    let result = handler.execute(
        "rustcodegraph_explore",
        json!({"query":"target"}).as_object().unwrap(),
    );
    let text = result
        .content
        .into_iter()
        .map(|c| c.text)
        .collect::<String>();
    handler.close_all();
    graph.close();
    std::fs::remove_dir_all(root).unwrap();
    assert!(
        text.contains("ESSENTIAL_FLOW_SOURCE"),
        "source lost behind caller summary: {}",
        text.len()
    );
    assert!(text.find("### Source Code").unwrap() < text.find("### Blast radius").unwrap());
}

#[test]
fn flow_does_not_cross_two_unnamed_bridges() {
    use rustcodegraph::mcp::tools::ToolHandler;
    use serde_json::json;
    let root = std::env::temp_dir().join(format!("rcg-flow-bridges-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("chain.ts"),"function entry() { firstBridge(); }\nfunction firstBridge() { secondBridge(); }\nfunction secondBridge() { finish(); }\nfunction finish() { return 1; }\n").unwrap();
    let mut graph = CodeGraph::init_sync(&root).unwrap();
    assert!(graph.index_all(IndexOptions::default()).success);
    let mut handler = ToolHandler::new(true);
    handler.set_default_code_graph(&graph);
    let result = handler.execute(
        "rustcodegraph_explore",
        json!({"query":"entry finish"}).as_object().unwrap(),
    );
    let text = result
        .content
        .into_iter()
        .map(|c| c.text)
        .collect::<String>();
    handler.close_all();
    graph.close();
    std::fs::remove_dir_all(root).unwrap();
    let flow = text
        .split("## Flow")
        .nth(1)
        .unwrap()
        .split("### Source Code")
        .next()
        .unwrap();
    assert!(!flow.contains("--calls"), "{flow}");
}
