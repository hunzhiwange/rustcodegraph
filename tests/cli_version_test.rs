use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

const BIN: &str = env!("CARGO_BIN_EXE_rustcodegraph");
const PKG_VERSION: &str = env!("CARGO_PKG_VERSION");

fn command(args: &[&str]) -> Command {
    let mut command = Command::new(BIN);
    command
        .args(args)
        // Keep version/help checks from detouring through daemon startup.
        .env("RUSTCODEGRAPH_NO_DAEMON", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

fn run(args: &[&str]) -> String {
    let output = command(args)
        .output()
        .unwrap_or_else(|err| panic!("failed to run rustcodegraph {args:?}: {err}"));

    assert!(
        output.status.success(),
        "rustcodegraph {args:?} failed with status {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn combined_output(args: &[&str]) -> String {
    let output = command(args)
        .output()
        .unwrap_or_else(|err| panic!("failed to run rustcodegraph {args:?}: {err}"));
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn help_lists_command(out: &str, command: &str) -> bool {
    out.lines().any(|line| {
        let trimmed = line.trim_start();
        line.len() != trimmed.len()
            && (trimmed == command
                || trimmed.starts_with(&format!("{command} "))
                || trimmed.starts_with(&format!("{command}\t")))
    })
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
        let path = std::env::temp_dir().join(format!("{prefix}-{}-{unique}", std::process::id()));
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
        let _ = fs::remove_dir_all(&self.path);
    }
}

mod codegraph_version_affordances {
    use super::*;

    #[test]
    fn version_subcommand_prints_exactly_the_package_version() {
        assert_eq!(run(&["version"]), PKG_VERSION);
    }

    #[test]
    fn lowercase_v_prints_exactly_the_package_version() {
        assert_eq!(run(&["-v"]), PKG_VERSION);
    }

    #[test]
    fn single_dash_version_prints_exactly_the_package_version() {
        assert_eq!(run(&["-version"]), PKG_VERSION);
    }

    #[test]
    fn double_dash_version_prints_exactly_the_package_version() {
        assert_eq!(run(&["--version"]), PKG_VERSION);
    }

    #[test]
    fn uppercase_v_prints_exactly_the_package_version() {
        assert_eq!(run(&["-V"]), PKG_VERSION);
    }

    #[test]
    fn lists_the_version_subcommand_in_help() {
        assert!(run(&["--help"]).contains("version"));
    }

    #[test]
    fn rustcodegraph_help_prints_usage_and_the_command_list() {
        let out = run(&["help"]);
        assert!(out.contains("Usage: rustcodegraph"));
        assert!(out.contains("Commands:"));
    }

    #[test]
    fn lists_user_facing_explore_and_node_commands_in_help() {
        let out = run(&["--help"]);
        assert!(help_lists_command(&out, "explore"), "{out}");
        assert!(help_lists_command(&out, "node"), "{out}");
    }

    #[test]
    fn lists_the_watch_command_in_help() {
        let out = run(&["--help"]);
        assert!(help_lists_command(&out, "watch"), "{out}");
    }

    #[test]
    fn hides_the_internal_serve_command_from_help() {
        let out = run(&["--help"]);
        assert!(!help_lists_command(&out, "serve"), "{out}");
    }

    #[test]
    fn trailing_v_is_still_the_subcommands_verbose_not_the_version_intercept() {
        let temp_dir = TempDir::new("codegraph-version-test");
        let temp_path = temp_dir.path().to_string_lossy().into_owned();
        let combined = combined_output(&["index", "-v", &temp_path]);

        assert_ne!(combined.trim(), PKG_VERSION);
        assert!(combined.contains("not initialized"), "{combined}");
    }
}
