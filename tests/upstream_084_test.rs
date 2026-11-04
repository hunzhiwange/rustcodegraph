use rustcodegraph::CodeGraph;

#[test]
fn case_084_module_scopes_and_local_sources() {
    let dir = std::env::temp_dir().join(format!(
        "rcg-084-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    for (path, source) in [
        (
            "main.tf",
            "module \"a\" {\n source = \"./modules/a\"\n region = var.region\n}\nmodule \"external\" { source = \"registry/example/a\" }\nvariable \"region\" {}\noutput \"result\" { value = module.a.id }\noutput \"remote\" { value = module.external.id }\noutput \"missing\" { value = var.absent }\n",
        ),
        (
            "modules/a/variables.tf",
            "variable \"region\" {}\noutput \"id\" { value = var.region }\n",
        ),
        (
            "modules/b/main.tf",
            "variable \"region\" {}\nvariable \"absent\" {}\noutput \"id\" { value = var.region }\n",
        ),
        (
            "nested/main.tf",
            "module \"a\" { source = \"../modules/b\" }\noutput \"result\" { value = module.a.id }\n",
        ),
    ] {
        let file = dir.as_path().join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, source).unwrap();
    }
    let mut graph = CodeGraph::init_sync(dir.as_path()).unwrap();
    let result = graph.index_all(Default::default());
    assert!(result.success, "{:?}", result.errors);
    let nodes: Vec<_> = [
        "main.tf",
        "nested/main.tf",
        "modules/a/variables.tf",
        "modules/b/main.tf",
    ]
    .iter()
    .flat_map(|f| graph.get_nodes_in_file(f))
    .collect();
    let node = |file: &str, name: &str| {
        nodes
            .iter()
            .find(|n| n.file_path == file && n.qualified_name == name)
            .unwrap()
    };
    let mut has = |file: &str, name: &str, to_file: &str, to: &str| {
        let from = node(file, name);
        let target = node(to_file, to);
        graph
            .get_outgoing_edges(&from.id)
            .iter()
            .any(|e| e.target == target.id)
    };
    assert!(has(
        "main.tf",
        "output.result",
        "modules/a/variables.tf",
        "output.id"
    ));
    assert!(has(
        "main.tf",
        "module.a",
        "modules/a/variables.tf",
        "var.region"
    ));
    assert!(has(
        "main.tf",
        "module.a",
        "modules/a/variables.tf",
        "modules/a/variables.tf"
    ));
    assert!(has(
        "nested/main.tf",
        "output.result",
        "modules/b/main.tf",
        "output.id"
    ));
    assert!(has(
        "modules/a/variables.tf",
        "output.id",
        "modules/a/variables.tf",
        "var.region"
    ));
    assert!(!has(
        "main.tf",
        "module.a",
        "modules/b/main.tf",
        "var.region"
    ));
    assert!(has(
        "main.tf",
        "output.remote",
        "main.tf",
        "module.external"
    ));
    for name in ["output.remote", "output.missing"] {
        assert!(
            graph
                .get_outgoing_edges(&node("main.tf", name).id)
                .iter()
                .all(|e| nodes.iter().find(|n| n.id == e.target).unwrap().file_path == "main.tf"),
            "unexpected cross-module edge for {name}"
        );
    }
    graph.close();
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn case_084_ambiguous_same_directory_stays_unresolved() {
    let dir = std::env::temp_dir().join(format!("rcg-084-ambiguous-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (file, text) in [
        ("a.tf", "variable \"x\" {}"),
        ("b.tf", "variable \"x\" {}"),
        ("use.tf", "output \"x\" { value = var.x }"),
    ] {
        std::fs::write(dir.join(file), text).unwrap();
    }
    let mut graph = CodeGraph::init_sync(&dir).unwrap();
    assert!(graph.index_all(Default::default()).success);
    let output = graph
        .get_nodes_in_file("use.tf")
        .into_iter()
        .find(|n| n.qualified_name == "output.x")
        .unwrap();
    assert!(graph.get_outgoing_edges(&output.id).is_empty());
    graph.close();
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn case_084_dynamic_missing_and_external_sources_stay_unresolved() {
    let dir = std::env::temp_dir().join(format!("rcg-084-boundaries-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("child")).unwrap();
    std::fs::write(
        dir.join("child/main.tf"),
        "variable \"x\" {}\noutput \"id\" { value = var.x }",
    )
    .unwrap();
    std::fs::write(
        dir.join("main.tf"),
        r#"
module "dynamic" { source = "./${var.path}" }
module "missing" { source = "./missing" }
module "external" { source = "git::https://example.com/child" }
output "dynamic" { value = module.dynamic.id }
output "missing" { value = module.missing.id }
output "external" { value = module.external.id }
"#,
    )
    .unwrap();
    let mut graph = CodeGraph::init_sync(&dir).unwrap();
    assert!(graph.index_all(Default::default()).success);
    for node in graph.get_nodes_in_file("main.tf") {
        let child_ids: Vec<_> = graph
            .get_nodes_in_file("child/main.tf")
            .into_iter()
            .map(|n| n.id)
            .collect();
        assert!(
            graph
                .get_outgoing_edges(&node.id)
                .iter()
                .all(|e| !child_ids.contains(&e.target))
        );
    }
    graph.close();
    std::fs::remove_dir_all(dir).unwrap();
}
