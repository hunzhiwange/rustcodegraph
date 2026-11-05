use rustcodegraph::{CodeGraph, types::NodeKind};

#[test]
fn case_323_mount_registration() {
    let root = std::env::temp_dir().join(format!("rcg323-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let write = |file: &str, source: &str| std::fs::write(root.join(file), source).unwrap();
    write("package.json", r#"{"dependencies":{"express":"*"}}"#);
    write(
        "app.ts",
        "import express from 'express'; import routes from './routes'; const app = express(); app.use('/api/v1/', routes);",
    );
    write(
        "routes.ts",
        "import { Router } from 'express'; import orders from './orders'; const router = Router(); router.use('/orders/', orders); export default router;",
    );
    write(
        "orders.ts",
        "import { Router } from 'express'; const router = Router(); function list() {} router.get('/', list); export default router;",
    );
    write(
        "fake.ts",
        "const app = other(); app.use('/fake', orders); // no import evidence\n",
    );
    let mut graph = CodeGraph::init_sync(&root).unwrap();
    assert!(graph.index_all(Default::default()).success);
    let names = |g: &mut CodeGraph| {
        g.get_nodes_in_file("orders.ts")
            .into_iter()
            .filter(|n| n.kind == NodeKind::Route)
            .map(|n| n.name)
            .collect::<Vec<_>>()
    };
    assert_eq!(names(&mut graph), ["GET /api/v1/orders"]);
    let count = graph.get_nodes_in_file("orders.ts").len();
    assert!(graph.index_all(Default::default()).success);
    assert_eq!(names(&mut graph), ["GET /api/v1/orders"]);
    assert_eq!(count, graph.get_nodes_in_file("orders.ts").len());
    // Two distinct mount paths are ambiguous, never pick the first.
    write(
        "app.ts",
        "import express from 'express'; import routes from './routes'; const app = express(); app.use('/api/v1', routes); app.use('/other', routes);",
    );
    graph.sync(Default::default());
    assert_eq!(names(&mut graph), ["GET /"]);
    // A dynamic prefix supplies no path evidence, including after sync removal.
    write(
        "app.ts",
        "import express from 'express'; import routes from './routes'; const app = express(); app.use(prefix, routes);",
    );
    graph.sync(Default::default());
    assert_eq!(names(&mut graph), ["GET /orders"]);
    // Without a resolvable target, a same-named local variable is not a mount.
    write(
        "routes.ts",
        "import { Router } from 'express'; const router = Router(); const orders = unknown(); router.use('/invented', orders); export default router;",
    );
    graph.sync(Default::default());
    assert_eq!(names(&mut graph), ["GET /"]);
    graph.close();
    std::fs::remove_dir_all(root).unwrap();
}
