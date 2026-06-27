use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rustcodegraph::{CodeGraph, IndexOptions, OpenOptions};

const BIN: &str = env!("CARGO_BIN_EXE_rustcodegraph");
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    path: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after Unix epoch")
            .as_nanos();
        let suffix = TEMP_COUNTER.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!(
            "rustcodegraph-cli-watch-{}-{unique}-{suffix}",
            std::process::id()
        ));
        fs::create_dir(&path)
            .unwrap_or_else(|err| panic!("failed to create temp dir {}: {err}", path.display()));
        fs::write(
            path.join("auth.ts"),
            "export function login(req: Request) {\n  return sessionMiddleware(req);\n}\n\n\
             export function sessionMiddleware(req: Request) {\n  return req.session.user;\n}\n",
        )
        .expect("auth fixture should be written");

        let mut cg = CodeGraph::init_sync(&path).expect("CodeGraph should initialize");
        let result = cg.index_all(IndexOptions::default());
        assert!(
            result.success,
            "index_all should succeed, errors: {:?}",
            result.errors
        );
        cg.close();

        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

struct WatchChild {
    child: Child,
    stdout_lines: Receiver<String>,
    stderr_lines: Receiver<String>,
}

impl WatchChild {
    fn spawn(path: &Path) -> Self {
        let mut child = Command::new(BIN)
            .args(["watch", "--debounce-ms", "100", "-p"])
            .arg(path)
            .env("RUSTCODEGRAPH_NO_DAEMON", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("watch command should spawn");

        let stdout = child.stdout.take().expect("stdout should be piped");
        let stderr = child.stderr.take().expect("stderr should be piped");
        let (stdout_tx, stdout_rx) = mpsc::channel();
        let (stderr_tx, stderr_rx) = mpsc::channel();

        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else {
                    break;
                };
                let _ = stdout_tx.send(line);
            }
        });
        thread::spawn(move || {
            for line in BufReader::new(stderr).lines() {
                let Ok(line) = line else {
                    break;
                };
                let _ = stderr_tx.send(line);
            }
        });

        Self {
            child,
            stdout_lines: stdout_rx,
            stderr_lines: stderr_rx,
        }
    }

    fn wait_for_stdout(&mut self, needle: &str, timeout_ms: u64) -> String {
        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        let mut seen = String::new();
        while Instant::now() < deadline {
            if let Ok(Some(status)) = self.child.try_wait() {
                panic!(
                    "watch command exited early with status {:?}\nstdout:\n{}\nstderr:\n{}",
                    status.code(),
                    seen,
                    self.drain_stderr()
                );
            }
            if let Ok(line) = self.stdout_lines.recv_timeout(Duration::from_millis(100)) {
                seen.push_str(&line);
                seen.push('\n');
                if line.contains(needle) {
                    return seen;
                }
            }
        }
        panic!(
            "timed out waiting for stdout containing `{needle}`\nstdout:\n{}\nstderr:\n{}",
            seen,
            self.drain_stderr()
        );
    }

    fn drain_stderr(&self) -> String {
        let mut out = String::new();
        while let Ok(line) = self.stderr_lines.try_recv() {
            out.push_str(&line);
            out.push('\n');
        }
        out
    }
}

impl Drop for WatchChild {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

fn wait_for(mut predicate: impl FnMut() -> bool, timeout_ms: u64) {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    while Instant::now() < deadline {
        if predicate() {
            return;
        }
        thread::sleep(Duration::from_millis(50));
    }
    panic!("condition not satisfied within {timeout_ms}ms");
}

#[test]
fn watch_cli_auto_syncs_new_files_until_interrupted() {
    let fixture = Fixture::new();
    let mut watch = WatchChild::spawn(fixture.path());
    let startup = watch.wait_for_stdout("Press Ctrl-C to stop.", 10_000);

    assert!(startup.contains("Synced 0 changed file(s)"), "{startup}");
    assert!(startup.contains("Watching"), "{startup}");

    fs::write(
        fixture.path().join("added.ts"),
        "export function added() { return 42; }\n",
    )
    .expect("new source file should be written");

    wait_for(
        || {
            let Ok(mut cg) = CodeGraph::open(
                fixture.path(),
                OpenOptions {
                    sync: false,
                    read_only: true,
                },
            ) else {
                return false;
            };
            let found = !cg.search_nodes("added", None).is_empty();
            found
        },
        10_000,
    );
}

#[test]
fn watch_cli_honors_explicit_path_from_a_different_cwd() {
    let fixture = Fixture::new();
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut child = Command::new(BIN)
        .current_dir(&repo_root)
        .args(["watch", "--debounce-ms", "100", "--path"])
        .arg(fixture.path())
        .env("RUSTCODEGRAPH_NO_DAEMON", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("watch command should spawn");

    let stdout = child.stdout.take().expect("stdout should be piped");
    let stderr = child.stderr.take().expect("stderr should be piped");
    let (stdout_tx, stdout_rx) = mpsc::channel();
    let (stderr_tx, stderr_rx) = mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else {
                break;
            };
            let _ = stdout_tx.send(line);
        }
    });
    thread::spawn(move || {
        for line in BufReader::new(stderr).lines() {
            let Ok(line) = line else {
                break;
            };
            let _ = stderr_tx.send(line);
        }
    });

    let deadline = Instant::now() + Duration::from_secs(10);
    let mut seen = String::new();
    while Instant::now() < deadline {
        if let Ok(Some(status)) = child.try_wait() {
            panic!(
                "watch command exited early with status {:?}\nstdout:\n{}\nstderr:\n{}",
                status.code(),
                seen,
                drain_receiver(&stderr_rx)
            );
        }
        if let Ok(line) = stdout_rx.recv_timeout(Duration::from_millis(100)) {
            seen.push_str(&line);
            seen.push('\n');
            if line.contains("Watching") {
                break;
            }
        }
    }

    assert!(
        seen.contains(&fixture.path().display().to_string()),
        "{seen}"
    );

    let _ = child.kill();
    let _ = child.wait();
}

fn drain_receiver(rx: &Receiver<String>) -> String {
    let mut out = String::new();
    while let Ok(line) = rx.try_recv() {
        out.push_str(&line);
        out.push('\n');
    }
    out
}
