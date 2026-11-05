use rustcodegraph::{
    CodeGraph,
    types::{Edge, EdgeProvenance},
};

fn check(files: &[(&str, &str)], run: impl FnOnce(&mut CodeGraph)) {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let root = Cleanup(std::env::temp_dir().join(format!(
        "rcg349-{}-{}",
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
    run(&mut g);
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
const CLIENT: &str = "import { io } from 'socket.io-client';\nconst socket = io();\nfunction receiveClient() {}\nsocket.on('reply', receiveClient);\nexport function sendClient() { socket.emit('request', {}); }\n";
const SERVER: &str = "import { Server } from 'socket.io';\nconst server = new Server();\nfunction receiveServer() {}\nserver.on('request', receiveServer);\nexport function sendServer() { server.emit('reply', {}); }\n";
#[test]
fn case_349_registered_opposite_directions_and_reindex() {
    check(
        &[
            ("package.json", "{}"),
            ("client.ts", CLIENT),
            ("server.ts", SERVER),
        ],
        |g| {
            for _ in 0..2 {
                for (file, name, target, tier, registration) in [
                    (
                        "client.ts",
                        "sendClient",
                        "receiveServer",
                        "client→server",
                        "server.ts:4",
                    ),
                    (
                        "server.ts",
                        "sendServer",
                        "receiveClient",
                        "server→client",
                        "client.ts:4",
                    ),
                ] {
                    let e = edges(g, file, name);
                    assert_eq!(e.len(), 1, "{name}: {e:?}");
                    assert_eq!(g.get_node(&e[0].target).unwrap().name, target);
                    let m = e[0].metadata.as_ref().unwrap();
                    assert_eq!(m["synthesizedBy"], "event-bus");
                    assert_eq!(m["channel"], "socket");
                    assert_eq!(m["tier"], tier);
                    assert_eq!(m["registeredAt"], registration);
                }
                let n = g.get_nodes_in_file("server.ts").len();
                assert!(g.index_all(Default::default()).success);
                assert_eq!(g.get_nodes_in_file("server.ts").len(), n);
            }
        },
    );
}
#[test]
fn case_349_no_transport_or_listener_evidence() {
    for server in [
        SERVER.replace(
            "import { Server } from 'socket.io';",
            "import { Server } from './fake';",
        ),
        SERVER.replace(
            "server.on('request', receiveServer);",
            "// server.on('request', receiveServer);",
        ),
        SERVER.replace(
            "server.on('request', receiveServer);",
            "server.on(eventName, receiveServer);",
        ),
    ] {
        check(&[("client.ts", CLIENT), ("server.ts", &server)], |g| {
            assert!(edges(g, "client.ts", "sendClient").is_empty())
        });
    }
}
#[test]
fn case_349_no_cross_application_or_same_direction() {
    check(
        &[
            ("a/package.json", "{}"),
            ("a/client.ts", CLIENT),
            ("b/package.json", "{}"),
            ("b/server.ts", SERVER),
        ],
        |g| assert!(edges(g, "a/client.ts", "sendClient").is_empty()),
    );
    let same_side = SERVER
        .replace(
            "import { Server } from 'socket.io';",
            "import { io } from 'socket.io-client';",
        )
        .replace("new Server()", "io()");
    check(&[("client.ts", CLIENT), ("server.ts", &same_side)], |g| {
        assert!(edges(g, "client.ts", "sendClient").is_empty())
    });
}

#[test]
fn case_349_import_aliases_and_inline_registration() {
    let client = "import { io as connectChat } from 'socket.io-client';\nconst transport = connectChat();\nexport function attachClient() { transport.once('reply', () => {}); }\nexport function sendClient() { transport.emit('request', {}); }";
    let server = "import { Server as ChatServer } from 'socket.io';\nconst transport = new ChatServer();\nfunction receiveServer() {}\ntransport.on('request', receiveServer);\nexport function sendServer() { transport.emit('reply', {}); }";
    check(&[("client.ts", client), ("server.ts", server)], |g| {
        let c = edges(g, "client.ts", "sendClient");
        assert_eq!(c.len(), 1);
        assert_eq!(g.get_node(&c[0].target).unwrap().name, "receiveServer");
        let s = edges(g, "server.ts", "sendServer");
        assert_eq!(s.len(), 1);
        assert_eq!(g.get_node(&s[0].target).unwrap().name, "attachClient");
        assert_eq!(
            s[0].metadata.as_ref().unwrap()["registeredAt"],
            "client.ts:3"
        );
    });
}

#[test]
fn case_349_shadowing_reassignment_and_unregistered_names() {
    for client in [
        CLIENT.replace("sendClient()", "sendClient(socket)"),
        CLIENT.replace("sendClient()", "sendClient(io)"),
        CLIENT.replace("export function", "socket = other;\nexport function"),
        CLIENT.replace("const socket = io();", "const socket = unrelated();"),
        CLIENT.replace("socket.emit('request', {})", "socket.emit(eventName, {})"),
    ] {
        check(&[("client.ts", &client), ("server.ts", SERVER)], |g| {
            assert!(edges(g, "client.ts", "sendClient").is_empty());
        });
    }
    let server = SERVER.replace(
        "function receiveServer() {}",
        "function other(receiveServer) {}\nfunction receiveServer() {}",
    );
    check(&[("client.ts", CLIENT), ("server.ts", &server)], |g| {
        assert!(edges(g, "client.ts", "sendClient").is_empty())
    });
}

#[test]
fn case_349_fake_registrations_generic_events_and_wrong_event() {
    for server in [
        SERVER.replace(
            "server.on('request', receiveServer);",
            "const fake = \"server.on('request', receiveServer)\";",
        ),
        SERVER.replace(
            "server.on('request', receiveServer);",
            "server.on('other', receiveServer);",
        ),
        SERVER.replace(
            "server.on('request', receiveServer);",
            "other.on('request', receiveServer);",
        ),
    ] {
        check(&[("client.ts", CLIENT), ("server.ts", &server)], |g| {
            assert!(edges(g, "client.ts", "sendClient").is_empty())
        });
    }
    check(
        &[
            ("client.ts", &CLIENT.replace("request", "connect")),
            ("server.ts", &SERVER.replace("request", "connect")),
        ],
        |g| assert!(edges(g, "client.ts", "sendClient").is_empty()),
    );
}

#[test]
fn case_349_connection_parameter_direction_is_lexically_scoped() {
    let server = "import { Server } from 'socket.io';\nconst server = new Server();\nfunction receiveServer() {}\nserver.on('connection', peer => { peer.on('request', receiveServer); });\nexport function sendServer() { server.emit('reply', {}); }";
    check(&[("client.ts", CLIENT), ("server.ts", server)], |g| {
        let e = edges(g, "client.ts", "sendClient");
        assert_eq!(e.len(), 1);
        assert_eq!(g.get_node(&e[0].target).unwrap().name, "receiveServer");
        assert_eq!(
            e[0].metadata.as_ref().unwrap()["registeredAt"],
            "server.ts:4"
        );
    });
    for server in [
        server
            .replace("peer.on('request', receiveServer);", "")
            .replace(
                "export function",
                "peer.on('request', receiveServer);\nexport function",
            ),
        server.replace("'connection'", "unknownEvent"),
        server.replace(
            "function receiveServer() {}",
            "function wrapper() { function receiveServer() {} }",
        ),
    ] {
        check(&[("client.ts", CLIENT), ("server.ts", &server)], |g| {
            assert!(edges(g, "client.ts", "sendClient").is_empty())
        });
    }
}
