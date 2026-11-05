//! Laravel event registration evidence; dispatch is a separate task.
pub use rustcodegraph::{resolution, types, web_tree_sitter};
// This registration-only fixture intentionally leaves production dispatch unused.
#[allow(dead_code)]
#[path = "../src/resolution/callback_synthesizer/laravel_event.rs"]
mod laravel_event;
use laravel_event::ListenerRegistry;
use rustcodegraph::CodeGraph;

fn check(source: &str, run: impl FnOnce(&mut CodeGraph, &mut ListenerRegistry)) {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "rcg310-{}-{}",
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
    std::fs::write(path.join("Listeners.php"), source).unwrap();
    let mut graph = CodeGraph::init_sync(&path).unwrap();
    assert!(graph.index_all(Default::default()).success);
    let mut registry = ListenerRegistry::default();
    registry.register(
        "Listeners.php",
        source,
        &graph.get_nodes_in_file("Listeners.php"),
    );
    run(&mut graph, &mut registry);
    graph.close();
}

#[test]
fn case_310_typed_union_map_and_stability() {
    let source = "<?php class Typed {\n public function handle(Changed|Other $event) {}\n}\nclass Untyped {\n public function handle() {}\n}\nclass Provider {\n protected $listen = [Mapped::class => [Untyped::class]];\n}";
    check(source, |g, r| {
        for event in ["Changed", "Other", "Mapped"] {
            assert_eq!(r.targets(event).len(), 1, "{event}");
        }
        let count = g.get_nodes_in_file("Listeners.php").len();
        assert!(g.index_all(Default::default()).success);
        r.register(
            "Listeners.php",
            source,
            &g.get_nodes_in_file("Listeners.php"),
        );
        assert_eq!(r.targets("Mapped").len(), 1);
        assert_eq!(count, g.get_nodes_in_file("Listeners.php").len());
        for n in g.get_nodes_in_file("Listeners.php") {
            assert!(g.get_outgoing_edges(&n.id).iter().all(|e| {
                e.metadata
                    .as_ref()
                    .and_then(|m| m.get("synthesizedBy"))
                    .and_then(|s| s.as_str())
                    != Some("laravel-event")
            }));
        }
        r.register("Listeners.php", "<?php", &[]);
        for event in ["Changed", "Mapped"] {
            assert!(r.targets(event).is_empty());
        }
    });
}
#[test]
fn case_310_comments_and_strings_are_not_evidence() {
    check(
        r#"<?php
// class Fake { function handle(Changed $e) {} }
class Plain { function handle($e) {} function text() { $s = 'function handle(Changed $e) {}'; } }
class Provider { /* protected $listen = [Changed::class => [Plain::class]]; */ }
"#,
        |_, r| assert!(r.targets("Changed").is_empty()),
    );
}
#[test]
fn case_310_dynamic_missing_and_primitive_are_not_evidence() {
    check(
        r#"<?php
class Plain { function handle(string $event) {} function other(Changed $event) {} }
class Provider { protected $listen = [Changed::class => [$listener], Dynamic::class => factory()]; }
abstract class Missing { abstract function handle(Absent $event); }
"#,
        |_, r| {
            for event in ["Changed", "Dynamic", "string", "Absent"] {
                assert!(r.targets(event).is_empty(), "{event}");
            }
        },
    );
}

#[test]
fn case_310_ambiguous_listener_names_and_exact_event_references() {
    check(
        r#"<?php
namespace One {
class Duplicate {
 public function handle() {}
}
}
namespace Two {
class Duplicate {
 public function handle() {}
}
class Typed {
 public function handle(\App\Events\Changed $event) {}
}
class Provider {
 protected $listen = [Mapped::class => [Duplicate::class]];
}
}
"#,
        |_, r| {
            assert!(r.targets("Mapped").is_empty());
            assert!(r.targets("Changed").is_empty());
            assert_eq!(r.targets("\\App\\Events\\Changed").len(), 1);
        },
    );
}
