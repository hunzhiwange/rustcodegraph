use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use rustcodegraph::{CodeGraph, IndexOptions};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

struct TempProject {
    path: PathBuf,
}

impl TempProject {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after the Unix epoch")
            .as_nanos();
        let sequence = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "rustcodegraph-utf8-safety-{}-{nonce}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("temporary project should be created");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn write(&self, relative: &str, source: &str) {
        let path = self.path.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("source parent directory should be created");
        }
        fs::write(path, source).expect("source file should be written");
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[test]
fn indexes_multilingual_sources_across_fixed_budget_boundaries() {
    let project = TempProject::new();

    let tsx_padding = "a".repeat(399);
    project.write(
        "src/routes.tsx",
        &format!(
            "const App = () => <Routes><Route{tsx_padding}🚀 path=\"/首页\" element={{<Home />}} /></Routes>;"
        ),
    );

    let java_padding = " ".repeat(599);
    project.write(
        "src/main/java/example/HomeController.java",
        &format!(
            "package example;\n@RestController\nclass HomeController {{\n@RequestMapping(\"/首页\"){java_padding}中\npublic String home() {{ return \"ok\"; }}\n}}"
        ),
    );

    let csharp_padding = " ".repeat(599);
    project.write(
        "Controllers/HomeController.cs",
        &format!(
            "[ApiController]\nclass HomeController {{\n[HttpGet(\"/首页\")]{csharp_padding}🚀\npublic string Home() {{ return \"ok\"; }}\n}}"
        ),
    );

    let rust_padding = " ".repeat(499);
    project.write(
        "src/server.rs",
        &format!(
            "fn configure(cfg: &mut web::ServiceConfig) {{ cfg.service(web::resource(\"/首页\"){rust_padding}中.route(web::get().to(home))); }}\nfn home() {{}}"
        ),
    );

    project.write(
        "lib/model.dart",
        &format!(
            "const greeting = \"{}中🚀\";\nvoid main() {{ print(greeting); }}",
            "a".repeat(99)
        ),
    );

    project.write(
        "include/example.h",
        &format!(
            "{}🚀\nnamespace 示例 {{ class Widget {{}}; }}",
            "/".repeat(8191)
        ),
    );

    let mut graph = CodeGraph::init_sync(project.path()).expect("CodeGraph should initialize");
    let result = graph.index_all(IndexOptions::default());

    assert!(result.success, "index errors: {:?}", result.errors);
    assert_eq!(
        result.files_indexed, 6,
        "all multilingual fixtures should index"
    );
    assert_eq!(result.files_errored, 0, "no source should fail extraction");
    graph.destroy();
}
