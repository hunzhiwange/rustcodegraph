struct Project(std::path::PathBuf);
impl Project {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "rcg-206-{}-{}",
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
fn case_206_cuda_cpp_paths_and_calls() {
    for extension in ["cu", "cuh", "CU", "CUH"] {
        let file = format!("kernels/计算.{extension}");
        assert_eq!(detect_language(&file, None), Language::Cpp);
        assert!(is_source_file(&file));
        let root = Project::new();
        std::fs::create_dir(root.path().join("kernels")).unwrap();
        std::fs::write(root.path().join(&file), "// 中文坐标\n__device__ int helper(int x) { return x + 1; }\n__global__ __launch_bounds__(256) void step(int *x) { x[0] = helper(x[0]); }\nvoid launch(int *x) { step<<<dim3{1,1,1}, 256>>>(x); cudaDeviceSynchronize(); }\n").unwrap();
        let mut graph = CodeGraph::init_sync(root.path()).unwrap();
        assert!(graph.index_all(Default::default()).success);
        let nodes = graph.get_nodes_in_file(&file);
        let find = |name: &str| {
            nodes
                .iter()
                .find(|n| n.name == name)
                .unwrap_or_else(|| panic!("missing {name}: {nodes:?}"))
        };
        let (helper, step, launch) = (find("helper"), find("step"), find("launch"));
        for node in [helper, step, launch] {
            assert_eq!(node.file_path, file);
            assert_eq!(node.language, Language::Cpp);
        }
        for (from, to) in [(launch, step), (step, helper)] {
            assert!(
                graph
                    .get_outgoing_edges(&from.id)
                    .iter()
                    .any(|e| e.kind == EdgeKind::Calls && e.target == to.id),
                "missing {} -> {}",
                from.name,
                to.name
            );
        }
        assert!(!nodes.iter().any(|n| n.name == "cudaDeviceSynchronize"));
        graph.close();
    }
}

#[test]
fn case_206_comments_strings_and_cpp_are_unchanged() {
    use rustcodegraph::extraction::index::extract_from_source;
    let source = "// __global__ void fake() {}\nconst char *text = R\"tag(__device__ void invented() {} k<<<1,2>>>();)tag\";\nint normal(int x) { return x << 2; }\n";
    let result = extract_from_source("ordinary.cpp", source, None, None);
    assert!(result.nodes.iter().any(|n| n.name == "normal"));
    assert!(
        !result
            .nodes
            .iter()
            .any(|n| matches!(n.name.as_str(), "fake" | "invented" | "k"))
    );
    assert_eq!(detect_language("fake.cu.txt", None), Language::Unknown);
    assert!(!is_source_file("fake.cu.txt"));
}

#[test]
fn case_206_unmatched_launch_preserves_following_code() {
    use rustcodegraph::extraction::index::extract_from_source;
    let result = extract_from_source(
        "broken.cu",
        "void broken() { k<<<1, 2; }\nint survivor() { return 7; }",
        None,
        None,
    );
    assert!(
        result.nodes.iter().any(|n| n.name == "survivor"),
        "{:?}",
        result.nodes
    );
}

#[test]
fn case_206_masking_preserves_utf8_and_literal_bytes() {
    use rustcodegraph::extraction::{
        languages::c_cpp::cpp_extractor, tree_sitter_types::LanguageExtractor,
    };
    let source = "// 中文\r\n__global__ void kernel() {}\r\nvoid host() { kernel<<<dim3{1,1,1},\n 256>>>() ; }\n";
    let masked = cpp_extractor().pre_parse(source).unwrap();
    assert_eq!(source.len(), masked.len());
    for (before, after) in source.bytes().zip(masked.bytes()) {
        if matches!(before, b'\r' | b'\n') {
            assert_eq!(before, after);
        }
    }
    assert_eq!(source.find("void kernel"), masked.find("void kernel"));
    assert!(masked.starts_with("// 中文\r\n"));
    for untouched in [
        "// __global__ kernel<<<1,2>>>()\nvoid normal() {}",
        "const char *x = \"__device__ k<<<1,2>>>()\";",
        "const char *x = R\"raw(__global__ \" k<<<1,2>>>())raw\";",
        "void host() { k<<<1,2; }\nvoid survivor() {}",
        "<<<<<<< ours\nhello\n=======\nworld\n>>>>>>> theirs\nvoid survivor() {}",
    ] {
        assert_eq!(cpp_extractor().pre_parse(untouched), None, "{untouched}");
    }
}
