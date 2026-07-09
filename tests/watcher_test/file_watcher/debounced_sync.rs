use crate::common::*;

#[test]
fn should_trigger_sync_after_file_change() {
    let _guard = TestGuard::new();
    let project = TempProject::new("codegraph-watcher");
    let (calls, sync_fn) = sync_mock(Vec::new(), Ok(ok(1, 10)));
    let mut watcher = FileWatcher::new(project.path(), sync_fn, inert_options(200));

    watcher.start();
    watcher
        .wait_until_ready(1000)
        .expect("watcher should be ready");
    assert!(__emit_watch_event_for_tests(project.path(), "src/new.ts"));

    wait_for(
        || {
            watcher.flush_due();
            calls.load(Ordering::SeqCst) > 0
        },
        2000,
        25,
    );
    assert!(calls.load(Ordering::SeqCst) > 0);

    watcher.stop();
}

#[test]
fn should_flush_during_a_sustained_event_stream_before_it_stops() {
    // 复制一个大文件夹会产生持续不断的文件事件流：只要新事件不停地在 debounce
    // 窗口内到来，纯 debounce 会被无限重置，sync 永远不触发——用户看到“文件变了”
    // 却没有自动更新。max-wait 上限保证即便事件流还在持续，到期也会先 flush 一次。
    let _guard = TestGuard::new();
    let project = TempProject::new("codegraph-watcher");
    let (calls, sync_fn) = sync_mock(Vec::new(), Ok(ok(1, 10)));
    // debounce 100ms，max-wait 上限设 300ms：事件每 40ms 来一次（< debounce，会持续
    // 重置 debounce），但持续 2s 远超 max-wait，期间必须至少 flush 一次。
    let mut options = inert_options(100);
    options.max_debounce_ms = Some(300);
    let mut watcher = FileWatcher::new(project.path(), sync_fn, options);

    watcher.start();
    watcher
        .wait_until_ready(1000)
        .expect("watcher should be ready");

    // 在“事件流仍在持续”的窗口内（每 40ms 注入一个新事件、共 ~2s），断言 sync 至少
    // 触发过一次。若 sync 只在事件停止后才可能触发，这个断言会失败（复现 bug）。
    let stream_start = Instant::now();
    let mut fired_during_stream = false;
    let mut i = 0;
    while stream_start.elapsed() < Duration::from_millis(2000) {
        assert!(__emit_watch_event_for_tests(
            project.path(),
            format!("src/copied{i}.ts")
        ));
        i += 1;
        thread::sleep(Duration::from_millis(40));
        watcher.flush_due();
        if calls.load(Ordering::SeqCst) > 0 {
            fired_during_stream = true;
            break;
        }
    }

    assert!(
        fired_during_stream,
        "sync should fire under a sustained event stream (large-folder copy), not wait for it to stop"
    );

    watcher.stop();
}

#[test]
fn should_hold_sustained_stream_until_configured_batch_window() {
    let _guard = TestGuard::new();
    let project = TempProject::new("codegraph-watcher");
    let (calls, sync_fn) = sync_mock(Vec::new(), Ok(ok(1, 10)));
    let mut options = inert_options(60_000);
    options.max_debounce_ms = Some(60_000);
    let mut watcher = FileWatcher::new(project.path(), sync_fn, options);

    watcher.start();
    watcher
        .wait_until_ready(1000)
        .expect("watcher should be ready");

    assert!(__emit_watch_event_for_tests(
        project.path(),
        "src/batch-window.ts"
    ));

    assert_eq!(
        watcher.__scheduled_delay_ms_for_tests(),
        Some(60_000),
        "an explicit one-minute batch window must not be truncated before scheduling"
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "configured batch window should hold automatic sync work"
    );
    println!(
        "one-minute batch window: scheduled_delay_ms={:?}, sync_calls={}",
        watcher.__scheduled_delay_ms_for_tests(),
        calls.load(Ordering::SeqCst)
    );

    watcher.stop();
}

