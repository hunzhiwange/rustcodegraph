//! Watch/sync memory-peak regression tests.
//!
//! Covers the incremental-sync prefilter: `changed_facade_files` must use the
//! persisted `size`/`modified_at` columns to skip reading files whose stat is
//! unchanged, only reading content for files whose stat differs. Correctness is
//! preserved: a file whose mtime changed but whose content is identical must NOT
//! be reported as modified.
//!
//! Also covers the watch-sync batching guards: a back-to-back-sync throttle
//! that keeps a minimum interval between heavy syncs, and the skipped-sync
//! state-machine path that keeps pending, stays active, and never permanently
//! degrades when a custom callback reports a recoverable skip.

mod watch_sync_memory {
    use std::collections::VecDeque;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
    use std::thread;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    use rusqlite::Connection;
    use rustcodegraph::sync::watcher::{
        FileWatcher, LockUnavailableError, SyncRunResult, WatchOptions as FileWatchOptions,
    };
    use rustcodegraph::{
        CodeGraph, IndexOptions, facade_file_content_reads, facade_synthesis_nodes_loaded,
        get_code_graph_dir,
    };

    static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

    struct TempProject {
        path: PathBuf,
    }

    impl TempProject {
        fn new(prefix: &str) -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time should be after Unix epoch")
                .as_nanos();
            let suffix = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("{prefix}-{}-{unique}", std::process::id()));
            let path = path.with_file_name(format!(
                "{}-{suffix}",
                path.file_name()
                    .and_then(|name| name.to_str())
                    .expect("temp directory name should be UTF-8")
            ));
            fs::create_dir_all(&path).unwrap_or_else(|err| {
                panic!("failed to create temp dir {}: {err}", path.display())
            });
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }

        fn src(&self) -> PathBuf {
            self.path.join("src")
        }

        fn write_src(&self, name: &str, content: &str) {
            fs::write(self.src().join(name), content)
                .unwrap_or_else(|err| panic!("failed to write src/{name}: {err}"));
        }
    }

    impl Drop for TempProject {
        fn drop(&mut self) {
            if self.path.exists() {
                let _ = fs::remove_dir_all(&self.path);
            }
        }
    }

    fn contains_path(paths: &[String], expected: &str) -> bool {
        paths.iter().any(|path| path == expected)
    }

    /// Build a project with N source files and index it.
    fn indexed_project(prefix: &str, n: usize) -> (TempProject, CodeGraph) {
        let project = TempProject::new(prefix);
        fs::create_dir_all(project.src()).expect("src directory should be created");
        for idx in 0..n {
            project.write_src(
                &format!("mod_{idx}.ts"),
                &format!("export function probe_{idx}() {{ return {idx}; }}"),
            );
        }
        let mut cg = CodeGraph::init_sync(project.path()).expect("CodeGraph should initialize");
        let _ = cg.index_all(IndexOptions::default());
        (project, cg)
    }

    #[test]
    fn changed_files_prefilter_reads_only_suspect_files() {
        let n = 24usize;
        let (project, mut cg) = indexed_project("codegraph-watch-prefilter", n);

        // (a) Only one file's content changes -> it is the sole `modified` entry,
        // and the prefilter should read content for only a handful of files
        // (the stat-changed one), not all N.
        project.write_src(
            "mod_3.ts",
            "export function probe_3_changed() { return 333; }",
        );

        let reads_before = facade_file_content_reads();
        let changes = cg.get_changed_files();
        let reads = facade_file_content_reads() - reads_before;

        assert!(
            reads < (n as u64),
            "prefilter should read far fewer than N={n} files, read {reads}"
        );
        assert_eq!(
            changes.added.len(),
            0,
            "no files added, got added={:?}",
            changes.added
        );
        assert_eq!(
            changes.removed.len(),
            0,
            "no files removed, got removed={:?}",
            changes.removed
        );
        assert_eq!(
            changes.modified,
            vec!["src/mod_3.ts".to_string()],
            "exactly the content-changed file should be modified, got {:?}",
            changes.modified
        );

        cg.destroy();
        drop(project);
    }

    #[test]
    fn changed_files_prefilter_ignores_mtime_only_touch() {
        let n = 24usize;
        let (project, mut cg) = indexed_project("codegraph-watch-mtime-touch", n);

        // (b) Rewrite identical content for one file, advancing its mtime past
        // the indexed value. The prefilter must read this stat-changed file, hash
        // it, and classify it as UNCHANGED (content identical) -> modified empty.
        let target = project.src().join("mod_7.ts");
        let original = fs::read_to_string(&target).expect("read original content");
        // Sleep briefly so the filesystem mtime clearly advances past indexed_at.
        std::thread::sleep(Duration::from_millis(1100));
        fs::write(&target, &original).expect("rewrite identical content");

        let reads_before = facade_file_content_reads();
        let changes = cg.get_changed_files();
        let reads = facade_file_content_reads() - reads_before;

        // Only the stat-changed file is read+hashed; it then proves identical.
        assert!(
            reads < (n as u64),
            "prefilter should read far fewer than N={n} files, read {reads}"
        );
        assert_eq!(
            changes.modified.len(),
            0,
            "an mtime-only touch with identical content must not be reported as modified, got {:?}",
            changes.modified
        );
        assert_eq!(
            changes.added.len(),
            0,
            "no files added, got {:?}",
            changes.added
        );
        assert_eq!(
            changes.removed.len(),
            0,
            "no files removed, got {:?}",
            changes.removed
        );
        assert!(!contains_path(&changes.modified, "src/mod_7.ts"));

        cg.destroy();
        drop(project);
    }

    /// (source, target, kind, provenance) triple, the edge identity we compare.
    type EdgeTriple = (String, String, String, Option<String>);

    fn read_all_edges(project_root: &Path) -> Vec<EdgeTriple> {
        let db_path = get_code_graph_dir(project_root).join("rustcodegraph.db");
        let conn = Connection::open(&db_path).expect("open facade db");
        let mut stmt = conn
            .prepare("SELECT source, target, kind, provenance FROM edges")
            .expect("prepare edge query");
        let mut edges = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            })
            .expect("query edges")
            .map(|row| row.expect("read edge row"))
            .collect::<Vec<_>>();
        edges.sort();
        edges
    }

    /// Build a fixture with N filler files plus a cross-file EventEmitter flow
    /// (`emitter.ts` emits an event handled by a function in `handler.ts`). The
    /// emitter→handler edge is synthesized by the dynamic-edge pass via the
    /// resolution context, so it exercises exactly the code under test.
    fn write_synthesis_fixture(project: &TempProject, n: usize) {
        for idx in 0..n {
            project.write_src(
                &format!("filler_{idx}.ts"),
                &format!("export function filler_{idx}() {{ return {idx}; }}"),
            );
        }
        project.write_src(
            "emitter.ts",
            "export class Emitter {\n  bus: any;\n  ping() {\n    this.bus.emit('ping');\n  }\n}\n",
        );
        project.write_src(
            "handler.ts",
            "export function onPing() { return 'pong'; }\nexport function register(bus: any) {\n  bus.on('ping', onPing);\n}\n",
        );
    }

    #[test]
    fn incremental_synthesis_matches_full_rebuild() {
        let n = 30usize;

        // Arm A: full index of the FINAL state.
        let full = TempProject::new("codegraph-synth-full");
        fs::create_dir_all(full.src()).expect("src dir");
        write_synthesis_fixture(&full, n);
        // emitter.ts in its final form (the body the incremental arm converges to).
        full.write_src(
            "emitter.ts",
            "export class Emitter {\n  bus: any;\n  ping() {\n    this.bus.emit('ping');\n  }\n  pong() {\n    this.bus.emit('ping');\n  }\n}\n",
        );
        let mut cg_full = CodeGraph::init_sync(full.path()).expect("init full");
        let _ = cg_full.index_all(IndexOptions::default());
        let full_edges = read_all_edges(full.path());
        cg_full.destroy();
        drop(cg_full);

        // Arm B: index the OLD state, then change emitter.ts to the final state
        // and run an incremental sync.
        let incr = TempProject::new("codegraph-synth-incr");
        fs::create_dir_all(incr.src()).expect("src dir");
        write_synthesis_fixture(&incr, n);
        let mut cg_incr = CodeGraph::init_sync(incr.path()).expect("init incr");
        let _ = cg_incr.index_all(IndexOptions::default());
        // Final state: add a second emitting method, identical to arm A's file.
        incr.write_src(
            "emitter.ts",
            "export class Emitter {\n  bus: any;\n  ping() {\n    this.bus.emit('ping');\n  }\n  pong() {\n    this.bus.emit('ping');\n  }\n}\n",
        );
        let _ = cg_incr.sync(IndexOptions::default());
        let incr_edges = read_all_edges(incr.path());
        cg_incr.destroy();
        drop(cg_incr);

        // Sanity: the fixture must actually produce a synthesized event-emitter
        // edge, otherwise the test proves nothing about the synthesis path.
        assert!(
            full_edges
                .iter()
                .any(|(_, _, _, prov)| prov.as_deref() == Some("heuristic")),
            "fixture should yield at least one synthesized (heuristic) edge; got {full_edges:?}"
        );

        assert_eq!(
            incr_edges, full_edges,
            "incremental sync edge set must equal a full rebuild of the same final state"
        );
    }

    #[test]
    fn incremental_synthesis_does_not_load_full_node_set() {
        let n = 40usize;
        let project = TempProject::new("codegraph-synth-noload");
        fs::create_dir_all(project.src()).expect("src dir");
        write_synthesis_fixture(&project, n);
        let mut cg = CodeGraph::init_sync(project.path()).expect("init");
        let _ = cg.index_all(IndexOptions::default());

        // Touch one file and run an incremental sync. The synthesis context on
        // the incremental path must NOT slurp the entire node set into memory.
        project.write_src(
            "filler_5.ts",
            "export function filler_5_changed() { return 555; }",
        );

        let loaded_before = facade_synthesis_nodes_loaded();
        let _ = cg.sync(IndexOptions::default());
        let loaded = facade_synthesis_nodes_loaded() - loaded_before;

        // A full in-memory context would load every node (well over n). The
        // DB-backed incremental context loads them on demand, not eagerly.
        assert_eq!(
            loaded, 0,
            "incremental synthesis must not eagerly load the full node set, eagerly loaded {loaded} nodes"
        );

        cg.destroy();
        drop(project);
    }

    // --- Task 3: throttle + recoverable skip handling -----------------------

    fn ok_changed(files_changed: usize) -> SyncRunResult {
        SyncRunResult {
            files_changed,
            duration_ms: 1,
            skipped: false,
        }
    }

    fn inert(debounce_ms: u64, min_sync_interval_ms: Option<u64>) -> FileWatchOptions {
        FileWatchOptions {
            debounce_ms: Some(debounce_ms),
            min_sync_interval_ms,
            inert_for_tests: true,
            ..FileWatchOptions::default()
        }
    }

    fn pump_until<F>(watcher: &mut FileWatcher, mut done: F, timeout_ms: u64)
    where
        F: FnMut(&mut FileWatcher) -> bool,
    {
        let start = Instant::now();
        loop {
            watcher.flush_due();
            if done(watcher) {
                return;
            }
            assert!(
                start.elapsed() <= Duration::from_millis(timeout_ms),
                "pump_until timed out after {timeout_ms}ms"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    /// Behavior 1 — back-to-back syncs are throttled to a minimum interval.
    ///
    /// With a short debounce but a longer min-sync interval, a fresh event that
    /// lands right after a sync completes must NOT immediately drive a second
    /// heavy sync: the second run waits out the interval, the pending file is not
    /// lost, and it does run once the interval elapses.
    #[test]
    fn watch_throttles_back_to_back_syncs() {
        let project = TempProject::new("codegraph-watch-throttle");
        fs::create_dir_all(project.src()).expect("src dir");

        let calls = Arc::new(AtomicUsize::new(0));
        let seen = Arc::clone(&calls);
        let debounce_ms = 30;
        let min_interval_ms = 400;
        let mut watcher = FileWatcher::new(
            project.path(),
            move || {
                seen.fetch_add(1, Ordering::SeqCst);
                Ok(ok_changed(1))
            },
            inert(debounce_ms, Some(min_interval_ms)),
        );
        watcher.start();
        watcher.wait_until_ready(1000).expect("watcher ready");

        // First event drives the first sync once the debounce elapses.
        watcher.ingest_event_for_tests("src/a.ts");
        pump_until(&mut watcher, |_| calls.load(Ordering::SeqCst) >= 1, 2000);
        assert_eq!(calls.load(Ordering::SeqCst), 1, "first sync should run");
        let after_first = Instant::now();

        // A new event right away must not back-to-back into a second heavy sync.
        watcher.ingest_event_for_tests("src/b.ts");
        // Within the min interval (minus slack), flushing must not run a 2nd sync.
        while after_first.elapsed() < Duration::from_millis(min_interval_ms - 120) {
            watcher.flush_due();
            assert_eq!(
                calls.load(Ordering::SeqCst),
                1,
                "second sync must be held back inside the min interval"
            );
            // Pending must survive the throttle so the edit is not dropped.
            assert!(
                watcher
                    .get_pending_files()
                    .iter()
                    .any(|p| p.path == "src/b.ts"),
                "throttled pending file must be retained"
            );
            thread::sleep(Duration::from_millis(15));
        }

        // Once the interval clears, the held sync runs (pending not starved).
        pump_until(&mut watcher, |_| calls.load(Ordering::SeqCst) >= 2, 2000);
        assert_eq!(
            calls.load(Ordering::SeqCst),
            2,
            "held-back sync must run once the min interval elapses"
        );
        println!(
            "back-to-back throttle: sync_calls={}, pending_files={}",
            calls.load(Ordering::SeqCst),
            watcher.get_pending_files().len()
        );

        watcher.stop();
        drop(project);
    }

    #[test]
    fn watch_respects_one_minute_min_interval_without_thirty_second_cap() {
        let project = TempProject::new("codegraph-watch-minute-throttle");
        fs::create_dir_all(project.src()).expect("src dir");

        let calls = Arc::new(AtomicUsize::new(0));
        let seen = Arc::clone(&calls);
        let mut watcher = FileWatcher::new(
            project.path(),
            move || {
                seen.fetch_add(1, Ordering::SeqCst);
                Ok(ok_changed(1))
            },
            inert(20, Some(60_000)),
        );
        watcher.start();
        watcher.wait_until_ready(1000).expect("watcher ready");

        watcher.ingest_event_for_tests("src/a.ts");
        watcher.flush_now();
        assert_eq!(calls.load(Ordering::SeqCst), 1, "first sync should run");

        watcher.ingest_event_for_tests("src/b.ts");
        thread::sleep(Duration::from_millis(30));
        watcher.flush_due();

        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "second sync must be held inside the one-minute min interval"
        );
        assert!(
            watcher
                .get_pending_files()
                .iter()
                .any(|p| p.path == "src/b.ts"),
            "throttled pending file must be retained"
        );
        assert!(
            watcher.__scheduled_delay_ms_for_tests().unwrap_or_default() > 59_000,
            "one-minute min interval should not be truncated to a thirty-second retry"
        );
        println!(
            "one-minute min interval: sync_calls={}, pending_files={}, scheduled_delay_ms={:?}",
            calls.load(Ordering::SeqCst),
            watcher.get_pending_files().len(),
            watcher.__scheduled_delay_ms_for_tests()
        );

        watcher.stop();
        drop(project);
    }

    #[test]
    fn lock_retry_backoff_is_not_blocked_by_min_sync_interval() {
        let project = TempProject::new("codegraph-watch-lock-retry");
        fs::create_dir_all(project.src()).expect("src dir");

        let calls = Arc::new(AtomicUsize::new(0));
        let seen = Arc::clone(&calls);
        let mut outcomes = VecDeque::from([
            Ok(ok_changed(1)),
            Err(LockUnavailableError::default().into()),
            Ok(ok_changed(1)),
        ]);
        let mut watcher = FileWatcher::new(
            project.path(),
            move || {
                seen.fetch_add(1, Ordering::SeqCst);
                outcomes.pop_front().unwrap_or_else(|| Ok(ok_changed(1)))
            },
            inert(25, Some(800)),
        );
        watcher.start();
        watcher.wait_until_ready(1000).expect("watcher ready");

        watcher.ingest_event_for_tests("src/a.ts");
        watcher.flush_now();
        assert_eq!(calls.load(Ordering::SeqCst), 1, "first sync should run");

        watcher.ingest_event_for_tests("src/b.ts");
        watcher.flush_now();
        assert_eq!(
            calls.load(Ordering::SeqCst),
            2,
            "lock contention attempt should run immediately via flush_now"
        );
        assert!(
            watcher
                .get_pending_files()
                .iter()
                .any(|p| p.path == "src/b.ts"),
            "lock contention must retain pending for retry"
        );

        pump_until(&mut watcher, |_| calls.load(Ordering::SeqCst) >= 3, 600);
        assert_eq!(
            calls.load(Ordering::SeqCst),
            3,
            "lock retry backoff should not wait for the automatic min interval"
        );
        println!(
            "lock retry backoff: sync_calls={}, pending_files={}",
            calls.load(Ordering::SeqCst),
            watcher.get_pending_files().len()
        );

        watcher.stop();
        drop(project);
    }

    #[test]
    fn watch_callback_skip_uses_slow_retry_backoff() {
        let project = TempProject::new("codegraph-watch-callback-skip-backoff");
        fs::create_dir_all(project.src()).expect("src dir");

        let calls = Arc::new(AtomicUsize::new(0));
        let skips = Arc::new(AtomicUsize::new(0));
        let seen_calls = Arc::clone(&calls);
        let seen_skips = Arc::clone(&skips);
        let debounce_ms = 20;
        let retry_ms = 250;
        let mut outcomes = VecDeque::from([true, true, false]);
        let mut watcher = FileWatcher::new(
            project.path(),
            move || {
                seen_calls.fetch_add(1, Ordering::SeqCst);
                let skipped = outcomes.pop_front().unwrap_or(false);
                if skipped {
                    seen_skips.fetch_add(1, Ordering::SeqCst);
                    Ok(SyncRunResult {
                        files_changed: 0,
                        duration_ms: 0,
                        skipped: true,
                    })
                } else {
                    Ok(ok_changed(1))
                }
            },
            FileWatchOptions {
                debounce_ms: Some(debounce_ms),
                min_sync_interval_ms: Some(retry_ms),
                inert_for_tests: true,
                ..FileWatchOptions::default()
            },
        );
        watcher.start();
        watcher.wait_until_ready(1000).expect("watcher ready");

        watcher.ingest_event_for_tests("src/deferred.ts");
        watcher.flush_now();

        assert_eq!(calls.load(Ordering::SeqCst), 1, "first sync should run");
        assert_eq!(skips.load(Ordering::SeqCst), 1, "first sync should skip");
        assert!(
            watcher.is_active(),
            "callback skip must keep watcher active"
        );
        assert!(
            watcher
                .get_pending_files()
                .iter()
                .any(|p| p.path == "src/deferred.ts"),
            "a skipped sync must retain pending"
        );
        assert!(
            watcher.__scheduled_delay_ms_for_tests().unwrap_or_default() >= retry_ms,
            "callback skip retry must use the slow retry window, not the short debounce"
        );

        thread::sleep(Duration::from_millis(debounce_ms + 40));
        watcher.flush_due();
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "skip retry must not run again after only the debounce window"
        );

        pump_until(&mut watcher, |_| calls.load(Ordering::SeqCst) >= 2, 1000);
        assert_eq!(
            skips.load(Ordering::SeqCst),
            2,
            "second scheduled attempt should also observe the simulated callback skip"
        );
        assert!(
            watcher.__scheduled_delay_ms_for_tests().unwrap_or_default() >= retry_ms,
            "consecutive callback skips must keep using the slow retry window"
        );

        pump_until(&mut watcher, |_| calls.load(Ordering::SeqCst) >= 3, 1000);
        assert_eq!(calls.load(Ordering::SeqCst), 3, "recovered sync should run");
        let pending_after_recovery = watcher.get_pending_files().len();
        assert_eq!(
            pending_after_recovery, 0,
            "successful sync after recovery must consume pending files"
        );

        watcher.ingest_event_for_tests("src/recovered.ts");
        assert_eq!(
            watcher.__scheduled_delay_ms_for_tests(),
            Some(debounce_ms),
            "after a successful sync, fresh events should use normal debounce again"
        );
        println!(
            "callback skip retry backoff: sync_calls={}, skip_calls={}, pending_after_recovery={}, fresh_delay_ms={:?}",
            calls.load(Ordering::SeqCst),
            skips.load(Ordering::SeqCst),
            pending_after_recovery,
            watcher.__scheduled_delay_ms_for_tests()
        );

        watcher.stop();
        drop(project);
    }

    /// Behavior 2 (watcher state) — a recoverable skip keeps the watcher healthy.
    ///
    /// When the sync callback reports `skipped`, the watcher
    /// must keep the pending file, stay active, and must NOT permanently degrade
    /// — a skip is recoverable, not a death.
    #[test]
    fn watch_skip_keeps_pending_and_stays_active() {
        let project = TempProject::new("codegraph-watch-skip-state");
        fs::create_dir_all(project.src()).expect("src dir");

        let calls = Arc::new(AtomicUsize::new(0));
        let seen = Arc::clone(&calls);
        let mut watcher = FileWatcher::new(
            project.path(),
            move || {
                seen.fetch_add(1, Ordering::SeqCst);
                // Simulate a recoverable skip returned by a custom watch callback.
                Ok(SyncRunResult {
                    files_changed: 0,
                    duration_ms: 0,
                    skipped: true,
                })
            },
            inert(30, Some(30)),
        );
        watcher.start();
        watcher.wait_until_ready(1000).expect("watcher ready");

        watcher.ingest_event_for_tests("src/c.ts");
        pump_until(&mut watcher, |_| calls.load(Ordering::SeqCst) >= 1, 2000);

        assert!(
            calls.load(Ordering::SeqCst) >= 1,
            "sync_fn should be invoked"
        );
        assert!(
            watcher.is_active(),
            "callback skip must keep watcher active"
        );
        assert!(
            !watcher.is_degraded(),
            "a recoverable callback skip must not permanently degrade the watcher"
        );
        assert!(
            watcher
                .get_pending_files()
                .iter()
                .any(|p| p.path == "src/c.ts"),
            "a skipped sync must retain pending so a later sync still picks it up"
        );

        watcher.stop();
        drop(project);
    }

    #[test]
    fn sync_does_not_skip_when_pending_has_no_real_changes() {
        let (_project, mut cg) = indexed_project("codegraph-watch-sync-noop", 8);

        let result = cg.sync(IndexOptions::default());

        assert!(
            !result.memory_skipped,
            "a no-op sync must clear watcher pending instead of reporting skipped"
        );
        assert_eq!(
            result.files_added + result.files_modified + result.files_removed,
            0
        );

        cg.destroy();
    }

    #[test]
    fn sync_consumes_small_changes_without_deferred_retry() {
        let (project, mut cg) = indexed_project("codegraph-watch-sync-small-change", 8);

        project.write_src(
            "mod_4.ts",
            "export function probe_4_changed() { return 444; }",
        );

        let result = cg.sync(IndexOptions::default());

        assert!(
            !result.memory_skipped,
            "a small edit should sync immediately so watch does not starve forever"
        );
        assert_eq!(result.files_modified, 1);
        assert!(
            result.nodes_updated > 0,
            "small edit should run the incremental indexing path"
        );
        assert!(
            cg.get_changed_files().modified.is_empty(),
            "the small edit should be consumed by the sync"
        );

        cg.destroy();
        drop(project);
    }
}
