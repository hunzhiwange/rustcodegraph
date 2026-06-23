//! Unindexed-workspace session policy tests.
//!
//! An MCP session attached to a workspace with no .rustcodegraph/ must go quiet
//! rather than fail loudly: `initialize` returns the short "inactive"
//! instructions variant (not the full playbook), `tools/list` returns an
//! EMPTY list, and a tool call that still arrives (cross-project
//! `projectPath`, or a host that skips tools/list) answers with a
//! SUCCESS-shaped guidance message - never `isError: true`. One or two early
//! isError responses teach an agent to abandon codegraph for the whole
//! session; that observed failure mode is what this suite guards.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rustcodegraph::mcp::tools::{ToolHandler, ToolResult};
use rustcodegraph::{CodeGraph, InitOptions};
use serde_json::{Map, Value, json};

const BIN: &str = env!("CARGO_BIN_EXE_rustcodegraph");
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Self {
        for _ in 0..100 {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time should be after Unix epoch")
                .as_nanos();
            let counter = TEMP_COUNTER.fetch_add(1, Ordering::SeqCst);
            let path = std::env::temp_dir()
                .join(format!("{prefix}{}-{nanos}-{counter}", std::process::id()));
            if fs::create_dir(&path).is_ok() {
                return Self { path };
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
        for attempt in 0..10 {
            match fs::remove_dir_all(&self.path) {
                Ok(()) => return,
                Err(_) if attempt < 9 => thread::sleep(Duration::from_millis(200)),
                Err(_) => return,
            }
        }
    }
}

struct SpawnedServer {
    child: Child,
    responses: Receiver<Result<Value, String>>,
}

impl SpawnedServer {
    fn request(&mut self, msg: Value, timeout: Duration) -> Value {
        write_mcp_message(
            self.child
                .stdin
                .as_mut()
                .expect("server stdin should be piped"),
            &msg,
        )
        .expect("request should be written");

        let id = msg.get("id").cloned().expect("request should have an id");
        loop {
            let response = self
                .responses
                .recv_timeout(timeout)
                .unwrap_or_else(|err| panic!("timeout waiting for response id={id}: {err}"))
                .unwrap_or_else(|err| panic!("failed to read MCP response: {err}"));
            if response.get("id") == Some(&id) {
                return response;
            }
        }
    }
}

impl Drop for SpawnedServer {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}

fn spawn_server(cwd: &Path) -> SpawnedServer {
    let mut child = Command::new(BIN)
        .args(["serve", "--mcp"])
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // Direct (in-process) mode - the unindexed path never has a daemon
        // anyway (the daemon socket lives in .rustcodegraph/), and this keeps the
        // suite from leaking a detached daemon in the indexed test.
        .env("RUSTCODEGRAPH_NO_DAEMON", "1")
        .env("RUSTCODEGRAPH_NO_DAEMON", "1")
        .spawn()
        .unwrap_or_else(|err| panic!("failed to spawn MCP server: {err}"));

    let stdout = child.stdout.take().expect("server stdout should be piped");
    let (tx, responses) = mpsc::channel();
    thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        loop {
            match read_mcp_message(&mut reader) {
                Ok(Some(value)) => {
                    if tx.send(Ok(value)).is_err() {
                        return;
                    }
                }
                Ok(None) => return,
                Err(err) => {
                    let _ = tx.send(Err(err));
                    return;
                }
            }
        }
    });

    SpawnedServer { child, responses }
}

/// Send a JSON-RPC request and resolve with the response matching its id.
fn request(server: &mut SpawnedServer, msg: Value) -> Value {
    server.request(msg, Duration::from_secs(15))
}

fn write_mcp_message(stdin: &mut ChildStdin, value: &Value) -> std::io::Result<()> {
    let body = serde_json::to_vec(value).expect("request should serialize");
    write!(stdin, "Content-Length: {}\r\n\r\n", body.len())?;
    stdin.write_all(&body)?;
    stdin.flush()
}

