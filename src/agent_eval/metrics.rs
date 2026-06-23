//! 从解析后的 JSONL 事件中提炼可比较指标。
//!
//! parser 负责“发生了什么”，本模块负责“如何计数”。这些口径直接支撑 README
//! 和动态分发验证，尤其要区分总 token、工具调用次数、Read/Grep 次数与 MCP attach race。

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::Path;

use regex::Regex;
use serde_json::Value;

use super::formatting::{number_f64, sequence_tag, short_codegraph_name};
use super::parser::parse_run_file;
use super::types::{BenchRun, ParsedRun, RepoMeta, RunMetrics, SeqMetrics, ToolPayload};

pub(super) fn run_metrics(parsed: &ParsedRun) -> RunMetrics {
    // codegraph 工具用名称包含匹配，兼容不同 MCP host 给工具名前面加的命名空间。
    let codegraph = parsed
        .tool_calls
        .iter()
        .filter(|call| call.name.contains("codegraph"))
        .collect::<Vec<_>>();
    RunMetrics {
        init_codegraph_tools: parsed.init_codegraph_tools.unwrap_or(0),
        reads: parsed
            .tool_calls
            .iter()
            .filter(|call| call.name == "Read")
            .count(),
        greps: parsed
            .tool_calls
            .iter()
            // Glob 和 Grep 都代表 agent 回退到文件系统搜索，评估里合并为 grep 类信号。
            .filter(|call| call.name == "Grep" || call.name == "Glob")
            .count(),
        codegraph_calls: codegraph.len(),
        codegraph_sequence: codegraph
            .iter()
            .map(|call| short_codegraph_name(&call.name))
            .collect(),
        codegraph_output: codegraph.iter().map(|call| call.out_len).sum(),
        trace_used: codegraph.iter().any(|call| call.name.contains("trace")),
        turns: parsed
            .result
            .as_ref()
            .and_then(|result| number_f64(result.get("num_turns"))),
        duration_seconds: parsed
            .result
            .as_ref()
            .and_then(|result| number_f64(result.get("duration_ms")))
            .map(|value| (value / 1000.0).round()),
        cost: parsed
            .result
            .as_ref()
            .and_then(|result| number_f64(result.get("total_cost_usd")))
            .unwrap_or(0.0),
        ok: parsed
            .result
            .as_ref()
            .and_then(|result| result.get("subtype"))
            .and_then(Value::as_str)
            == Some("success"),
    }
}

pub(super) fn seq_metrics(parsed: &ParsedRun) -> SeqMetrics {
    let base = run_metrics(parsed);
    let mut per_tool = BTreeMap::new();
    let mut trace_index = None;
    // 记录 trace 之后 agent 又调用了什么，用来判断“首个结构答案是否足够让它停止阅读”。
    for (codegraph_index, call) in parsed
        .tool_calls
        .iter()
        .filter(|call| call.name.contains("codegraph"))
        .enumerate()
    {
        if trace_index.is_none() && call.name.contains("trace") {
            trace_index = Some(codegraph_index);
        }
        let name = short_codegraph_name(&call.name);
        let entry: &mut ToolPayload = per_tool.entry(name).or_default();
        entry.n += 1;
        entry.out += call.out_len;
    }
    let after_trace = trace_index.map(|index| {
        base.codegraph_sequence
            .iter()
            .skip(index + 1)
            .cloned()
            .collect()
    });
    SeqMetrics {
        base,
        per_tool,
        sequence: parsed
            .tool_calls
            .iter()
            .map(|call| sequence_tag(&call.name))
            .collect(),
        after_trace,
    }
}

pub(super) fn bench_run_metrics(file: &Path) -> Result<Option<BenchRun>, String> {
    if !file.exists() {
        return Ok(None);
    }
    let parsed = parse_run_file(file)?;
    if parsed
        .result
        .as_ref()
        .and_then(|result| result.get("subtype"))
        .and_then(Value::as_str)
        != Some("success")
    {
        // benchmark 只比较成功完成的 run；失败 run 需要单独看日志，而不是进入节省率。
        return Ok(None);
    }
    let tools = parsed
        .tool_calls
        .iter()
        .filter(|call| call.name != "ToolSearch")
        .count();
    let result = parsed.result.as_ref().expect("result checked above");
    Ok(Some(BenchRun {
        duration: number_f64(result.get("duration_ms")).unwrap_or(0.0) / 1000.0,
        tools,
        tokens: parsed.assistant_total_tokens,
        cost: number_f64(result.get("total_cost_usd")).unwrap_or(0.0),
        raced: parsed.raced,
    }))
}

pub(super) fn read_repo_meta(file: &Path) -> Result<HashMap<String, RepoMeta>, String> {
    let mut meta = HashMap::new();
    if !file.exists() {
        return Ok(meta);
    }
    let line_re = Regex::new(r"^\|\s*([^|]+?)\s*\|\s*(S|M|L)\s*\|\s*`([^`]+)`\s*\|\s*(\d+)\s*\|")
        .expect("matrix metadata regex should compile");
    // 从 Markdown 表格读取 repo 文件数，避免 seq-matrix 报告再去扫描外部 corpus。
    for line in fs::read_to_string(file)
        .map_err(|err| format!("failed to read {}: {err}", file.display()))?
        .lines()
    {
        if let Some(captures) = line_re.captures(line) {
            meta.insert(
                captures[3].to_string(),
                RepoMeta {
                    files: captures[4].parse().unwrap_or(0),
                },
            );
        }
    }
    Ok(meta)
}
