//! MCP `initialize` handshake regression tests.
//!
//! Issue #172: on slow filesystems (Docker Desktop VirtioFS on macOS, WSL2),
//! the MCP server was blocking the initialize response on CodeGraph.open() and
//! parser/graph initialization, which could take longer than
//! Claude Code's ~30s handshake timeout. The child process stayed alive and had
//! received the request, but never sent a response, so tools never appeared in
//! the client. The fix sends the initialize response before kicking off the
//! heavy init in the background. These tests guard the contract that initialize
//! is fast regardless of how much work init does.
//!
//! This is the Rust port of `__tests__/mcp-initialize.test.ts`.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rustcodegraph::CodeGraph;
use serde_json::{Value, json};

const BIN: &str = env!("CARGO_BIN_EXE_rustcodegraph");
static TEMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Debug)]
struct StreamEvent {
    seq: usize,
    stream: &'static str,
    text: String,
}

type SharedEvents = Arc<Mutex<Vec<StreamEvent>>>;

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after the Unix epoch")
            .as_nanos();
        let seq = TEMP_COUNTER.fetch_add(1, Ordering::SeqCst);
        let path =
            std::env::temp_dir().join(format!("{prefix}{}-{seq}-{unique}", std::process::id()));
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

struct ChildGuard {
    child: Child,
}

impl ChildGuard {
    fn new(child: Child) -> Self {
        Self { child }
    }

    fn child_mut(&mut self) -> &mut Child {
        &mut self.child
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn spawn_server(cwd: &Path) -> Child {
    Command::new(BIN)
        .args(["serve", "--mcp"])
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // Pin to direct (in-process) mode. #172 is a contract about the
        // in-process server's init ordering; direct mode also avoids leaking a
        // detached daemon from this suite.
        .env("RUSTCODEGRAPH_NO_DAEMON", "1")
        .spawn()
        .unwrap_or_else(|err| panic!("failed to spawn codegraph MCP server: {err}"))
}

fn send_initialize(child: &mut Child, project_path: &Path) {
    send_message(
        child,
        json!({
            "jsonrpc": "2.0",
            "id": 0,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": { "name": "test", "version": "0.0.0" },
                "rootUri": format!("file://{}", project_path.display()),
            },
        }),
    );
}

fn send_message(child: &mut Child, message: Value) {
    let body = serde_json::to_vec(&message).expect("MCP message should serialize");
    let stdin = child.stdin.as_mut().expect("child stdin should be piped");
    write!(stdin, "Content-Length: {}\r\n\r\n", body.len()).expect("MCP header should be written");
    stdin.write_all(&body).expect("MCP body should be written");
    stdin.flush().expect("MCP message should flush");
}

/// Collect stdout messages and stderr text from the child, tagging each piece
/// with a monotonic sequence number. Lets us assert ordering between the
/// JSON-RPC response (stdout) and side-effect logs (stderr).
fn tag_streams(child: &mut Child) -> SharedEvents {
    let events = Arc::new(Mutex::new(Vec::new()));
    let seq = Arc::new(AtomicUsize::new(0));

    let stdout = child.stdout.take().expect("child stdout should be piped");
    let stdout_events = Arc::clone(&events);
    let stdout_seq = Arc::clone(&seq);
    thread::spawn(move || read_stdout_frames(stdout, stdout_events, stdout_seq));

    let stderr = child.stderr.take().expect("child stderr should be piped");
    let stderr_events = Arc::clone(&events);
    let stderr_seq = Arc::clone(&seq);
    thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            let Ok(line) = line else {
                return;
            };
            push_event(&stderr_events, &stderr_seq, "stderr", line);
        }
    });

    events
}

fn read_stdout_frames(stdout: impl Read, events: SharedEvents, seq: Arc<AtomicUsize>) {
    let mut reader = BufReader::new(stdout);
    loop {
        let mut content_length = None;
        loop {
            let mut line = String::new();
            let bytes = match reader.read_line(&mut line) {
                Ok(bytes) => bytes,
                Err(_) => return,
            };
            if bytes == 0 {
                return;
            }
            let trimmed = line.trim_end_matches(['\r', '\n']);
            if trimmed.is_empty() {
                break;
            }
            let lower = trimmed.to_ascii_lowercase();
            if let Some(value) = lower.strip_prefix("content-length:") {
                content_length = value.trim().parse::<usize>().ok();
            }
        }

        let Some(length) = content_length else {
            continue;
        };
        let mut body = vec![0_u8; length];
        if reader.read_exact(&mut body).is_err() {
            return;
        }
        let text = String::from_utf8(body).expect("MCP stdout body should be UTF-8 JSON");
        push_event(&events, &seq, "stdout", text);
    }
}

