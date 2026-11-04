use rustcodegraph::extraction::{
    grammars, index::extract_from_source, languages::index::extractor_for,
};

#[test]
fn case_117_detection_and_native_entry() {
    for path in ["sample.erl", "records.hrl", "tool.escript"] {
        let language = grammars::detect_language(path, None);
        assert_eq!(grammars::language_key(&language), "erlang");
        assert_eq!(serde_json::to_string(&language).unwrap(), "\"erlang\"");
        assert!(grammars::is_source_file(path));
        assert!(extractor_for("erlang").is_some());
        let mut parser = grammars::get_parser(language).expect("native Erlang parser");
        let tree = parser
            .parse("-module(sample).\nrun() -> ok.\n", None)
            .unwrap();
        assert_eq!(tree.root_node.node_type(), "source_file");
        assert!(!tree.root_node.has_error);
        assert!(
            tree.root_node
                .named_children
                .iter()
                .any(|n| n.node_type() == "fun_decl")
        );
    }
}

#[test]
fn case_117_empty_unicode_and_damaged_input() {
    for source in [
        "",
        "-module(sample).\nrun() -> \"你好🌍\".\n",
        "-module(broken). run( ->",
    ] {
        let language = grammars::detect_language("sample.erl", None);
        let tree = grammars::get_parser(language)
            .unwrap()
            .parse(source, None)
            .unwrap();
        assert_eq!(tree.root_node.has_error, source.contains("broken"));
        let result = extract_from_source("sample.erl", source, None, None);
        assert_eq!(
            result.nodes.len(),
            1,
            "grammar-only integration creates a file node"
        );
        assert_eq!(grammars::language_key(&result.nodes[0].language), "erlang");
    }
}

#[test]
fn case_117_real_files_and_sqlite() {
    let dir = std::env::temp_dir().join(format!("rustcodegraph-117-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (path, source) in [
        ("sample.erl", "-module(sample). run() -> ok."),
        ("records.hrl", ""),
        ("tool.escript", "%% 中文\nmain(_) -> ok."),
    ] {
        std::fs::write(dir.join(path), source).unwrap();
    }
    let mut graph = rustcodegraph::CodeGraph::init_sync(&dir).unwrap();
    let result = graph.index_all(Default::default());
    assert!(result.success, "{:?}", result.errors);
    for path in ["sample.erl", "records.hrl", "tool.escript"] {
        assert_eq!(graph.get_nodes_in_file(path).len(), 1);
    }
    graph.close();
    std::fs::remove_dir_all(dir).unwrap();
}
