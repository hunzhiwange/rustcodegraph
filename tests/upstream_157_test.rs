use rustcodegraph::CodeGraph;

fn edges(files: &[(&str, &str)]) -> Vec<(String, String, String, String)> {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "rcg-157-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&root).unwrap();
    for (path, source) in files {
        std::fs::create_dir_all(root.join(path).parent().unwrap()).unwrap();
        std::fs::write(root.join(path), source).unwrap();
    }
    let mut graph = CodeGraph::init_sync(&root).unwrap();
    let result = graph.index_all(Default::default());
    assert!(result.success, "{:?}", result.errors);
    graph.close();
    let db = rusqlite::Connection::open(root.join(".rustcodegraph/rustcodegraph.db")).unwrap();
    let result = db.prepare("SELECT s.name,e.kind,t.file_path,t.name FROM edges e JOIN nodes s ON s.id=e.source JOIN nodes t ON t.id=e.target WHERE e.kind IN ('calls','imports') ORDER BY 1,2,3,4").unwrap().query_map([], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).unwrap().map(Result::unwrap).collect();
    drop(db);
    std::fs::remove_dir_all(root).unwrap();
    result
}
// Static COBOL flow audit against upstream 15483713: dynamic CALL is deliberately skipped.
#[test]
fn case_157_static_perform_to_cross_program_call_chain() {
    let e = edges(&[
        (
            "main.cbl",
            "IDENTIFICATION DIVISION.\nPROGRAM-ID. MAIN.\nPROCEDURE DIVISION.\nSTART-PARA.\n    PERFORM DISPATCH THRU FINISH.\nDISPATCH.\n    CALL 'WORKER'.\nFINISH.\n    STOP RUN.\n",
        ),
        (
            "worker.cbl",
            "IDENTIFICATION DIVISION.\nPROGRAM-ID. WORKER.\nPROCEDURE DIVISION.\n    CALL 'LEAF'.\n",
        ),
        (
            "leaf.cbl",
            "IDENTIFICATION DIVISION.\nPROGRAM-ID. LEAF.\nPROCEDURE DIVISION.\n    STOP RUN.\n",
        ),
    ]);
    for (from, to) in [
        ("START-PARA", "DISPATCH"),
        ("START-PARA", "FINISH"),
        ("DISPATCH", "WORKER"),
        ("WORKER", "LEAF"),
    ] {
        assert!(
            e.iter().any(|x| x.0 == from && x.1 == "calls" && x.3 == to),
            "missing {from} -> {to}: {e:?}"
        );
    }
    assert_eq!(e.iter().filter(|x| x.1 == "calls").count(), 4, "{e:?}");
}

#[test]
fn case_157_dynamic_call_does_not_guess_program() {
    let e = edges(&[
        (
            "main.cbl",
            "IDENTIFICATION DIVISION.\nPROGRAM-ID. MAIN.\nDATA DIVISION.\nWORKING-STORAGE SECTION.\n01 WORKER PIC X(8).\nPROCEDURE DIVISION.\nSTART-PARA.\n    CALL WORKER.\n",
        ),
        (
            "worker.cbl",
            "IDENTIFICATION DIVISION.\nPROGRAM-ID. WORKER.\nPROCEDURE DIVISION.\n    STOP RUN.\n",
        ),
    ]);
    assert!(e.is_empty(), "{e:?}");
}

#[test]
fn case_157_ambiguous_program_breaks_chain_conservatively() {
    let worker =
        "IDENTIFICATION DIVISION.\nPROGRAM-ID. WORKER.\nPROCEDURE DIVISION.\n    STOP RUN.\n";
    let e = edges(&[
        (
            "main.cbl",
            "IDENTIFICATION DIVISION.\nPROGRAM-ID. MAIN.\nPROCEDURE DIVISION.\nSTART-PARA.\n    PERFORM DISPATCH.\nDISPATCH.\n    CALL 'WORKER'.\n",
        ),
        ("a.cbl", worker),
        ("b.cbl", worker),
    ]);
    assert!(
        e.iter().any(|x| x.0 == "START-PARA" && x.3 == "DISPATCH"),
        "{e:?}"
    );
    assert!(!e.iter().any(|x| x.3 == "WORKER"), "{e:?}");
}
