//! Same-named symbols across monorepo apps (#764).
//!
//! A NestJS-style monorepo has one `UserService` (and friends) per app. The
//! graph keeps them as distinct nodes (import + proximity resolution), but the
//! MCP tools used to AGGREGATE them: callers/callees returned one merged list
//! and impact merged both blast radii.
//!
//! This is the Rust port of `__tests__/same-name-disambiguation.test.ts`.
//! Tool-output cases exercise the Rust MCP graph-query facade directly so
//! callers/callees/impact stay definition-scoped.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use rustcodegraph::mcp::tools::{ToolHandler, ToolResult};
use rustcodegraph::{CodeGraph, IndexOptions};
use serde_json::{Map, Value, json};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

struct TempProject {
    root: PathBuf,
}

impl TempProject {
    fn new() -> Self {
        let base = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after Unix epoch")
            .as_nanos();
        for _ in 0..100 {
            let seq = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
            let root =
                std::env::temp_dir().join(format!("cg-764-{}-{base}-{seq}", std::process::id()));
            match fs::create_dir(&root) {
                Ok(()) => return Self { root },
                Err(err) if err.kind() == ErrorKind::AlreadyExists => continue,
                Err(err) => panic!("failed to create temp root {}: {err}", root.display()),
            }
        }
        panic!("failed to allocate unique temp root for same-name disambiguation test");
    }

    fn path(&self) -> &Path {
        &self.root
    }

    fn mk(&self, rel: &str, content: &str) {
        let path = self.root.join(rel);
        fs::create_dir_all(path.parent().expect("fixture path should have a parent"))
            .unwrap_or_else(|err| {
                panic!(
                    "failed to create fixture parent for {}: {err}",
                    path.display()
                )
            });
        fs::write(&path, content)
            .unwrap_or_else(|err| panic!("failed to write fixture {}: {err}", path.display()));
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
        let temp = TempProject::new();

        for app in ["billing", "admin"] {
            temp.mk(
                &format!("apps/{app}/src/users/user.service.ts"),
                &[
                    "import { UserRepository } from './user.repository';",
                    "export class UserService {",
                    "  constructor(private readonly repo: UserRepository) {}",
                    "  findAll(): string[] {",
                    &format!("    return this.repo.load_{app}();"),
                    "  }",
                    "}",
                ]
                .join("\n"),
            );
            temp.mk(
                &format!("apps/{app}/src/users/user.repository.ts"),
                &format!(
                    "export class UserRepository {{\n  load_{app}(): string[] {{ return []; }}\n}}\n"
                ),
            );
            temp.mk(
                &format!("apps/{app}/src/users/user.controller.ts"),
                &[
                    "import { UserService } from './user.service';",
                    "export class UserController {",
                    "  constructor(private readonly users: UserService) {}",
                    "  list(): string[] { return this.users.findAll(); }",
                    "}",
                ]
                .join("\n"),
            );
            temp.mk(
                &format!("apps/{app}/src/users/user.audit.ts"),
                &[
                    "import { UserService } from './user.service';",
                    "export class UserAudit {",
                    "  constructor(private readonly users: UserService) {}",
                    "  snapshot(): string[] { return this.users.findAll(); }",
                    "}",
                ]
                .join("\n"),
            );
        }

        let mut cg = CodeGraph::init_sync(temp.path()).expect("failed to initialize CodeGraph");
        let result = cg.index_all(IndexOptions::default());
        assert!(
            result.success,
            "index_all should succeed, errors: {:?}",
            result.errors
        );
        let mut handler = ToolHandler::new(true);
        handler.set_default_code_graph(&cg);

        Self {
            _temp: temp,
            cg,
            handler,
        }
    }

    fn text(&mut self, tool: &str, args: Map<String, Value>) -> String {
        let result = self.handler.execute(tool, &args);
        first_text(&result).to_string()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.handler.close_all();
        self.cg.destroy();
    }
}

fn first_text(result: &ToolResult) -> &str {
    result
        .content
        .first()
        .map(|content| content.text.as_str())
        .unwrap_or("")
}

fn symbol_args(symbol: &str) -> Map<String, Value> {
    let mut args = Map::new();
    args.insert("symbol".to_string(), json!(symbol));
    args
}

fn symbol_file_args(symbol: &str, file: &str) -> Map<String, Value> {
    let mut args = symbol_args(symbol);
    args.insert("file".to_string(), json!(file));
    args
}

fn section_until_next_definition(out: &str, marker: &str) -> String {
    let Some(start) = out.find(marker) else {
        return String::new();
    };
    let section = &out[start..];
    let end = section
        .get(3..)
        .and_then(|tail| tail.find("###").map(|idx| idx + 3))
        .unwrap_or(section.len());
    section[..end].to_string()
}

