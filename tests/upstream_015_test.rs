use rustcodegraph::DatabaseConnection;
use rustcodegraph::db::sqlite_adapter::{PragmaOptions, SqliteError};
fn interval(db: &mut DatabaseConnection) -> String {
    db.get_db()
        .pragma("wal_autocheckpoint", Some(PragmaOptions { simple: true }))
        .unwrap()
        .unwrap()
        .into_string_lossy()
        .unwrap()
}
#[test]
fn case_015_success_error_panic_and_reader() {
    let dir = std::env::temp_dir().join(format!("rcg015-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("graph.db");
    let mut db = DatabaseConnection::initialize(&path).unwrap();
    let reader = rusqlite::Connection::open(&path).unwrap();
    reader
        .execute_batch("BEGIN; SELECT count(*) FROM nodes;")
        .unwrap();
    db.get_db().exec("PRAGMA wal_autocheckpoint=17").unwrap();
    db.with_deferred_wal(true, |db| {
        assert_eq!(interval(db), "0");
        db.get_db()
            .exec("CREATE TABLE window_test(value); INSERT INTO window_test VALUES(42)")?;
        Ok(())
    })
    .unwrap();
    assert_eq!(interval(&mut db), "17");
    reader.execute_batch("COMMIT").unwrap();
    assert_eq!(
        reader
            .query_row("SELECT value FROM window_test", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        42
    );
    let err = db.with_deferred_wal(true, |db| {
        assert_eq!(interval(db), "0");
        Err::<(), _>(SqliteError::new("expected"))
    });
    assert!(err.is_err());
    assert_eq!(interval(&mut db), "17");
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = db.with_deferred_wal(true, |_| -> Result<(), SqliteError> {
            panic!("expected unwind")
        });
    }));
    assert!(panic.is_err());
    assert_eq!(interval(&mut db), "17");
    db.with_deferred_wal(false, |db| {
        assert_eq!(interval(db), "17");
        Ok(())
    })
    .unwrap();
    drop(db);
    let reopened = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(
        reopened
            .query_row("SELECT value FROM window_test", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        42
    );
    drop(reopened);
    // Windows cannot remove the database files while the reader is still open.
    reader.close().unwrap();
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn case_015_native_window_and_non_wal() {
    use rustcodegraph::db::index::WalWriteWindow;
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.pragma_update(None, "wal_autocheckpoint", 23).unwrap();
    let window = WalWriteWindow::new(conn, true).unwrap();
    assert_eq!(
        window
            .query_row("PRAGMA wal_autocheckpoint", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        23
    );
    let dir = std::env::temp_dir().join(format!("rcg015-native-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("db");
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE IF NOT EXISTS t(v)")
        .unwrap();
    let window = WalWriteWindow::new(conn, true).unwrap();
    assert_eq!(
        window
            .query_row("PRAGMA wal_autocheckpoint", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    window.execute("INSERT INTO t VALUES(1)", []).unwrap();
    drop(window);
    drop(rusqlite::Connection::open(&path).unwrap());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn case_015_crash_recovery() {
    if let Ok(path) = std::env::var("RCG015_CRASH_DB") {
        let conn = rusqlite::Connection::open(path).unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0; CREATE TABLE recovery(v); INSERT INTO recovery VALUES(7)").unwrap();
        std::process::exit(19); // bypass Drop/SQLite close, leaving committed WAL
    }
    let dir = std::env::temp_dir().join(format!("rcg015-crash-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("db");
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "case_015_crash_recovery"])
        .env("RCG015_CRASH_DB", &path)
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(19));
    let conn = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(
        conn.query_row("SELECT v FROM recovery", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        7
    );
    assert_eq!(
        conn.query_row("PRAGMA wal_autocheckpoint", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1000
    );
    drop(conn);
    std::fs::remove_dir_all(dir).unwrap();
}
