use rustcodegraph::{extraction::index::extract_from_source, types::ReferenceKind};

#[test]
fn case_083_references_and_ownership() {
    let source = r#"module "app" {
 providers = { aws.child = aws.east, google = google }
 value = module.network[0].outputs.id
 state = data.terraform_remote_state.shared.outputs.url
 nested { value = var.region }
}
locals {
 name = "你好${local.prefix}-${module.app.url}"
}
resource "aws_instance" "main" {
 provider = aws.east
 value = aws_vpc.main.id
}
"#;
    let r = extract_from_source("main.tf", source, None, None);
    assert!(r.errors.is_empty());
    let refs: Vec<_> = r
        .unresolved_references
        .iter()
        .map(|v| {
            let owner = r.nodes.iter().find(|n| n.id == v.from_node_id).unwrap();
            assert_eq!(v.reference_kind, ReferenceKind::References);
            (owner.qualified_name.as_str(), v.reference_name.as_str())
        })
        .collect();
    for pair in [
        ("module.app", "provider.aws.east"),
        ("module.app", "provider.google"),
        ("module.app", "module.network"),
        ("module.app", "module.network:output.outputs"),
        ("module.app", "module.network:remote-output.id"),
        ("module.app", "data.terraform_remote_state.shared"),
        ("module.app", "var.region"),
        ("local.name", "local.prefix"),
        ("local.name", "module.app:output.url"),
        ("aws_instance.main", "provider.aws.east"),
        ("aws_instance.main", "aws_vpc.main"),
    ] {
        assert!(refs.contains(&pair), "missing {pair:?}: {refs:?}");
    }
    assert!(
        !refs
            .iter()
            .any(|(_, name)| *name == "aws.child" || *name == "aws.east")
    );
    let reference = r
        .unresolved_references
        .iter()
        .find(|v| v.reference_name == "local.prefix")
        .unwrap();
    assert_eq!(
        (reference.line, reference.column),
        (8, " name = \"你好${".len() as u64)
    );
}

#[test]
fn case_083_literals_comments_and_builtins() {
    let r = extract_from_source(
        "main.tf",
        r#"output "x" {
 value = "module.fake.out and data.terraform_remote_state.fake.outputs.x"
 # module.comment.out
 other = [each.value, count.index, path.module, terraform.workspace, self.id]
}
unknown { value = module.unknown.out }
"#,
        None,
        None,
    );
    assert!(
        r.unresolved_references.is_empty(),
        "{:?}",
        r.unresolved_references
    );
}

#[test]
fn case_083_dynamic_provider_and_incomplete_addresses() {
    let r = extract_from_source(
        "main.tofu",
        r#"resource "x" "y" {
 provider = var.choose ? aws.east : aws.west
 value = [var, local, module, data.only, bare]
}
module "m" { providers = { aws = "aws.fake" } }
"#,
        None,
        None,
    );
    assert!(
        !r.unresolved_references
            .iter()
            .any(|v| v.reference_name.starts_with("provider.") || v.reference_name == "aws.fake")
    );
    assert!(
        r.unresolved_references
            .iter()
            .all(|v| v.reference_name == "var.choose")
    );
}

#[test]
fn case_083_real_files_sqlite() {
    let dir = std::env::temp_dir().join(format!("rustcodegraph-083-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("main.tf"),
        "variable \"region\" {}\noutput \"x\" { value = var.region }\n",
    )
    .unwrap();
    let mut graph = rustcodegraph::CodeGraph::init_sync(&dir).unwrap();
    let result = graph.index_all(Default::default());
    assert!(result.success, "{:?}", result.errors);
    assert_eq!(graph.get_nodes_in_file("main.tf").len(), 3);
    let nodes = graph.get_nodes_in_file("main.tf");
    let output = nodes
        .iter()
        .find(|n| n.qualified_name == "output.x")
        .unwrap();
    let variable = nodes
        .iter()
        .find(|n| n.qualified_name == "var.region")
        .unwrap();
    assert!(
        graph
            .get_outgoing_edges(&output.id)
            .iter()
            .any(|e| e.target == variable.id)
    );
    graph.close();
    std::fs::remove_dir_all(dir).unwrap();
}
