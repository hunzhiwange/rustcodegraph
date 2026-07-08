//! MCP project-resolution regression tests (issue #196).
//!
//! This is the Rust port of `__tests__/mcp-roots.test.ts`. The test drives the
//! real stdio transport via a spawned subprocess, preserving the original
//! roots/list, rootUri, temp-directory, process-cleanup, and assertion paths.

use std::env;
use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rustcodegraph::{CodeGraph, IndexOptions};
use serde_json::{Value, json};

const BIN: &str = env!("CARGO_BIN_EXE_rustcodegraph");

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Self {
        for attempt in 0..100 {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock should be after Unix epoch")
                .as_nanos();
            let path =
                env::temp_dir().join(format!("{prefix}{}-{nanos}-{attempt}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => return Self { path },
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(err) => panic!("failed to create temp dir {}: {err}", path.display()),
            }
        }
        panic!("failed to create unique temp dir for {prefix}");
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

struct MessageCollector {
    rx: Receiver<Value>,
    messages: Vec<Value>,
}

struct SpawnedServer {
    child: Child,
    messages: MessageCollector,
}

impl Drop for SpawnedServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn client_info() -> Value {
    json!({ "name": "test", "version": "0.0.0" })
}

fn spawn_server(cwd: &Path) -> SpawnedServer {
    let mut child = Command::new(BIN)
        .args(["serve", "--mcp", "--no-watch"])
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap_or_else(|err| panic!("failed to spawn MCP server in {}: {err}", cwd.display()));
    let stdout = child
        .stdout
        .take()
        .expect("MCP server stdout should be piped");
    let messages = collect_messages(stdout);
    SpawnedServer { child, messages }
}

/// Parse every JSON-RPC message the server writes to stdout into an array.
fn collect_messages(stdout: ChildStdout) -> MessageCollector {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        loop {
            match read_mcp_frame(&mut reader) {
                Ok(Some(message)) => {
                    if tx.send(message).is_err() {
                        break;
                    }
                }
                Ok(None) => break,
                Err(_) => break,
            }
        }
    });
    MessageCollector {
        rx,
        messages: Vec::new(),
    }
}

fn wait_for_message<F>(messages: &mut MessageCollector, predicate: F, timeout_ms: u64) -> Value
where
    F: Fn(&Value) -> bool,
{
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        if let Some(hit) = messages.messages.iter().find(|message| predicate(message)) {
            return hit.clone();
        }
        let now = Instant::now();
        if now >= deadline {
            panic!(
                "Timed out. Messages so far: {}",
                serde_json::to_string(&messages.messages)
                    .unwrap_or_else(|_| "<unserializable>".to_string())
            );
        }
        let remaining = deadline.saturating_duration_since(now);
        let wait = remaining.min(Duration::from_millis(20));
        match messages.rx.recv_timeout(wait) {
            Ok(message) => messages.messages.push(message),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                panic!(
                    "MCP server stdout closed. Messages so far: {}",
                    serde_json::to_string(&messages.messages)
                        .unwrap_or_else(|_| "<unserializable>".to_string())
                );
            }
        }
    }
}

fn send(child: &mut SpawnedServer, msg: Value) {
    let stdin = child
        .child
        .stdin
        .as_mut()
        .expect("MCP server stdin should be piped");
    send_mcp_frame(stdin, &msg).expect("failed to send MCP frame");
}

fn send_mcp_frame(stdin: &mut ChildStdin, msg: &Value) -> io::Result<()> {
    let body =
        serde_json::to_vec(msg).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    write!(stdin, "Content-Length: {}\r\n\r\n", body.len())?;
    stdin.write_all(&body)?;
    stdin.flush()
}

fn read_mcp_frame<R: BufRead>(reader: &mut R) -> io::Result<Option<Value>> {
    let mut content_length = None;
    loop {
        let mut line = String::new();
        let bytes = reader.read_line(&mut line)?;
        if bytes == 0 {
            return Ok(None);
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
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "MCP message missing Content-Length header",
        ));
    };
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body)?;
    serde_json::from_slice(&body).map(Some).map_err(|err| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid MCP JSON: {err}"),
        )
    })
}

fn response_text(resp: &Value) -> &str {
    resp.pointer("/result/content/0/text")
        .and_then(Value::as_str)
        .expect("tool response should include text content")
}

fn file_uri(path: &Path) -> String {
    let value = path.to_string_lossy().replace('\\', "/");
    if value.starts_with('/') {
        format!("file://{value}")
    } else {
        format!("file:///{value}")
    }
}

