//! Codex JSONL 运行日志解析器。
//!
//! 这里尽量宽容：无法解析的行会跳过，字段缺失会退回默认值。agent-eval 的目标是
//! 从大量实验日志里提取趋势，单条脏事件不应让整份报告失败。

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::formatting::{number_u64, truncate_chars};
use super::types::{ParsedRun, TokenTotals, ToolCall};

pub(super) fn parse_run_file(file: &Path) -> Result<ParsedRun, String> {
    let text = fs::read_to_string(file)
        .map_err(|err| format!("failed to read {}: {err}", file.display()))?;
    let mut parsed = ParsedRun::default();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let Ok(event) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        match event.get("type").and_then(Value::as_str) {
            Some("system") if event.get("subtype").and_then(Value::as_str) == Some("init") => {
                // 初始化快照里的工具数量用来判断 MCP 是否成功暴露；实际采用还要看 tool_calls。
                parsed.init_codegraph_tools =
                    event.get("tools").and_then(Value::as_array).map(|tools| {
                        tools
                            .iter()
                            .filter(|tool| {
                                tool.as_str().is_some_and(|name| name.contains("codegraph"))
                            })
                            .count()
                    });
            }
            Some("assistant") => {
                if let Some(usage) = event.pointer("/message/usage") {
                    // result.usage 只代表最后一轮；这里逐 assistant turn 累加，才是 README benchmark 口径。
                    parsed.assistant_total_tokens += number_u64(usage.get("input_tokens"));
                    parsed.assistant_total_tokens += number_u64(usage.get("output_tokens"));
                    parsed.assistant_total_tokens +=
                        number_u64(usage.get("cache_read_input_tokens"));
                    parsed.assistant_total_tokens +=
                        number_u64(usage.get("cache_creation_input_tokens"));
                }
                for block in content_blocks(&event) {
                    if block.get("type").and_then(Value::as_str) != Some("tool_use") {
                        continue;
                    }
                    let name = block
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    parsed.tool_calls.push(ToolCall {
                        id: block
                            .get("id")
                            .and_then(Value::as_str)
                            .map(ToOwned::to_owned),
                        detail: tool_detail(block, &name),
                        name,
                        out_len: 0,
                    });
                }
            }
            Some("user") => {
                for block in content_blocks(&event) {
                    if block.get("type").and_then(Value::as_str) != Some("tool_result") {
                        continue;
                    }
                    let text = tool_result_text(block.get("content"));
                    if text.contains("No such tool available") {
                        // MCP 还没 attach 完成时会出现这个错误，benchmark 默认把这类 raced run 排除。
                        parsed.raced = true;
                    }
                    let Some(id) = block.get("tool_use_id").and_then(Value::as_str) else {
                        continue;
                    };
                    if let Some(call) = parsed
                        .tool_calls
                        .iter_mut()
                        .find(|call| call.id.as_deref() == Some(id))
                    {
                        call.out_len = text.chars().count();
                    }
                }
            }
            Some("result") => parsed.result = Some(event),
            _ => {}
        }
    }
    Ok(parsed)
}

fn content_blocks(event: &Value) -> Vec<&Value> {
    // Codex JSONL 把 assistant tool_use 和 user tool_result 都放在 message.content 数组里。
    event
        .pointer("/message/content")
        .and_then(Value::as_array)
        .map(|blocks| blocks.iter().collect())
        .unwrap_or_default()
}

fn tool_detail(block: &Value, name: &str) -> String {
    let input = block.get("input").unwrap_or(&Value::Null);
    if name == "Task" {
        let subagent = input
            .get("subagent_type")
            .and_then(Value::as_str)
            .unwrap_or("?");
        let description = input
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("");
        return format!(
            " [subagent_type={subagent}] {}",
            truncate_chars(description, 40)
        );
    }
    if name.contains("codegraph") {
        // 报告只保留足够识别意图的输入片段，避免把完整查询/源码塞进一行工具序列。
        let empty = Value::String(String::new());
        let value = ["query", "task", "symbol"]
            .iter()
            .find_map(|key| input.get(*key))
            .unwrap_or(&empty);
        return format!(
            " {}",
            truncate_chars(&serde_json::to_string(value).unwrap_or_default(), 60)
        );
    }
    if name == "Bash" {
        return format!(
            " {}",
            truncate_chars(
                input
                    .get("command")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                50
            )
        );
    }
    if name == "Read" {
        // Read 只展示 basename，保护本地路径，同时让回退到哪些文件一眼可见。
        let file = input
            .get("file_path")
            .and_then(Value::as_str)
            .and_then(|path| Path::new(path).file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("");
        return format!(" {file}");
    }
    String::new()
}

fn tool_result_text(content: Option<&Value>) -> String {
    // 不同 host 可能返回字符串、富文本数组或任意 JSON；统一成文本后才能统计输出长度。
    match content {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|item| item.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join(""),
        Some(other) => other.to_string(),
        None => String::new(),
    }
}

pub(super) fn tally_tool_counts(file: &Path) -> Result<BTreeMap<String, usize>, String> {
    let parsed = parse_run_file(file)?;
    let mut counts = BTreeMap::new();
    for call in parsed.tool_calls {
        *counts.entry(call.name).or_default() += 1;
    }
    Ok(counts)
}

pub(super) fn sum_tokens(file: &Path) -> Result<TokenTotals, String> {
    let text = fs::read_to_string(file)
        .map_err(|err| format!("failed to read {}: {err}", file.display()))?;
    let mut tokens = TokenTotals::default();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let Ok(event) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let Some(usage) = event.pointer("/message/usage") else {
            continue;
        };
        // fresh/cached/generated 分开统计，方便解释“token 降幅”和“成本降幅”不完全一致。
        tokens.generated += number_u64(usage.get("output_tokens"));
        tokens.fresh += number_u64(usage.get("input_tokens"))
            + number_u64(usage.get("cache_creation_input_tokens"));
        tokens.cached += number_u64(usage.get("cache_read_input_tokens"));
    }
    Ok(tokens)
}

pub(super) fn jsonl_files(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = fs::read_dir(dir)
        .map_err(|err| format!("failed to read {}: {err}", dir.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("jsonl"))
        .collect::<Vec<_>>();
    files.sort();
    Ok(files)
}
