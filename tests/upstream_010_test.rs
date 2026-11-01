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
fn case_010_cached_misses_and_invalidation() {
    let dir = std::env::temp_dir().join(format!("rcg010-{}-{}", std::process::id(), now_ms()));
    std::fs::create_dir(&dir).unwrap();
    let mut db = DatabaseConnection::initialize(dir.as_path().join("graph.db")).unwrap();
    let mut resolver = ReferenceResolver::new(
        dir.as_path().to_string_lossy(),
        QueryBuilder::new(db.get_db()),
    );
    for name in ["os.py", "__init__.py", "missing.py"] {
        assert!(resolver.get_nodes_by_name(name).is_empty());
        let mut node = make_node_named(name, name);
        node.kind = NodeKind::File;
        node.language = Language::Python;
        node.file_path = format!("pkg/{name}");
        resolver.queries.insert_node(&node).unwrap();
        // A new SQL query would see the inserted row; cached misses remain empty.
        for _ in 0..10 {
            assert!(resolver.get_nodes_by_name(name).is_empty());
        }
    }
    resolver.clear_caches();
    for name in ["os.py", "__init__.py", "missing.py"] {
        assert_eq!(resolver.get_nodes_by_name(name).len(), 1);
    }
    drop(resolver);
    db.close().unwrap();
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn case_010_package_and_distinct_callers() {
    use rustcodegraph::resolution::{import_resolver::resolve_via_import, types::UnresolvedRef};
    use rustcodegraph::types::ReferenceKind;
    let dir = std::env::temp_dir().join(format!(
        "rcg010-package-{}-{}",
        std::process::id(),
        now_ms()
    ));
    std::fs::create_dir(&dir).unwrap();
    let mut db = DatabaseConnection::initialize(dir.join("graph.db")).unwrap();
    let mut resolver =
        ReferenceResolver::new(dir.to_string_lossy(), QueryBuilder::new(db.get_db()));
    let mut node = make_node_named("package", "__init__.py");
    node.kind = NodeKind::File;
    node.language = Language::Python;
    node.file_path = dir
        .join("vendor/pkg/sub/__init__.py")
        .to_string_lossy()
        .into_owned();
    resolver.queries.insert_node(&node).unwrap();
    for caller in ["a.py", "nested/b.py", "vendor/pkg/sub/__init__.py"] {
        let file = dir.join(caller);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "import pkg.sub\n").unwrap();
        let reference = UnresolvedRef {
            from_node_id: caller.into(),
            reference_name: "pkg.sub".into(),
            reference_kind: ReferenceKind::Imports,
            line: 1,
            column: 0,
            file_path: file.to_string_lossy().into_owned(),
            language: Language::Python,
            candidates: None,
        };
        let result = resolve_via_import(&reference, &mut resolver);
        assert_eq!(result.is_some(), caller != "vendor/pkg/sub/__init__.py");
    }
    drop(resolver);
    db.close().unwrap();
    std::fs::remove_dir_all(dir).unwrap();
}
