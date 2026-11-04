use rustcodegraph::CodeGraph;

fn edges(files: &[(&str, &str)]) -> Vec<(String, String, String, String)> {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "rcg-156-{}-{}",
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
const MAIN: &str = "IDENTIFICATION DIVISION.\nPROGRAM-ID. MAIN.\nDATA DIVISION.\nWORKING-STORAGE SECTION.\nCOPY 'common'.\nPROCEDURE DIVISION.\nSTART-PARA.\n    CALL 'worker'.\n    PERFORM finish.\nFINISH.\n    STOP RUN.\n";
#[test]
fn case_156_cross_program_copy_and_local_perform() {
    let e = edges(&[
        ("app/main.cbl", MAIN),
        ("app/common.cpy", "       BOOK-PARA.\n           EXIT.\n"),
        ("other/common.cpy", "       OTHER-PARA.\n           EXIT.\n"),
        (
            "worker.cbl",
            "IDENTIFICATION DIVISION.\nPROGRAM-ID. WORKER.\nPROCEDURE DIVISION.\nFINISH.\n    STOP RUN.\n",
        ),
    ]);
    assert!(
        e.iter()
            .any(|x| x.0 == "MAIN" && x.1 == "imports" && x.2 == "app/common.cpy"),
        "{e:?}"
    );
    assert!(
        e.iter().any(|x| x.0 == "START-PARA" && x.3 == "WORKER"),
        "{e:?}"
    );
    assert!(
        e.iter()
            .any(|x| x.0 == "START-PARA" && x.2 == "app/main.cbl" && x.3 == "FINISH"),
        "{e:?}"
    );
}
#[test]
fn case_156_external_and_unimported_paragraph_stay_unresolved() {
    let e = edges(&[
        (
            "main.cbl",
            "IDENTIFICATION DIVISION.\nPROGRAM-ID. MAIN.\nDATA DIVISION.\nWORKING-STORAGE SECTION.\nCOPY MISSING.\nPROCEDURE DIVISION.\nSTART-PARA.\n    CALL 'ABSENT'.\n    PERFORM FOREIGN.\n",
        ),
        (
            "other.cbl",
            "IDENTIFICATION DIVISION.\nPROGRAM-ID. OTHER.\nPROCEDURE DIVISION.\nFOREIGN.\n    STOP RUN.\n",
        ),
        (
            "missing.ts",
            "export function MISSING() {}\nexport function ABSENT() {}",
        ),
    ]);
    assert!(e.is_empty(), "{e:?}");
}
#[test]
fn case_156_ambiguous_copybooks_and_programs_stay_unresolved() {
    let e = edges(&[
        ("main.cbl", MAIN),
        ("a/common.cpy", "       A-PARA.\n           EXIT.\n"),
        ("b/common.cpy", "       B-PARA.\n           EXIT.\n"),
        (
            "a/worker.cbl",
            "IDENTIFICATION DIVISION.\nPROGRAM-ID. WORKER.\nPROCEDURE DIVISION.\n    STOP RUN.\n",
        ),
        (
            "b/worker.cbl",
            "IDENTIFICATION DIVISION.\nPROGRAM-ID. WORKER.\nPROCEDURE DIVISION.\n    STOP RUN.\n",
        ),
    ]);
    assert!(
        e.iter().all(|x| x.1 != "imports" && x.3 != "WORKER"),
        "{e:?}"
    );
}

#[test]
fn case_156_explicit_relative_copy_and_call_namespace() {
    let e = edges(&[
        (
            "app/main.cbl",
            "IDENTIFICATION DIVISION.\nPROGRAM-ID. MAIN.\nDATA DIVISION.\nWORKING-STORAGE SECTION.\nCOPY '../books/common.cpy'.\nPROCEDURE DIVISION.\nSTART-PARA.\n    CALL 'WORKER'.\n    PERFORM WORKER.\nWORKER.\n    STOP RUN.\n",
        ),
        ("books/common.cpy", "       BOOK-PARA.\n           EXIT.\n"),
        ("else/common.cpy", "       ELSE-PARA.\n           EXIT.\n"),
        (
            "worker.cbl",
            "IDENTIFICATION DIVISION.\nPROGRAM-ID. WORKER.\nPROCEDURE DIVISION.\n    STOP RUN.\n",
        ),
    ]);
    assert!(
        e.iter()
            .any(|x| x.1 == "imports" && x.2 == "books/common.cpy"),
        "{e:?}"
    );
    assert_eq!(
        e.iter()
            .filter(|x| x.0 == "START-PARA" && x.1 == "calls" && x.3 == "WORKER")
            .count(),
        2,
        "{e:?}"
    );
    assert!(!e.iter().any(|x| x.2 == "else/common.cpy"), "{e:?}");
}

#[test]
fn case_156_paragraphs_are_scoped_to_the_owning_program() {
    let e = edges(&[(
        "multi.cbl",
        "IDENTIFICATION DIVISION.\nPROGRAM-ID. FIRST-PROG.\nPROCEDURE DIVISION.\nFIRST-START.\n    PERFORM finish.\nFINISH.\n    STOP RUN.\nEND PROGRAM FIRST-PROG.\nIDENTIFICATION DIVISION.\nPROGRAM-ID. SECOND-PROG.\nPROCEDURE DIVISION.\nSECOND-START.\n    PERFORM finish.\nFINISH.\n    STOP RUN.\nEND PROGRAM SECOND-PROG.\n",
    )]);
    assert_eq!(e.iter().filter(|x| x.1 == "calls").count(), 2, "{e:?}");
}
