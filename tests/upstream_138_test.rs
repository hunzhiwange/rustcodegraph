fn check(source: &str, expected: &[&str]) {
    let dir = std::env::temp_dir().join(format!(
        "rcg138-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    for (path, text) in [
        (
            "lib/One.vb",
            "Namespace One\nModule Tools\nSub Work()\nEnd Sub\nEnd Module\nEnd Namespace\n",
        ),
        (
            "other/Two.vb",
            "Namespace Two\nModule Tools\nSub Work()\nEnd Sub\nEnd Module\nEnd Namespace\n",
        ),
        ("app/Main.vb", source),
    ] {
        let path = dir.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    let mut graph = rustcodegraph::CodeGraph::init_sync(&dir).unwrap();
    assert!(graph.index_all(Default::default()).success);
    graph.close();
    let db = rusqlite::Connection::open(rustcodegraph::get_database_path(&dir)).unwrap();
    let mut stmt = db.prepare("SELECT target.qualified_name FROM edges e JOIN nodes caller ON caller.id=e.source JOIN nodes target ON target.id=e.target WHERE caller.name='Run' AND e.kind='calls' ORDER BY target.qualified_name").unwrap();
    let actual: Vec<String> = stmt
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    drop(stmt);
    drop(db);
    std::fs::remove_dir_all(dir).unwrap();
    assert_eq!(actual, expected, "{source}");
}

#[test]
fn case_138_import_alias_and_case_insensitive_cross_file() {
    check(
        "Imports Pick = One.Tools\nModule Main\nSub Run()\npick.work()\nEnd Sub\nEnd Module\n",
        &["One.vb::One::Tools::Work"],
    );
}
#[test]
fn case_138_namespace_import_disambiguates() {
    check(
        "Imports One\nModule Main\nSub Run()\nTools.Work()\nEnd Sub\nEnd Module\n",
        &["One.vb::One::Tools::Work"],
    );
}
#[test]
fn case_138_external_and_ambiguous_names_stay_unresolved() {
    check(
        "Imports External\nModule Main\nSub Run()\nExternal.Tools.Work()\nTools.Work()\nWork()\nEnd Sub\nEnd Module\n",
        &[],
    );
    check(
        "Imports One\nImports Two\nModule Main\nSub Run()\nTools.Work()\nEnd Sub\nEnd Module\n",
        &[],
    );
}

#[test]
fn case_138_global_and_relative_namespace_names() {
    check(
        "Module Main\nSub Run()\nGlobal.One.Tools.Work()\nEnd Sub\nEnd Module\n",
        &["One.vb::One::Tools::Work"],
    );
    check(
        "Namespace One\nModule Main\nSub Run()\nTools.Work()\nEnd Sub\nEnd Module\nEnd Namespace\n",
        &["One.vb::One::Tools::Work"],
    );
}

#[test]
fn case_138_import_edges_bind_local_namespace_only() {
    let dir = std::env::temp_dir().join(format!("rcg138-imports-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("Types.vb"),
        "Namespace Company.Product\nClass Widget\nEnd Class\nEnd Namespace\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("Main.vb"),
        "Imports Company.Product\nImports External.Product\nImports Missing = External.Widget\n",
    )
    .unwrap();
    let mut graph = rustcodegraph::CodeGraph::init_sync(&dir).unwrap();
    assert!(graph.index_all(Default::default()).success);
    graph.close();
    let db = rusqlite::Connection::open(rustcodegraph::get_database_path(&dir)).unwrap();
    let names: Vec<String> = db
        .prepare("SELECT n.name FROM edges e JOIN nodes n ON n.id=e.target WHERE e.kind='imports'")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    drop(db);
    std::fs::remove_dir_all(dir).unwrap();
    assert_eq!(names, ["Company.Product"]);
}
