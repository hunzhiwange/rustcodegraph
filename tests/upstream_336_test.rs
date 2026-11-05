use rustcodegraph::{CodeGraph, types::EdgeProvenance};

fn check(source: &str, run: impl FnOnce(&mut CodeGraph)) {
    let root = std::env::temp_dir().join(format!("rcg336-{}-{}", std::process::id(), source.len()));
    std::fs::create_dir_all(&root).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(root.clone());
    std::fs::write(root.join("jobs.ts"), source).unwrap();
    let mut g = CodeGraph::init_sync(&root).unwrap();
    assert!(g.index_all(Default::default()).success);
    run(&mut g);
    g.close();
}
fn targets(g: &mut CodeGraph) -> Vec<String> {
    let start = g
        .get_nodes_in_file("jobs.ts")
        .into_iter()
        .find(|n| n.name == "start")
        .unwrap();
    let edges = g.get_outgoing_edges(&start.id);
    edges
        .into_iter()
        .filter(|e| {
            e.metadata
                .as_ref()
                .and_then(|m| m.get("synthesizedBy"))
                .and_then(|v| v.as_str())
                == Some("queue-job")
        })
        .map(|e| {
            assert_eq!(e.provenance, Some(EdgeProvenance::Heuristic));
            let m = e.metadata.unwrap();
            assert_eq!(m["queue"], "email");
            assert!(m["registeredAt"].as_str().unwrap().starts_with("jobs.ts:"));
            g.get_node(&e.target).unwrap().name
        })
        .collect()
}
#[test]
fn case_336_named_queue_and_registered_consumer() {
    check(
        "import Queue from 'bull';\nconst queue = new Queue('email');\nfunction receive(job) {}\nqueue.process('welcome', receive);\nexport function start() { queue.add('welcome', {}); }",
        |g| {
            assert_eq!(targets(g), ["receive"]);
            let count = g.get_nodes_in_file("jobs.ts").len();
            assert!(g.index_all(Default::default()).success);
            assert_eq!(targets(g), ["receive"]);
            assert_eq!(count, g.get_nodes_in_file("jobs.ts").len());
        },
    );
}
#[test]
fn case_336_missing_or_dynamic_registration() {
    check(
        "import Queue from 'bull';\nconst queue = new Queue(queueName);\nfunction receive(job) {}\nqueue.process('welcome', receive);\nexport function start() { queue.add('welcome', {}); }",
        |g| assert!(targets(g).is_empty()),
    );
    check(
        "const queue = unknown();\nfunction receive(job) {}\nqueue.process('welcome', receive);\nexport function start() { queue.add('welcome', {}); }",
        |g| assert!(targets(g).is_empty()),
    );
}
#[test]
fn case_336_wrong_queue_job_and_fake_evidence() {
    check(
        "import Queue from 'bull';\nconst queue = new Queue('email');\nconst other = new Queue('other');\nfunction receive(job) {}\nother.process('welcome', receive);\nqueue.process('other', receive);\n// queue.process('welcome', receive);\nconst text = \"queue.process('welcome', receive)\";\nexport function start() { queue.add('welcome', {}); }",
        |g| assert!(targets(g).is_empty()),
    );
}

#[test]
fn case_336_import_backed_worker_and_shadowing() {
    check(
        "import { Queue, Worker } from 'bullmq';\nconst queue = new Queue('email');\nfunction receive(job) {}\nnew Worker('email', receive);\nexport function start() { queue.add('welcome', {}); }",
        |g| assert_eq!(targets(g), ["receive"]),
    );
    check(
        "import Queue from 'bull';\nconst queue = new Queue('email');\nfunction receive(job) {}\nqueue.process('welcome', receive);\nexport function start(queue) { queue.add('welcome', {}); }",
        |g| assert!(targets(g).is_empty()),
    );
    check(
        "import Queue from './fake';\nconst queue = new Queue('email');\nfunction receive(job) {}\nqueue.process('welcome', receive);\nexport function start() { queue.add('welcome', {}); }",
        |g| assert!(targets(g).is_empty()),
    );
}
#[test]
fn case_336_ambiguous_consumers_are_not_targets() {
    check(
        "import Queue from 'bull';\nconst queue = new Queue('email');\nfunction receive(job) {}\nfunction duplicate(job) {}\nqueue.process('welcome', receive);\nqueue.process('welcome', duplicate);\nexport function start() { queue.add('welcome', {}); }",
        |g| assert!(targets(g).is_empty()),
    );
}
