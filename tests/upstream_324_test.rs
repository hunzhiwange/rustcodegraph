use rustcodegraph::{CodeGraph, mcp::tools::ToolHandler, types::NodeKind};
use serde_json::json;

#[test]
fn case_324_http_dispatch() {
    let root = std::env::temp_dir().join(format!("rcg324-{}", std::process::id()));
    std::fs::create_dir_all(root.join("other")).unwrap();
    let write = |p: &str, s: &str| std::fs::write(root.join(p), s).unwrap();
    write(
        "package.json",
        r#"{"dependencies":{"express":"*","axios":"*"}}"#,
    );
    write("other/package.json", "{}");
    write(
        "other/server.ts",
        "import express from 'express'; const app = express(); function finish() {} app.get('/orders', finish); app.get('/foreign', finish);",
    );
    write(
        "mounted.ts",
        "import { Router } from 'express'; const router = Router(); function endpoint() {} router.get('/list', endpoint); export default router;",
    );
    write(
        "server.ts",
        "import express from 'express'; const app = express(); import routes from './mounted'; app.use('/api', routes);\nfunction finish() {}\napp.get('/orders', finish);\napp.post('/orders', finish);\napp.get('/dup', finish);\napp.get('/dup', finish);",
    );
    write(
        "client.ts",
        "import axios from 'axios';\nexport function mounted() { return fetch('/api/list'); }\nexport function start() { return fetch('/orders'); }\nexport function post() { return axios.post('/orders'); }\nexport function dynamic(path: string) { return fetch(path); }\nexport function absent() { return fetch('/missing'); }\nexport function foreign() { return fetch('/foreign'); }\nexport function ambiguous() { return fetch('/dup'); }\nexport function method(m: string) { return fetch('/orders', { method: m }); }\nexport function explicit() { return fetch('/orders', { method: 'POST' }); }",
    );
    let mut g = CodeGraph::init_sync(&root).unwrap();
    assert!(g.index_all(Default::default()).success);
    let check = |g: &mut CodeGraph, name: &str, expected: bool| {
        let n = g
            .get_nodes_in_file("client.ts")
            .into_iter()
            .find(|n| n.name == name && n.kind == NodeKind::Function)
            .unwrap();
        let calls = g.get_callees(&n.id, 1);
        assert_eq!(
            calls.iter().any(|c| c.node.kind == NodeKind::Route),
            expected,
            "{name}: {calls:?}"
        );
    };
    check(&mut g, "start", true);
    check(&mut g, "mounted", true);
    check(&mut g, "post", true);
    check(&mut g, "explicit", true);
    for name in ["dynamic", "absent", "foreign", "ambiguous", "method"] {
        check(&mut g, name, false);
    }
    let mut h = ToolHandler::new(true);
    h.set_default_code_graph(&g);
    let result = h.execute(
        "rustcodegraph_explore",
        json!({"query":"start finish"}).as_object().unwrap(),
    );
    let text = serde_json::to_value(result).unwrap().to_string();
    assert!(
        text.contains("http-client") && text.contains("Flow") && text.contains("server.ts"),
        "{text}"
    );
    h.close_all();
    write(
        "server.ts",
        "import express from 'express'; const app = express(); function finish() {}",
    );
    g.sync(Default::default());
    check(&mut g, "start", false);
    check(&mut g, "mounted", false);
    check(&mut g, "post", false);
    g.close();
    std::fs::remove_dir_all(root).unwrap();
}
