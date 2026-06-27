//! Rust CLI 的静态帮助元数据。
//!
//! 命令、选项和职责集中在这里，`render_help` 与 `render_command_help` 共用同一份
//! spec，避免主帮助和子命令帮助在文案或默认值上漂移。

/// 生成帮助文本时展示的一项 CLI 选项。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliOptionSpec {
    pub flags: &'static str,
    pub description: &'static str,
    pub default_value: Option<&'static str>,
}

/// 一个命令的声明式描述。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliCommandSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub options: Vec<CliOptionSpec>,
    pub responsibilities: Vec<&'static str>,
}

fn opt(
    flags: &'static str,
    description: &'static str,
    default_value: Option<&'static str>,
) -> CliOptionSpec {
    CliOptionSpec {
        flags,
        description,
        default_value,
    }
}

pub fn command_specs() -> Vec<CliCommandSpec> {
    // 这里刻意使用静态字符串而不是 clap runtime introspection：测试可以直接断言
    // 帮助文案，release helpers 也能复用职责描述而不启动完整 CLI parser。
    vec![
        CliCommandSpec {
            name: "init [path]",
            description: "Initialize RustCodeGraph in a project directory",
            options: vec![
                opt(
                    "-i, --index",
                    "Build the initial index after initialization",
                    None,
                ),
                opt("-f, --force", "Initialize unsafe roots explicitly", None),
                opt(
                    "-v, --verbose",
                    "Show worker lifecycle and memory info",
                    None,
                ),
            ],
            responsibilities: vec![
                "create .rustcodegraph",
                "initialize database",
                "run full index when -i/--index is passed",
            ],
        },
        CliCommandSpec {
            name: "uninit [path]",
            description: "Remove RustCodeGraph from a project (deletes .rustcodegraph/ directory)",
            options: vec![opt("-f, --force", "Skip confirmation prompt", None)],
            responsibilities: vec!["confirm deletion", "remove .rustcodegraph directory"],
        },
        CliCommandSpec {
            name: "index [path]",
            description: "Index all files in the project",
            options: vec![
                opt("-f, --force", "Index unsafe roots explicitly", None),
                opt("-q, --quiet", "Suppress progress output", None),
                opt(
                    "-v, --verbose",
                    "Show worker lifecycle and memory info",
                    None,
                ),
            ],
            responsibilities: vec![
                "open or initialize project",
                "run full index",
                "print summary",
            ],
        },
        CliCommandSpec {
            name: "sync [path]",
            description: "Sync changes since last index",
            options: vec![opt("-q, --quiet", "Suppress output", None)],
            responsibilities: vec!["open project", "run incremental sync"],
        },
        CliCommandSpec {
            name: "watch [path]",
            description: "Watch project files and auto-sync the index on changes",
            options: vec![
                opt("-p, --path <path>", "Project path", None),
                opt(
                    "--debounce-ms <number>",
                    "Delay before auto-sync after a file change",
                    Some("2000"),
                ),
            ],
            responsibilities: vec![
                "open project",
                "run initial catch-up sync",
                "watch files until interrupted",
            ],
        },
        CliCommandSpec {
            name: "status [path]",
            description: "Show index status and statistics",
            options: vec![opt("-j, --json", "Output as JSON", None)],
            responsibilities: vec!["inspect database", "surface backend and stale-index hints"],
        },
        CliCommandSpec {
            name: "query <search>",
            description: "Search for symbols in the codebase",
            options: vec![
                opt("-p, --path <path>", "Project path", None),
                opt("-l, --limit <number>", "Maximum results", Some("10")),
                opt("-k, --kind <kind>", "Filter by node kind", None),
                opt("-j, --json", "Output as JSON", None),
            ],
            responsibilities: vec!["mirror rustcodegraph_search MCP output"],
        },
        CliCommandSpec {
            name: "explore <query...>",
            description: "Explore relevant symbols' source plus call paths",
            options: vec![
                opt("-p, --path <path>", "Project path", None),
                opt("--max-files <number>", "Maximum source files", None),
            ],
            responsibilities: vec!["mirror rustcodegraph_explore MCP output"],
        },
        CliCommandSpec {
            name: "node <name>",
            description: "Show one symbol or file with line numbers",
            options: vec![
                opt("-p, --path <path>", "Project path", None),
                opt(
                    "-f, --file <file>",
                    "File mode or disambiguation file",
                    None,
                ),
                opt("--offset <number>", "File mode start line", None),
                opt("--limit <number>", "File mode line limit", None),
                opt("--symbols-only", "File mode symbol map only", None),
            ],
            responsibilities: vec!["mirror rustcodegraph_node MCP output"],
        },
        CliCommandSpec {
            name: "files",
            description: "Show project file structure from the index",
            options: vec![
                opt("-p, --path <path>", "Project path", None),
                opt("--filter <dir>", "Filter to directory", None),
                opt("--pattern <glob>", "Filter by glob", None),
                opt("--format <format>", "tree, flat, grouped", Some("tree")),
                opt("--max-depth <number>", "Maximum tree depth", None),
                opt("--no-metadata", "Hide file metadata", None),
                opt("-j, --json", "Output as JSON", None),
            ],
            responsibilities: vec!["mirror rustcodegraph_files MCP output"],
        },
        CliCommandSpec {
            name: "daemon",
            description: "Manage running RustCodeGraph background daemons",
            options: vec![],
            responsibilities: vec!["list daemons", "stop selected daemon"],
        },
        CliCommandSpec {
            name: "serve",
            description: "Start RustCodeGraph as an MCP server for AI assistants",
            options: vec![
                opt("-p, --path <path>", "Project path", None),
                opt("--mcp", "Run stdio MCP transport", None),
                opt("--no-watch", "Disable file watcher", None),
            ],
            responsibilities: vec!["bootstrap MCP server", "optionally start watcher"],
        },
        CliCommandSpec {
            name: "unlock [path]",
            description: "Remove a stale lock file that is blocking indexing",
            options: vec![],
            responsibilities: vec!["delete stale lock after path validation"],
        },
        CliCommandSpec {
            name: "callers <symbol>",
            description: "Find all functions/methods that call a specific symbol",
            options: vec![
                opt("-p, --path <path>", "Project path", None),
                opt("--file <file>", "Narrow to definition in file", None),
                opt("-l, --limit <number>", "Maximum results", Some("20")),
                opt("-j, --json", "Output as JSON", None),
            ],
            responsibilities: vec!["mirror rustcodegraph_callers MCP output"],
        },
        CliCommandSpec {
            name: "callees <symbol>",
            description: "Find all functions/methods that a specific symbol calls",
            options: vec![
                opt("-p, --path <path>", "Project path", None),
                opt("--file <file>", "Narrow to definition in file", None),
                opt("-l, --limit <number>", "Maximum results", Some("20")),
                opt("-j, --json", "Output as JSON", None),
            ],
            responsibilities: vec!["mirror rustcodegraph_callees MCP output"],
        },
        CliCommandSpec {
            name: "impact <symbol>",
            description: "Analyze what code is affected by changing a symbol",
            options: vec![
                opt("-p, --path <path>", "Project path", None),
                opt("--file <file>", "Narrow to definition in file", None),
                opt("-d, --depth <number>", "Traversal depth", Some("2")),
                opt("-j, --json", "Output as JSON", None),
            ],
            responsibilities: vec!["mirror rustcodegraph_impact MCP output"],
        },
        CliCommandSpec {
            name: "affected [files...]",
            description: "Find test files affected by changed source files",
            options: vec![
                opt("-p, --path <path>", "Project path", None),
                opt("--stdin", "Read file list from stdin", None),
                opt("-d, --depth <number>", "Traversal depth", Some("5")),
                opt("-f, --filter <glob>", "Custom test glob", None),
                opt("-j, --json", "Output as JSON", None),
                opt("-q, --quiet", "Only output file paths", None),
            ],
            responsibilities: vec!["walk dependents", "filter test files"],
        },
        CliCommandSpec {
            name: "install",
            description: "Install RustCodeGraph MCP server into one or more agents (Claude Code, Cursor, Codex CLI, opencode, Hermes Agent)",
            options: vec![
                opt("-t, --target <ids>", "Target agents", None),
                opt("-l, --location <where>", "global or local", None),
                opt("-y, --yes", "Use non-interactive defaults", None),
                opt("--no-permissions", "Skip auto-allow permissions", None),
                opt("--print-config <id>", "Print config snippet only", None),
            ],
            responsibilities: vec!["resolve installer targets", "plan agent config changes"],
        },
        CliCommandSpec {
            name: "uninstall",
            description: "Remove RustCodeGraph from your agents (Claude Code, Cursor, Codex CLI, opencode, Hermes Agent)",
            options: vec![
                opt("-t, --target <ids>", "Target agents", Some("all")),
                opt("-l, --location <where>", "global or local", None),
                opt("-y, --yes", "Use non-interactive defaults", None),
            ],
            responsibilities: vec!["resolve installer targets", "plan agent config removals"],
        },
        CliCommandSpec {
            name: "telemetry [action]",
            description: "Show or change anonymous usage telemetry",
            options: vec![],
            responsibilities: vec!["status", "on", "off"],
        },
        CliCommandSpec {
            name: "upgrade [version]",
            description: "Update RustCodeGraph to the latest release or a specific version",
            options: vec![
                opt("--check", "Check without installing", None),
                opt("-f, --force", "Reinstall target version", None),
            ],
            responsibilities: vec!["detect install method", "plan upgrade command"],
        },
        CliCommandSpec {
            name: "version",
            description: "Print the installed RustCodeGraph version",
            options: vec![],
            responsibilities: vec!["print package version"],
        },
        CliCommandSpec {
            name: "prepare-release [version]",
            description: "Promote CHANGELOG.md [Unreleased] entries into a version block",
            options: vec![],
            responsibilities: vec!["prepare changelog release notes", "append release link ref"],
        },
        CliCommandSpec {
            name: "extract-release-notes <version|--stdin>",
            description: "Extract and unwrap release notes from CHANGELOG.md or stdin",
            options: vec![opt(
                "--stdin",
                "Read release-note markdown from stdin",
                None,
            )],
            responsibilities: vec![
                "extract changelog version block",
                "unwrap hard-wrapped bullets",
            ],
        },
        CliCommandSpec {
            name: "agent-eval <command>",
            description: "Run Rust-owned retrieval benchmark parsers and deterministic probes",
            options: vec![],
            responsibilities: vec!["parse eval JSONL logs", "run deterministic MCP probes"],
        },
        CliCommandSpec {
            name: "add-lang <command>",
            description: "Run Rust-owned add-language development helpers",
            options: vec![],
            responsibilities: vec![
                "check grammar health",
                "dump native ASTs",
                "verify extraction",
            ],
        },
    ]
}

