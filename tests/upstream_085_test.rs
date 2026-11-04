use rustcodegraph::CodeGraph;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);

fn check(files: &[(&str, &str)], from: (&str, &str), target: (&str, &str), expected: bool) {
    let dir = std::env::temp_dir().join(format!(
        "rcg-085-{}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    for (path, text) in files {
        let path = dir.as_path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    let mut graph = CodeGraph::init_sync(dir.as_path()).unwrap();
    assert!(graph.index_all(Default::default()).success);
    let find = |g: &mut CodeGraph, (file, name): (&str, &str)| {
        g.get_nodes_in_file(file)
            .into_iter()
            .find(|n| n.qualified_name == name)
            .unwrap()
    };
    let from = find(&mut graph, from);
    let target = find(&mut graph, target);
    let edges = graph.get_outgoing_edges(&from.id);
    assert_eq!(edges.iter().any(|e| e.target == target.id), expected);
    if expected && from.qualified_name == "output.consume" {
        let bridge = edges.iter().find(|e| e.target == target.id).unwrap();
        assert_eq!(
            bridge.provenance,
            Some(rustcodegraph::types::EdgeProvenance::Heuristic)
        );
        let metadata = bridge.metadata.as_ref().unwrap();
        assert_eq!(metadata["synthesizedBy"], "terraform-remote-state");
        assert!(
            metadata["registeredAt"]
                .as_str()
                .unwrap()
                .starts_with("app/main.tf:")
        );
        let local = graph
            .get_nodes_in_file(&target.file_path)
            .into_iter()
            .find(|n| n.qualified_name == "local.result")
            .unwrap();
        assert!(
            graph
                .get_outgoing_edges(&target.id)
                .iter()
                .any(|e| e.target == local.id)
        );
    }
    let count: usize = files
        .iter()
        .map(|(f, _)| graph.get_nodes_in_file(f).len())
        .sum();
    assert!(graph.index_all(Default::default()).success);
    let after: usize = files
        .iter()
        .map(|(f, _)| graph.get_nodes_in_file(f).len())
        .sum();
    assert_eq!(count, after, "re-index must not grow the node graph");
    assert_eq!(graph.get_outgoing_edges(&from.id).len(), edges.len());
    graph.close();
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn case_085_provider_ancestor() {
    check(
        &[
            ("providers.tf", "provider \"aws\" { alias = \"east\" }"),
            (
                "child/main.tf",
                "resource \"aws_bucket\" \"x\" { provider = aws.east }",
            ),
        ],
        ("child/main.tf", "aws_bucket.x"),
        ("providers.tf", "provider.aws.east"),
        true,
    );
}

#[test]
fn case_085_provider_sibling_is_not_inherited() {
    check(
        &[
            (
                "sibling/providers.tf",
                "provider \"aws\" { alias = \"east\" }",
            ),
            (
                "child/main.tf",
                "resource \"aws_bucket\" \"x\" { provider = aws.east }",
            ),
        ],
        ("child/main.tf", "aws_bucket.x"),
        ("sibling/providers.tf", "provider.aws.east"),
        false,
    );
}

#[test]
fn case_085_provider_ambiguous_nearest_does_not_fall_back() {
    check(
        &[
            ("providers.tf", "provider \"aws\" { alias = \"east\" }"),
            ("child/a.tf", "provider \"aws\" { alias = \"east\" }"),
            ("child/b.tf", "provider \"aws\" { alias = \"east\" }"),
            (
                "child/deep/main.tf",
                "resource \"aws_bucket\" \"x\" { provider = aws.east }",
            ),
        ],
        ("child/deep/main.tf", "aws_bucket.x"),
        ("providers.tf", "provider.aws.east"),
        false,
    );
}

fn remote(component: &str, source: &str, extra: &[(&str, &str)], expected: bool) {
    let wiring = format!(
        "module \"state\" {{\n source = \"{source}\"\n component = {component}\n}}\noutput \"consume\" {{ value = module.state.outputs.id }}"
    );
    let mut files = vec![
        ("app/main.tf", wiring.as_str()),
        (
            "components/vpc/out.tf",
            "output \"id\" { value = local.result }\nlocals { result = \"id\" }",
        ),
    ];
    files.extend_from_slice(extra);
    check(
        &files,
        ("app/main.tf", "output.consume"),
        ("components/vpc/out.tf", "output.id"),
        expected,
    );
}

#[test]
fn case_085_remote_literal_and_variable_default() {
    remote(
        "\"vpc\"",
        "cloudposse/stack-config/yaml//modules/remote-state",
        &[],
        true,
    );
    remote(
        "var.component",
        "cloudposse/stack-config/yaml//modules/remote-state",
        &[(
            "app/vars.tf",
            "variable \"component\" { default = \"vpc\" }",
        )],
        true,
    );
}

#[test]
fn case_085_remote_dynamic_and_unrelated_source() {
    remote(
        "var.unknown",
        "cloudposse/stack-config/yaml//modules/remote-state",
        &[],
        false,
    );
    remote(
        "\"${var.component}\"",
        "cloudposse/stack-config/yaml//modules/remote-state",
        &[],
        false,
    );
    remote("\"vpc\"", "other/module", &[], false);
}

#[test]
fn case_085_remote_ambiguous_component_and_duplicate_output() {
    remote(
        "\"vpc\"",
        "cloudposse/stack-config/yaml//modules/remote-state",
        &[("legacy/vpc/out.tf", "output \"id\" { value = \"old\" }")],
        false,
    );
    remote(
        "\"vpc\"",
        "cloudposse/stack-config/yaml//modules/remote-state",
        &[(
            "components/vpc/duplicate.tf",
            "output \"id\" { value = \"duplicate\" }",
        )],
        false,
    );
}

#[test]
fn case_085_provider_declarations_and_nearest_scope() {
    use rustcodegraph::{extraction::index::extract_from_source, types::NodeKind};
    let extraction = extract_from_source(
        "providers.tf",
        "provider \"aws\" {}\nprovider \"aws\" { alias = \"east\" }\nprovider \"aws\" { alias = var.dynamic }",
        None,
        None,
    );
    let configs: Vec<_> = extraction
        .nodes
        .iter()
        .filter(|n| n.kind == NodeKind::Namespace)
        .collect();
    assert_eq!(configs.len(), 2);
    assert!(configs.iter().any(|n| n.qualified_name == "provider.aws"));
    assert!(
        configs
            .iter()
            .any(|n| n.qualified_name == "provider.aws.east")
    );
    check(
        &[
            ("providers.tf", "provider \"aws\" { alias = \"east\" }"),
            (
                "child/providers.tf",
                "provider \"aws\" { alias = \"east\" }",
            ),
            (
                "child/deep/main.tf",
                "module \"x\" { providers = { aws = aws.east } }",
            ),
        ],
        ("child/deep/main.tf", "module.x"),
        ("child/providers.tf", "provider.aws.east"),
        true,
    );
    check(
        &[
            (
                "providers.tf",
                "provider \"aws\" {}\nprovider \"aws\" { alias = \"east\" }",
            ),
            (
                "child/main.tf",
                "resource \"aws_bucket\" \"x\" { provider = aws.east }",
            ),
        ],
        ("child/main.tf", "aws_bucket.x"),
        ("providers.tf", "provider.aws"),
        false,
    );
}
