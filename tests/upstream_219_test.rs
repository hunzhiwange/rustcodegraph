struct Project(std::path::PathBuf);
impl Project {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "rcg-219-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn path(&self) -> &std::path::Path {
        &self.0
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
use rustcodegraph::{
    CodeGraph,
    extraction::grammars::{detect_language, is_source_file},
    types::{EdgeKind, Language},
};
#[test]
fn case_219_metal_shader_paths_and_calls() {
    for ext in ["metal", "METAL"] {
        let root = Project::new();
        let file = format!("计算.{ext}");
        assert_eq!(detect_language(&file, None), Language::Cpp);
        assert!(is_source_file(&file));
        std::fs::write(root.path().join(&file), "// 中文\nstruct Vertex { float4 position [[position]]; };\nfloat helper(float x) { return x; }\nkernel void compute(device float *data [[buffer(0)]], uint i [[thread_position_in_grid]]) { data[i] = helper(data[i]); external_runtime(); }\nvertex float4 shader(uint id [[vertex_id]]) { return float4(helper(id)); }\nfragment float shade() { return helper(1); }\n").unwrap();
        let mut graph = CodeGraph::init_sync(root.path()).unwrap();
        assert!(graph.index_all(Default::default()).success);
        let nodes = graph.get_nodes_in_file(&file);
        for name in ["Vertex", "position", "helper", "compute", "shader", "shade"] {
            assert!(
                nodes
                    .iter()
                    .any(|n| n.name == name && n.file_path == file && n.language == Language::Cpp),
                "missing {name}: {nodes:?}"
            );
        }
        let helpers: Vec<_> = nodes
            .iter()
            .filter(|n| n.name == "helper")
            .map(|n| &n.id)
            .collect();
        for name in ["compute", "shader", "shade"] {
            let caller = nodes.iter().find(|n| n.name == name).unwrap();
            assert!(
                graph
                    .get_outgoing_edges(&caller.id)
                    .iter()
                    .any(|e| e.kind == EdgeKind::Calls && helpers.contains(&&e.target)),
                "missing {name} -> helper: {:?}; nodes {nodes:?}",
                graph.get_outgoing_edges(&caller.id)
            );
        }
        assert!(!nodes.iter().any(|n| n.name == "external_runtime"));
        graph.close();
    }
}
#[test]
fn case_219_non_metal_extension() {
    assert_eq!(detect_language("fake.metal.txt", None), Language::Unknown);
    assert!(!is_source_file("fake.metal.txt"));
}
#[test]
fn case_219_attributes_preserve_bytes_and_cpp() {
    use rustcodegraph::extraction::{
        languages::c_cpp::cpp_extractor, tree_sitter_types::LanguageExtractor,
    };
    let source = "// 中文\r\nfloat value [[position]];\n";
    let masked = cpp_extractor()
        .pre_parse_for_file(source, "x.metal")
        .unwrap();
    assert_eq!(masked.len(), source.len());
    assert_eq!(masked, "// 中文\r\nfloat value             ;\n");
    assert_eq!(cpp_extractor().pre_parse_for_file(source, "x.cpp"), None);
}

#[test]
fn case_219_literals_comments_and_lambda_are_untouched() {
    use rustcodegraph::extraction::{
        languages::c_cpp::cpp_extractor, tree_sitter_types::LanguageExtractor,
    };
    for source in [
        "// [[position]]\nvoid normal() {}",
        "/* [[buffer(0)]] */ void normal() {}",
        r#"const char *text = "[[position]]";"#,
        r#"const char *text = R"raw([[position]] " [[buffer(0)]])raw";"#,
        "void normal() { arr[[]{ return 0; }()]; }",
        "float broken [[buffer((0))]];",
    ] {
        assert_eq!(
            cpp_extractor().pre_parse_for_file(source, "shader.metal"),
            None,
            "{source}"
        );
    }
    let source =
        "// 中文\nfloat x [[buffer(0), raster_order_group(0)]];\nfloat y [[\nposition\n]];";
    let masked = cpp_extractor()
        .pre_parse_for_file(source, "shader.METAL")
        .unwrap();
    assert_eq!(source.len(), masked.len());
    for (before, after) in source.bytes().zip(masked.bytes()) {
        if matches!(before, b'\r' | b'\n') {
            assert_eq!(before, after);
        }
    }
    assert!(!masked.contains("[["));
}
