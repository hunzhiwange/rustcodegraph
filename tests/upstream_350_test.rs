use rustcodegraph::{
    CodeGraph,
    types::{Edge, EdgeProvenance},
};

fn check(files: &[(&str, &str)], run: impl FnOnce(&mut CodeGraph, &std::path::Path)) {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let root = Cleanup(std::env::temp_dir().join(format!(
        "rcg350-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )));
    std::fs::create_dir_all(&root.0).unwrap();
    for (path, source) in files {
        let path = root.0.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, source).unwrap();
    }
    let mut g = CodeGraph::init_sync(&root.0).unwrap();
    let r = g.index_all(Default::default());
    assert!(r.success, "{:?}", r.errors);
    run(&mut g, &root.0);
    g.close();
}
fn edges(g: &mut CodeGraph, file: &str, name: &str) -> Vec<Edge> {
    let node = g
        .get_nodes_in_file(file)
        .into_iter()
        .find(|n| n.name == name)
        .unwrap();
    g.get_outgoing_edges(&node.id)
        .into_iter()
        .filter(|e| e.provenance == Some(EdgeProvenance::Heuristic))
        .collect()
}
const CLIENT: &str = "import { io } from 'socket.io-client';
const socket = io();
export function start() { send(); }
function send() { socket.emit('request', {}); }
";
const SERVER: &str = "import { Server } from 'socket.io';
import { receive as handle } from './handler';
const server = new Server();
server.on('connection', peer => { peer.on('request', handle); });
";
const HANDLER: &str = "export function receive() { finish(); }
function finish() {}
";
fn socket_edges(g: &mut CodeGraph) -> Vec<Edge> {
    edges(g, "client.ts", "send")
        .into_iter()
        .filter(|e| {
            e.metadata
                .as_ref()
                .is_some_and(|m| m["channel"] == "socket")
        })
        .collect()
}
#[test]
fn case_350_imported_listener_flow_and_incremental_removal() {
    check(
        &[
            ("package.json", "{}"),
            ("client.ts", CLIENT),
            ("server.ts", SERVER),
            ("handler.ts", HANDLER),
            ("unrelated.ts", HANDLER),
        ],
        |g, root| {
            let e = socket_edges(g);
            assert_eq!(e.len(), 1, "imported listener must connect: {e:?}");
            assert_eq!(g.get_node(&e[0].target).unwrap().file_path, "handler.ts");
            let mut h = rustcodegraph::mcp::tools::ToolHandler::new(true);
            h.set_default_code_graph(g);
            let response = h.execute(
                "rustcodegraph_explore",
                serde_json::json!({"query":"start send receive finish"})
                    .as_object()
                    .unwrap(),
            );
            let value = serde_json::to_value(response).unwrap();
            let text = value["content"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|c| c["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n");
            let flow = text
                .split("## Flow")
                .nth(1)
                .expect(&text)
                .split("\n## ")
                .next()
                .unwrap();
            assert!(
                flow.lines().any(|line| [
                    "start (client.ts:",
                    "send (client.ts:",
                    "receive (handler.ts:",
                    "finish (handler.ts:",
                    "event-bus",
                    "socket",
                    "client→server",
                    "registeredAt: server.ts:4",
                ]
                .iter()
                .all(|token| line.contains(token))),
                "complete path missing: {flow}"
            );
            h.close_all();
            let count = ["client.ts", "server.ts", "handler.ts", "unrelated.ts"]
                .iter()
                .map(|f| g.get_nodes_in_file(f).len())
                .sum::<usize>();
            assert!(g.index_all(Default::default()).success);
            assert_eq!(
                ["client.ts", "server.ts", "handler.ts", "unrelated.ts"]
                    .iter()
                    .map(|f| g.get_nodes_in_file(f).len())
                    .sum::<usize>(),
                count
            );
            assert_eq!(socket_edges(g).len(), 1);
            std::fs::write(
                root.join("server.ts"),
                SERVER.replace("peer.on('request', handle);", ""),
            )
            .unwrap();
            assert_eq!(g.sync(Default::default()).files_modified, 1);
            assert!(socket_edges(g).is_empty());
        },
    );
}
#[test]
fn case_350_missing_dynamic_and_unexported_registration() {
    for server in [
        SERVER.replace("peer.on('request', handle);", ""),
        SERVER.replace("'request'", "eventName"),
        SERVER.replace("./handler", "./missing"),
        SERVER.replace("peer =>", "(peer, handle) =>"),
    ] {
        check(
            &[
                ("client.ts", CLIENT),
                ("server.ts", &server),
                ("handler.ts", HANDLER),
            ],
            |g, _| assert!(socket_edges(g).is_empty()),
        );
    }
    check(
        &[
            ("client.ts", CLIENT),
            ("server.ts", SERVER),
            ("handler.ts", &HANDLER.replace("export ", "")),
        ],
        |g, _| assert!(socket_edges(g).is_empty()),
    );
}
#[test]
fn case_350_no_cross_app_import_or_same_direction() {
    check(
        &[
            ("package.json", "{}"),
            ("client.ts", CLIENT),
            ("server.ts", SERVER),
            ("handler/package.json", "{}"),
            ("handler/index.ts", HANDLER),
        ],
        |g, _| assert!(socket_edges(g).is_empty()),
    );
    let same = SERVER
        .replace(
            "import { Server } from 'socket.io';",
            "import { io } from 'socket.io-client';",
        )
        .replace("new Server()", "io()")
        .replace(
            "server.on('connection', peer => { peer.on('request', handle); });",
            "server.on('request', handle);",
        );
    check(
        &[
            ("client.ts", CLIENT),
            ("server.ts", &same),
            ("handler.ts", HANDLER),
        ],
        |g, _| assert!(socket_edges(g).is_empty()),
    );
}

#[test]
fn case_350_server_to_imported_client_listener() {
    let server = "import { Server } from 'socket.io';
const transport = new Server();
export function sendReply() { transport.emit('reply', {}); }";
    let client = "import { io } from 'socket.io-client';
import { receive as handle } from './handler';
const transport = io();
transport.once('reply', handle);";
    check(
        &[
            ("server.ts", server),
            ("client.ts", client),
            ("handler.ts", HANDLER),
        ],
        |g, root| {
            let e = edges(g, "server.ts", "sendReply");
            assert_eq!(e.len(), 1, "{e:?}");
            assert_eq!(g.get_node(&e[0].target).unwrap().file_path, "handler.ts");
            let m = e[0].metadata.as_ref().unwrap();
            assert_eq!(m["tier"], "server→client");
            assert_eq!(m["registeredAt"], "client.ts:4");
            std::fs::remove_file(root.join("client.ts")).unwrap();
            assert_eq!(g.sync(Default::default()).files_removed, 1);
            assert!(
                edges(g, "server.ts", "sendReply").is_empty(),
                "{:?}",
                edges(g, "server.ts", "sendReply")
            );
        },
    );
}
