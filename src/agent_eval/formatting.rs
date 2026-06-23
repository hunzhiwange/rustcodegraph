//! agent-eval 报告共享的格式化和小型解析工具。
//!
//! 这些函数故意保持无状态：报告模块可以组合它们生成固定宽度表格、稳定工具名和
//! 容错数值转换，而不会把统计口径分散到每个报表里。

use std::collections::BTreeMap;
use std::path::Path;

use regex::Regex;
use serde_json::Value;

pub(super) fn required_arg<'a>(
    args: &'a [String],
    index: usize,
    usage: &str,
) -> Result<&'a str, String> {
    args.get(index)
        .map(String::as_str)
        .filter(|arg| !arg.trim().is_empty())
        .ok_or_else(|| usage.to_string())
}

pub(super) fn format_counts(counts: &BTreeMap<String, usize>) -> String {
    if counts.is_empty() {
        return "    (none)".to_string();
    }
    let mut entries = counts.iter().collect::<Vec<_>>();
    // 先按次数降序、再按名称升序，保证同样输入跨平台输出一致。
    entries.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
    entries
        .into_iter()
        .map(|(name, count)| format!("    {:>3}  {name}", count))
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn count_named(counts: &BTreeMap<String, usize>, name: &str) -> usize {
    counts.get(name).copied().unwrap_or(0)
}

pub(super) fn count_explore_tools(counts: &BTreeMap<String, usize>) -> usize {
    counts
        .iter()
        .filter(|(name, _)| name.contains("codegraph") && name.contains("explore"))
        .map(|(_, count)| *count)
        .sum()
}

pub(super) fn short_codegraph_name(name: &str) -> String {
    name.replace("mcp__rustcodegraph__rustcodegraph_", "")
        .replace("mcp__rustcodegraph__", "")
        .replace("rustcodegraph_", "")
}

pub(super) fn sequence_tag(name: &str) -> String {
    // 序列表格需要压缩到一行里读；常见工具用短码，rustcodegraph 工具保留语义名。
    match name {
        "Read" => "R".to_string(),
        "Grep" => "G".to_string(),
        "Glob" => "Gl".to_string(),
        "Bash" => "B".to_string(),
        "Task" => "Ag".to_string(),
        "ToolSearch" => "TS".to_string(),
        _ if name.contains("codegraph") => short_codegraph_name(name),
        _ => name.to_string(),
    }
}

pub(super) fn arm_label(arm: &str) -> &'static str {
    // arm 标签是实验协议的一部分，改名会影响历史报告的可比性。
    match arm {
        "A" => "A all/none(old)",
        "H" => "H body-trace/none",
        "I" => "I bodytrace+dest",
        "B" => "B all/steer(thin)",
        "F" => "F all/steer(body)",
        "G" => "G ported(noprompt)",
        "C" => "C no-explore",
        "D" => "D trace-centric",
        "E" => "E nonflow-probe",
        _ => arm_label_fallback(arm),
    }
}

fn arm_label_fallback(_: &str) -> &'static str {
    "unknown"
}

pub(super) fn number_u64(value: Option<&Value>) -> u64 {
    // Codex JSONL 不同版本可能把 usage 写成整数或浮点；这里统一做非负整数兜底。
    value
        .and_then(Value::as_u64)
        .or_else(|| {
            value
                .and_then(Value::as_f64)
                .map(|number| number.max(0.0) as u64)
        })
        .unwrap_or(0)
}

pub(super) fn number_f64(value: Option<&Value>) -> Option<f64> {
    // 保留 None 让调用方区分“字段缺失”和“真实 0”，尤其是 duration/turns 这类结果字段。
    value.and_then(|value| {
        value
            .as_f64()
            .or_else(|| value.as_u64().map(|number| number as f64))
            .or_else(|| value.as_i64().map(|number| number as f64))
    })
}

pub(super) fn file_name(file: &Path) -> String {
    if let Some(name) = file.file_name().and_then(|name| name.to_str()) {
        name.to_string()
    } else {
        file.to_string_lossy().into_owned()
    }
}

pub(super) fn truncate_chars(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

pub(super) fn pad(text: &str, width: usize) -> String {
    format!("{text:<width$}")
}

pub(super) fn left_pad(text: &str, width: usize) -> String {
    format!("{text:>width$}")
}

pub(super) fn avg<T>(items: &[T], f: impl Fn(&T) -> f64) -> f64 {
    if items.is_empty() {
        0.0
    } else {
        items.iter().map(f).sum::<f64>() / items.len() as f64
    }
}

pub(super) fn k_chars(value: f64) -> String {
    format!("{:.1}", value / 1000.0)
}

pub(super) fn k_chars_number(value: f64) -> String {
    format!("{:.1}", value / 1000.0)
}

pub(super) fn k_tokens(value: u64) -> String {
    format!("{:.1}k", value as f64 / 1000.0)
}

pub(super) fn median(mut values: Vec<f64>) -> f64 {
    // agent 运行波动很大，benchmark 用中位数比平均值更稳。
    values.sort_by(|a, b| a.total_cmp(b));
    match values.len() {
        0 => 0.0,
        n if n % 2 == 1 => values[(n - 1) / 2],
        n => (values[n / 2 - 1] + values[n / 2]) / 2.0,
    }
}

pub(super) fn fmt_time(seconds: f64) -> String {
    if seconds >= 60.0 {
        format!(
            "{}m {}s",
            (seconds / 60.0).floor() as i64,
            (seconds % 60.0).round() as i64
        )
    } else {
        format!("{}s", seconds.round() as i64)
    }
}

pub(super) fn fmt_tokens(tokens: f64) -> String {
    if tokens >= 1_000_000.0 {
        format!("{:.1}M", tokens / 1_000_000.0)
    } else {
        format!("{}k", (tokens / 1000.0).round() as i64)
    }
}

pub(super) fn pct(with: f64, without: f64) -> i64 {
    // 正值表示 with-rustcodegraph 相对 without-rustcodegraph 节省了多少。
    if without > 0.0 {
        ((1.0 - with / without) * 100.0).round() as i64
    } else {
        0
    }
}

pub(super) fn avg_i64(values: Option<&Vec<i64>>) -> i64 {
    values
        .filter(|values| !values.is_empty())
        .map(|values| (values.iter().sum::<i64>() as f64 / values.len() as f64).round() as i64)
        .unwrap_or(0)
}

pub(super) fn fmt_option(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.0}"))
        .unwrap_or_else(|| "?".to_string())
}

pub(super) fn tier(files: usize) -> &'static str {
    if files < 200 {
        "S(<200)"
    } else if files < 2000 {
        "M(<2000)"
    } else {
        "L(>=2000)"
    }
}

pub(super) fn empty_as_none(value: &str) -> String {
    if value.is_empty() {
        "(none)".to_string()
    } else {
        value.to_string()
    }
}

pub(super) fn option_value(args: &[String], flag: &str) -> Option<String> {
    // 支持 `--flag value` 和 `--flag=value`，方便 shell 脚本与手工命令共用。
    for (index, arg) in args.iter().enumerate() {
        if arg == flag {
            return args.get(index + 1).cloned();
        }
        if let Some(value) = arg.strip_prefix(&format!("{flag}=")) {
            return Some(value.to_string());
        }
    }
    None
}

pub(super) fn regex_is_match(pattern: &str, text: &str) -> bool {
    Regex::new(pattern)
        .expect("probe signal regex should compile")
        .is_match(text)
}