mod mcp_project_resolution_via_roots_list_issue_196 {
    use super::*;

    #[test]
    fn status_is_diagnostic_and_does_not_run_catch_up_sync() {
        let project_dir = TempDir::new("codegraph-mcp-status-");
        fs::write(
            project_dir.path().join("original.ts"),
            "export function original() { return 1; }\n",
        )
        .expect("fixture source should be written");
        let mut cg = CodeGraph::init_sync(project_dir.path()).expect("CodeGraph should initialize");
        let result = cg.index_all(IndexOptions::default());
        assert!(
            result.success,
            "index_all should succeed, errors: {:?}",
            result.errors
        );
        cg.close();
        fs::write(
            project_dir.path().join("added-after-index.ts"),
            "export function addedAfterIndex() { return 2; }\n",
        )
        .expect("post-index fixture source should be written");

        let mut child = spawn_server(project_dir.path());
        send(
            &mut child,
            json!({
                "jsonrpc": "2.0",
                "id": 0,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2025-11-25",
                    "capabilities": {},
                    "clientInfo": client_info(),
                    "rootUri": file_uri(project_dir.path()),
                },
            }),
        );
        wait_for_message(
            &mut child.messages,
            |m| m.get("id") == Some(&json!(0)) && m.get("result").is_some(),
            5_000,
        );

