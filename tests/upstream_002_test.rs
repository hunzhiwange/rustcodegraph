#![cfg(unix)]
use std::{fs, os::unix::fs::PermissionsExt, process::Command};

static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
struct Temp(std::path::PathBuf);
impl Temp {
    fn path(&self) -> &std::path::Path {
        &self.0
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn run(agent: i32, parser: i32) -> std::process::Output {
    let dir = Temp(std::env::temp_dir().join(format!(
        "rcg-002-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )));
    fs::create_dir(dir.path()).unwrap();
    fs::create_dir(dir.path().join(".rustcodegraph")).unwrap();
    let executable = |name: &str, body: String| {
        let path = dir.path().join(name);
        fs::write(&path, body).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    };
    executable("claude", format!("#!/bin/sh\nexit {agent}\n"));
    let binary = executable(
        "chosen-binary",
        format!("#!/bin/sh\necho selected-parser\nexit {parser}\n"),
    );
    Command::new("bash")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/agent-eval/run-all.sh"
        ))
        .arg(dir.path())
        .arg("question")
        .env(
            "PATH",
            format!(
                "{}:{}",
                dir.path().display(),
                std::env::var("PATH").unwrap()
            ),
        )
        .env("RUSTCODEGRAPH_BIN", binary)
        .env("AGENT_EVAL_OUT", dir.path().join("output"))
        .output()
        .unwrap()
}

#[test]
fn case_002_explicit_binary() {
    let result = run(0, 0);
    assert!(result.status.success());
    assert_eq!(
        String::from_utf8_lossy(&result.stdout)
            .matches("selected-parser")
            .count(),
        2
    );
}
#[test]
fn case_002_agent_failure() {
    assert_eq!(run(17, 0).status.code(), Some(17));
}
#[test]
fn case_002_parser_failure() {
    assert_eq!(run(0, 23).status.code(), Some(23));
}
