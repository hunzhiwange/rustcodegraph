//! Hermes Agent installer target.
//!
//! Hermes 使用 YAML，但这里故意不引入通用 YAML 解析器：我们只维护两个
//! 已知块，按行范围做保格式 upsert/remove，避免重写用户手写配置。

use std::env;
use std::path::PathBuf;

use super::shared::{atomic_write_file_sync, path_to_string, read_text};
use super::types::{
    AgentTarget, DetectionResult, FileWrite, InstallOptions, Location, TargetId, WriteAction,
    WriteResult, file_write,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LineRange {
    // 半开区间 `[start, end)`，便于直接传给 `Vec::splice` / `drain`。
    start: usize,
    end: usize,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct HermesTarget;

fn hermes_home() -> PathBuf {
    env::var_os("HERMES_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| super::shared::home_dir().join(".hermes"))
}

fn config_path() -> PathBuf {
    hermes_home().join("config.yaml")
}

impl AgentTarget for HermesTarget {
    fn id(&self) -> TargetId {
        TargetId::Hermes
    }

    fn display_name(&self) -> &'static str {
        "Hermes Agent"
    }

    fn docs_url(&self) -> Option<&'static str> {
        Some("https://hermes-agent.nousresearch.com")
    }

    fn supports_location(&self, loc: Location) -> bool {
        loc == Location::Global
    }

    fn detect(&self, loc: Location) -> DetectionResult {
        if loc != Location::Global {
            return DetectionResult::default();
        }
        let file = config_path();
        let content = read_text(&file);
        DetectionResult {
            installed: hermes_home().exists() || file.exists(),
            already_configured: has_rustcodegraph_mcp_server(&content),
            config_path: Some(path_to_string(file)),
        }
    }

    fn install(&self, loc: Location, _opts: InstallOptions) -> WriteResult {
        if loc != Location::Global {
            return WriteResult::empty().with_notes([
                "Hermes Agent uses $HERMES_HOME/config.yaml; re-run with --location=global.",
            ]);
        }
        WriteResult::with_file(write_hermes_config())
            .with_notes(["Start a new Hermes session for MCP changes to take effect."])
    }

    fn uninstall(&self, loc: Location) -> WriteResult {
        if loc != Location::Global {
            return WriteResult::empty();
        }
        let file = config_path();
        if !file.exists() {
            return WriteResult::with_file(file_write(path_to_string(file), WriteAction::NotFound));
        }
        let before = read_text(&file);
        let after = remove_rustcodegraph_toolset(&remove_rustcodegraph_mcp_server(&before));
        let action = if after == before {
            WriteAction::NotFound
        } else {
            let _ = atomic_write_file_sync(&file, &after);
            WriteAction::Removed
        };
        WriteResult::with_file(file_write(path_to_string(file), action))
    }

    fn print_config(&self, loc: Location) -> String {
        if loc != Location::Global {
            return "# Hermes Agent uses $HERMES_HOME/config.yaml; use --location=global.\n"
                .to_owned();
        }
        [
            format!("# Add to {}", path_to_string(config_path())),
            String::new(),
            render_rustcodegraph_mcp_block().join("\n"),
            String::new(),
            "platform_toolsets:".to_owned(),
            "  cli:".to_owned(),
            "    - hermes-cli".to_owned(),
            "    - mcp-rustcodegraph".to_owned(),
            String::new(),
        ]
        .join("\n")
    }

    fn describe_paths(&self, loc: Location) -> Vec<String> {
        if loc == Location::Global {
            vec![path_to_string(config_path())]
        } else {
            Vec::new()
        }
    }
}

fn write_hermes_config() -> FileWrite {
    let file = config_path();
    let existed = file.exists();
    let before = read_text(&file);
    if existed && has_rustcodegraph_mcp_server(&before) && has_rustcodegraph_toolset(&before) {
        return file_write(path_to_string(file), WriteAction::Unchanged);
    }
    let after = upsert_rustcodegraph_toolset(&upsert_rustcodegraph_mcp_server(&before));
    let action = if after == before {
        WriteAction::Unchanged
    } else if existed {
        WriteAction::Updated
    } else {
        WriteAction::Created
    };
    if action != WriteAction::Unchanged {
        let _ = atomic_write_file_sync(&file, &after);
    }
    file_write(path_to_string(file), action)
}

fn split_lines(content: &str) -> Vec<String> {
    content
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .split('\n')
        .map(str::to_owned)
        .collect()
}

fn join_lines(mut lines: Vec<String>) -> String {
    while lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    format!("{}\n", lines.join("\n"))
}

fn top_level_range(lines: &[String], key: &str) -> Option<LineRange> {
    // 只识别顶层 `key:`，空行不截断块；下一个顶层 YAML key 才是结束。
    let start = lines
        .iter()
        .position(|line| line.trim() == format!("{key}:"))?;
    let mut end = lines.len();
    for (idx, line) in lines.iter().enumerate().skip(start + 1) {
        if line.trim().is_empty() {
            continue;
        }
        if !line.starts_with(' ') && line.trim_end().ends_with(':') {
            end = idx;
            break;
        }
    }
    Some(LineRange { start, end })
}

fn child_range(lines: &[String], parent: LineRange, child: &str) -> Option<LineRange> {
    let needle = format!("  {child}:");
    let start = (parent.start + 1..parent.end).find(|idx| lines[*idx].trim_end() == needle)?;
    let mut end = parent.end;
    for (idx, line) in lines.iter().enumerate().take(parent.end).skip(start + 1) {
        if line.trim().is_empty() {
            continue;
        }
        if line.starts_with("  ") && !line.starts_with("    ") && !line.starts_with("  - ") {
            end = idx;
            break;
        }
    }
    Some(LineRange { start, end })
}

fn list_child_block(
    lines: &[String],
    parent: LineRange,
    child: &str,
) -> Option<(LineRange, String)> {
    // platform_toolsets.cli 是列表，用户可能用 2 或 4 空格缩进；
    // 返回 item_indent 以便新增项沿用现有风格。
    let needle = format!("  {child}:");
    let start = (parent.start + 1..parent.end).find(|idx| lines[*idx].trim_end() == needle)?;
    let mut end = parent.end;
    for (idx, line) in lines.iter().enumerate().take(parent.end).skip(start + 1) {
        if line.trim().is_empty() {
            continue;
        }
        let indent = line.chars().take_while(|ch| *ch == ' ').count();
        if indent >= 4 {
            continue;
        }
        if indent == 2 && line.starts_with("  - ") {
            continue;
        }
        end = idx;
        break;
    }
    while end > start + 1 && lines[end - 1].trim().is_empty() {
        end -= 1;
    }

    let item_indent = (start + 1..end)
        .find_map(|idx| {
            let line = &lines[idx];
            let spaces = line.chars().take_while(|ch| *ch == ' ').count();
            line[spaces..].starts_with("- ").then(|| " ".repeat(spaces))
        })
        .unwrap_or_else(|| "    ".to_owned());
    Some((LineRange { start, end }, item_indent))
}

fn render_rustcodegraph_mcp_child() -> Vec<String> {
    [
        "  rustcodegraph:",
        "    command: rustcodegraph",
        "    args:",
        "      - serve",
        "      - --mcp",
        "    timeout: 120",
        "    connect_timeout: 60",
        "    enabled: true",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

fn render_rustcodegraph_mcp_block() -> Vec<String> {
    let mut out = vec!["mcp_servers:".to_owned()];
    out.extend(render_rustcodegraph_mcp_child());
    out
}

fn has_rustcodegraph_mcp_server(content: &str) -> bool {
    let lines = split_lines(content);
    let Some(parent) = top_level_range(&lines, "mcp_servers") else {
        return false;
    };
    child_range(&lines, parent, "rustcodegraph").is_some()
}

fn has_rustcodegraph_toolset(content: &str) -> bool {
    content
        .lines()
        .any(|line| line.trim() == "- mcp-rustcodegraph")
}

fn upsert_rustcodegraph_mcp_server(content: &str) -> String {
    let mut lines = split_lines(content);
    let parent = top_level_range(&lines, "mcp_servers");
    let child = parent.and_then(|range| child_range(&lines, range, "rustcodegraph"));
    let replacement = render_rustcodegraph_mcp_child();
    match (parent, child) {
        (None, _) => {
            while lines.last().is_some_and(|line| line.is_empty()) {
                lines.pop();
            }
            if !lines.is_empty() {
                lines.push(String::new());
            }
            lines.extend(render_rustcodegraph_mcp_block());
        }
        (Some(parent), None) => {
            lines.splice(parent.end..parent.end, replacement);
        }
        (_, Some(child)) => {
            if lines[child.start..child.end] != replacement {
                lines.splice(child.start..child.end, replacement);
            }
        }
    }
    join_lines(lines)
}

fn remove_rustcodegraph_mcp_server(content: &str) -> String {
    let mut lines = split_lines(content);
    let Some(parent) = top_level_range(&lines, "mcp_servers") else {
        return content.to_owned();
    };
    let Some(child) = child_range(&lines, parent, "rustcodegraph") else {
        return content.to_owned();
    };
    lines.drain(child.start..child.end);
    join_lines(lines)
}

fn upsert_rustcodegraph_toolset(content: &str) -> String {
    // Hermes 需要 MCP server 定义和 cli toolset 同时存在；少任一项都会导致
    // agent 看得到配置但不会加载 rustcodegraph。
    let mut lines = split_lines(content);
    let parent = top_level_range(&lines, "platform_toolsets");
    match parent {
        None => {
            while lines.last().is_some_and(|line| line.is_empty()) {
                lines.pop();
            }
            if !lines.is_empty() {
                lines.push(String::new());
            }
            lines.extend([
                "platform_toolsets:".to_owned(),
                "  cli:".to_owned(),
                "    - hermes-cli".to_owned(),
                "    - mcp-rustcodegraph".to_owned(),
            ]);
        }
        Some(parent) => {
            let cli = list_child_block(&lines, parent, "cli");
            match cli {
                None => {
                    lines.splice(
                        parent.end..parent.end,
                        [
                            "  cli:".to_owned(),
                            "    - hermes-cli".to_owned(),
                            "    - mcp-rustcodegraph".to_owned(),
                        ],
                    );
                }
                Some((cli, item_indent)) => {
                    if lines[cli.start + 1..cli.end]
                        .iter()
                        .any(|line| line.trim() == "- mcp-rustcodegraph")
                    {
                        return join_lines(lines);
                    }
                    lines.splice(
                        cli.end..cli.end,
                        [format!("{item_indent}- mcp-rustcodegraph")],
                    );
                }
            };
        }
    }
    join_lines(lines)
}

fn remove_rustcodegraph_toolset(content: &str) -> String {
    let mut lines = split_lines(content);
    let Some(parent) = top_level_range(&lines, "platform_toolsets") else {
        return content.to_owned();
    };
    let Some((cli, _indent)) = list_child_block(&lines, parent, "cli") else {
        return content.to_owned();
    };
    if !lines[cli.start + 1..cli.end]
        .iter()
        .any(|line| line.trim() == "- mcp-rustcodegraph")
    {
        return content.to_owned();
    }
    lines = lines
        .into_iter()
        .enumerate()
        .filter_map(|(idx, line)| {
            ((idx <= cli.start || idx >= cli.end) || line.trim() != "- mcp-rustcodegraph")
                .then_some(line)
        })
        .collect();
    join_lines(lines)
}
