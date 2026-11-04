use rustcodegraph::{
    extraction::index::extract_from_source,
    types::{EdgeKind, NodeKind},
};

const SOURCE: &str = "resource \"aws_bucket\" \"main\" {\n tags { value = 1 }\n}\ndata \"aws_image\" \"latest\" {}\nmodule \"network\" { source = \"./network\" }\nvariable \"region\" {}\noutput \"address\" {}\nlocals {\n name = \"你好\"\n values = [for item in [] : item]\n}\n";

#[test]
fn case_082_declarations() {
    let result = extract_from_source("main.tf", SOURCE, None, None);
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    let expected = [
        (
            "aws_bucket.main",
            "aws_bucket.main",
            NodeKind::Class,
            1,
            3,
            false,
        ),
        (
            "aws_image.latest",
            "data.aws_image.latest",
            NodeKind::Class,
            4,
            4,
            false,
        ),
        ("network", "module.network", NodeKind::Module, 5, 5, false),
        ("region", "var.region", NodeKind::Variable, 6, 6, true),
        ("address", "output.address", NodeKind::Variable, 7, 7, true),
        ("name", "local.name", NodeKind::Constant, 9, 9, false),
        ("values", "local.values", NodeKind::Constant, 10, 10, false),
    ];
    assert_eq!(result.nodes.len(), expected.len() + 1);
    for (name, qname, kind, start, end, exported) in expected {
        let n = result.nodes.iter().find(|n| n.name == name).unwrap();
        assert_eq!(
            (
                &n.qualified_name,
                n.kind,
                n.start_line,
                n.end_line,
                n.is_exported
            ),
            (&qname.to_owned(), kind, start, end, Some(exported))
        );
        assert!(n.signature.as_ref().is_some_and(|s| !s.is_empty()));
        assert!(result.edges.iter().any(|e| e.kind == EdgeKind::Contains
            && e.target == n.id
            && e.source == result.nodes[0].id));
    }
}

#[test]
fn case_082_boundaries() {
    for source in [
        "",
        "region = 1\n",
        "resource \"\" \"bad\" {}\nvariable \"\" {}\n",
        "terraform { required_providers {} }\n",
        "resource \"only_one\" {}\nmodule \"a\" \"extra\" {}\nlocals \"labeled\" { bad = 1 }\n",
        "unknown { resource \"fake\" \"nested\" {} }\n",
    ] {
        assert_eq!(
            extract_from_source("main.tf", source, None, None)
                .nodes
                .len(),
            1,
            "{source}"
        );
    }
    let r = extract_from_source(
        "main.tofu",
        "resource aws_bucket bare {}\nlocals { nested = { key = 1 } }\n",
        None,
        None,
    );
    assert_eq!(r.nodes.len(), 3);
    assert!(
        r.nodes
            .iter()
            .any(|n| n.qualified_name == "aws_bucket.bare")
    );
}

#[test]
fn case_082_sqlite_reindex() {
    let dir = std::env::temp_dir().join(format!("rustcodegraph-082-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("main.tf"), SOURCE).unwrap();
    let mut graph = rustcodegraph::CodeGraph::init_sync(&dir).unwrap();
    for _ in 0..2 {
        let result = graph.index_all(Default::default());
        assert!(result.success, "{:?}", result.errors);
        let nodes = graph.get_nodes_in_file("main.tf");
        assert_eq!(nodes.len(), 8);
        for expected in extract_from_source("main.tf", SOURCE, None, None).nodes {
            let stored = nodes.iter().find(|n| n.id == expected.id).unwrap();
            assert_eq!(stored.qualified_name, expected.qualified_name);
            assert_eq!(stored.kind, expected.kind);
            assert_eq!(stored.is_exported, expected.is_exported);
        }
    }
    graph.close();
    std::fs::remove_dir_all(dir).unwrap();
}
