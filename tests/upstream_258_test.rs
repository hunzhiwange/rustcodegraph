//! Registration-only prerequisite for Celery dispatch (upstream 15483713).
pub use rustcodegraph::{resolution, types, web_tree_sitter};
#[path = "../src/resolution/callback_synthesizer/celery.rs"]
mod celery;
use celery::TaskRegistry;
use rustcodegraph::CodeGraph;

fn check(source: &str, run: impl FnOnce(&mut CodeGraph, &mut TaskRegistry)) {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "rcg258-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&path).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(path.clone());
    std::fs::write(path.join("tasks.py"), source).unwrap();
    std::fs::write(path.join("other.py"), "@app.task\ndef job(): pass\n").unwrap();
    let mut graph = CodeGraph::init_sync(&path).unwrap();
    assert!(graph.index_all(Default::default()).success);
    let mut registry = TaskRegistry::default();
    registry.register("tasks.py", source, &graph.get_nodes_in_file("tasks.py"));
    registry.register(
        "other.py",
        "@app.task\ndef job(): pass\n",
        &graph.get_nodes_in_file("other.py"),
    );
    run(&mut graph, &mut registry);
    graph.close();
}

#[test]
fn case_258_task_decorators_and_stable_index() {
    check(
        "from celery import shared_task, task\n@shared_task\ndef email(): pass\n@app.task(\n bind=True,\n)\ndef crunch(): pass\n@task()\nasync def refresh(): pass\n@celery.shared_task\ndef qualified(): pass\n",
        |g, table| {
            for name in ["email", "crunch", "refresh", "qualified"] {
                let target = table.resolve(name, "views.py").expect(name);
                assert_eq!(target.name, name);
                assert_eq!(target.file_path, "tasks.py");
            }
            let count = g.get_nodes_in_file("tasks.py").len();
            assert!(g.index_all(Default::default()).success);
            assert_eq!(count, g.get_nodes_in_file("tasks.py").len());
            for n in g.get_nodes_in_file("tasks.py") {
                assert!(g.get_outgoing_edges(&n.id).iter().all(|e| {
                    e.metadata
                        .as_ref()
                        .and_then(|m| m.get("synthesizedBy"))
                        .and_then(|v| v.as_str())
                        != Some("celery-dispatch")
                }));
            }
        },
    );
}

#[test]
fn case_258_no_evidence_no_target() {
    check(
        "# @shared_task\ndef comment(): pass\ntext = '''@app.task\ndef fake(): pass'''\ndef plain(): pass\n@mytask\ndef similar(): pass\n@shared_task\ndef real(): pass\ndef following(): pass\nclass Jobs:\n @app.task\n def method(self): pass\n",
        |_, table| {
            assert!(table.resolve("real", "tasks.py").is_some());
            for name in [
                "comment",
                "fake",
                "plain",
                "similar",
                "following",
                "method",
                "missing",
            ] {
                assert!(table.resolve(name, "tasks.py").is_none(), "{name}");
            }
        },
    );
}

#[test]
fn case_258_ambiguity_and_registration_removal() {
    check("@shared_task\ndef job(): pass\n", |g, table| {
        assert!(table.resolve("job", "views.py").is_none());
        assert_eq!(
            table.resolve("job", "tasks.py").unwrap().file_path,
            "tasks.py"
        );
        table.register(
            "tasks.py",
            "def job(): pass\n",
            &g.get_nodes_in_file("tasks.py"),
        );
        assert_eq!(
            table.resolve("job", "views.py").unwrap().file_path,
            "other.py"
        );
        table.register(
            "other.py",
            "def job(): pass\n",
            &g.get_nodes_in_file("other.py"),
        );
        assert!(table.resolve("job", "views.py").is_none());
    });
}
