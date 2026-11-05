//! MediatR registration evidence; dispatch is a separate task.
pub use rustcodegraph::{resolution, types, web_tree_sitter};
// This registration-only fixture intentionally leaves production dispatch unused.
#[allow(dead_code)]
#[path = "../src/resolution/callback_synthesizer/mediatr.rs"]
mod mediatr;
use mediatr::HandlerRegistry;
use rustcodegraph::CodeGraph;

fn check(source: &str, run: impl FnOnce(&mut CodeGraph, &mut HandlerRegistry)) {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "rcg284-{}-{}",
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
    std::fs::write(&path.join("handlers.cs"), source).unwrap();
    let mut graph = CodeGraph::init_sync(&path).unwrap();
    assert!(graph.index_all(Default::default()).success);
    let mut registry = HandlerRegistry::default();
    registry.register(
        "handlers.cs",
        source,
        &graph.get_nodes_in_file("handlers.cs"),
    );
    run(&mut graph, &mut registry);
    graph.close();
}

#[test]
fn case_284_registration_and_stability() {
    let source = "class Handler : IRequestHandler<Order, int> { public int Handle(Order request) => 1; }\nclass Notice : INotificationHandler<Changed> { public void Handle(Changed request) {} }\nclass Custom : ICommandHandler<Save, int> { public int Handle(Save request) => 1; }";
    check(source, |g, r| {
        for request in ["Order", "Changed", "Save"] {
            assert_eq!(r.targets(request).len(), 1, "{request}");
            assert_eq!(r.targets(request)[0].name, "Handle");
        }
        let count = g.get_nodes_in_file("handlers.cs").len();
        assert!(g.index_all(Default::default()).success);
        r.register("handlers.cs", source, &g.get_nodes_in_file("handlers.cs"));
        assert_eq!(count, g.get_nodes_in_file("handlers.cs").len());
        assert_eq!(r.targets("Order").len(), 1);
        for n in g.get_nodes_in_file("handlers.cs") {
            assert!(g.get_outgoing_edges(&n.id).iter().all(|e| {
                e.metadata
                    .as_ref()
                    .and_then(|m| m.get("synthesizedBy"))
                    .and_then(|v| v.as_str())
                    != Some("mediatr-dispatch")
            }));
        }
        r.register("handlers.cs", "", &[]);
        assert!(r.targets("Order").is_empty());
    });
}
#[test]
fn case_284_no_interface_or_mismatched_request() {
    check(
        r#"class Plain { public void Handle(Order request) {} }
class Wrong : IRequestHandler<Order, int> { public int Handle(Other request) => 1; }
class Text { const string Hint = "IRequestHandler<Order>"; public void Handle(Order request) {} }"#,
        |_, r| {
            assert!(r.targets("Order").is_empty());
            assert!(r.targets("Other").is_empty());
        },
    );
}
#[test]
fn case_284_nested_and_generic_parameters_are_not_evidence() {
    check(
        "class Outer : IRequestHandler<Order> { class Inner { public void Handle(Order request) {} } }\nclass Open<T> : IRequestHandler<T> { public void Handle(T request) {} }\nclass ResponseOnly : IQueryHandler<Order, Reply> { public void Handle(Reply request) {} }",
        |_, r| {
            for request in ["Order", "T", "Reply"] {
                assert!(r.targets(request).is_empty(), "{request}");
            }
        },
    );
}

#[test]
fn case_284_qualified_wrapped_interfaces_and_overloads() {
    check(
        "class Wrapped\n : MediatR.IRequestHandler<Shop.Order, int>\n { public int Handle(Shop.Order request) => 1; public int Handle(Unrelated request) => 0; }\nclass Static : IRequestHandler<StaticOrder> { public static void Handle(StaticOrder request) {} }\nabstract class Abstract : IRequestHandler<AbstractOrder> { public abstract void Handle(AbstractOrder request); }\n// class Fake : IRequestHandler<CommentOrder> {}\n",
        |_, r| {
            assert_eq!(r.targets("Shop.Order").len(), 1);
            for request in [
                "Order",
                "Unrelated",
                "StaticOrder",
                "AbstractOrder",
                "CommentOrder",
            ] {
                assert!(r.targets(request).is_empty(), "{request}");
            }
        },
    );
}
