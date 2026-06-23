//! Installer config-writer compatibility tests.
//!
//! This is the Rust port of `__tests__/installer.test.ts`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use rustcodegraph::installer::config_writer::write_mcp_config;
use rustcodegraph::installer::targets::types::{Location, WriteAction};
use serde_json::{Value, json};

const MCP_SERVER_KEY: &str = "rustcodegraph";
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);
static CWD_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn create_temp_dir() -> PathBuf {
    for _ in 0..100 {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after the Unix epoch")
            .as_nanos();
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "rustcodegraph-installer-test-{}-{unique}-{counter}",
            std::process::id()
        ));
        match fs::create_dir(&dir) {
            Ok(()) => return dir,
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(err) => panic!("failed to create temp dir {}: {err}", dir.display()),
        }
    }
    panic!("failed to create unique temp dir");
}

fn cleanup_temp_dir(dir: &Path) {
    if dir.exists() {
        let _ = fs::remove_dir_all(dir);
    }
}

fn read_json(path: &Path) -> Value {
    let raw = fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
    serde_json::from_str(&raw)
        .unwrap_or_else(|err| panic!("failed to parse {} as JSON: {err}", path.display()))
}

fn run_cli(args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_rustcodegraph"))
        .args(args)
        .output()
        .unwrap_or_else(|err| panic!("failed to run rustcodegraph {args:?}: {err}"));
    assert!(
        output.status.success(),
        "rustcodegraph {args:?} failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .unwrap_or_else(|err| panic!("CLI stdout should be valid UTF-8: {err}"))
}

struct TestEnv {
    _lock: MutexGuard<'static, ()>,
    orig_cwd: PathBuf,
    temp_dir: PathBuf,
}

impl TestEnv {
    fn new() -> Self {
        let lock = CWD_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("cwd test lock should not be poisoned");
        let temp_dir = create_temp_dir();
        let orig_cwd = std::env::current_dir().expect("current dir should resolve");
        std::env::set_current_dir(&temp_dir)
            .unwrap_or_else(|err| panic!("failed to chdir to {}: {err}", temp_dir.display()));
        Self {
            _lock: lock,
            orig_cwd,
            temp_dir,
        }
    }

    fn path(&self) -> &Path {
        &self.temp_dir
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.orig_cwd);
        cleanup_temp_dir(&self.temp_dir);
    }
}

mod installer_config_writer {
    use super::*;

    mod read_json_file_error_handling {
        use super::*;

        #[test]
        fn should_return_empty_object_for_non_existent_file() {
            let env = TestEnv::new();

            let result = write_mcp_config(Location::Local);

            let mcp_json = env.path().join(".mcp.json");
            assert!(mcp_json.exists());
            assert_eq!(result.action, WriteAction::Created);

            let content = read_json(&mcp_json);
            assert!(content.get("mcpServers").is_some());
            assert!(content["mcpServers"].get(MCP_SERVER_KEY).is_some());
        }

        #[test]
        fn should_handle_corrupted_json_by_creating_backup() {
            let env = TestEnv::new();
            let mcp_json = env.path().join(".mcp.json");
            fs::write(&mcp_json, "{ this is not valid json !!!")
                .expect("corrupted .mcp.json fixture should be written");

            let result = write_mcp_config(Location::Local);

            assert_eq!(result.action, WriteAction::Updated);

            let mut backup_path = mcp_json.clone();
            backup_path.as_mut_os_string().push(".backup");
            assert!(backup_path.exists());
            let backup = fs::read_to_string(&backup_path)
                .unwrap_or_else(|err| panic!("failed to read backup: {err}"));
            assert!(backup.contains("this is not valid json"));

            let content = read_json(&mcp_json);
            assert!(content["mcpServers"].get(MCP_SERVER_KEY).is_some());
        }

        #[test]
        fn should_preserve_existing_valid_config_when_adding_rustcodegraph() {
            let env = TestEnv::new();
            let mcp_json = env.path().join(".mcp.json");
            fs::write(
                &mcp_json,
                serde_json::to_string_pretty(&json!({
                    "mcpServers": { "other": { "command": "other-tool" } },
                    "customField": "preserved",
                }))
                .expect("fixture config should serialize"),
            )
            .expect("fixture config should be written");

            let result = write_mcp_config(Location::Local);

            assert_eq!(result.action, WriteAction::Updated);

            let content = read_json(&mcp_json);
            assert!(content["mcpServers"].get(MCP_SERVER_KEY).is_some());
            assert!(content["mcpServers"].get("other").is_some());
            assert_eq!(content["customField"], "preserved");
        }
    }
}

mod quick_installer_cli {
    use super::*;

    #[test]
    fn print_config_uses_rustcodegraph_for_codex() {
        let stdout = run_cli(&["install", "--print-config", "codex"]);

        assert!(stdout.contains("[mcp_servers.rustcodegraph]"));
        assert!(stdout.contains("command = \"rustcodegraph\""));
        assert!(stdout.contains("args = [\"serve\", \"--mcp\"]"));
        assert!(!stdout.contains("[mcp_servers.codegraph]"));
        assert!(!stdout.contains("command = \"codegraph\""));
    }

    #[test]
    fn print_config_uses_rustcodegraph_for_opencode() {
        let stdout = run_cli(&["install", "--print-config", "opencode"]);

        assert!(stdout.contains("\"rustcodegraph\""));
        assert!(stdout.contains("\"serve\""));
        assert!(stdout.contains("\"--mcp\""));
        assert!(!stdout.contains("\"codegraph\""));
        assert!(!stdout.contains("[\"codegraph\","));
    }

    #[test]
    fn print_config_uses_rustcodegraph_for_cursor() {
        let stdout = run_cli(&["install", "--print-config", "cursor"]);

        assert!(stdout.contains("\"rustcodegraph\""));
        assert!(stdout.contains("\"serve\""));
        assert!(stdout.contains("\"--mcp\""));
        assert!(stdout.contains("\"${workspaceFolder}\""));
        assert!(!stdout.contains("\"codegraph\""));
    }
}
