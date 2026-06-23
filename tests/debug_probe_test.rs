use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use rustcodegraph::{CodeGraph, IndexOptions};

static COUNTER: AtomicU64 = AtomicU64::new(0);

#[test]
fn debug_java_value_ref_shadow() {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::current_dir()
        .unwrap()
        .join("target/tmp")
        .join(format!(
            "debug-java-value-ref-{}-{nanos}",
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("Shadow.java"),
        [
            "class Shadow {",
            "  static final int TIMEOUT = 30;",
            "  int usesConst() { return TIMEOUT; }",
            "  int shadows() { int TIMEOUT = 5; return TIMEOUT; }",
            "}",
        ]
        .join("\n"),
    )
    .unwrap();

    let mut cg = CodeGraph::init_sync(&root).unwrap();
    let result = cg.index_all(IndexOptions::default());
    assert!(result.success, "{:?}", result.errors);

    for result in cg.search_nodes("TIMEOUT", None) {
        let node = result.node;
        eprintln!(
            "target {} {:?} {}:{}",
            node.id, node.kind, node.file_path, node.start_line
        );
        for edge in cg.get_incoming_edges(&node.id) {
            eprintln!("incoming {:?}", edge);
            if let Some(source) = cg.get_node(&edge.source) {
                eprintln!("source {} {:?} {}", source.name, source.kind, source.id);
            }
        }
    }

    let _ = fs::remove_dir_all(&root);
}
