//! 单个 Codex JSONL run 的可读摘要。
//!
//! 这个报告用于排查某次运行为什么回退到 Read/Grep：先看初始化时 rustcodegraph
//! 工具是否暴露，再看完整工具序列和最后结果摘要。

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use serde_json::Value;

use super::formatting::{file_name, number_f64, number_u64};
use super::parser::parse_run_file;

pub fn parse_run_report(file: &Path) -> Result<String, String> {
    let parsed = parse_run_file(file)?;
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for call in &parsed.tool_calls {
        *counts.entry(call.name.clone()).or_default() += 1;
    }

    let mut out = String::new();
    writeln!(out, "\n=== {} ===", file_name(file)).unwrap();
    writeln!(
        out,
        "rustcodegraph tools exposed: {}",
        parsed
            .init_codegraph_tools
            .map(|count| count.to_string())
            .unwrap_or_else(|| "?".to_string())
    )
    .unwrap();
    writeln!(out, "\nTool calls ({}):", parsed.tool_calls.len()).unwrap();
    writeln!(
        out,
        "  by type: {}",
        serde_json::to_string(&counts).map_err(|err| err.to_string())?
    )
    .unwrap();
    for (index, call) in parsed.tool_calls.iter().enumerate() {
        // detail 已在 parser 中截断，这里保持一行一个调用，方便肉眼比较调用顺序。
        writeln!(out, "  {}. {}{}", index + 1, call.name, call.detail).unwrap();
    }

    if let Some(result) = parsed.result.as_ref() {
        // 这里展示的是结果事件自带 usage；README benchmark 的总 token 口径在 parser 中逐轮累计。
        let usage = result.get("usage").unwrap_or(&Value::Null);
        let total_in = number_u64(usage.get("input_tokens"))
            + number_u64(usage.get("cache_read_input_tokens"))
            + number_u64(usage.get("cache_creation_input_tokens"));
        let output_tokens = number_u64(usage.get("output_tokens"));
        let duration = number_f64(result.get("duration_ms")).unwrap_or(0.0) / 1000.0;
        let subtype = result
            .get("subtype")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let turns = number_u64(result.get("num_turns"));
        let cost = number_f64(result.get("total_cost_usd")).unwrap_or(0.0);
        writeln!(
            out,
            "\nResult: {subtype} | duration {:.0}s | turns {turns}",
            duration
        )
        .unwrap();
        writeln!(
            out,
            "  tokens: in={total_in} out={output_tokens} | cost ${cost:.3}"
        )
        .unwrap();
    }

    Ok(out)
}
