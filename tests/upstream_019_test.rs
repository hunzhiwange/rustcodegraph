use rustcodegraph::extraction::tree_sitter::extract_from_source;
use rustcodegraph::resolution::index::ReferenceResolver;
use rustcodegraph::types::*;
use rustcodegraph::{DatabaseConnection, QueryBuilder};

fn run(source: &str, method: &str) -> (Vec<Node>, Vec<String>) {
    let dir = std::env::temp_dir().join(format!(
        "rcg19-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.as_path().join("Example.java"), source).unwrap();
    let result = extract_from_source("Example.java", source, Some(Language::Java), None);
    assert!(result.errors.is_empty());
    let mut db = DatabaseConnection::initialize(dir.as_path().join("graph.db")).unwrap();
    let mut queries = QueryBuilder::new(db.get_db());
    queries.insert_nodes(&result.nodes).unwrap();
    let mut resolver = ReferenceResolver::new(dir.as_path().to_string_lossy(), queries);
    let refs = result
        .unresolved_references
        .iter()
        .filter(|r| {
            r.reference_kind == ReferenceKind::Calls
                && r.reference_name.rsplit('.').next() == Some(method)
        })
        .cloned()
        .collect::<Vec<_>>();
    assert!(
        !refs.is_empty(),
        "no extracted calls: {:?}",
        result.unresolved_references
    );
    let resolved = resolver.resolve_all(&refs, None);
    let targets = resolved
        .resolved
        .into_iter()
        .map(|r| r.target_node_id)
        .collect();
    drop(resolver);
    db.close().unwrap();
    std::fs::remove_dir_all(dir).unwrap();
    (result.nodes, targets)
}
#[test]
fn case_019_literal_overload() {
    let (nodes, targets) = run(
        "class Example { void pick(int x) {}\n void pick(String x) {}\n void run() { pick(\"yes\"); } }",
        "pick",
    );
    assert_eq!(targets.len(), 1);
    assert!(
        nodes
            .iter()
            .find(|n| n.id == targets[0])
            .unwrap()
            .signature
            .as_deref()
            .unwrap()
            .contains("String")
    );
}
#[test]
fn case_019_unknown_argument_does_not_guess() {
    let (_, targets) = run(
        "class Example { void pick(int x) {}\n void pick(String x) {}\n void run() { pick(external()); } }",
        "pick",
    );
    assert!(targets.is_empty());
}
#[test]
fn case_019_lambda_constraint() {
    let (nodes, targets) = run(
        "import java.util.function.Consumer;\nclass Other { void act() {} }\nclass Worker { void act() {} }\nclass Example { void accept(Consumer<Worker> f) {}\n void run() { accept(w -> { w.act(); }); } }",
        "act",
    );
    assert_eq!(targets.len(), 1);
    assert_eq!(
        nodes
            .iter()
            .find(|n| n.id == targets[0])
            .unwrap()
            .qualified_name,
        "Example.java::Worker::act"
    );
}

#[test]
fn case_019_untyped_lambda_does_not_guess() {
    let (_, targets) = run(
        "class Worker { void act() {} }\nclass Example { void run() { external(w -> { w.act(); }); } }",
        "act",
    );
    assert!(targets.is_empty());
}
#[test]
fn case_019_lambda_uses_its_parameter_position() {
    let (_, targets) = run(
        "import java.util.function.Consumer;\nclass Worker { void act() {} }\nclass Example { void accept(Consumer<Worker> f, Object other) {}\nvoid run() { accept(null, w -> { w.act(); }); } }",
        "act",
    );
    assert!(targets.is_empty());
}

#[test]
fn case_019_parameter_and_widening_before_boxing() {
    let (nodes, targets) = run(
        "class Example { void pick(Integer x) {}\nvoid pick(long x) {}\nvoid run(int value) { pick(value); } }",
        "pick",
    );
    assert_eq!(targets.len(), 1);
    assert!(
        nodes
            .iter()
            .find(|n| n.id == targets[0])
            .unwrap()
            .signature
            .as_deref()
            .unwrap()
            .contains("long")
    );
}
#[test]
fn case_019_class_isolation() {
    let (nodes, targets) = run(
        "class Other { static void pick(String x) {} }\nclass Example { static void pick(int x) {}\nstatic void pick(String x) {}\nvoid run() { Example.pick(4); } }",
        "pick",
    );
    assert_eq!(targets.len(), 1);
    let node = nodes.iter().find(|n| n.id == targets[0]).unwrap();
    assert_eq!(node.qualified_name, "Example.java::Example::pick");
    assert!(node.signature.as_deref().unwrap().contains("int"));
}
#[test]
fn case_019_same_line_calls_keep_their_arguments() {
    let (nodes, targets) = run(
        "class Example { void pick(int x) {}\nvoid pick(String x) {}\nvoid run() { pick(1); pick(\"yes\"); } }",
        "pick",
    );
    assert_eq!(targets.len(), 2);
    assert_ne!(targets[0], targets[1]);
    assert!(
        targets
            .iter()
            .all(|id| nodes.iter().any(|n| &n.id == id && n.name == "pick"))
    );
}

#[test]
fn case_019_unqualified_call_stays_in_its_class() {
    let (nodes, targets) = run(
        "class Other { void pick(int x) {} }\nclass Example { void pick(String x) {}\nvoid pick(int x) {}\nvoid run() { pick(4); } }",
        "pick",
    );
    assert_eq!(targets.len(), 1);
    assert_eq!(
        nodes
            .iter()
            .find(|n| n.id == targets[0])
            .unwrap()
            .qualified_name,
        "Example.java::Example::pick"
    );
}

#[test]
fn case_019_inherited_call_is_preserved() {
    let (nodes, targets) = run(
        "class Base { void pick(int x) {} }\nclass Example extends Base { void run() { pick(4); } }",
        "pick",
    );
    assert_eq!(targets.len(), 1);
    assert_eq!(
        nodes
            .iter()
            .find(|n| n.id == targets[0])
            .unwrap()
            .qualified_name,
        "Example.java::Base::pick"
    );
}