#[test]
fn should_not_flush_before_scaled_batch_window_under_sustained_stream() {
    let _guard = TestGuard::new();
    let project = TempProject::new("codegraph-watcher");
    let (calls, sync_fn) = sync_mock(Vec::new(), Ok(ok(1, 10)));
    let mut options = inert_options(120);
    options.max_debounce_ms = Some(5_000);
    let mut watcher = FileWatcher::new(project.path(), sync_fn, options);

    watcher.start();
    watcher
        .wait_until_ready(1000)
        .expect("watcher should be ready");

    let stream_start = Instant::now();
    let mut i = 0;
    let max_wait = Duration::from_millis(5_000);
    let ci_pause_margin = Duration::from_millis(1_000);
    while stream_start.elapsed() < Duration::from_millis(1_000) {
        assert!(__emit_watch_event_for_tests(
            project.path(),
            format!("src/batch{i}.ts")
        ));
        i += 1;
        thread::sleep(Duration::from_millis(40));
        if max_wait.saturating_sub(stream_start.elapsed()) <= ci_pause_margin {
            break;
        }
        watcher.flush_due();
        assert_eq!(
            calls.load(Ordering::SeqCst),
            0,
            "sustained stream should not flush before the configured batch window"
        );
        assert!(
            !watcher.get_pending_files().is_empty(),
            "pending files must be retained while the batch window is still open"
        );
    }

    assert_eq!(calls.load(Ordering::SeqCst), 0);
    println!(
        "scaled sustained stream: elapsed_ms={}, sync_calls={}",
        stream_start.elapsed().as_millis(),
        calls.load(Ordering::SeqCst)
    );

    watcher.stop();
}

#[test]
fn should_flush_under_sustained_stream_with_default_max_wait() {
    // 默认配置（不显式设 max_debounce_ms）下，max-wait 取 debounce 的若干倍，持续
    // 事件流也必须在该上限内 flush——保证用户什么都不配也不会“文件变了但永不同步”。
    let _guard = TestGuard::new();
    let project = TempProject::new("codegraph-watcher");
    let (calls, sync_fn) = sync_mock(Vec::new(), Ok(ok(1, 10)));
    // debounce 60ms → 默认 max-wait = 300ms；事件每 25ms 来一次持续 ~1.5s。
    let mut watcher = FileWatcher::new(project.path(), sync_fn, inert_options(60));

    watcher.start();
    watcher
        .wait_until_ready(1000)
        .expect("watcher should be ready");

    let stream_start = Instant::now();
    let mut fired_during_stream = false;
    let mut i = 0;
    while stream_start.elapsed() < Duration::from_millis(1500) {
        assert!(__emit_watch_event_for_tests(
            project.path(),
            format!("src/bulk{i}.ts")
        ));
        i += 1;
        thread::sleep(Duration::from_millis(25));
        watcher.flush_due();
        if calls.load(Ordering::SeqCst) > 0 {
            fired_during_stream = true;
            break;
        }
    }

    assert!(
        fired_during_stream,
        "default max-wait must flush under a sustained event stream without any env/option tuning"
    );

    watcher.stop();
}

#[test]
fn should_debounce_rapid_changes_into_a_single_sync() {
    let _guard = TestGuard::new();
    let project = TempProject::new("codegraph-watcher");
    let (calls, sync_fn) = sync_mock(Vec::new(), Ok(ok(1, 10)));
    let mut watcher = FileWatcher::new(project.path(), sync_fn, inert_options(400));

    watcher.start();
    watcher
        .wait_until_ready(1000)
        .expect("watcher should be ready");

    for i in 0..5 {
        assert!(__emit_watch_event_for_tests(
            project.path(),
            format!("src/file{i}.ts")
        ));
        thread::sleep(Duration::from_millis(50));
        watcher.flush_due();
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    wait_for(
        || {
            watcher.flush_due();
            calls.load(Ordering::SeqCst) > 0
        },
        2000,
        25,
    );

    assert_eq!(calls.load(Ordering::SeqCst), 1);

    watcher.stop();
}
