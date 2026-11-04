use rustcodegraph::extraction::languages::index::extractor_for;
use rustcodegraph::extraction::{grammars::*, index::extract_from_source};

#[test]
fn case_081_detection_and_serialization() {
    let lang = detect_language("main.TF", None);
    assert_eq!(serde_json::to_string(&lang).unwrap(), "\"terraform\"");
    assert_eq!(
        serde_json::from_str::<rustcodegraph::types::Language>("\"terraform\"").unwrap(),
        lang
    );
    for name in ["main.tf", "variables.tfvars", "main.tofu", "MAIN.TF"] {
        assert!(is_source_file(name));
        assert_eq!(detect_language(name, None), lang);
    }
    assert!(!is_source_file("main.hcl"));
    assert!(!is_source_file("main.tf.bak"));
    assert!(extractor_for("terraform").is_some());
    assert!(is_language_supported(lang));
    assert!(get_supported_languages().contains(&lang));
    assert_eq!(get_language_display_name(lang), "Terraform");
    assert!(rustcodegraph::web_tree_sitter::Language::load("terraform").is_ok());
}

#[test]
fn case_081_native_parser() {
    let lang = detect_language("main.tf", None);
    let mut parser = get_parser(lang).expect("Terraform native parser");
    let tree = parser
        .parse(
            "resource \"aws_s3_bucket\" \"demo\" { bucket = \"demo\" }",
            None,
        )
        .unwrap();
    assert_eq!(tree.root_node.type_name(), "config_file");
    assert!(!tree.root_node.has_error);
    assert!(format!("{:?}", tree.root_node).contains("block"));
}

#[test]
fn case_081_boundary_inputs() {
    for source in [
        "",
        "output \"message\" { value = \"你好🌍\" }",
        "resource \"broken\" { x = ",
    ] {
        let mut parser = get_parser(detect_language("main.tf", None)).unwrap();
        let tree = parser.parse(source, None).unwrap();
        assert_eq!(tree.root_node.end_index(), source.len());
        assert_eq!(
            tree.root_node.has_error,
            source == "resource \"broken\" { x = "
        );
        let result = extract_from_source("main.tf", source, None, None);
        assert_eq!(
            result
                .nodes
                .iter()
                .filter(|node| node.kind == rustcodegraph::types::NodeKind::File)
                .count(),
            1,
            "file node: {source}"
        );
        assert_eq!(
            serde_json::to_string(&result.nodes[0].language).unwrap(),
            "\"terraform\""
        );
    }
}

#[test]
fn case_081_real_files_and_sqlite() {
    let temp =
        TempDir(std::env::temp_dir().join(format!("rustcodegraph-081-{}", std::process::id())));
    std::fs::create_dir_all(temp.path()).unwrap();
    for (name, source) in [
        ("main.tf", "output \"message\" { value = \"你好\" }"),
        ("empty.tf", ""),
        ("broken.tofu", "resource \"broken\" { x = "),
    ] {
        std::fs::write(temp.path().join(name), source).unwrap();
    }
    let mut graph = rustcodegraph::CodeGraph::init_sync(temp.path()).unwrap();
    let result = graph.index_all(rustcodegraph::IndexOptions::default());
    assert!(result.success, "{:?}", result.errors);
    assert_eq!(result.files_indexed, 3);
    for name in ["main.tf", "empty.tf", "broken.tofu"] {
        let nodes = graph.get_nodes_in_file(name);
        assert_eq!(
            nodes
                .iter()
                .filter(|node| node.kind == rustcodegraph::types::NodeKind::File)
                .count(),
            1
        );
        assert_eq!(language_key(&nodes[0].language), "terraform");
    }
    graph.close();
}

struct TempDir(std::path::PathBuf);
impl TempDir {
    fn path(&self) -> &std::path::Path {
        &self.0
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
