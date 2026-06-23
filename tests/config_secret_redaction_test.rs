//! Config secret redaction (#383).
//!
//! Rust port of `__tests__/config-secret-redaction.test.ts`.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use rustcodegraph::mcp::tools::{ToolHandler, ToolResult};
use rustcodegraph::types::{Language, Node, NodeKind};
use rustcodegraph::{CodeGraph, IndexOptions};
use serde_json::{Map, Value, json};

const SECRET: &str = "sk-live-DO-NOT-LEAK-2f9a4c7e1b";
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

struct TempProject {
    root: PathBuf,
}

impl TempProject {
    fn new(prefix: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after the Unix epoch")
            .as_nanos();
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("{prefix}-{}-{nanos}-{counter}", std::process::id()));
        fs::create_dir_all(&root)
            .unwrap_or_else(|err| panic!("failed to create temp root {}: {err}", root.display()));
        Self { root }
    }

    fn path(&self) -> &Path {
        &self.root
    }

    fn mkdir(&self, relative_path: &str) {
        let path = self.root.join(relative_path);
        fs::create_dir_all(&path)
            .unwrap_or_else(|err| panic!("failed to create {}: {err}", path.display()));
    }

    fn write(&self, relative_path: &str, content: &str) {
        let path = self.root.join(relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .unwrap_or_else(|err| panic!("failed to create {}: {err}", parent.display()));
        }
        fs::write(&path, content)
            .unwrap_or_else(|err| panic!("failed to write {}: {err}", path.display()));
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

struct Fixture {
    _temp: TempProject,
    cg: CodeGraph,
    handler: ToolHandler,
}

impl Fixture {
    fn new() -> Self {
        let temp = TempProject::new("cg-config-secret");
        let java_dir = "src/main/java/com/example";
        let res_dir = "src/main/resources";
        temp.mkdir(java_dir);
        temp.mkdir(res_dir);

        // pom.xml triggers Spring detection so the resolver parses the config files.
        temp.write(
            "pom.xml",
            "<project><dependencies><dependency><groupId>org.springframework.boot</groupId><artifactId>spring-boot-starter</artifactId></dependency></dependencies></project>\n",
        );
        temp.write(
            &format!("{res_dir}/application.properties"),
            &format!("server.port=8080\nspring.datasource.password={SECRET}\n"),
        );
        temp.write(
            &format!("{res_dir}/application.yml"),
            &format!("app:\n  api:\n    key: \"{SECRET}\"\n"),
        );
        temp.write(
            &format!("{java_dir}/DataConfig.java"),
            "package com.example;\n\
             import org.springframework.beans.factory.annotation.Value;\n\
             public class DataConfig {\n\
               @Value(\"${spring.datasource.password}\") private String dbPass;\n\
               @Value(\"${app.api.key}\") private String apiKey;\n\
             }\n",
        );

        let mut cg = CodeGraph::init_sync(temp.path()).expect("CodeGraph should initialize");
        let result = cg.index_all(IndexOptions::default());
        assert!(result.success, "indexing failed: {:?}", result.errors);
        let mut handler = ToolHandler::new(true);
        handler.set_default_code_graph(&cg);

        Self {
            _temp: temp,
            cg,
            handler,
        }
    }

    fn config_keys(&mut self) -> Vec<Node> {
        self.cg
            .get_nodes_by_kind(NodeKind::Constant)
            .into_iter()
            .filter(|node| node.language == Language::Yaml || node.language == Language::Properties)
            .collect()
    }

    fn execute_text(&mut self, tool: &str, args: Map<String, Value>) -> String {
        text(&self.handler.execute(tool, &args))
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.handler.close_all();
        self.cg.destroy();
    }
}

fn text(result: &ToolResult) -> String {
    assert!(
        result.is_error != Some(true),
        "tool should not fail: {:?}",
        result
    );
    result
        .content
        .iter()
        .map(|content| content.text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

fn explore_args(query: &str) -> Map<String, Value> {
    let mut args = Map::new();
    args.insert("query".to_string(), json!(query));
    args
}

fn node_args(symbol: &str, include_code: bool) -> Map<String, Value> {
    let mut args = Map::new();
    args.insert("symbol".to_string(), json!(symbol));
    args.insert("includeCode".to_string(), json!(include_code));
    args
}

mod config_secret_redaction_383 {
    use super::*;

    #[test]
    fn still_indexes_config_keys_as_nodes_resolution_must_not_regress() {
        let mut fixture = Fixture::new();
        let keys = fixture.config_keys();
        let by_qn = |qn: &str| keys.iter().find(|node| node.qualified_name == qn);

        assert!(
            by_qn("spring.datasource.password").is_some(),
            ".properties key indexed"
        );
        assert!(by_qn("app.api.key").is_some(), "yaml key indexed");
    }

    #[test]
    fn never_stores_the_secret_value_in_node_metadata_docstring_signature_name() {
        let mut fixture = Fixture::new();
        let keys = fixture.config_keys();
        assert!(!keys.is_empty(), "fixture should produce config key nodes");

        for node in keys {
            assert!(
                !node.docstring.as_deref().unwrap_or("").contains(SECRET),
                "docstring of {} should not contain the secret",
                node.qualified_name
            );
            assert!(
                !node.signature.as_deref().unwrap_or("").contains(SECRET),
                "signature of {} should not contain the secret",
                node.qualified_name
            );
            assert!(
                !node.name.contains(SECRET),
                "name of {} should not contain the secret",
                node.qualified_name
            );
        }
    }

    #[test]
    fn codegraph_explore_surfaces_the_config_key_but_never_the_secret_value() {
        let mut fixture = Fixture::new();
        let text = fixture.execute_text(
            "rustcodegraph_explore",
            explore_args("DataConfig dbPass apiKey spring.datasource.password app.api.key"),
        );

        assert!(
            text.contains("password"),
            "the key should be in scope: {text}"
        );
        assert!(
            !text.contains(SECRET),
            "the secret value must not be dumped: {text}"
        );
    }

    #[test]
    fn codegraph_node_include_code_returns_the_key_not_the_secret_value() {
        let mut fixture = Fixture::new();
        let text = fixture.execute_text(
            "rustcodegraph_node",
            node_args("spring.datasource.password", true),
        );

        assert!(
            text.contains("password"),
            "the node should be found: {text}"
        );
        assert!(
            !text.contains(SECRET),
            "the value should be redacted from the code path: {text}"
        );
    }
}
