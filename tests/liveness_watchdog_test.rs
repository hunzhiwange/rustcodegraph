//! Main-thread liveness watchdog tests.
//!
//! This is the Rust port of `__tests__/liveness-watchdog.test.ts`.

use std::env;
use std::ffi::OsString;
use std::process::{Command, Stdio};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use rustcodegraph::mcp::liveness_watchdog::{
    DEFAULT_WATCHDOG_TIMEOUT_MS, WatchdogHandle, derive_check_interval_ms,
    install_main_thread_watchdog, parse_watchdog_timeout_ms,
};

const CHILD_MODE_ENV: &str = "RUSTCODEGRAPH_LIVENESS_WATCHDOG_TEST_CHILD";
const MANUAL_HEARTBEAT_ENV: &str = "RUSTCODEGRAPH_WATCHDOG_MANUAL_HEARTBEAT";
const RUSTCODEGRAPH_NO_WATCHDOG_ENV: &str = "RUSTCODEGRAPH_NO_WATCHDOG";
const RUST_NO_WATCHDOG_ENV: &str = "RUSTCODEGRAPH_NO_WATCHDOG";

static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

struct EnvGuard {
    _lock: MutexGuard<'static, ()>,
    originals: Vec<(&'static str, Option<OsString>)>,
}

impl EnvGuard {
    fn set(updates: &[(&'static str, &str)], removals: &[&'static str]) -> Self {
        let names = updates
            .iter()
            .map(|(name, _)| *name)
            .chain(removals.iter().copied())
            .collect::<Vec<_>>();
        let guard = Self::capture(&names);
        unsafe {
            for name in removals {
                env::remove_var(name);
            }
            for (name, value) in updates {
                env::set_var(name, value);
            }
        }
        guard
    }

    fn capture(names: &[&'static str]) -> Self {
        let lock = ENV_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("liveness watchdog env lock should not be poisoned");
        let mut originals = Vec::new();
        for name in names {
            if originals
                .iter()
                .any(|(existing, _): &(&'static str, Option<OsString>)| existing == name)
            {
                continue;
            }
            originals.push((*name, env::var_os(name)));
        }
        Self {
            _lock: lock,
            originals,
        }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        unsafe {
            for (name, value) in &self.originals {
                if let Some(value) = value {
                    env::set_var(name, value);
                } else {
                    env::remove_var(name);
                }
            }
        }
    }
}

mod config_parsing {
    use super::*;

    #[test]
    fn parse_watchdog_timeout_ms_falls_back_for_missing_invalid_input() {
        assert_eq!(
            parse_watchdog_timeout_ms(None, DEFAULT_WATCHDOG_TIMEOUT_MS),
            DEFAULT_WATCHDOG_TIMEOUT_MS
        );
        assert_eq!(
            parse_watchdog_timeout_ms(Some("not-a-number"), DEFAULT_WATCHDOG_TIMEOUT_MS),
            DEFAULT_WATCHDOG_TIMEOUT_MS
        );
        assert_eq!(
            parse_watchdog_timeout_ms(Some("0"), DEFAULT_WATCHDOG_TIMEOUT_MS),
            DEFAULT_WATCHDOG_TIMEOUT_MS
        );
        assert_eq!(
            parse_watchdog_timeout_ms(Some("-5"), DEFAULT_WATCHDOG_TIMEOUT_MS),
            DEFAULT_WATCHDOG_TIMEOUT_MS
        );
        assert_eq!(
            parse_watchdog_timeout_ms(Some("1500"), DEFAULT_WATCHDOG_TIMEOUT_MS),
            1500
        );
    }

    #[test]
    fn derive_check_interval_ms_stays_within_50_2000_and_scales_with_the_timeout() {
        assert_eq!(derive_check_interval_ms(60_000), 2000); // clamped high
        assert_eq!(derive_check_interval_ms(500), 100); // 500/5
        assert_eq!(derive_check_interval_ms(10), 50); // clamped low
    }
}

mod install_main_thread_watchdog_opt_out {
    use super::*;

    #[test]
    fn returns_null_spawns_nothing_when_codegraph_no_watchdog_is_set() {
        let _env = EnvGuard::set(
            &[(RUSTCODEGRAPH_NO_WATCHDOG_ENV, "1")],
            &[RUST_NO_WATCHDOG_ENV],
        );
        assert!(install_main_thread_watchdog().is_none());
    }
}

#[derive(Debug, Clone, Copy)]
enum ChildMode {
    SyncLoop,
    HeapPressure,
    Healthy,
    NoWatchdog,
}

impl ChildMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::SyncLoop => "sync-loop",
            Self::HeapPressure => "heap-pressure",
            Self::Healthy => "healthy",
            Self::NoWatchdog => "no-watchdog",
        }
    }

    fn from_str(raw: &str) -> Option<Self> {
        match raw {
            "sync-loop" => Some(Self::SyncLoop),
            "heap-pressure" => Some(Self::HeapPressure),
            "healthy" => Some(Self::Healthy),
            "no-watchdog" => Some(Self::NoWatchdog),
            _ => None,
        }
    }
}

#[derive(Debug)]
struct ChildResult {
    code: Option<i32>,
    signal: Option<&'static str>,
}

fn node_available() -> bool {
    Command::new(env::var_os("NODE").unwrap_or_else(|| OsString::from("node")))
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn run_child(envs: &[(&str, &str)], mode: ChildMode, hard_timeout_ms: u64) -> ChildResult {
    assert!(node_available(), "node is required for the watchdog child");

    let mut child = Command::new(env::current_exe().expect("test binary path should resolve"));
    child
        .arg("--exact")
        .arg("liveness_watchdog_spawn_child_entry")
        .arg("--nocapture")
        .env(CHILD_MODE_ENV, mode.as_str())
        .env(MANUAL_HEARTBEAT_ENV, "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    for (key, value) in envs {
        child.env(key, value);
    }

    let mut child = child
        .spawn()
        .unwrap_or_else(|err| panic!("failed to spawn liveness child {mode:?}: {err}"));
    let deadline = Instant::now() + Duration::from_millis(hard_timeout_ms);

    loop {
        if let Some(status) = child
            .try_wait()
            .unwrap_or_else(|err| panic!("failed waiting for liveness child {mode:?}: {err}"))
        {
            return ChildResult {
                code: status.code(),
                signal: status_signal(&status),
            };
        }

        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return ChildResult {
                code: None,
                signal: Some("TIMEOUT"),
            };
        }

        thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(unix)]
fn status_signal(status: &std::process::ExitStatus) -> Option<&'static str> {
    use std::os::unix::process::ExitStatusExt;

    match status.signal() {
        Some(9) => Some("SIGKILL"),
        Some(_) => Some("SIGNAL"),
        None => None,
    }
}

#[cfg(not(unix))]
fn status_signal(_status: &std::process::ExitStatus) -> Option<&'static str> {
    None
}

fn expect_killed(r: &ChildResult) {
    assert!(
        r.signal == Some("SIGKILL") || (r.signal.is_none() && r.code.is_some_and(|code| code != 0)),
        "expected watchdog kill, got {r:?}"
    );
}

fn pump_for(handle: &impl WatchdogHandleExt, duration_ms: u64) {
    let end = Instant::now() + Duration::from_millis(duration_ms);
    while Instant::now() < end {
        handle.heartbeat();
        thread::sleep(Duration::from_millis(50));
    }
}

trait WatchdogHandleExt: WatchdogHandle {
    fn heartbeat(&self);
}

impl WatchdogHandleExt for rustcodegraph::mcp::liveness_watchdog::DeferredWatchdogHandle {
    fn heartbeat(&self) {
        rustcodegraph::mcp::liveness_watchdog::DeferredWatchdogHandle::heartbeat(self);
    }
}

fn spin_forever() -> ! {
    loop {
        std::hint::spin_loop();
    }
}

fn spin_for(duration_ms: u64) {
    let end = Instant::now() + Duration::from_millis(duration_ms);
    while Instant::now() < end {
        std::hint::spin_loop();
    }
}

fn exit_child(code: i32) -> ! {
    std::process::exit(code);
}

#[test]
fn liveness_watchdog_spawn_child_entry() {
    let Some(mode) = env::var(CHILD_MODE_ENV)
        .ok()
        .and_then(|raw| ChildMode::from_str(&raw))
    else {
        return;
    };

    match mode {
        ChildMode::SyncLoop => {
            let handle =
                install_main_thread_watchdog().expect("watchdog should install for sync loop");
            pump_for(&handle, 150);
            spin_forever();
        }
        ChildMode::HeapPressure => {
            let handle =
                install_main_thread_watchdog().expect("watchdog should install for heap pressure");
            handle.heartbeat();
            let retained = (0..40)
                .map(|i| vec![i as u8; 1024 * 1024])
                .collect::<Vec<_>>();
            std::hint::black_box(&retained);
            pump_for(&handle, 150);
            spin_forever();
        }
        ChildMode::Healthy => {
            let handle =
                install_main_thread_watchdog().expect("watchdog should install for healthy child");
            pump_for(&handle, 1500);
            handle.stop();
            exit_child(7);
        }
        ChildMode::NoWatchdog => {
            assert!(install_main_thread_watchdog().is_none());
            thread::sleep(Duration::from_millis(150));
            spin_for(1500);
            exit_child(3);
        }
    }
}

mod liveness_watchdog_spawned_real_watchdog_process {
    use super::*;

