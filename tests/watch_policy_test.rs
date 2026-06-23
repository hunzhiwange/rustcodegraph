//! Watch policy tests.
//!
//! This is the Rust port of `__tests__/watch-policy.test.ts`.

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rustcodegraph::sync::watch_policy::{WatchProbe, watch_disabled_reason};
use rustcodegraph::sync::watcher::{FileWatcher, SyncRunResult, WatchOptions};

fn env_map(entries: &[(&str, &str)]) -> HashMap<String, String> {
    entries
        .iter()
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect()
}

fn probe(entries: &[(&str, &str)], is_wsl: bool) -> WatchProbe {
    WatchProbe {
        env: Some(env_map(entries)),
        is_wsl: Some(is_wsl),
    }
}

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after Unix epoch")
            .as_nanos();
        let path = env::temp_dir().join(format!("{prefix}-{}-{unique}", std::process::id()));
        fs::create_dir(&path)
            .unwrap_or_else(|err| panic!("failed to create temp dir {}: {err}", path.display()));
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        if self.path.exists() {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

struct EnvVarGuard {
    key: &'static str,
    previous: Option<String>,
}

impl EnvVarGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let previous = env::var(key).ok();
        unsafe {
            env::set_var(key, value);
        }
        Self { key, previous }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        unsafe {
            if let Some(value) = &self.previous {
                env::set_var(self.key, value);
            } else {
                env::remove_var(self.key);
            }
        }
    }
}

mod watch_disabled_reason {
    use super::*;

    #[test]
    fn returns_a_reason_when_codegraph_no_watch_eq_1() {
        let reason = watch_disabled_reason(
            "/home/me/project",
            Some(&probe(&[("RUSTCODEGRAPH_NO_WATCH", "1")], false)),
        );

        assert!(reason.is_some());
        assert!(reason.unwrap().contains("RUSTCODEGRAPH_NO_WATCH"));
    }

    #[test]
    fn auto_disables_on_a_wsl2_mnt_drive() {
        let reason = watch_disabled_reason("/mnt/d/code/project", Some(&probe(&[], true)));

        assert!(reason.is_some());
        assert!(reason.unwrap().contains("mnt"));
    }

    #[test]
    fn does_not_disable_on_a_native_wsl_home_path() {
        assert_eq!(
            watch_disabled_reason("/home/me/project", Some(&probe(&[], true))),
            None
        );
    }

    #[test]
    fn does_not_disable_on_mnt_when_not_running_under_wsl() {
        // A real Linux box may legitimately have a fast /mnt mount.
        assert_eq!(
            watch_disabled_reason("/mnt/d/code/project", Some(&probe(&[], false))),
            None
        );
    }

    #[test]
    fn does_not_treat_mnt_wsl_fast_linux_mount_as_a_windows_drive() {
        assert_eq!(
            watch_disabled_reason("/mnt/wsl/project", Some(&probe(&[], true))),
            None
        );
    }

    #[test]
    fn codegraph_force_watch_eq_1_overrides_wsl_auto_detect() {
        let reason = watch_disabled_reason(
            "/mnt/d/code/project",
            Some(&probe(&[("RUSTCODEGRAPH_FORCE_WATCH", "1")], true)),
        );

        assert_eq!(reason, None);
    }

    #[test]
    fn codegraph_no_watch_wins_over_codegraph_force_watch() {
        let reason = watch_disabled_reason(
            "/home/me/project",
            Some(&probe(
                &[
                    ("RUSTCODEGRAPH_NO_WATCH", "1"),
                    ("RUSTCODEGRAPH_FORCE_WATCH", "1"),
                ],
                false,
            )),
        );

        assert!(reason.is_some());
    }
}

mod file_watcher_honors_the_watch_policy {
    use super::*;

    #[test]
    fn does_not_start_when_codegraph_no_watch_eq_1() {
        let test_dir = TempDir::new("codegraph-nowatch");
        let _env = EnvVarGuard::set("RUSTCODEGRAPH_NO_WATCH", "1");
        let sync_fn = || {
            Ok(SyncRunResult {
                files_changed: 0,
                duration_ms: 0,
            })
        };
        let mut watcher = FileWatcher::new(test_dir.path(), sync_fn, WatchOptions::default());

        assert!(!watcher.start());
        assert!(!watcher.is_active());
    }
}
