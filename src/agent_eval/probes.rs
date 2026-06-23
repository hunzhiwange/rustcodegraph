//! Deterministic probes for rustcodegraph MCP tools.
//!
//! 与 agent A/B 不同，probe 直接在当前进程里调用 ToolHandler，排除模型选择和提示词波动。
//! 它们用来验证某个 indexed repo 的工具输出是否包含关键结构信号。

use std::collections::HashSet;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::time::Instant;

use serde_json::{Map, Value};

use crate::CodeGraph;
use crate::mcp::tools::ToolHandler;

use super::formatting::{left_pad, option_value, pad, regex_is_match, truncate_chars};
use super::types::{ProbeSignals, SweepRow, SweepSubject};

pub fn probe_explore_text(repo: &Path, query: &str) -> Result<String, String> {
    let mut args = Map::new();
    args.insert("query".to_string(), Value::String(query.to_string()));
    execute_probe_tool(repo, "rustcodegraph_explore", args)
}

pub fn probe_node_text(repo: &Path, symbol: &str, include_code: bool) -> Result<String, String> {
    let mut args = Map::new();
    args.insert("symbol".to_string(), Value::String(symbol.to_string()));
    args.insert("includeCode".to_string(), Value::Bool(include_code));
    execute_probe_tool(repo, "rustcodegraph_node", args)
}

pub fn probe_sweep_report(args: &[String]) -> Result<String, String> {
    let tool = option_value(args, "--tool").unwrap_or_else(|| "explore".to_string());
    if tool != "explore" {
        return Err(
            "probe-sweep only supports --tool=explore; context and trace are retired".to_string(),
        );
    }
    let filter_repos = option_value(args, "--repos").map(|value| {
        // 允许只跑一个子集，便于调试某个 framework 时不用扫完整 corpus。
        value
            .split(',')
            .map(|item| item.to_string())
            .collect::<HashSet<_>>()
    });
    let subjects = sweep_subjects()
        .into_iter()
        .filter(|subject| {
            filter_repos
                .as_ref()
                .is_none_or(|repos| repos.contains(subject.id))
        })
        .collect::<Vec<_>>();

    let started = Instant::now();
    let mut rows = Vec::new();
    // 每个 subject 独立记录错误；一次未索引或探针失败不应遮住其他仓库的信号。
    for subject in &subjects {
        let probe_started = Instant::now();
        match probe_explore_text(Path::new(subject.repo), subject.query) {
            Ok(text) => rows.push(SweepRow {
                id: subject.id.to_string(),
                ms: probe_started.elapsed().as_millis() as u64,
                chars: text.chars().count(),
                lines: text.lines().count(),
                signals: detect_probe_signals(&text),
                error: None,
            }),
            Err(err) => rows.push(SweepRow {
                id: subject.id.to_string(),
                ms: 0,
                chars: 0,
                lines: 0,
                signals: ProbeSignals::default(),
                error: Some(truncate_chars(&err, 80)),
            }),
        }
    }

    let mut out = String::new();
    writeln!(
        out,
        "=== probe-sweep tool={tool} n={} ({}ms total) ===",
        subjects.len(),
        started.elapsed().as_millis()
    )
    .unwrap();
    writeln!(out, "  id            chars  lines    ms signals").unwrap();
    writeln!(out, "  {}", "-".repeat(56)).unwrap();
    for row in &rows {
        if let Some(error) = &row.error {
            writeln!(out, "  {} ERROR: {error}", pad(&row.id, 13)).unwrap();
        } else {
            writeln!(
                out,
                "  {} {}c {}L {}ms {}{}{}{}{}",
                pad(&row.id, 13),
                left_pad(&row.chars.to_string(), 6),
                left_pad(&row.lines.to_string(), 4),
                left_pad(&row.ms.to_string(), 4),
                if row.signals.has_entry_points {
                    "EP "
                } else {
                    "   "
                },
                if row.signals.has_flow_trace {
                    "TRC "
                } else {
                    "    "
                },
                if row.signals.has_route_manifest {
                    "MAN "
                } else {
                    "    "
                },
                if row.signals.has_top_handler {
                    "HND "
                } else {
                    "    "
                },
                if row.signals.has_small_repo_tail {
                    "TAIL"
                } else {
                    "    "
                }
            )
            .unwrap();
        }
    }

    let mut sizes = rows
        .iter()
        .filter(|row| row.error.is_none())
        .map(|row| row.chars)
        .collect::<Vec<_>>();
    sizes.sort_unstable();
    let median_size = sizes.get(sizes.len() / 2).copied().unwrap_or(0);
    let sum = sizes.iter().sum::<usize>();
    let ok_rows = rows.iter().filter(|row| row.error.is_none()).count();
    writeln!(out, "  {}", "-".repeat(64)).unwrap();
    writeln!(
        out,
        "  median={median_size}c  total={sum}c  manifest={}/{}  top-handler={}/{}",
        rows.iter()
            .filter(|row| row.error.is_none() && row.signals.has_route_manifest)
            .count(),
        ok_rows,
        rows.iter()
            .filter(|row| row.error.is_none() && row.signals.has_top_handler)
            .count(),
        ok_rows
    )
    .unwrap();
    Ok(out)
}

