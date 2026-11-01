use rustcodegraph::DatabaseConnection;
use rustcodegraph::db::sqlite_adapter::{SqliteError, SqliteParams};

fn has(db: &mut DatabaseConnection, name: &str) -> bool {
    db.get_db()
        .prepare("SELECT name FROM sqlite_master WHERE type='index' AND name=?")
        .unwrap()
        .get(SqliteParams::positional(vec![name.into()]))
        .unwrap()
        .is_some()
}

#[test]
fn case_014_window_and_recovery() {
    let dir = std::env::temp_dir().join(format!("rcg014-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("graph.db");
    let mut db = DatabaseConnection::initialize(&path).unwrap();
    db.get_db().exec("INSERT INTO nodes(id,kind,name,qualified_name,file_path,language,start_line,end_line,start_column,end_column,updated_at) VALUES ('a','function','a','a','a.ts','typescript',1,1,0,0,0);").unwrap();
    db.with_bulk_edge_load(true, |db| {
        db.get_db()
            .exec("INSERT INTO edges(source,target,kind) VALUES ('a','a','calls');")
            .unwrap();
        assert!(!has(db, "idx_edges_provenance"));
        assert!(!has(db, "idx_edges_kind"));
        assert!(has(db, "idx_edges_source_kind"));
        assert!(has(db, "idx_edges_target_kind"));
        Ok(())
    })
    .unwrap();
    assert!(
        db.get_db()
            .prepare("SELECT * FROM edges WHERE source='a' AND target='a' AND kind='calls'")
            .unwrap()
            .get(SqliteParams::None)
            .unwrap()
            .is_some()
    );
    assert!(has(&mut db, "idx_edges_kind"));
    assert!(
        db.with_bulk_edge_load(true, |_| Err::<(), _>(SqliteError::new("injected")))
            .is_err()
    );
    assert!(has(&mut db, "idx_edges_provenance"));
    db.with_bulk_edge_load(false, |db| {
        assert!(has(db, "idx_edges_kind"));
        Ok(())
    })
    .unwrap();
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = db.with_bulk_edge_load(true, |_| -> Result<(), SqliteError> { panic!("injected") });
    }));
    assert!(panic.is_err());
    assert!(has(&mut db, "idx_edges_kind"));
    db.get_db()
        .exec("DROP INDEX idx_edges_kind; DROP INDEX idx_edges_provenance;")
        .unwrap();
    drop(db);
    let mut db = DatabaseConnection::open(&path).unwrap();
    assert!(has(&mut db, "idx_edges_kind"));
    assert!(has(&mut db, "idx_edges_provenance"));
    drop(db);
    std::fs::remove_dir_all(dir).unwrap();
}
