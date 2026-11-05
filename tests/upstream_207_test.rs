//! CUDA headers and incremental graph audit; upstream 15483713.
use rustcodegraph::{CodeGraph, types::EdgeKind};
use std::path::Path;
type Snapshot = (Vec<String>, Vec<String>);
fn snapshot(root: &Path) -> Snapshot {
    let db = rusqlite::Connection::open(rustcodegraph::get_database_path(root)).unwrap();
    let read = |sql| {
        db.prepare(sql)
            .unwrap()
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .map(|row| {
                let raw = row.unwrap();
                let mut value: serde_json::Value = serde_json::from_str(&raw).unwrap();
                if let Some(items) = value.as_array_mut()
                    && items.len() == 11
                    && let Some(meta) = items[10].as_str()
                {
                    items[10] = serde_json::from_str(meta).unwrap();
                }
                value.to_string()
            })
            .collect::<Vec<_>>()
    };
    // Stable semantic keys exclude generated ids and indexing timestamps.
    (
        read(
            "SELECT json_array(file_path,kind,name,qualified_name,start_line,end_line) FROM nodes ORDER BY 1",
        ),
        read(
            "SELECT json_array(s.file_path,s.kind,s.qualified_name,s.start_line,e.kind,t.file_path,t.kind,t.qualified_name,t.start_line,e.provenance,e.metadata) FROM edges e JOIN nodes s ON s.id=e.source JOIN nodes t ON t.id=e.target ORDER BY 1",
        ),
    )
}
struct Project(std::path::PathBuf);
impl Project {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "rcg-207-{}-{}",
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

#[test]
fn case_207_cuda_headers_and_incremental_deletion() {
    let root = Project::new();
    std::fs::create_dir(root.path().join("gpu")).unwrap();
    std::fs::create_dir(root.path().join("include")).unwrap();
    std::fs::write(
        root.path().join("gpu/ops.cuh"),
        "__device__ int helper() { return 7; }\n",
    )
    .unwrap();
    std::fs::write(
        root.path().join("include/base.h"),
        "int base() { return 1; }\n",
    )
    .unwrap();
    std::fs::write(
        root.path().join("main.cu"),
        r#"#include "ops.cuh"
#include "base.h"
#include <cuda_runtime.h>
#include "missing.cuh"
__global__ void step() { helper(); base(); }
"#,
    )
    .unwrap();
    let mut graph = CodeGraph::init_sync(root.path()).unwrap();
    assert!(graph.index_all(Default::default()).success);
    let nodes = graph.get_nodes_in_file("main.cu");
    let step = nodes.iter().find(|n| n.name == "step").unwrap();
    for (file, name) in [("gpu/ops.cuh", "helper"), ("include/base.h", "base")] {
        let targets = graph.get_nodes_in_file(file);
        let target = targets.iter().find(|n| n.name == name).unwrap();
        assert!(
            graph
                .get_outgoing_edges(&step.id)
                .iter()
                .any(|e| e.kind == EdgeKind::Calls && e.target == target.id)
        );
        let imported = nodes
            .iter()
            .flat_map(|n| graph.get_outgoing_edges(&n.id))
            .any(|e| e.kind == EdgeKind::Imports && targets.iter().any(|n| n.id == e.target));
        assert!(imported, "missing include edge to {file}");
    }
    for missing in ["cuda_runtime.h", "missing.cuh"] {
        assert!(graph.get_nodes_in_file(missing).is_empty());
        let import = nodes.iter().find(|n| n.name == missing).unwrap();
        assert!(graph.get_outgoing_edges(&import.id).is_empty());
    }
    // Replacing a header must restore references from the unchanged CUDA caller.
    std::fs::write(
        root.path().join("gpu/ops.cuh"),
        "__device__ int helper() { return 123; }\n",
    )
    .unwrap();
    graph.sync(Default::default());
    let incremental = snapshot(root.path());
    assert!(graph.index_all(Default::default()).success);
    assert_eq!(incremental, snapshot(root.path()));
    for file in ["gpu/ops.cuh", "include/base.h", "main.cu"] {
        std::fs::remove_file(root.path().join(file)).unwrap();
        graph.sync(Default::default());
        assert!(graph.get_nodes_in_file(file).is_empty());
        let incremental = snapshot(root.path());
        let db = rusqlite::Connection::open(rustcodegraph::get_database_path(root.path())).unwrap();
        let dangling: i64 = db.query_row("SELECT count(*) FROM edges e LEFT JOIN nodes s ON s.id=e.source LEFT JOIN nodes t ON t.id=e.target WHERE s.id IS NULL OR t.id IS NULL", [], |r| r.get(0)).unwrap();
        assert_eq!(dangling, 0);
        drop(db);
        assert!(graph.index_all(Default::default()).success);
        assert_eq!(incremental, snapshot(root.path()));
    }
    graph.close();
}

#[test]
fn case_207_cuh_only_directory_is_discovered() {
    let root = Project::new();
    std::fs::create_dir(root.path().join("device")).unwrap();
    std::fs::write(
        root.path().join("device/ops.CUH"),
        "__device__ int helper() { return 1; }",
    )
    .unwrap();
    let dirs = rustcodegraph::resolution::import_resolver::load_cpp_include_dirs(root.path());
    assert!(dirs.iter().any(|dir| dir == "device"));
}