        send(
            &mut child,
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": { "name": "rustcodegraph_status", "arguments": {} },
            }),
        );

        let resp = wait_for_message(
            &mut child.messages,
            |m| m.get("id") == Some(&json!(1)),
            8_000,
        );
        let text = response_text(&resp);
        assert!(text.contains("RustCodeGraph Status"), "{text}");
        assert!(text.contains("**Files indexed:** 1"), "{text}");
    }

    #[test]
    fn resolves_the_project_from_the_client_roots_list_when_no_root_uri_is_sent() {
        let cwd_dir = TempDir::new("codegraph-mcp-cwd-");
        let project_dir = TempDir::new("codegraph-mcp-proj-");
        let mut cg = CodeGraph::init_sync(project_dir.path()).expect("CodeGraph should initialize");
        cg.close();

        let mut child = spawn_server(cwd_dir.path());

        // Advertise the roots capability but pass NO rootUri/workspaceFolders.
        send(
            &mut child,
            json!({
                "jsonrpc": "2.0",
                "id": 0,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2025-11-25",
                    "capabilities": { "roots": {} },
                    "clientInfo": client_info(),
                },
            }),
        );
        wait_for_message(
            &mut child.messages,
            |m| m.get("id") == Some(&json!(0)) && m.get("result").is_some(),
            5_000,
        );
        send(
            &mut child,
            json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
        );

        // First tool call (no projectPath) drives the server to ask us for roots.
        send(
            &mut child,
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": { "name": "rustcodegraph_status", "arguments": {} },
            }),
        );

        let roots_req = wait_for_message(
            &mut child.messages,
            |m| m.get("method").and_then(Value::as_str) == Some("roots/list"),
            5_000,
        );
        assert!(
            roots_req.get("id").and_then(Value::as_str).is_some(),
            "server-initiated id should be a string: {roots_req}"
        );
        send(
            &mut child,
            json!({
                "jsonrpc": "2.0",
                "id": roots_req.get("id").cloned().expect("roots/list id should exist"),
                "result": { "roots": [{ "uri": file_uri(project_dir.path()), "name": "proj" }] },
            }),
        );

        // The status call now succeeds against the resolved project.
        let resp = wait_for_message(
            &mut child.messages,
            |m| m.get("id") == Some(&json!(1)),
            8_000,
        );
        let text = response_text(&resp);
        assert!(text.contains("RustCodeGraph Status"), "{text}");
        assert!(
            !text.contains("No RustCodeGraph project is loaded"),
            "{text}"
        );
    }

    #[test]
    fn tools_list_resolves_the_project_from_client_roots_before_returning_empty_tools() {
        let cwd_dir = TempDir::new("codegraph-mcp-cwd-");
        let project_dir = TempDir::new("codegraph-mcp-proj-");
        let mut cg = CodeGraph::init_sync(project_dir.path()).expect("CodeGraph should initialize");
        cg.close();

        let mut child = spawn_server(cwd_dir.path());

        send(
            &mut child,
            json!({
                "jsonrpc": "2.0",
                "id": 0,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2025-11-25",
                    "capabilities": { "roots": {} },
                    "clientInfo": client_info(),
                },
            }),
        );
        wait_for_message(
            &mut child.messages,
            |m| m.get("id") == Some(&json!(0)) && m.get("result").is_some(),
            5_000,
        );
        send(
            &mut child,
            json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
        );

        send(
            &mut child,
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/list",
            }),
        );

        let roots_req = wait_for_message(
            &mut child.messages,
            |m| m.get("method").and_then(Value::as_str) == Some("roots/list"),
            5_000,
        );
        send(
            &mut child,
            json!({
                "jsonrpc": "2.0",
                "id": roots_req.get("id").cloned().expect("roots/list id should exist"),
                "result": { "roots": [{ "uri": file_uri(project_dir.path()), "name": "proj" }] },
            }),
        );

        let resp = wait_for_message(
            &mut child.messages,
            |m| m.get("id") == Some(&json!(1)),
            8_000,
        );
        let tools = resp["result"]["tools"]
            .as_array()
            .expect("tools/list should return an array");
        assert!(!tools.is_empty(), "{resp}");
        assert!(
            tools
                .iter()
                .filter_map(|tool| tool.get("name").and_then(Value::as_str))
                .any(|name| name == "rustcodegraph_explore"),
            "{tools:?}"
        );
    }

    #[test]
    fn returns_an_actionable_error_when_there_is_no_root_uri_and_no_roots_capability() {
        let cwd_dir = TempDir::new("codegraph-mcp-cwd-");
        let _project_dir = TempDir::new("codegraph-mcp-proj-");

        let mut child = spawn_server(cwd_dir.path());

        send(
            &mut child,
            json!({
                "jsonrpc": "2.0",
                "id": 0,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2025-11-25",
                    "capabilities": {},
                    "clientInfo": client_info(),
                },
            }),
        );
        wait_for_message(
            &mut child.messages,
            |m| m.get("id") == Some(&json!(0)) && m.get("result").is_some(),
            5_000,
        );
        send(
            &mut child,
            json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
        );

        send(
            &mut child,
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": { "name": "rustcodegraph_status", "arguments": {} },
            }),
        );
        let resp = wait_for_message(
            &mut child.messages,
            |m| m.get("id") == Some(&json!(1)),
            8_000,
        );
        let text = response_text(&resp);

        assert!(
            text.contains("No RustCodeGraph project is loaded"),
            "{text}"
        );
        assert!(text.contains("projectPath"), "{text}");
        assert!(text.contains("--path"), "{text}");
        // Names the directory it actually searched (the wrong cwd) so the user can
        // see why detection missed. basename survives any symlink realpath-ing.
        assert!(
            text.contains(
                cwd_dir
                    .path()
                    .file_name()
                    .and_then(|name| name.to_str())
                    .expect("temp cwd should have basename")
            ),
            "{text}"
        );
        // It must not have hung waiting on roots/list - the client never offered it.
        assert!(
            !child
                .messages
                .messages
                .iter()
                .any(|m| m.get("method").and_then(Value::as_str) == Some("roots/list"))
        );
    }

    #[test]
    fn honors_an_explicit_root_uri_without_asking_the_client_for_roots() {
        let cwd_dir = TempDir::new("codegraph-mcp-cwd-");
        let project_dir = TempDir::new("codegraph-mcp-proj-");
        let mut cg = CodeGraph::init_sync(project_dir.path()).expect("CodeGraph should initialize");
        cg.close();

        let mut child = spawn_server(cwd_dir.path());

        send(
            &mut child,
            json!({
                "jsonrpc": "2.0",
                "id": 0,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2025-11-25",
                    "capabilities": { "roots": {} },
                    "clientInfo": client_info(),
                    "rootUri": file_uri(project_dir.path()),
                },
            }),
        );
        wait_for_message(
            &mut child.messages,
            |m| m.get("id") == Some(&json!(0)) && m.get("result").is_some(),
            5_000,
        );
        send(
            &mut child,
            json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
        );

        send(
            &mut child,
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": { "name": "rustcodegraph_status", "arguments": {} },
            }),
        );
        let resp = wait_for_message(
            &mut child.messages,
            |m| m.get("id") == Some(&json!(1)),
            8_000,
        );
        let text = response_text(&resp);

        assert!(text.contains("RustCodeGraph Status"), "{text}");
        // rootUri is a stronger signal than roots - we never needed to ask.
        assert!(
            !child
                .messages
                .messages
                .iter()
                .any(|m| m.get("method").and_then(Value::as_str) == Some("roots/list"))
        );
    }
}