fn push_event(events: &SharedEvents, seq: &AtomicUsize, stream: &'static str, text: String) {
    let event = StreamEvent {
        seq: seq.fetch_add(1, Ordering::SeqCst),
        stream,
        text,
    };
    events
        .lock()
        .expect("events lock should not be poisoned")
        .push(event);
}

fn wait_for(
    events: &SharedEvents,
    mut predicate: impl FnMut(&StreamEvent) -> bool,
    timeout_ms: u64,
) -> StreamEvent {
    let started = Instant::now();
    loop {
        let snapshot = events
            .lock()
            .expect("events lock should not be poisoned")
            .clone();
        if let Some(hit) = snapshot.into_iter().find(|event| predicate(event)) {
            return hit;
        }
        assert!(
            started.elapsed() <= Duration::from_millis(timeout_ms),
            "Timed out waiting for predicate. Events: {:?}",
            events.lock().expect("events lock should not be poisoned")
        );
        thread::sleep(Duration::from_millis(20));
    }
}

fn parse_event_json(event: &StreamEvent) -> Value {
    serde_json::from_str(&event.text)
        .unwrap_or_else(|err| panic!("event should be JSON ({err}): {}", event.text))
}

fn reply_for(events: &SharedEvents, id: i64) -> Value {
    let event = wait_for(
        events,
        |event| {
            if event.stream != "stdout" {
                return false;
            }
            serde_json::from_str::<Value>(&event.text)
                .ok()
                .and_then(|json| json.get("id").and_then(Value::as_i64).map(|hit| hit == id))
                .unwrap_or(false)
        },
        5_000,
    );
    parse_event_json(&event)
}

mod mcp_initialize_handshake_issue_172 {
    use super::*;

    #[test]
    fn responds_to_initialize_quickly_when_no_codegraph_exists_in_cwd() {
        let temp_dir = TempDir::new("codegraph-mcp-init-");
        let mut child = ChildGuard::new(spawn_server(temp_dir.path()));
        let events = tag_streams(child.child_mut());

        send_initialize(child.child_mut(), temp_dir.path());
        let response = wait_for(&events, |event| event.stream == "stdout", 5_000);
        let json = parse_event_json(&response);

        assert_eq!(json["jsonrpc"], "2.0");
        assert_eq!(json["id"], 0);
        assert!(json["result"]["protocolVersion"].is_string());
        assert!(json["result"]["capabilities"]["tools"].is_object());
    }

    #[test]
    fn sends_initialize_response_before_try_initialize_default_finishes() {
        // Seed a real .rustcodegraph so the server's tryInitializeDefault path runs
        // its full body. The stderr log is observable evidence that the
        // post-response initialization side effect has completed. The contract
        // we're protecting: the JSON-RPC response on stdout must arrive BEFORE
        // that stderr log.
        let temp_dir = TempDir::new("codegraph-mcp-init-");
        let mut cg = CodeGraph::init_sync(temp_dir.path()).expect("CodeGraph should initialize");
        cg.close();

        let mut child = ChildGuard::new(spawn_server(temp_dir.path()));
        let events = tag_streams(child.child_mut());
        send_initialize(child.child_mut(), temp_dir.path());

        let response = wait_for(&events, |event| event.stream == "stdout", 10_000);
        let watcher_log = wait_for(
            &events,
            |event| event.stream == "stderr" && event.text.contains("File watcher active"),
            10_000,
        );
        assert!(response.seq < watcher_log.seq);

        let json = parse_event_json(&response);
        assert_eq!(json["id"], 0);
        assert_eq!(json["result"]["serverInfo"]["name"], "rustcodegraph");
    }

    #[test]
    fn answers_resources_list_and_prompts_list_with_empty_lists_not_32601_issue_621() {
        let temp_dir = TempDir::new("codegraph-mcp-init-");
        let mut child = ChildGuard::new(spawn_server(temp_dir.path()));
        let events = tag_streams(child.child_mut());
        send_initialize(child.child_mut(), temp_dir.path());
        wait_for(&events, |event| event.stream == "stdout", 5_000);

        send_message(
            child.child_mut(),
            json!({ "jsonrpc": "2.0", "id": 1, "method": "resources/list", "params": {} }),
        );
        send_message(
            child.child_mut(),
            json!({ "jsonrpc": "2.0", "id": 2, "method": "prompts/list", "params": {} }),
        );

        let resources = reply_for(&events, 1);
        assert!(resources.get("error").is_none());
        assert_eq!(resources["result"]["resources"], json!([]));

        let prompts = reply_for(&events, 2);
        assert!(prompts.get("error").is_none());
        assert_eq!(prompts["result"]["prompts"], json!([]));
    }
}
