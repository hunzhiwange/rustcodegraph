use rustcodegraph::resolution::{
    index::ReferenceResolver,
    types::{ResolutionContext, now_ms},
};
use rustcodegraph::types::{Language, Node, NodeKind};
use rustcodegraph::{DatabaseConnection, QueryBuilder};
fn make_node_named(id: &str, name: &str) -> Node {
    Node {
        id: id.to_owned(),
        kind: NodeKind::Function,
        name: name.to_owned(),
        qualified_name: name.to_owned(),
        file_path: "a.ts".to_owned(),
        language: Language::TypeScript,
        start_line: 1,
        end_line: 1,
        start_column: 0,
        end_column: 0,
        docstring: None,
        signature: None,
        visibility: None,
        is_exported: None,
        is_async: None,
        is_static: None,
        is_abstract: None,
        decorators: None,
        type_parameters: None,
        return_type: None,
        updated_at: now_ms(),
    }
}

#[test]
fn case_011_batch_reuse_isolation_and_refresh() {
    let dir = std::env::temp_dir().join(format!("rcg011-{}-{}", std::process::id(), now_ms()));
    std::fs::create_dir(&dir).unwrap();
    let mut db = DatabaseConnection::initialize(dir.join("graph.db")).unwrap();
    let mut resolver =
        ReferenceResolver::new(dir.to_string_lossy(), QueryBuilder::new(db.get_db()));
    let mut class = make_node_named("a", "Same");
    class.kind = NodeKind::Class;
    class.language = Language::Java;
    class.end_line = 20;
    let mut field = class.clone();
    field.id = "field".into();
    field.name = "sink".into();
    field.kind = NodeKind::Field;
    field.start_line = 2;
    field.end_line = 2;
    field.signature = Some("private Writer sink".into());
    resolver.queries.insert_node(&class).unwrap();
    resolver.queries.insert_node(&field).unwrap();
    assert_eq!(
        resolver.get_member_receiver_type(&class, "sink").as_deref(),
        Some("Writer")
    );
    // Evict the raw-node snapshot so this checks analyzed-member reuse rather
    // than merely the lower-level file-node cache.
    let limit = std::env::var("RUSTCODEGRAPH_RESOLVER_CACHE_SIZE")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(5000);
    for i in 0..=limit {
        resolver.get_nodes_in_file(&format!("unused-{i}.java"));
    }
    // Change only the underlying SQL row: the analyzed declaration must survive within this batch.
    field.signature = Some("private Reader sink".into());
    resolver.queries.insert_node(&field).unwrap();
    assert_eq!(
        resolver.get_member_receiver_type(&class, "sink").as_deref(),
        Some("Writer")
    );
    let mut other = class.clone();
    other.id = "b".into();
    other.file_path = "b.java".into();
    resolver.queries.insert_node(&other).unwrap();
    assert_eq!(resolver.get_member_receiver_type(&other, "sink"), None);
    assert_eq!(resolver.get_member_receiver_type(&class, "missing"), None);
    resolver.clear_caches();
    assert_eq!(
        resolver.get_member_receiver_type(&class, "sink").as_deref(),
        Some("Reader")
    );
    drop(resolver);
    db.close().unwrap();
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn case_011_native_nested_and_inherited_declarations() {
    use rustcodegraph::CodeGraph;
    let dir =
        std::env::temp_dir().join(format!("rcg011-native-{}-{}", std::process::id(), now_ms()));
    std::fs::create_dir(&dir).unwrap();
    std::fs::write(dir.join("Example.java"), "class Base {\n Writer sink;\n}\nclass Child extends Base {\n class Inner {\n Reader sink;\n }\n}\n").unwrap();
    let mut graph = CodeGraph::init_sync(&dir).unwrap();
    assert!(graph.index_all(Default::default()).success);
    let classes = graph.get_nodes_by_kind(NodeKind::Class);
    let base = classes.iter().find(|n| n.name == "Base").unwrap();
    let child = classes.iter().find(|n| n.name == "Child").unwrap();
    let inner = classes.iter().find(|n| n.name == "Inner").unwrap();
    let fields = graph.get_nodes_by_kind(NodeKind::Field);
    assert_eq!(fields.len(), 2);
    // Use the real parser's declarations in a real SQLite resolver context.
    let mut db = DatabaseConnection::initialize(dir.join("members.db")).unwrap();
    let mut resolver =
        ReferenceResolver::new(dir.to_string_lossy(), QueryBuilder::new(db.get_db()));
    for node in classes.iter().chain(fields.iter()) {
        resolver.queries.insert_node(node).unwrap();
    }
    // The native Java extractor currently omits field signatures. Preserve
    // this conservative miss rather than inventing a receiver type.
    assert!(fields.iter().all(|field| field.signature.is_none()));
    assert_eq!(resolver.get_member_receiver_type(base, "sink"), None);
    assert_eq!(resolver.get_member_receiver_type(inner, "sink"), None);
    // Own-declaration caching must neither copy a base declaration nor leak a nested one.
    assert_eq!(resolver.get_member_receiver_type(child, "sink"), None);
    drop(resolver);
    db.close().unwrap();
    graph.destroy();
    std::fs::remove_dir_all(dir).unwrap();
}