mod same_named_symbols_across_apps_764 {
    use super::*;

    #[test]
    fn graph_keeps_the_apps_apart_no_cross_app_edges_at_all() {
        let mut fixture = Fixture::new();
        let billing = fixture
            .cg
            .get_nodes_by_name("findAll")
            .into_iter()
            .filter(|node| node.file_path.contains("billing"))
            .map(|node| node.id)
            .collect::<Vec<_>>();

        for id in billing {
            for edge in fixture.cg.get_incoming_edges(&id) {
                let src = fixture.cg.get_node(&edge.source);
                assert!(
                    !src.as_ref()
                        .is_some_and(|node| node.file_path.contains("admin")),
                    "incoming edge from admin node: {src:?}"
                );
            }
        }
    }

    #[test]
    fn callers_one_section_per_distinct_definition_each_with_only_its_own_callers() {
        let mut fixture = Fixture::new();
        let out = fixture.text("rustcodegraph_callers", symbol_args("findAll"));

        assert!(out.contains("2 distinct definitions"));
        // Section per definition...
        assert!(out.contains("apps/admin/src/users/user.service.ts"));
        assert!(out.contains("apps/billing/src/users/user.service.ts"));
        // ...and the billing section must list the billing controller, not admin's.
        let billing_body =
            section_until_next_definition(&out, "apps/billing/src/users/user.service.ts");
        assert!(billing_body.contains("apps/billing/src/users/user.controller.ts"));
        assert!(!billing_body.contains("apps/admin/src/users/user.controller.ts"));
    }

    #[test]
    fn callers_file_narrows_to_one_definition_flat_list_no_stale_aggregation_note() {
        let mut fixture = Fixture::new();
        let out = fixture.text(
            "rustcodegraph_callers",
            symbol_file_args("findAll", "apps/billing/src/users/user.service.ts"),
        );

        assert!(!out.contains("distinct definitions"));
        assert!(out.contains("apps/billing/src/users/user.controller.ts"));
        assert!(!out.contains("apps/admin/"));
        assert!(!out.contains("Aggregated results"));
    }

    #[test]
    fn callers_limit_caps_each_definition_output() {
        let mut fixture = Fixture::new();
        let mut args = symbol_file_args("findAll", "apps/billing/src/users/user.service.ts");
        args.insert("limit".to_string(), json!(1));
        let out = fixture.text("rustcodegraph_callers", args);

        let caller_rows = out
            .lines()
            .filter(|line| line.starts_with("- ") && line.contains(" via "))
            .count();
        assert_eq!(caller_rows, 1, "{out}");
    }

    #[test]
    fn callers_a_non_matching_file_falls_back_to_all_definitions_with_a_note() {
        let mut fixture = Fixture::new();
        let out = fixture.text(
            "rustcodegraph_callers",
            symbol_file_args("findAll", "apps/nonexistent/x.ts"),
        );

        assert!(out.contains("no definition of \"findAll\" matches file"));
        assert!(out.contains("2 distinct definitions"));
    }

    #[test]
    fn impact_separate_blast_radius_per_definition_never_a_merged_one() {
        let mut fixture = Fixture::new();
        let out = fixture.text("rustcodegraph_impact", symbol_args("UserService"));

        assert!(out.contains("2 distinct definitions"));
        // Each section's count covers ONE app (service + ctor + findAll +
        // controller side, plus same-app repository dependencies when cross-file
        // resolution can see them), not the union of both apps.
        let re = regex::Regex::new(r"affects (\d+) symbols").expect("regex should compile");
        let counts = re
            .captures_iter(&out)
            .map(|captures| {
                captures
                    .get(1)
                    .expect("count capture should exist")
                    .as_str()
                    .parse::<usize>()
                    .expect("count capture should be numeric")
            })
            .collect::<Vec<_>>();
        assert_eq!(counts.len(), 2);
        for count in counts {
            assert!(count <= 10);
        }
    }

    #[test]
    fn impact_lists_dependents_not_downstream_callees() {
        let mut fixture = Fixture::new();
        let out = fixture.text(
            "rustcodegraph_impact",
            symbol_file_args("findAll", "apps/billing/src/users/user.service.ts"),
        );

        assert!(
            out.contains("apps/billing/src/users/user.controller.ts"),
            "{out}"
        );
        assert!(
            !out.contains("load_billing"),
            "impact should not include functions called by findAll:\n{out}"
        );
    }

    #[test]
    fn callees_grouped_the_same_way() {
        let mut fixture = Fixture::new();
        let out = fixture.text("rustcodegraph_callees", symbol_args("list"));

        assert!(out.contains("2 distinct definitions"));
    }
}
