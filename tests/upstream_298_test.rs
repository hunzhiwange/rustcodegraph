use rustcodegraph::{CodeGraph, mcp::tools::ToolHandler, types::EdgeProvenance};
use serde_json::json;
struct Project(std::path::PathBuf);
impl Project {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let p = Self(std::env::temp_dir().join(format!(
            "rcg298-{}-{}",
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
                .is_some_and(|s| s == "spring-event")
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
            && l.contains("registeredAt")
            && l.contains("spring-event")),
        "{text}"
    );
    h.close_all();
}
#[test]
fn case_298_flow_and_incremental_removal() {
    let p = Project::new();
    let listener = "package shop; class Listener { @EventListener public void Receive(Changed event) { Finish(); } public void Finish() {} }";
    p.write("shop/Listener.java", listener);
    p.write("shop/Entry.java", "package shop; class Controller { public void Entry() { publisher.publishEvent(new Changed()); } }");
    p.write("other/Listener.java", listener);
    let mut g = CodeGraph::init_sync(&p.0).unwrap();
    assert!(g.index_all(Default::default()).success);
    let target = g
        .get_nodes_in_file("shop/Listener.java")
        .into_iter()
        .find(|n| n.name == "Receive" && n.kind == rustcodegraph::types::NodeKind::Method)
        .unwrap();
    assert_eq!(
        pairs(&mut g, "shop/Entry.java"),
        vec![("Entry".into(), target.id)]
    );
    flow(&g, "Entry Receive Finish", &["Entry", "Receive", "Finish"]);
    let files = [
        "shop/Entry.java",
        "shop/Listener.java",
        "other/Listener.java",
    ];
    let counts: Vec<_> = files
        .iter()
        .map(|file| g.get_nodes_in_file(file).len())
        .collect();
    assert!(g.index_all(Default::default()).success);
    assert_eq!(
        counts,
        files
            .iter()
            .map(|file| g.get_nodes_in_file(file).len())
            .collect::<Vec<_>>()
    );
    assert_eq!(pairs(&mut g, "shop/Entry.java").len(), 1);
    p.write(
        "shop/Listener.java",
        &listener.replace("@EventListener", ""),
    );
    g.sync(Default::default());
    assert!(pairs(&mut g, "shop/Entry.java").is_empty());
    g.close();
}
#[test]
fn case_298_exact_types_fanout_and_application_boundaries() {
    let p = Project::new();
    for app in ["one", "two"] {
        p.write(&format!("src/{app}/pom.xml"), "<project />");
        for (package, method) in [("a", "First"), ("a", "Second"), ("b", "Wrong")] {
            p.write(&format!("src/{app}/{package}/{method}.java"), &format!("package {package}; class {method} {{ @TransactionalEventListener public void {method}(Changed event) {{}} }}"));
        }
    }
    p.write("src/one/api/Entry.java", "package api; import a.Changed; class Entry { public void Imported() { publisher.publishEvent(new Changed()); } public void Qualified() { publisher.publishEvent(new a.Changed()); } }");
    let mut g = CodeGraph::init_sync(&p.0).unwrap();
    assert!(g.index_all(Default::default()).success);
    let edges = pairs(&mut g, "src/one/api/Entry.java");
    assert_eq!(edges.len(), 4, "{edges:?}");
    for (_, id) in edges {
        assert!(["First", "Second"].iter().any(|name| {
            g.get_nodes_in_file(&format!("src/one/a/{name}.java"))
                .iter()
                .any(|n| n.id == id)
        }));
    }
    g.close();
}
#[test]
fn case_298_missing_dynamic_and_non_code_are_silent() {
    let p = Project::new();
    p.write("shop/Listener.java", "class Listener { @EventListener public void Receive(Changed event) {} public void Plain(Missing event) {} }");
    p.write(
        "shop/Entry.java",
        r#"class Entry {
        public void Dynamic(Changed event) { publisher.publishEvent(event); }
        public void Missing() { publisher.publishEvent(new Missing()); }
        public void Computed() { factory().publishEvent(new Changed()); }
        public void Anonymous() { publisher.publishEvent(new Changed() {}); }
        public void Text() { String s = "publisher.publishEvent(new Changed())"; }
        // publisher.publishEvent(new Changed());
    }"#,
    );
    p.write(
        "other/Entry.java",
        "class Entry { public void Isolated() { publisher.publishEvent(new Changed()); } }",
    );
    let mut g = CodeGraph::init_sync(&p.0).unwrap();
    assert!(g.index_all(Default::default()).success);
    assert!(pairs(&mut g, "shop/Entry.java").is_empty());
    assert!(pairs(&mut g, "other/Entry.java").is_empty());
    g.close();
}

#[test]
fn case_298_wildcard_and_conflicting_imports_remain_unresolved() {
    let p = Project::new();
    p.write(
        "shop/Listener.java",
        "package a; class Listener { @EventListener public void Receive(Changed event) {} }",
    );
    p.write("shop/Wildcard.java", "package api; import a.*; class Wildcard { public void Wildcard() { publisher.publishEvent(new Changed()); } }");
    p.write("shop/Conflict.java", "package api; import a.Changed; import b.Changed; class Conflict { public void Conflict() { publisher.publishEvent(new Changed()); } }");
    let mut g = CodeGraph::init_sync(&p.0).unwrap();
    assert!(g.index_all(Default::default()).success);
    assert!(pairs(&mut g, "shop/Wildcard.java").is_empty());
    assert!(pairs(&mut g, "shop/Conflict.java").is_empty());
    g.close();
}

#[test]
fn case_298_gradle_module_scopes_and_qualified_fields() {
    let p = Project::new();
    for app in ["one", "two"] {
        p.write(&format!("src/{app}/build.gradle.kts"), "plugins {}");
        p.write(
            &format!("src/{app}/Listener.java"),
            "package shop; class Listener { @EventListener public void Receive(Changed event) {} }",
        );
    }
    p.write("src/one/Entry.java", "package shop; class Entry { public void Fire() { this.publisher.publishEvent(new Changed()); } }");
    let mut g = CodeGraph::init_sync(&p.0).unwrap();
    assert!(g.index_all(Default::default()).success);
    let edges = pairs(&mut g, "src/one/Entry.java");
    assert_eq!(edges.len(), 1);
    assert!(
        g.get_nodes_in_file("src/one/Listener.java")
            .iter()
            .any(|n| n.id == edges[0].1)
    );
    g.close();
}
