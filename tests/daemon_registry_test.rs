//! Global daemon registry + stop/list control.
//!
//! This is the Rust port of `__tests__/daemon-registry.test.ts`.

use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rustcodegraph::mcp::daemon_registry::{
    DaemonRecord, deregister_daemon, get_registry_dir, is_process_alive, list_daemons,
    register_daemon,
};

static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

struct TempHome {
    path: PathBuf,
    _lock: MutexGuard<'static, ()>,
    prev_home: Option<OsString>,
    prev_user_profile: Option<OsString>,
}

impl TempHome {
    fn new() -> Self {
        let lock = ENV_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("daemon-registry env lock should not be poisoned");
        let path = make_temp_dir("cg-reg-home");
        let prev_home = env::var_os("HOME");
        let prev_user_profile = env::var_os("USERPROFILE");

        unsafe {
            env::set_var("HOME", &path);
            env::set_var("USERPROFILE", &path);
        }

        // Sanity: the registry must resolve under our temp home, or the test
        // would pollute the real daemon registry.
        assert!(
            get_registry_dir().starts_with(&path),
            "registry dir {} should be under temp home {}",
            get_registry_dir().display(),
            path.display()
        );

        Self {
            path,
            _lock: lock,
            prev_home,
            prev_user_profile,
        }
    }
}

impl Drop for TempHome {
    fn drop(&mut self) {
        unsafe {
            if let Some(value) = &self.prev_home {
                env::set_var("HOME", value);
            } else {
                env::remove_var("HOME");
            }

            if let Some(value) = &self.prev_user_profile {
                env::set_var("USERPROFILE", value);
            } else {
                env::remove_var("USERPROFILE");
            }
        }

        let _ = fs::remove_dir_all(&self.path);
    }
}

fn make_temp_dir(prefix: &str) -> PathBuf {
    for attempt in 0..100 {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after Unix epoch")
            .as_nanos();
        let path = env::temp_dir().join(format!(
            "{prefix}-{}-{unique}-{attempt}",
            std::process::id()
        ));
        match fs::create_dir(&path) {
            Ok(()) => return path,
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(err) => panic!("failed to create temp dir {}: {err}", path.display()),
        }
    }
    panic!("failed to create unique temp dir for {prefix}");
}

/// A pid that's guaranteed dead: spawn a trivial process, let it exit, reap it.
fn dead_pid() -> u32 {
    let mut child = trivial_exit_command()
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap_or_else(|err| panic!("failed to spawn trivial child: {err}"));
    let pid = child.id();
    child
        .wait()
        .unwrap_or_else(|err| panic!("failed to wait for trivial child {pid}: {err}"));
    thread::sleep(Duration::from_millis(50)); // let the OS reap it
    pid
}

#[cfg(windows)]
fn trivial_exit_command() -> Command {
    let mut command = Command::new("cmd");
    command.args(["/C", "exit", "0"]);
    command
}

#[cfg(not(windows))]
fn trivial_exit_command() -> Command {
    let mut command = Command::new("sh");
    command.args(["-c", "exit 0"]);
    command
}

fn rec(root: &str, pid: u32, started_at: Option<i64>) -> DaemonRecord {
    DaemonRecord {
        root: root.to_string(),
        pid,
        version: "1.0.0".to_string(),
        socket_path: format!("{root}/.rustcodegraph/daemon.sock"),
        started_at: started_at.unwrap_or_else(now_ms),
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn json_records(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .unwrap_or_else(|err| panic!("failed to read registry dir {}: {err}", dir.display()))
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
        .collect()
}

mod daemon_registry {
    use super::*;

    mod is_process_alive_suite {
        use super::*;

        #[test]
        fn is_true_for_our_own_process_and_false_for_junk_dead_pids() {
            let _home = TempHome::new();

            assert!(is_process_alive(std::process::id()));
            assert!(!is_process_alive(0));
            // Rust cannot pass TS's -1 / NaN values to a u32 API, so these
            // preserve the same junk-PID preflight path with a value that would
            // otherwise wrap to -1 on Unix.
            let ts_negative_one_pid = u32::MAX;
            let ts_nan_pid = u32::MAX;
            assert!(!is_process_alive(ts_negative_one_pid));
            assert!(!is_process_alive(ts_nan_pid));
            assert!(!is_process_alive(dead_pid()));
        }
    }

    #[test]
    fn list_daemons_returns_empty_when_nothing_is_registered_no_dir_yet() {
        let _home = TempHome::new();

        assert_eq!(list_daemons(true), Vec::<DaemonRecord>::new());
    }

    #[test]
    fn register_list_shows_a_live_daemon_deregister_removes_it() {
        let _home = TempHome::new();

        register_daemon(&rec("/proj/a", std::process::id(), None));
        let live = list_daemons(true);
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].root, "/proj/a");
        assert_eq!(live[0].pid, std::process::id());

        deregister_daemon("/proj/a");
        assert_eq!(list_daemons(true), Vec::<DaemonRecord>::new());
    }

    #[test]
    fn prunes_records_whose_process_is_dead() {
        let _home = TempHome::new();
        let dead = dead_pid();

        register_daemon(&rec("/proj/dead", dead, None));
        register_daemon(&rec("/proj/live", std::process::id(), None));

        let live = list_daemons(true);
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].root, "/proj/live");

        // The dead record's file was deleted as a side effect.
        assert_eq!(json_records(&get_registry_dir()).len(), 1);
    }

    #[test]
    fn peeking_with_prune_false_leaves_dead_records_on_disk() {
        let _home = TempHome::new();
        let dead = dead_pid();

        register_daemon(&rec("/proj/dead", dead, None));
        assert_eq!(list_daemons(false), Vec::<DaemonRecord>::new()); // dead is filtered from results

        // ...but the file survives for the caller to inspect.
        assert_eq!(json_records(&get_registry_dir()).len(), 1);
    }

    #[test]
    fn lists_multiple_live_daemons_newest_first() {
        let _home = TempHome::new();

        register_daemon(&rec("/proj/old", std::process::id(), Some(1000)));
        register_daemon(&rec("/proj/new", std::process::id(), Some(2000)));

        let roots = list_daemons(true)
            .into_iter()
            .map(|daemon| daemon.root)
            .collect::<Vec<_>>();
        assert_eq!(roots, vec!["/proj/new", "/proj/old"]);
    }
}
