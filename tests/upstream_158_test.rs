//! Incremental COBOL audit against upstream 15483713; real parser and SQLite.
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
            .map(Result::unwrap)
            .collect::<Vec<_>>()
    };
    // Stable semantic keys exclude generated ids and indexing timestamps.
    (
        read(
            "SELECT json_array(file_path,kind,name,qualified_name,start_line,end_line) FROM nodes ORDER BY 1",
        ),
        read(
            "SELECT json_array(s.file_path,s.kind,s.qualified_name,s.start_line,e.kind,t.file_path,t.kind,t.qualified_name,t.start_line,e.provenance) FROM edges e JOIN nodes s ON s.id=e.source JOIN nodes t ON t.id=e.target ORDER BY 1",
        ),
    )
}
fn program(name: &str) -> String {
    format!(
        "IDENTIFICATION DIVISION.\nPROGRAM-ID. {name}.\nPROCEDURE DIVISION.\nFINISH.\n    STOP RUN.\n"
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
            "rcg158-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("main.cbl"), "IDENTIFICATION DIVISION.\nPROGRAM-ID. MAIN.\nDATA DIVISION.\nWORKING-STORAGE SECTION.\nCOPY 'common'.\nPROCEDURE DIVISION.\nSTART-PARA.\n    CALL 'WORKER'.\n    PERFORM FINISH.\nFINISH.\n    STOP RUN.\n").unwrap();
        std::fs::write(root.join("worker.cbl"), program("WORKER")).unwrap();
        std::fs::write(
            root.join("common.cpy"),
            "       BOOK-PARA.\n           EXIT.\n",
        )
        .unwrap();
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
#[test]
fn case_158_changed_declarations_restore_unchanged_callers() {
    let mut f = Fixture::new();
    std::fs::write(
        f.root.join("worker.cbl"),
        program("WORKER").replace("FINISH", "NEW-FINISH"),
    )
    .unwrap();
    f.check();
    std::fs::write(f.root.join("main.cbl"), program("RENAMED")).unwrap();
    f.check();
    assert!(!snapshot(&f.root).0.iter().any(|x| x.contains("START-PARA")));
}
#[test]
fn case_158_deleted_targets_and_copybooks_can_return() {
    let mut f = Fixture::new();
    for path in ["worker.cbl", "common.cpy"] {
        std::fs::remove_file(f.root.join(path)).unwrap();
    }
    f.check();
    assert!(
        !snapshot(&f.root)
            .0
            .iter()
            .any(|x| x.contains("worker.cbl") || x.contains("common.cpy"))
    );
    std::fs::write(f.root.join("worker.cbl"), program("WORKER")).unwrap();
    std::fs::write(
        f.root.join("common.cpy"),
        "       NEW-BOOK.\n           EXIT.\n",
    )
    .unwrap();
    f.check();
    assert!(
        snapshot(&f.root)
            .1
            .iter()
            .any(|x| x.contains("worker.cbl") && x.contains("calls"))
    );
}
#[test]
fn case_158_added_ambiguous_program_invalidates_old_match() {
    let mut f = Fixture::new();
    std::fs::write(f.root.join("duplicate.cbl"), program("WORKER")).unwrap();
    f.check();
    assert!(
        !snapshot(&f.root)
            .1
            .iter()
            .any(|x| x.contains("calls") && x.contains("WORKER"))
    );
    std::fs::remove_file(f.root.join("duplicate.cbl")).unwrap();
    f.check();
}