fn read_mcp_message<R: BufRead>(reader: &mut R) -> Result<Option<Value>, String> {
    let mut content_length = None;
    loop {
        let mut line = String::new();
        let bytes = reader
            .read_line(&mut line)
            .map_err(|err| format!("failed to read MCP header: {err}"))?;
        if bytes == 0 {
            return Ok(None);
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        if let Some(value) = trimmed.to_ascii_lowercase().strip_prefix("content-length:") {
            content_length = value.trim().parse::<usize>().ok();
        }
    }

    let Some(length) = content_length else {
        return Err("MCP message missing Content-Length header".to_owned());
    };
    let mut body = vec![0; length];
    reader
        .read_exact(&mut body)
        .map_err(|err| format!("failed to read MCP body: {err}"))?;
    serde_json::from_slice(&body).map_err(|err| format!("invalid MCP JSON: {err}"))
}

fn initialize_params(project_path: &Path) -> Value {
    json!({
        "protocolVersion": "2025-11-25",
        "capabilities": {},
        "clientInfo": { "name": "test", "version": "0.0.0" },
        "rootUri": file_uri(project_path),
    })
}

fn file_uri(path: &Path) -> String {
    let value = path.to_string_lossy().replace('\\', "/");
    if value.starts_with('/') {
        format!("file://{value}")
    } else {
        format!("file:///{value}")
    }
}

fn tool_args(items: &[(&str, Value)]) -> Map<String, Value> {
    items
        .iter()
        .map(|(key, value)| ((*key).to_owned(), value.clone()))
        .collect()
}

fn first_text(result: &ToolResult) -> &str {
    result
        .content
        .first()
        .map(|content| content.text.as_str())
        .unwrap_or("")
}

mod unindexed_workspace_session_policy {
    use super::*;

    #[test]
    fn initialize_returns_the_short_inactive_instructions_not_the_playbook() {
        let temp_dir = TempDir::new("codegraph-unindexed-");
        fs::write(temp_dir.path().join("index.ts"), "export const x = 1;\n")
            .expect("fixture should be written");
        let mut child = spawn_server(temp_dir.path());

        let res = request(
            &mut child,
            json!({
                "jsonrpc": "2.0",
                "id": 0,
                "method": "initialize",
                "params": initialize_params(temp_dir.path()),
            }),
        );
        let instructions = res["result"]["instructions"]
            .as_str()
            .expect("initialize should include instructions");

        assert!(instructions.to_ascii_lowercase().contains("inactive"));
        assert!(
            instructions.contains("rustcodegraph init"),
            "{instructions}"
        );
        // The full playbook must NOT be sent into a session where every call fails.
        assert!(!instructions.contains("Tool selection by intent"));
        assert!(!instructions.contains("rustcodegraph_explore"));
    }

    #[test]
    fn tools_list_returns_an_empty_list_when_the_workspace_has_no_index() {
        let temp_dir = TempDir::new("codegraph-unindexed-");
        let mut child = spawn_server(temp_dir.path());
        let _ = request(
            &mut child,
            json!({
                "jsonrpc": "2.0",
                "id": 0,
                "method": "initialize",
                "params": initialize_params(temp_dir.path()),
            }),
        );

        let res = request(
            &mut child,
            json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }),
        );
        assert_eq!(res["result"]["tools"], json!([]));
    }

    #[test]
    fn an_indexed_workspace_still_gets_the_full_playbook_and_all_tools() {
        let temp_dir = TempDir::new("codegraph-unindexed-");
        fs::write(
            temp_dir.path().join("index.ts"),
            "export function hello(): string { return \"hi\"; }\n",
        )
        .expect("fixture should be written");
        let mut cg = CodeGraph::init(temp_dir.path(), InitOptions { index: true })
            .expect("CodeGraph should initialize");
        cg.close();

        let mut child = spawn_server(temp_dir.path());
        let init = request(
            &mut child,
            json!({
                "jsonrpc": "2.0",
                "id": 0,
                "method": "initialize",
                "params": initialize_params(temp_dir.path()),
            }),
        );
        let instructions = init["result"]["instructions"]
            .as_str()
            .expect("initialize should include instructions");
        assert!(instructions.contains("Tool selection by intent"));
        assert!(!instructions.to_ascii_lowercase().contains("inactive"));

        let list = request(
            &mut child,
            json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }),
        );
        let tools = list["result"]["tools"]
            .as_array()
            .expect("tools/list should return an array");
        // A 1-file project triggers the pre-existing tiny-repo tool gating in
        // the TypeScript server. The contract under test is
        // "indexed -> tools are PRESENT", in contrast to the unindexed empty
        // list above.
        assert!(tools.len() >= 3, "{tools:?}");
        assert!(
            tools
                .iter()
                .filter_map(|tool| tool.get("name").and_then(Value::as_str))
                .any(|name| name == "rustcodegraph_explore"),
            "{tools:?}"
        );
    }
}

