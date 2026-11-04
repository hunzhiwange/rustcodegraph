//! Incremental ArkTS audit against upstream 15483713; real parser and SQLite.
use rustcodegraph::CodeGraph;
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
                if let Some(items) = value.as_array_mut() {
                    if items.len() == 11 {
                        if let Some(meta) = items[10].as_str() {
                            items[10] = serde_json::from_str(meta).unwrap();
                        }
                    }
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
struct Fixture {
    root: std::path::PathBuf,
    graph: CodeGraph,
}
impl Fixture {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "rcg194-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("Child.ets"), CHILD).unwrap();
        std::fs::write(root.join("Home.ets"), HOME).unwrap();
        std::fs::write(
            root.join("control.ts"),
            "export function keep() { return 42; }\n",
        )
        .unwrap();
        let mut graph = CodeGraph::init_sync(&root).unwrap();
        let result = graph.index_all(Default::default());
        assert!(result.success, "{:?}", result.errors);
        Self { root, graph }
    }
    fn check(&mut self) {
        let control = |state: Snapshot| -> Snapshot {
            (
                state
                    .0
                    .into_iter()
                    .filter(|x| x.contains("control.ts"))
                    .collect(),
                state
                    .1
                    .into_iter()
                    .filter(|x| x.contains("control.ts"))
                    .collect(),
            )
        };
        let before = control(snapshot(&self.root));
        self.graph.sync(Default::default());
        let db = rusqlite::Connection::open(rustcodegraph::get_database_path(&self.root)).unwrap();
        let dangling: i64 = db.query_row("SELECT count(*) FROM edges e LEFT JOIN nodes s ON s.id=e.source LEFT JOIN nodes t ON t.id=e.target WHERE s.id IS NULL OR t.id IS NULL", [], |r| r.get(0)).unwrap();
        assert_eq!(dangling, 0);
        drop(db);
        let incremental = snapshot(&self.root);
        assert!(incremental.0.iter().any(|x| x.contains("keep")));
        assert_eq!(before, control(incremental.clone()));
        let result = self.graph.index_all(Default::default());
        assert!(result.success, "{:?}", result.errors);
        assert_eq!(incremental, snapshot(&self.root));
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.graph.close();
        std::fs::remove_dir_all(&self.root).unwrap();
    }
}

const CHILD: &str = "@Component\nexport struct Child { build() { Text('child') } }";
const HOME: &str = "import { Child } from './Child';\n@Component\nstruct Home {\n @State count: number = 0;\n increment() { this.count++; }\n build() { Child() }\n}\n";

#[test]
fn case_194_changed_declarations_and_state_writes() {
    let mut f = Fixture::new();
    assert!(snapshot(&f.root).1.iter().any(|e| e.contains("heuristic")));
    std::fs::write(
        f.root.join("Home.ets"),
        HOME.replace(
            "increment() { this.count++; }",
            "read() { return this.count; }",
        ),
    )
    .unwrap();
    f.check();
    assert!(!snapshot(&f.root).0.iter().any(|n| n.contains("increment")));
    assert!(!snapshot(&f.root).1.iter().any(|e| e.contains("heuristic")));
    std::fs::write(f.root.join("Home.ets"), HOME).unwrap();
    f.check();
    assert!(snapshot(&f.root).1.iter().any(|e| e.contains("heuristic")));
}

#[test]
fn case_194_deleted_import_target_and_restoration() {
    let mut f = Fixture::new();
    let linked = |s: Snapshot| {
        s.1.iter().any(|e| {
            e.contains("instantiates") && e.contains("Child.ets") && e.contains("Home.ets")
        })
    };
    assert!(linked(snapshot(&f.root)), "{:?}", snapshot(&f.root));
    std::fs::write(
        f.root.join("Child.ets"),
        CHILD.replace("Text('child')", "Text('updated')"),
    )
    .unwrap();
    f.check();
    assert!(linked(snapshot(&f.root)));
    std::fs::remove_file(f.root.join("Child.ets")).unwrap();
    f.check();
    assert!(!snapshot(&f.root).0.iter().any(|n| n.contains("Child.ets")));
    std::fs::write(f.root.join("Child.ets"), CHILD).unwrap();
    f.check();
    assert!(linked(snapshot(&f.root)), "{:?}", snapshot(&f.root));
}

#[test]
fn case_194_deleted_caller_and_empty_project_language() {
    let mut f = Fixture::new();
    std::fs::remove_file(f.root.join("Home.ets")).unwrap();
    f.check();
    assert!(!snapshot(&f.root).1.iter().any(|e| e.contains("Home.ets")));
    std::fs::remove_file(f.root.join("Child.ets")).unwrap();
    f.check();
    assert!(!snapshot(&f.root).0.iter().any(|n| n.contains(".ets")));
}