const CLI_NAME: &str = "rustcodegraph";

// 只在顶层帮助展示用户常用命令；调试/内部子命令仍可通过 `help <command>` 查看。
const VISIBLE_COMMANDS: &[&str] = &[
    "init",
    "uninit",
    "index",
    "sync",
    "watch",
    "status",
    "query",
    "explore",
    "node",
    "files",
    "unlock",
    "callers",
    "callees",
    "impact",
    "affected",
    "install",
    "uninstall",
    "upgrade",
    "version",
];

fn command_key(name: &str) -> &str {
    // spec.name 可能带参数占位符，如 `query <search>`；匹配和排序只看第一个 token。
    name.split_whitespace().next().unwrap_or(name)
}

fn usage_name(spec: &CliCommandSpec) -> String {
    if spec.options.is_empty() {
        spec.name.to_owned()
    } else {
        let key = command_key(spec.name);
        let rest = spec.name.strip_prefix(key).unwrap_or("").trim_start();
        if rest.is_empty() {
            format!("{key} [options]")
        } else {
            format!("{key} [options] {rest}")
        }
    }
}

fn wrap_description(text: &str, width: usize) -> Vec<String> {
    // 帮助输出面向终端阅读，保持简单按词换行即可；不处理 ANSI 宽度或 CJK 宽字符。
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let next_len = if current.is_empty() {
            word.len()
        } else {
            current.len() + 1 + word.len()
        };
        if next_len > width && !current.is_empty() {
            lines.push(current);
            current = word.to_owned();
        } else {
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(word);
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

fn format_row(left: &str, right: &str, left_width: usize, wrap_width: usize) -> String {
    let wrapped = wrap_description(right, wrap_width);
    let mut out = String::new();
    if let Some(first) = wrapped.first() {
        out.push_str(&format!("  {left:<left_width$}{first}\n"));
        for line in wrapped.iter().skip(1) {
            out.push_str(&format!("  {:<left_width$}{line}\n", ""));
        }
    } else {
        out.push_str(&format!("  {left}\n"));
    }
    out
}

pub fn render_help() -> String {
    // 顶层帮助按 VISIBLE_COMMANDS 的固定顺序输出，避免新增内部命令改变用户熟悉布局。
    let specs = command_specs();
    let mut out = String::new();
    out.push_str(&format!("Usage: {CLI_NAME} [options] [command]\n\n"));
    out.push_str("Code intelligence and knowledge graph for any codebase\n\n");
    out.push_str("Options:\n");
    out.push_str(&format_row(
        "-V, --version",
        "output the version number",
        32,
        80,
    ));
    out.push_str(&format_row(
        "-h, --help",
        "display help for command",
        32,
        80,
    ));
    out.push('\n');
    out.push_str("Commands:\n");
    for key in VISIBLE_COMMANDS {
        if let Some(spec) = specs.iter().find(|spec| command_key(spec.name) == *key) {
            out.push_str(&format_row(&usage_name(spec), spec.description, 32, 80));
        }
    }
    out.push_str(&format_row(
        "help [command]",
        "display help for command",
        32,
        80,
    ));
    out.trim_end().to_owned()
}

pub fn render_command_help(command: &str) -> Option<String> {
    // 子命令帮助允许展示默认值，但不展开 responsibilities；职责字段主要给测试和文档使用。
    let specs = command_specs();
    let spec = specs
        .iter()
        .find(|spec| command_key(spec.name) == command)?;
    let mut out = String::new();
    out.push_str(&format!("Usage: {CLI_NAME} {}\n\n", usage_name(spec)));
    out.push_str(spec.description);
    out.push_str("\n\n");
    out.push_str("Options:\n");
    for option in &spec.options {
        let description = match option.default_value {
            Some(default) => format!("{} (default: {default})", option.description),
            None => option.description.to_owned(),
        };
        out.push_str(&format_row(option.flags, &description, 32, 80));
    }
    out.push_str(&format_row(
        "-h, --help",
        "display help for command",
        32,
        80,
    ));
    Some(out.trim_end().to_owned())
}
