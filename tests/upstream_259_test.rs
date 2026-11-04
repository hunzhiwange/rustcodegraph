use rustcodegraph::{CodeGraph, mcp::tools::ToolHandler, types::EdgeProvenance};
use serde_json::json;
struct Project(std::path::PathBuf);
impl Project {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let p = Self(std::env::temp_dir().join(format!(
            "rcg259-{}-{}",
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
                .is_some_and(|s| s == "celery-dispatch")
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
fn case_259_dispatch_flow_and_removal() {
    let p = Project::new();
    let tasks =
        "from celery import shared_task\n@shared_task\ndef work(): finish()\ndef finish(): pass\n";
    p.write("tasks.py", tasks);
    p.write(
        "other/tasks.py",
        tasks.replace("finish", "wrong_end").as_str(),
    );
    p.write("entry.py", "from tasks import work as selected\ndef Entry(): selected.delay()\ndef AsyncEntry(): selected.apply_async(args=[])\n");
    let mut g = CodeGraph::init_sync(&p.0).unwrap();
    assert!(g.index_all(Default::default()).success);
    assert_eq!(pairs(&mut g, "entry.py").len(), 2);
    let target = g
        .get_nodes_in_file("tasks.py")
        .into_iter()
        .find(|n| n.name == "work")
        .unwrap();
    assert!(
        pairs(&mut g, "entry.py")
            .iter()
            .all(|(_, id)| id == &target.id)
    );
    flow(
        &g,
        "Entry work finish",
        &["work (tasks.py:", "finish (tasks.py:", "celery-dispatch"],
    );
    let count = g.get_nodes_in_file("tasks.py").len();
    assert!(g.index_all(Default::default()).success);
    assert_eq!(count, g.get_nodes_in_file("tasks.py").len());
    p.write("tasks.py", &tasks.replace("@shared_task\n", ""));
    g.sync(Default::default());
    assert!(pairs(&mut g, "entry.py").is_empty());
    g.close();
}
#[test]
fn case_259_local_and_module_alias() {
    let p = Project::new();
    p.write(
        "tasks.py",
        "@app.task\ndef local(): pass\ndef Entry(): local.delay()\n",
    );
    p.write(
        "entry.py",
        "import tasks as jobs\ndef Entry(): jobs.local.apply_async()\n",
    );
    let mut g = CodeGraph::init_sync(&p.0).unwrap();
    assert!(g.index_all(Default::default()).success);
    assert_eq!(pairs(&mut g, "tasks.py").len(), 1);
    assert_eq!(pairs(&mut g, "entry.py").len(), 1);
    g.close();
}
#[test]
fn case_259_missing_dynamic_shadowed_and_cross_app() {
    let p = Project::new();
    p.write(
        "tasks.py",
        "@app.task\ndef work(): pass\ndef plain(): pass\n",
    );
    p.write("entry.py", "from tasks import work, plain\ndef Shadow(work): work.delay()\ndef Dynamic(key): getattr(work,key)()\ndef Missing(): plain.delay()\ndef Text():\n text='work.delay()'\n # work.delay()\n pass\n");
    p.write("other/entry.py", "def Foreign(): work.delay()\n");
    let mut g = CodeGraph::init_sync(&p.0).unwrap();
    assert!(g.index_all(Default::default()).success);
    assert!(pairs(&mut g, "entry.py").is_empty());
    assert!(pairs(&mut g, "other/entry.py").is_empty());
    g.close();
}

#[test]
fn case_259_relative_multiline_and_conflicting_bindings() {
    let p = Project::new();
    p.write("pkg/tasks.py", "@app.task\ndef work(): pass\n");
    p.write(
        "pkg/entry.py",
        "from .tasks import (\n work as selected,\n)\ndef Entry(): selected.delay()\n",
    );
    p.write(
        "rebound.py",
        "from pkg.tasks import work\nwork = factory()\ndef Entry(): work.delay()\n",
    );
    p.write(
        "override.py",
        "from pkg.tasks import work\ndef work(): pass\ndef Entry(): work.delay()\n",
    );
    p.write(
        "nested.py",
        "from pkg.tasks import work\ndef Entry():\n def work(): pass\n work.delay()\n",
    );
    p.write(
        "fake.py",
        "text='from pkg.tasks import work'\ndef Entry(): work.delay()\n",
    );
    let mut g = CodeGraph::init_sync(&p.0).unwrap();
    assert!(g.index_all(Default::default()).success);
    assert_eq!(pairs(&mut g, "pkg/entry.py").len(), 1);
    for file in ["rebound.py", "override.py", "nested.py", "fake.py"] {
        assert!(pairs(&mut g, file).is_empty(), "{file}");
    }
    g.close();
}
