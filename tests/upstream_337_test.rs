use rustcodegraph::{CodeGraph, mcp::tools::ToolHandler};
use serde_json::json;
struct Project(std::path::PathBuf);
impl Project {
    fn new(label: &str) -> Self {
        let p = Self(std::env::temp_dir().join(format!("rcg337-{}-{label}", std::process::id())));
        std::fs::create_dir_all(&p.0).unwrap();
        p.write("package.json", "{}");
        p
    }
    fn write(&self, path: &str, text: &str) {
        let p = self.0.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    fn graph(&self) -> CodeGraph {
        let mut g = CodeGraph::init_sync(&self.0).unwrap();
        assert!(g.index_all(Default::default()).success);
        g
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn targets(g: &mut CodeGraph, name: &str) -> Vec<String> {
    let n = g
        .get_nodes_in_file("producer.ts")
        .into_iter()
        .find(|n| n.name == name)
        .unwrap();
    g.get_outgoing_edges(&n.id)
        .into_iter()
        .filter(|e| {
            e.metadata
                .as_ref()
                .and_then(|m| m.get("synthesizedBy"))
                .and_then(|v| v.as_str())
                == Some("queue-job")
        })
        .map(|e| g.get_node(&e.target).unwrap().name)
        .collect()
}
#[test]
fn case_337_imports_flow_and_registration_removal() {
    let p = Project::new("flow");
    p.write(
        "queues.ts",
        "import Queue from 'bull';\nexport const emailQueue = new Queue('email');",
    );
    p.write("producer.ts", "import { emailQueue as jobs } from './queues';\nexport function entry() { return publish(); }\nfunction publish() { return jobs.add('welcome', {}); }");
    p.write("consumer.ts", "import { emailQueue } from './queues';\nfunction receive(job) { finish(); }\nfunction finish() {}\nemailQueue.process('welcome', receive);");
    let mut g = p.graph();
    assert_eq!(targets(&mut g, "publish"), ["receive"]);
    let mut h = ToolHandler::new(true);
    h.set_default_code_graph(&g);
    let r = h.execute(
        "rustcodegraph_explore",
        json!({"query":"entry publish finish"}).as_object().unwrap(),
    );
    let text = serde_json::to_value(r).unwrap().to_string();
    let flow = text
        .split("Flow")
        .nth(1)
        .expect(&text)
        .split("## ")
        .next()
        .unwrap();
    for expected in [
        "entry",
        "publish",
        "receive",
        "finish",
        "queue-job",
        "registeredAt",
    ] {
        assert!(flow.contains(expected), "missing {expected}: {text}");
    }
    h.close_all();
    let count = g.get_nodes_in_file("queues.ts").len()
        + g.get_nodes_in_file("producer.ts").len()
        + g.get_nodes_in_file("consumer.ts").len();
    assert!(g.index_all(Default::default()).success);
    assert_eq!(
        count,
        g.get_nodes_in_file("queues.ts").len()
            + g.get_nodes_in_file("producer.ts").len()
            + g.get_nodes_in_file("consumer.ts").len()
    );
    assert_eq!(targets(&mut g, "publish"), ["receive"]);
    // Neither endpoint changes when only the registration/binding file changes.
    p.write(
        "queues.ts",
        "import Queue from 'bull';\nexport const emailQueue = new Queue(queueName);",
    );
    g.sync(Default::default());
    assert!(targets(&mut g, "publish").is_empty());
    p.write(
        "queues.ts",
        "import Queue from 'bull';\nexport const emailQueue = new Queue('email');",
    );
    g.sync(Default::default());
    assert_eq!(targets(&mut g, "publish"), ["receive"]);
    p.write(
        "consumer.ts",
        "function receive(job) { finish(); }\nfunction finish() {}",
    );
    g.sync(Default::default());
    assert!(targets(&mut g, "publish").is_empty());
    g.close();
}
#[test]
fn case_337_scopes_dynamic_and_missing_registration() {
    let p = Project::new("negative");
    p.write(
        "queues.ts",
        "import { Queue } from 'bullmq';\nexport const jobs = new Queue('email');",
    );
    p.write("producer.ts", "import { jobs } from './queues';\nexport function publish() { jobs.add('welcome', {}); }\nexport function dynamic(job) { jobs.add(job, {}); }\nexport function missing() { jobs.add('absent', {}); }");
    p.write("consumer.ts", "import Queue from 'bull';\nconst jobs = new Queue('email');\nfunction receive(job) {}\njobs.process('welcome', receive);");
    p.write("other/package.json", "{}");
    p.write("other/consumer.ts", "import Queue from 'bull';\nconst jobs = new Queue('email');\nfunction wrong(job) {}\njobs.process('welcome', wrong);\njobs.process('absent', wrong);");
    let mut g = p.graph();
    assert_eq!(targets(&mut g, "publish"), ["receive"]);
    for n in ["dynamic", "missing"] {
        assert!(targets(&mut g, n).is_empty());
    }
    g.close();
}
#[test]
fn case_337_imported_worker_handler() {
    let p = Project::new("worker");
    p.write("producer.ts", "import { Queue } from 'bullmq';\nconst queue = new Queue('email');\nexport function publish() { queue.add('welcome', {}); }");
    p.write(
        "handler.ts",
        "export function receive(job) { finish(); }\nfunction finish() {}",
    );
    p.write("worker.ts", "import { Worker } from 'bullmq';\nimport { receive as handle } from './handler';\nnew Worker('email', handle);");
    let mut g = p.graph();
    assert_eq!(targets(&mut g, "publish"), ["receive"]);
    let start = g
        .get_nodes_in_file("producer.ts")
        .into_iter()
        .find(|n| n.name == "publish")
        .unwrap();
    let edge = g
        .get_outgoing_edges(&start.id)
        .into_iter()
        .find(|e| {
            e.metadata
                .as_ref()
                .and_then(|m| m.get("synthesizedBy"))
                .and_then(|v| v.as_str())
                == Some("queue-job")
        })
        .unwrap();
    assert_eq!(edge.metadata.unwrap()["registeredAt"], "worker.ts:3");
    g.close();
}

#[test]
fn case_337_private_shadowed_and_cross_package_imports() {
    let p = Project::new("binding-negatives");
    p.write("queues.ts", "import Queue from 'bull';\nconst hidden = new Queue('email');\nexport const jobs = new Queue('email');");
    p.write("other/package.json", "{}");
    p.write(
        "other/queues.ts",
        "import Queue from 'bull';\nexport const foreign = new Queue('email');",
    );
    p.write("producer.ts", "import { hidden, jobs } from './queues';\nimport { foreign } from './other/queues';\nexport function privateQueue() { hidden.add('welcome', {}); }\nexport function shadowed(jobs) { jobs.add('welcome', {}); }\nexport function crossApp() { foreign.add('welcome', {}); }\nexport function dynamic() { jobs.add(`welcome`, {}); }\nexport const arrow = jobs => jobs.add('welcome', {});");
    p.write("consumer.ts", "import Queue from 'bull';\nconst jobs = new Queue('email');\nfunction receive(job) {}\njobs.process('welcome', receive);");
    let mut g = p.graph();
    for n in ["privateQueue", "shadowed", "crossApp", "dynamic", "arrow"] {
        assert!(targets(&mut g, n).is_empty(), "{n}");
    }
    g.close();
}

#[test]
fn case_337_invalid_handler_bindings() {
    for (label, body) in [
        ("reassigned", "function receive(job) {}\nreceive = unknown;"),
        ("nested", "function setup() { function receive(job) {} }"),
        ("dynamic", "const receive = chooseHandler();"),
    ] {
        let p = Project::new(label);
        p.write("producer.ts", "import { Queue } from 'bullmq';\nconst queue = new Queue('email');\nexport function publish() { queue.add('welcome', {}); }");
        p.write(
            "worker.ts",
            &format!("import {{ Worker }} from 'bullmq';\n{body}\nnew Worker('email', receive);"),
        );
        let mut g = p.graph();
        assert!(targets(&mut g, "publish").is_empty(), "{label}");
        g.close();
    }
}
