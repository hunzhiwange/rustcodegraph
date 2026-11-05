use rustcodegraph::{CodeGraph, mcp::tools::ToolHandler, types::EdgeProvenance};
use serde_json::json;
struct Project(std::path::PathBuf);
impl Project {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let p = Self(std::env::temp_dir().join(format!(
            "rcg311-{}-{}",
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
                .is_some_and(|s| s == "laravel-event")
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
    let flow = text.split("### Source Code").next().unwrap();
    assert!(symbols.iter().all(|s| flow.contains(s)), "{text}");
    assert!(flow.contains("[laravel-event; registeredAt:"), "{text}");
    h.close_all();
}
#[test]
fn case_311_flow_and_removal() {
    let p = Project::new();
    p.write("shop/composer.json", "{}");
    p.write(
        "shop/Listener.php",
        "<?php namespace App; class Listener {
 public function handle(Changed $event) { $this->finish(); }
 public function finish() {}
}",
    );
    p.write(
        "shop/Entry.php",
        "<?php namespace Web; use App\\Changed as Selected; class Controller {
 public function entry() { event(new Selected()); }
}",
    );
    let mut g = CodeGraph::init_sync(&p.0).unwrap();
    assert!(g.index_all(Default::default()).success);
    assert_eq!(pairs(&mut g, "shop/Entry.php").len(), 1);
    flow(&g, "entry handle finish", &["entry", "handle", "finish"]);
    let count = g.get_nodes_in_file("shop/Listener.php").len();
    assert!(g.index_all(Default::default()).success);
    assert_eq!(count, g.get_nodes_in_file("shop/Listener.php").len());
    p.write(
        "shop/Listener.php",
        "<?php namespace App; class Listener { public function handle($event) {} }",
    );
    g.sync(Default::default());
    assert!(pairs(&mut g, "shop/Entry.php").is_empty());
    g.close();
}
#[test]
fn case_311_mapping_scope_and_namespace() {
    let p = Project::new();
    for app in ["src/one", "src/two"] {
        p.write(&format!("{app}/composer.json"), "{}");
        p.write(
            &format!("{app}/Listener.php"),
            "<?php namespace App; class Listener {
 public function handle() {}
}",
        );
        p.write(&format!("{app}/Provider.php"), "<?php namespace App; class Provider { protected $listen = [Changed::class => [Listener::class]]; }");
    }
    p.write(
        "src/one/Other.php",
        "<?php namespace Other; class Listener {
 public function handle(Changed $event) {}
}",
    );
    p.write(
        "src/one/Entry.php",
        "<?php namespace App; function entry() { event(new Changed()); }",
    );
    let mut g = CodeGraph::init_sync(&p.0).unwrap();
    assert!(g.index_all(Default::default()).success);
    let target = g
        .get_nodes_in_file("src/one/Listener.php")
        .into_iter()
        .find(|n| n.name == "handle" && n.kind == rustcodegraph::types::NodeKind::Method)
        .unwrap();
    assert_eq!(
        pairs(&mut g, "src/one/Entry.php"),
        vec![("entry".into(), target.id)]
    );
    p.write(
        "src/one/Provider.php",
        "<?php namespace App; class Provider {}",
    );
    g.sync(Default::default());
    assert!(pairs(&mut g, "src/one/Entry.php").is_empty());
    g.close();
}
#[test]
fn case_311_dynamic_missing_and_unrelated() {
    let p = Project::new();
    p.write(
        "one/Listener.php",
        "<?php class Listener {
 public function handle(Changed $event) {}
}",
    );
    p.write("one/Entry.php", "<?php function dynamicCall($e, $class) { event($e); event(new $class()); } function missing() { event(new Absent()); } function unrelated() { other(new Changed()); } // event(new Changed())");
    p.write(
        "two/Entry.php",
        "<?php function isolated() { event(new Changed()); }",
    );
    let mut g = CodeGraph::init_sync(&p.0).unwrap();
    assert!(g.index_all(Default::default()).success);
    assert!(pairs(&mut g, "one/Entry.php").is_empty());
    assert!(pairs(&mut g, "two/Entry.php").is_empty());
    g.close();
}

#[test]
fn case_311_bracketed_namespaces_and_ambiguous_maps() {
    let p = Project::new();
    p.write("shop/composer.json", "{}");
    p.write(
        "shop/Listeners.php",
        r#"<?php
namespace First {
 use Events\Changed as Selected;
 class Listener {
  public function handle(Selected|\Events\Other $e) {}
 }
}
namespace Second {
 use Other\Changed as Selected;
 class Listener {
  public function handle(Selected $e) {}
 }
}
"#,
    );
    p.write(
        "shop/Entry.php",
        r#"<?php
namespace Api;
use Events\Changed as Selected;
function entry() { \event(new Selected()); }
function other() { event(new \Events\Other()); }
function absent() { event(new Missing()); }
"#,
    );
    p.write(
        "shop/Duplicate1.php",
        "<?php namespace App; class Duplicate {\n public function handle() {}\n}",
    );
    p.write(
        "shop/Duplicate2.php",
        "<?php namespace App; class Duplicate {\n public function handle() {}\n}",
    );
    p.write("shop/Provider.php", "<?php namespace App; class Provider { protected $listen = [Mapped::class => [Duplicate::class]]; } function ambiguous() { event(new Mapped()); }");
    let mut g = CodeGraph::init_sync(&p.0).unwrap();
    assert!(g.index_all(Default::default()).success);
    let targets = pairs(&mut g, "shop/Entry.php");
    assert_eq!(targets.len(), 2, "{targets:?}");
    assert!(
        targets
            .iter()
            .all(|(name, _)| name == "entry" || name == "other")
    );
    assert_eq!(targets[0].1, targets[1].1);
    assert!(pairs(&mut g, "shop/Provider.php").is_empty());
    g.close();
}