fn execute_probe_tool(repo: &Path, tool: &str, args: Map<String, Value>) -> Result<String, String> {
    let root = fs::canonicalize(repo)
        .map_err(|err| format!("failed to resolve {}: {err}", repo.display()))?;
    if !CodeGraph::is_initialized(&root) {
        return Err(format!("{} is not indexed", root.display()));
    }
    // 复用真实 MCP ToolHandler，而不是走 CLI 文本层，确保 probe 覆盖的就是 agent 会调用的路径。
    let mut handler = ToolHandler::new(true);
    handler.set_default_project_root(root.to_string_lossy().into_owned());
    let result = handler.execute(tool, &args);
    let text = result
        .content
        .first()
        .map(|content| content.text.clone())
        .unwrap_or_else(|| "(no text)".to_string());
    if result.is_error == Some(true) {
        Err(text)
    } else {
        Ok(text)
    }
}

fn detect_probe_signals(text: &str) -> ProbeSignals {
    // 这些标题是 explore 输出的高层契约，能快速发现报告结构或预算裁剪造成的退化。
    ProbeSignals {
        has_entry_points: regex_is_match(r"(?m)^### Entry Points", text),
        has_flow_trace: regex_is_match(r"(?m)^## Inline flow trace", text),
        has_route_manifest: regex_is_match(r"(?m)^## Routing manifest", text),
        has_top_handler: regex_is_match(r"(?m)^### Top handler file", text),
        has_small_repo_tail: text.contains("This project is small"),
    }
}

fn sweep_subjects() -> Vec<SweepSubject> {
    // corpus 路径是本地 agent-eval 约定；不存在的仓库会在 sweep 中以单行 ERROR 呈现。
    vec![
        SweepSubject {
            id: "gin-rw",
            repo: "/tmp/codegraph-corpus/gin-realworld",
            query: "How does this Gin app route a request through its middleware chain to a handler?",
        },
        SweepSubject {
            id: "go-mux",
            repo: "/tmp/codegraph-corpus/go-mux",
            query: "How does this gorilla/mux app route a request to its handler?",
        },
        SweepSubject {
            id: "fastapi-rw",
            repo: "/tmp/codegraph-corpus/fastapi-realworld",
            query: "How does FastAPI route a request through its dependencies to a handler?",
        },
        SweepSubject {
            id: "spring-pc",
            repo: "/tmp/codegraph-corpus/spring-petclinic",
            query: "How does Spring route an HTTP request to a controller method?",
        },
        SweepSubject {
            id: "axum-rw",
            repo: "/tmp/codegraph-corpus/rust-axum-realworld",
            query: "How does Axum route a request to its handler in this app?",
        },
        SweepSubject {
            id: "express-rw",
            repo: "/tmp/codegraph-corpus/express-realworld",
            query: "How does this Express app route a request through middleware to a handler?",
        },
        SweepSubject {
            id: "kotlin-pc",
            repo: "/tmp/codegraph-corpus/kotlin-petclinic",
            query: "How does the Kotlin Spring app route an HTTP request to its handler?",
        },
        SweepSubject {
            id: "flask-mb",
            repo: "/tmp/codegraph-corpus/flask-microblog",
            query: "How does this Flask app route a request to a view function?",
        },
        SweepSubject {
            id: "vapor-tpl",
            repo: "/tmp/codegraph-corpus/vapor-template",
            query: "How does Vapor route an HTTP request to its handler?",
        },
        SweepSubject {
            id: "cpp-leveldb",
            repo: "/tmp/codegraph-corpus/cpp-leveldb",
            query: "How does LevelDB handle a Put operation through to disk?",
        },
        SweepSubject {
            id: "lualine",
            repo: "/tmp/codegraph-corpus/lualine.nvim",
            query: "How does lualine assemble and render the statusline?",
        },
        SweepSubject {
            id: "drupal-admin",
            repo: "/tmp/codegraph-corpus/drupal-admintoolbar",
            query: "How does the Drupal admin toolbar module render its toolbar?",
        },
        SweepSubject {
            id: "svelte-rw",
            repo: "/tmp/codegraph-corpus/svelte-realworld",
            query: "How does this SvelteKit app route a request to a handler?",
        },
        SweepSubject {
            id: "react-rw",
            repo: "/tmp/codegraph-corpus/react-realworld",
            query: "How does this React app fetch and display articles?",
        },
        SweepSubject {
            id: "rails-rw",
            repo: "/tmp/codegraph-corpus/rails-realworld",
            query: "How does Rails route a request to a controller action?",
        },
        SweepSubject {
            id: "flask-rest",
            repo: "/tmp/codegraph-corpus/flask-restful-realworld",
            query: "How does Flask-RESTful route a request to a resource method?",
        },
        SweepSubject {
            id: "laravel-rw",
            repo: "/tmp/codegraph-corpus/laravel-realworld",
            query: "How does Laravel route a request to the controller method?",
        },
        SweepSubject {
            id: "aspnet-rw",
            repo: "/tmp/codegraph-corpus/aspnet-realworld",
            query: "How does ASP.NET route a request to the controller action?",
        },
        SweepSubject {
            id: "cobra",
            repo: "/tmp/codegraph-corpus/cobra",
            query: "How does cobra parse commands and flags?",
        },
        SweepSubject {
            id: "sinatra",
            repo: "/tmp/codegraph-corpus/sinatra",
            query: "How does sinatra route a request to its handler?",
        },
        SweepSubject {
            id: "slim",
            repo: "/tmp/codegraph-corpus/slim",
            query: "How does slim route a request and apply middleware?",
        },
    ]
}