    #[test]
    fn sigkills_a_process_whose_main_thread_wedges_in_a_sync_loop() {
        let r = run_child(
            &[("RUSTCODEGRAPH_WATCHDOG_TIMEOUT_MS", "500")],
            ChildMode::SyncLoop,
            8000,
        );
        expect_killed(&r);
    }

    #[test]
    fn sigkills_a_non_allocating_wedge_under_heap_pressure_the_case_worker_threads_stalled_on() {
        let r = run_child(
            &[("RUSTCODEGRAPH_WATCHDOG_TIMEOUT_MS", "500")],
            ChildMode::HeapPressure,
            8000,
        );
        expect_killed(&r);
    }

    #[test]
    fn does_not_kill_a_healthy_process_that_keeps_its_event_loop_turning() {
        let r = run_child(
            &[("RUSTCODEGRAPH_WATCHDOG_TIMEOUT_MS", "500")],
            ChildMode::Healthy,
            8000,
        );
        assert_eq!(r.signal, None); // never signalled
        assert_eq!(r.code, Some(7)); // exited on its own terms
    }

    #[test]
    fn does_not_kill_a_wedged_process_when_codegraph_no_watchdog_1() {
        let r = run_child(
            &[
                ("RUSTCODEGRAPH_WATCHDOG_TIMEOUT_MS", "500"),
                ("RUSTCODEGRAPH_NO_WATCHDOG", "1"),
            ],
            ChildMode::NoWatchdog,
            8000,
        );
        // It exits with its OWN code 3 - proving nothing killed it. Checking
        // only signal=null is insufficient on Windows, where a kill also
        // reports null.
        assert_eq!(r.signal, None);
        assert_eq!(r.code, Some(3));
    }
}
