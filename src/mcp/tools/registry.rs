//! MCP tool registry and schema helpers.
//!
//! 这里定义 agent 能看到的工具面、参数 schema 和按项目规模注入的 explore 预算。
//! 描述文字本身会影响宿主模型的工具选择，所以修改时要和 `server_instructions`
//! 里的推荐路径保持一致。

use std::collections::HashSet;
use std::env;

use serde_json::Value;

use super::budget::get_explore_budget;
use super::{DEFAULT_MCP_TOOLS, InputSchema, MCP_TOOLS_ENV, ToolDefinition};

pub fn tools() -> Vec<ToolDefinition> {
    vec![
        tool(
            "rustcodegraph_search",
            "Quick symbol search by name. Returns locations only (no code). Use rustcodegraph_explore instead to get the actual source / understand an area in one call.",
            props(&[
                string_prop(
                    "query",
                    "Symbol name or partial name (e.g., \"auth\", \"signIn\", \"UserService\")",
                ),
                enum_prop(
                    "kind",
                    "Filter by node kind",
                    &[
                        "function",
                        "method",
                        "class",
                        "interface",
                        "type",
                        "variable",
                        "route",
                        "component",
                    ],
                ),
                number_prop("limit", "Maximum results (default: 10)", Some(10)),
                project_path_prop(),
            ]),
            Some(vec!["query"]),
        ),
        tool(
            "rustcodegraph_callers",
            "List functions that call <symbol>. For the full flow, use rustcodegraph_explore.",
            props(&[
                string_prop(
                    "symbol",
                    "Name of the function, method, or class to find callers for",
                ),
                string_prop(
                    "file",
                    "Narrow to the definition in this file (path or suffix) when several same-named symbols exist",
                ),
                number_prop(
                    "limit",
                    "Maximum number of callers to return (default: 20)",
                    Some(20),
                ),
                project_path_prop(),
            ]),
            Some(vec!["symbol"]),
        ),
        tool(
            "rustcodegraph_callees",
            "List functions that <symbol> calls. For the full flow, use rustcodegraph_explore.",
            props(&[
                string_prop(
                    "symbol",
                    "Name of the function, method, or class to find callees for",
                ),
                string_prop(
                    "file",
                    "Narrow to the definition in this file (path or suffix) when several same-named symbols exist",
                ),
                number_prop(
                    "limit",
                    "Maximum number of callees to return (default: 20)",
                    Some(20),
                ),
                project_path_prop(),
            ]),
            Some(vec!["symbol"]),
        ),
        tool(
            "rustcodegraph_impact",
            "List symbols affected by changing <symbol>. Use before a refactor.",
            props(&[
                string_prop("symbol", "Name of the symbol to analyze impact for"),
                string_prop(
                    "file",
                    "Narrow to the definition in this file (path or suffix) when several same-named symbols exist",
                ),
                number_prop(
                    "depth",
                    "How many levels of dependencies to traverse (default: 2)",
                    Some(2),
                ),
                project_path_prop(),
            ]),
            Some(vec!["symbol"]),
        ),
        tool(
            "rustcodegraph_node",
            "Two modes. (1) READ A FILE: pass file with no symbol and it returns current source with line numbers, narrowable with offset/limit like Read, plus dependents. (2) ONE SYMBOL: location, signature, optional source, and caller/callee trail. For ambiguous names it returns every matching definition body in one call.",
            props(&[
                string_prop(
                    "symbol",
                    "Name of the symbol to read. Omit and pass file alone to read a whole file.",
                ),
                bool_prop(
                    "includeCode",
                    "Symbol mode: include the symbol's full body (default: false).",
                    Some(false),
                ),
                string_prop(
                    "file",
                    "A file path or basename. Alone, reads the file; with symbol, disambiguates an overloaded name.",
                ),
                number_prop(
                    "offset",
                    "File mode: 1-based line to start reading from, exactly like Read's offset.",
                    None,
                ),
                number_prop(
                    "limit",
                    "File mode: maximum number of lines to return, exactly like Read's limit.",
                    None,
                ),
                bool_prop(
                    "symbolsOnly",
                    "File mode: return just the file's symbol map + dependents.",
                    Some(false),
                ),
                number_prop(
                    "line",
                    "Symbol mode only: disambiguate to the definition at/around this line.",
                    None,
                ),
                project_path_prop(),
            ]),
            None,
        ),
        tool(
            "rustcodegraph_explore",
            "PRIMARY TOOL - call FIRST for almost any question OR before an edit. Returns relevant verbatim source grouped by file in one capped call, plus the call path among named symbols.",
            props(&[
                string_prop(
                    "query",
                    "Symbol names, file names, or short code terms to explore. For a flow question, name the symbols spanning the flow.",
                ),
                number_prop(
                    "maxFiles",
                    "Maximum number of files to include source code from (default: 12)",
                    Some(12),
                ),
                project_path_prop(),
            ]),
            Some(vec!["query"]),
        ),
        tool(
            "rustcodegraph_status",
            "Index health check (files / nodes / edges). Skip unless debugging.",
            props(&[project_path_prop()]),
            None,
        ),
        tool(
            "rustcodegraph_files",
            "Indexed file tree with language + symbol counts. Faster than Glob for project layout.",
            props(&[
                string_prop(
                    "path",
                    "Filter to files under this directory path. Returns all files if not specified.",
                ),
                string_prop("pattern", "Filter files matching this glob pattern."),
                enum_prop("format", "Output format", &["tree", "flat", "grouped"]),
                bool_prop(
                    "includeMetadata",
                    "Include file metadata like language and symbol count (default: true)",
                    Some(true),
                ),
                number_prop(
                    "maxDepth",
                    "Maximum directory depth to show (default: unlimited)",
                    None,
                ),
                project_path_prop(),
            ]),
            None,
        ),
    ]
}