mod no_error_policy_on_expected_conditions {
    use super::*;

    #[test]
    fn cross_project_query_to_an_unindexed_path_is_success_shaped_guidance_not_is_error() {
        let temp_dir = TempDir::new("codegraph-noerror-");
        let mut handler = ToolHandler::new(false);
        let res = handler.execute(
            "rustcodegraph_search",
            &tool_args(&[
                ("query", json!("anything")),
                ("projectPath", json!(temp_dir.path().to_string_lossy())),
            ]),
        );

        assert_eq!(res.is_error, None);
        let text = first_text(&res);
        assert!(text.contains("isn't indexed"), "{text}");
        assert!(text.contains("rustcodegraph init"), "{text}");
        assert!(text.contains("built-in tools"), "{text}");
    }

    #[test]
    fn no_default_project_working_directory_detection_miss_is_success_shaped_guidance() {
        let mut handler = ToolHandler::new(false);
        let res = handler.execute(
            "rustcodegraph_search",
            &tool_args(&[("query", json!("anything"))]),
        );

        assert_eq!(res.is_error, None);
        let text = first_text(&res);
        assert!(
            text.contains("No RustCodeGraph project is loaded"),
            "{text}"
        );
        assert!(text.contains("projectPath"), "{text}");
    }

    #[cfg(not(windows))]
    #[test]
    fn sensitive_path_refusal_stays_a_hard_error_no_retry_encouragement() {
        let mut handler = ToolHandler::new(false);
        let res = handler.execute(
            "rustcodegraph_search",
            &tool_args(&[("query", json!("anything")), ("projectPath", json!("/etc"))]),
        );

        assert_eq!(res.is_error, Some(true));
        assert!(!first_text(&res).contains("retry the call once"));
    }
}

mod search_kind_filter {
    use super::*;

    #[test]
    fn kind_type_the_advertised_enum_value_finds_type_aliases() {
        let temp_dir = TempDir::new("codegraph-kind-");
        fs::write(
            temp_dir.path().join("types.ts"),
            "export type PaymentMethod = { id: string };\nexport function pay(): void {}\n",
        )
        .expect("fixture should be written");
        let mut cg = CodeGraph::init(temp_dir.path(), InitOptions { index: true })
            .expect("CodeGraph should initialize");
        let mut handler = ToolHandler::new(true);
        handler.set_default_code_graph(&cg);

        let res = handler.execute(
            "rustcodegraph_search",
            &tool_args(&[("query", json!("PaymentMethod")), ("kind", json!("type"))]),
        );

        assert_eq!(res.is_error, None);
        let text = first_text(&res);
        assert!(text.contains("PaymentMethod"), "{text}");
        assert!(!text.contains("No results found"), "{text}");
        cg.close();
    }
}
