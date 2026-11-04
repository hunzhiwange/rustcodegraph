use rustcodegraph::{CodeGraph, mcp::tools::ToolHandler, types::EdgeProvenance};
use serde_json::json;
struct Project(std::path::PathBuf);
impl Project {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let p = Self(std::env::temp_dir().join(format!(
            "rcg285-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        )));
        std::fs::create_dir_all(&p.0).unwrap();
        p
    }
    fn write(&self, name: &str, source: &str) {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, source).unwrap();
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn pairs(g: &mut CodeGraph, file: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    for n in g.get_nodes_in_file(file) {
        for e in g.get_outgoing_edges(&n.id) {
            if e.metadata
                .as_ref()
                .and_then(|m| m.get("synthesizedBy"))
                .and_then(|v| v.as_str())
                .is_some_and(|s| s == "mediatr-dispatch")
            {
                assert_eq!(e.provenance, Some(EdgeProvenance::Heuristic));
                assert!(
                    e.metadata.as_ref().unwrap()["registeredAt"]
                        .as_str()
                        .unwrap()
                        .starts_with(&format!("{file}:"))
                );
                pairs.push((n.name.clone(), e.target));
            }
        }
    }
    pairs.sort();
    pairs
}
fn flow(g: &CodeGraph, query: &str, symbols: &[&str]) {
    let mut h = ToolHandler::new(true);
    h.set_default_code_graph(g);
    let args = serde_json::from_value(json!({"query":query})).unwrap();
    let r = serde_json::to_value(h.execute("rustcodegraph_explore", &args)).unwrap();
    let text = r["content"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|c| c["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        text.contains("## Flow (call path among the symbols you queried)"),
        "{text}"
    );
    assert!(
        text.lines().any(|l| l.starts_with("- Entry (")
            && symbols.iter().all(|s| l.contains(s))
            && l.contains("registeredAt")),
        "{text}"
    );
    h.close_all();
}
#[test]
fn case_285_flow_and_incremental_removal() {
    let p = Project::new();
    let handler = "class Handler : ICommandHandler<Order, int> { public int Handle(Order request) { return Finish(); } public int Finish() { return 1; } }";
    p.write("shop/handlers.cs", handler);
    p.write(
        "shop/entry.cs",
        "class Controller { public void Entry() { mediator.Send(new Order()); } }",
    );
    p.write("sample/handlers.cs", handler);
    let mut g = CodeGraph::init_sync(&p.0).unwrap();
    assert!(g.index_all(Default::default()).success);
    let target = g
        .get_nodes_in_file("shop/handlers.cs")
        .into_iter()
        .find(|n| n.name == "Handle")
        .unwrap();
    assert_eq!(
        pairs(&mut g, "shop/entry.cs"),
        vec![("Entry".into(), target.id)]
    );
    flow(&g, "Entry Handle Finish", &["Entry", "Handle", "Finish"]);
    let count = g.get_nodes_in_file("shop/handlers.cs").len();
    assert!(g.index_all(Default::default()).success);
    assert_eq!(count, g.get_nodes_in_file("shop/handlers.cs").len());
    assert_eq!(pairs(&mut g, "shop/entry.cs").len(), 1);
    p.write(
        "shop/handlers.cs",
        &handler.replace(": ICommandHandler<Order, int>", ""),
    );
    g.sync(Default::default());
    assert!(pairs(&mut g, "shop/entry.cs").is_empty());
    g.close();
}
#[test]
fn case_285_publish_modules_and_send_ambiguity() {
    let p = Project::new();
    for name in [
        "shop/a/handler.cs",
        "shop/b/handler.cs",
        "sample/handler.cs",
    ] {
        p.write(name, "class Handler : INotificationHandler<Changed> { public void Handle(Changed request) {} }");
    }
    p.write("shop/entry.cs", "class Controller { public void Broadcast() { publisher.Publish(new Changed()); } public void Ambiguous() { sender.Send(new Changed()); } }");
    let mut g = CodeGraph::init_sync(&p.0).unwrap();
    assert!(g.index_all(Default::default()).success);
    let edges = pairs(&mut g, "shop/entry.cs");
    assert_eq!(edges.len(), 2, "{edges:?}");
    assert!(edges.iter().all(|(name, _)| name == "Broadcast"));
    g.close();
}
#[test]
fn case_285_no_dynamic_missing_or_unrelated_dispatch() {
    let p = Project::new();
    p.write(
        "shop/handler.cs",
        "class Handler : IRequestHandler<Order> { public void Handle(Order request) {} }",
    );
    p.write("shop/entry.cs", "class Controller { public void Dynamic(object request) { mediator.Send(request); } public void Missing() { mediator.Send(new Missing()); } public void Other() { client.Send(new Order()); } public void Computed() { factory().mediator.Send(new Order()); } }");
    p.write(
        "other/entry.cs",
        "class Controller { public void Isolated() { mediator.Send(new Order()); } }",
    );
    let mut g = CodeGraph::init_sync(&p.0).unwrap();
    assert!(g.index_all(Default::default()).success);
    assert!(pairs(&mut g, "shop/entry.cs").is_empty());
    assert!(pairs(&mut g, "other/entry.cs").is_empty());
    g.close();
}

#[test]
fn case_285_solution_boundaries_override_top_level_folders() {
    let p = Project::new();
    p.write("src/one/App.sln", "");
    p.write("src/two/App.sln", "");
    p.write("src/one/api/Api.csproj", "<Project />");
    p.write("src/one/domain/Domain.csproj", "<Project />");
    let handler = "class Handler : IRequestHandler<Order> { public void Handle(Order request) {} }";
    p.write("src/one/domain/handler.cs", handler);
    p.write("src/two/handler.cs", handler);
    p.write(
        "src/one/api/entry.cs",
        "class Controller { public void Entry() { mediator.Send(new Order()); } }",
    );
    let mut g = CodeGraph::init_sync(&p.0).unwrap();
    assert!(g.index_all(Default::default()).success);
    let target = g
        .get_nodes_in_file("src/one/domain/handler.cs")
        .into_iter()
        .find(|n| n.name == "Handle")
        .unwrap();
    assert_eq!(
        pairs(&mut g, "src/one/api/entry.cs"),
        vec![("Entry".into(), target.id)]
    );
    g.close();
}