pub fn get_static_tools() -> Vec<ToolDefinition> {
    filter_tools(tools(), None)
}

pub(super) fn filter_tools(
    mut all: Vec<ToolDefinition>,
    file_count: Option<usize>,
) -> Vec<ToolDefinition> {
    let allow = tool_allowlist();
    if let Some(allow) = allow {
        // 显式 allowlist 是调试和实验开关；接受短名和完整名，便于用户从文档
        // 或 MCP payload 中直接复制。
        all.retain(|tool| allow.contains(&short_tool_name(&tool.name)));
    } else {
        // 默认工具面保持很小，降低 agent 在小仓库里误选低价值工具的概率。
        let default = DEFAULT_MCP_TOOLS
            .iter()
            .map(|name| name.to_string())
            .collect::<HashSet<_>>();
        all.retain(|tool| default.contains(&short_tool_name(&tool.name)));
    }

    if let Some(file_count) = file_count {
        let budget = get_explore_budget(file_count);
        for tool in &mut all {
            if tool.name == "rustcodegraph_explore" {
                // 预算放进 tool description，而不是另开工具或要求 agent 记忆规则；
                // 这样它沿用已经会调用的 explore 工具即可。
                tool.description = format!(
                    "{} Budget: make at most {} calls for this project ({} files indexed).",
                    tool.description, budget, file_count
                );
            }
        }
    }
    all
}

pub(super) fn tool_allowlist() -> Option<HashSet<String>> {
    let raw = mcp_tools_env_raw().ok()?;
    if raw.trim().is_empty() {
        return None;
    }
    let set = raw
        .split(',')
        .map(|item| short_tool_name(item.trim()))
        .filter(|item| !item.is_empty())
        .collect::<HashSet<_>>();
    if set.is_empty() { None } else { Some(set) }
}

pub(super) fn is_tool_allowed(name: &str) -> bool {
    tool_allowlist()
        .map(|allow| allow.contains(&short_tool_name(name)))
        .unwrap_or(true)
}

pub(super) fn mcp_tools_env_raw() -> Result<String, env::VarError> {
    env::var(MCP_TOOLS_ENV)
}

pub(super) fn short_tool_name(name: &str) -> String {
    name.strip_prefix("rustcodegraph_")
        .unwrap_or(name)
        .to_string()
}

fn tool(
    name: &str,
    description: &str,
    properties: serde_json::Map<String, Value>,
    required: Option<Vec<&str>>,
) -> ToolDefinition {
    ToolDefinition {
        name: name.to_string(),
        description: description.to_string(),
        input_schema: InputSchema {
            schema_type: "object".to_string(),
            properties,
            required: required.map(|items| items.into_iter().map(str::to_string).collect()),
        },
    }
}

fn props(items: &[(&str, Value)]) -> serde_json::Map<String, Value> {
    items
        .iter()
        .map(|(key, value)| ((*key).to_string(), value.clone()))
        .collect()
}

fn string_prop(name: &'static str, description: &str) -> (&'static str, Value) {
    (
        name,
        serde_json::json!({
            "type": "string",
            "description": description,
        }),
    )
}

fn number_prop(
    name: &'static str,
    description: &str,
    default: Option<i64>,
) -> (&'static str, Value) {
    let mut value = serde_json::json!({
        "type": "number",
        "description": description,
    });
    if let Some(default) = default {
        value["default"] = Value::from(default);
    }
    (name, value)
}

fn bool_prop(
    name: &'static str,
    description: &str,
    default: Option<bool>,
) -> (&'static str, Value) {
    let mut value = serde_json::json!({
        "type": "boolean",
        "description": description,
    });
    if let Some(default) = default {
        value["default"] = Value::from(default);
    }
    (name, value)
}

fn enum_prop(name: &'static str, description: &str, values: &[&str]) -> (&'static str, Value) {
    (
        name,
        serde_json::json!({
            "type": "string",
            "description": description,
            "enum": values,
        }),
    )
}

fn project_path_prop() -> (&'static str, Value) {
    string_prop(
        "projectPath",
        "Path to a different project with .rustcodegraph/ initialized. If omitted, uses current project. Use this to query other codebases.",
    )
}
