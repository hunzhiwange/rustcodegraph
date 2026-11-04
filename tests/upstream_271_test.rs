//! Sidekiq registration evidence; dispatch is a separate task.
pub use rustcodegraph::{types, web_tree_sitter};
#[path = "../src/resolution/callback_synthesizer/sidekiq.rs"]
mod sidekiq;
use rustcodegraph::CodeGraph;
use sidekiq::WorkerRegistry;

fn check(source: &str, run: impl FnOnce(&mut CodeGraph, &mut WorkerRegistry)) {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "rcg271-{}-{}",
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
    std::fs::write(&path.join("workers.rb"), source).unwrap();
    let mut graph = CodeGraph::init_sync(&path).unwrap();
    assert!(graph.index_all(Default::default()).success);
    let mut registry = WorkerRegistry::default();
    registry.register("workers.rb", source, &graph.get_nodes_in_file("workers.rb"));
    run(&mut graph, &mut registry);
    graph.close();
}

#[test]
fn case_271_registration_and_stability() {
    let source = "class EmailWorker\n include Sidekiq::Worker\n def perform(id)\n end\nend\nclass ModernWorker\n include Sidekiq::Job\n def perform\n end\nend\n";
    check(source, |g, r| {
        for name in ["EmailWorker", "ModernWorker"] {
            assert_eq!(r.resolve(name).expect(name).name, "perform");
        }
        let count = g.get_nodes_in_file("workers.rb").len();
        assert!(g.index_all(Default::default()).success);
        r.register("workers.rb", source, &g.get_nodes_in_file("workers.rb"));
        assert_eq!(count, g.get_nodes_in_file("workers.rb").len());
        assert!(r.resolve("EmailWorker").is_some());
        for n in g.get_nodes_in_file("workers.rb") {
            assert!(g.get_outgoing_edges(&n.id).iter().all(|e| {
                e.metadata
                    .as_ref()
                    .and_then(|m| m.get("synthesizedBy"))
                    .and_then(|v| v.as_str())
                    != Some("sidekiq-dispatch")
            }));
        }
        r.register("workers.rb", "", &[]);
        assert!(r.resolve("EmailWorker").is_none());
    });
}

#[test]
fn case_271_no_registration_evidence() {
    check(
        "class Plain\n # include Sidekiq::Worker\n TEXT = 'include Sidekiq::Job'\n def perform\n end\nend\nclass Similar\n include Sidekiq::Jobber\n def perform\n end\nend\nclass Dynamic\n def setup\n include Sidekiq::Job\n end\n def perform\n end\nend\n",
        |_, r| {
            for name in ["Plain", "Similar", "Dynamic"] {
                assert!(r.resolve(name).is_none(), "{name}");
            }
        },
    );
}

#[test]
fn case_271_perform_must_belong_to_worker() {
    check(
        "class Outer\n include Sidekiq::Job\n class Inner\n def perform\n end\n end\nend\nclass Singleton\n include Sidekiq::Job\n def self.perform\n end\nend\nclass Unregistered\n class Nested\n include Sidekiq::Worker\n def perform\n end\n end\n def perform\n end\nend\n",
        |_, r| {
            for name in ["Outer", "Singleton", "Unregistered"] {
                assert!(r.resolve(name).is_none(), "{name}");
            }
            assert!(r.resolve("Nested").is_some());
        },
    );
}

#[test]
fn case_271_namespace_ambiguity_is_not_a_target() {
    check(
        "module A\n class Notify\n include Sidekiq::Job\n def perform\n end\n end\nend\nmodule B\n class Notify\n include Sidekiq::Worker\n def perform\n end\n end\nend\n",
        |_, r| {
            assert!(r.resolve("Notify").is_none());
            let a = r.resolve("A::Notify").expect("exact A worker");
            let b = r.resolve("B::Notify").expect("exact B worker");
            assert_ne!(a.id, b.id);
            assert!(r.resolve("Missing::Notify").is_none());
        },
    );
}
