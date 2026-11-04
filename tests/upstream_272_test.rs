use rustcodegraph::{CodeGraph, mcp::tools::ToolHandler, types::EdgeProvenance};
use serde_json::json;
struct Project(std::path::PathBuf);
impl Project {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let p = Self(std::env::temp_dir().join(format!(
            "rcg272-{}-{}",
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

fn edges(g: &mut CodeGraph, file: &str) -> Vec<rustcodegraph::types::Edge> {
    g.get_nodes_in_file(file)
        .iter()
        .flat_map(|n| g.get_outgoing_edges(&n.id))
        .filter(|e| {
            e.metadata
                .as_ref()
                .is_some_and(|m| m.get("synthesizedBy") == Some(&json!("sidekiq-dispatch")))
        })
        .collect()
}
#[test]
fn case_272_flow_and_registration_removal() {
    let p = Project::new();
    let worker = "module Mail\n class Notify\n include Sidekiq::Job\n def perform(id)\n deliver(id)\n end\n def deliver(id)\n end\n end\nend\n";
    p.write("app/workers/notify.rb", worker);
    p.write(
        "app/services/send.rb",
        "def enqueue\n Mail::Notify.perform_async(1)\nend\n",
    );
    let mut g = CodeGraph::init_sync(&p.0).unwrap();
    assert!(g.index_all(Default::default()).success);
    let e = edges(&mut g, "app/services/send.rb");
    assert_eq!(e.len(), 1);
    assert_eq!(e[0].provenance, Some(EdgeProvenance::Heuristic));
    assert_eq!(
        e[0].metadata.as_ref().unwrap()["registeredAt"],
        "app/services/send.rb:2"
    );
    let mut h = ToolHandler::new(true);
    h.set_default_code_graph(&g);
    let result = h.execute(
        "rustcodegraph_explore",
        json!({"query":"enqueue perform deliver"})
            .as_object()
            .unwrap(),
    );
    let value = serde_json::to_value(result).unwrap();
    let text = value["content"]
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
        text.lines().any(|line| line.starts_with("- enqueue (")
            && ["perform", "deliver", "sidekiq-dispatch", "registeredAt"]
                .iter()
                .all(|s| line.contains(s))),
        "{text}"
    );
    h.close_all();
    let count = g.get_nodes_in_file("app/workers/notify.rb").len();
    p.write(
        "app/workers/notify.rb",
        &worker.replace("include Sidekiq::Job", "# registration removed"),
    );
    assert_eq!(g.sync(Default::default()).files_modified, 1);
    assert!(edges(&mut g, "app/services/send.rb").is_empty());
    assert_eq!(count, g.get_nodes_in_file("app/workers/notify.rb").len());
    g.close();
}
#[test]
fn case_272_ambiguity_dynamic_and_unregistered() {
    let p = Project::new();
    p.write("workers.rb","module A\n class Notify\n include Sidekiq::Worker\n def perform\n end\n end\nend\nmodule B\n class Notify\n include Sidekiq::Job\n def perform\n end\n end\nend\nclass Plain\n def perform\n end\nend\n");
    p.write("send.rb","def ambiguous\n Notify.perform_async\nend\ndef dynamic(worker)\n worker.perform_async\n Object.const_get('A::Notify').perform_async\nend\ndef plain\n Plain.perform_async\nend\ndef exact\n A::Notify.perform_async\nend\n");
    let mut g = CodeGraph::init_sync(&p.0).unwrap();
    assert!(g.index_all(Default::default()).success);
    let e = edges(&mut g, "send.rb");
    assert_eq!(e.len(), 1);
    let exact = g
        .get_nodes_in_file("send.rb")
        .into_iter()
        .find(|n| n.name == "exact")
        .unwrap();
    assert_eq!(e[0].source, exact.id);
    g.close();
}
#[test]
fn case_272_cross_app_is_silent() {
    let p = Project::new();
    p.write(
        "one/app/workers/notify.rb",
        "class Notify\n include Sidekiq::Job\n def perform\n end\nend\n",
    );
    p.write(
        "two/app/services/send.rb",
        "def enqueue\n Notify.perform_async\nend\n",
    );
    let mut g = CodeGraph::init_sync(&p.0).unwrap();
    assert!(g.index_all(Default::default()).success);
    assert!(edges(&mut g, "two/app/services/send.rb").is_empty());
    g.close();
}

#[test]
fn case_272_scheduling_and_lexical_shadow() {
    let p = Project::new();
    p.write(
        "worker.rb",
        "module A\n class Notify\n include Sidekiq::Worker\n def perform\n end\n end\nend\n",
    );
    p.write("send.rb","def later\n A::Notify.perform_in(60)\nend\ndef scheduled\n A::Notify.perform_at(1234)\nend\nmodule Other\n def relative\n A::Notify.perform_async\n end\n def absolute\n ::A::Notify.perform_async\n end\nend\n# A::Notify.perform_async\nTEXT='A::Notify.perform_async'\n");
    let mut g = CodeGraph::init_sync(&p.0).unwrap();
    assert!(g.index_all(Default::default()).success);
    let e = edges(&mut g, "send.rb");
    let mut names: Vec<_> = g
        .get_nodes_in_file("send.rb")
        .iter()
        .filter(|n| e.iter().any(|e| e.source == n.id))
        .map(|n| n.name.clone())
        .collect();
    names.sort();
    assert_eq!(names, ["absolute", "later", "scheduled"]);
    g.close();
}
