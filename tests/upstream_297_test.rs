//! Spring event registration evidence; dispatch is a separate task.
pub use rustcodegraph::{resolution, types, web_tree_sitter};
// This registration-only fixture intentionally leaves production dispatch unused.
#[allow(dead_code)]
#[path = "../src/resolution/callback_synthesizer/spring_event.rs"]
mod spring_event;
use rustcodegraph::CodeGraph;
use spring_event::ListenerRegistry;

fn check(source: &str, run: impl FnOnce(&mut CodeGraph, &mut ListenerRegistry)) {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "rcg297-{}-{}",
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
    std::fs::write(path.join("Listeners.java"), source).unwrap();
    let mut graph = CodeGraph::init_sync(&path).unwrap();
    assert!(graph.index_all(Default::default()).success);
    let mut registry = ListenerRegistry::default();
    registry.register(
        "Listeners.java",
        source,
        &graph.get_nodes_in_file("Listeners.java"),
    );
    run(&mut graph, &mut registry);
    graph.close();
}

#[test]
fn case_297_registration_and_stability() {
    let source = "class Listeners { @EventListener public void changed(final Changed event) {} @TransactionalEventListener public void committed(Changed event) {} @EventListener public void qualified(shop.Changed event) {} }";
    check(source, |g, r| {
        assert_eq!(r.targets("Changed").len(), 2);
        assert_eq!(r.targets("shop.Changed").len(), 1);
        for node in g.get_nodes_in_file("Listeners.java") {
            assert!(g.get_outgoing_edges(&node.id).iter().all(|edge| {
                edge.metadata
                    .as_ref()
                    .and_then(|m| m.get("synthesizedBy"))
                    .and_then(|v| v.as_str())
                    != Some("spring-event")
            }));
        }
        let count = g.get_nodes_in_file("Listeners.java").len();
        assert!(g.index_all(Default::default()).success);
        r.register(
            "Listeners.java",
            source,
            &g.get_nodes_in_file("Listeners.java"),
        );
        assert_eq!(count, g.get_nodes_in_file("Listeners.java").len());
        assert_eq!(r.targets("Changed").len(), 2);
        r.register("Listeners.java", "", &[]);
        assert!(r.targets("Changed").is_empty());
    });
}
#[test]
fn case_297_annotation_must_belong_to_method() {
    check(
        r#"@EventListener class Listeners {
        public void plain(Changed event) {}
        @Deprecated public void unrelated(Changed event) {}
        public void text(Changed event) { String hint = "@EventListener"; }
        // @EventListener
        public void commented(Changed event) {}
        @EventListener public void real(Other event) {}
        public void adjacent(Changed event) {}
    }"#,
        |_, r| {
            assert!(r.targets("Changed").is_empty());
            assert_eq!(r.targets("Other").len(), 1);
        },
    );
}
#[test]
fn case_297_missing_or_unsupported_type_is_not_evidence() {
    check(
        "abstract class Listeners { @EventListener public void empty() {} @EventListener public void primitive(int event) {} @EventListener public void array(Changed[] event) {} @EventListener public void many(Changed first, Other second) {} @EventListener public abstract void absent(Changed event); }",
        |_, r| {
            for name in ["Changed", "Other", "int", ""] {
                assert!(r.targets(name).is_empty(), "{name}");
            }
        },
    );
}

#[test]
fn case_297_qualified_annotations_and_file_replacement() {
    let source = r#"class Listeners {
        @org.springframework.context.event.EventListener
        public void qualified(@NotNull final shop.Changed event) {}
        @EventListener(Changed.class)
        public void explicit(Changed event) {}
        @Other.EventListener public void unrelated(Changed event) {}
        @EventListener public void variadic(Changed... events) {}
        @EventListener public void dimensions(Changed event[]) {}
    }"#;
    check(source, |g, r| {
        assert_eq!(r.targets("shop.Changed").len(), 1);
        assert_eq!(r.targets("Changed").len(), 1);
        assert_eq!(r.targets("Changed")[0].name, "explicit");
        let nodes = g.get_nodes_in_file("Listeners.java");
        r.register("wrong.txt", source, &nodes);
        assert_eq!(r.targets("Changed").len(), 1);
        r.register("Listeners.java", "class Listeners {}", &nodes);
        assert!(r.targets("Changed").is_empty());
        assert!(r.targets("shop.Changed").is_empty());
    });
}
