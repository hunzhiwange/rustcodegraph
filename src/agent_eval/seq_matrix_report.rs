//! 生成 agent-eval 矩阵的“调用序列/负载”报告。
//!
//! 这个报告不重新跑评测，只消费每个 cell 已落盘的 JSONL 记录和旁路日志，
//! 用同一套口径比较 with/without rustcodegraph 的轮次、工具调用和输出体积。

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use regex::Regex;

use super::formatting::{empty_as_none, fmt_option, k_chars, k_chars_number, pad, tier};
use super::metrics::{read_repo_meta, seq_metrics};
use super::parser::parse_run_file;
use super::types::{MatrixCell, ToolPayload};

pub fn seq_matrix_report(root: &Path, docs_root: &Path) -> Result<String, String> {
    if !root.exists() {
        return Err(format!("no {}", root.display()));
    }
    let repo_meta = read_repo_meta(&docs_root.join("docs/benchmarks/rustcodegraph-ab-matrix.md"))?;
    // cell 目录名可能是短 id；日志里的 repo: 行才是最终展示和元数据匹配用的仓库名。
    let repo_re = Regex::new(r"repo:\s*\S*/([^\s/]+)").expect("repo regex should compile");
    let mut cells = Vec::new();
    for entry in fs::read_dir(root).map_err(|err| format!("failed to read root: {err}"))? {
        let entry = entry.map_err(|err| err.to_string())?;
        if !entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
            continue;
        }
        let dir = entry.path();
        let with_file = dir.join("run-headless-with.jsonl");
        if !with_file.exists() {
            continue;
        }
        let cell_name = entry.file_name().to_string_lossy().to_string();
        let log_file = root.join(format!("{cell_name}.log"));
        let log = fs::read_to_string(&log_file).unwrap_or_default();
        let repo = repo_re
            .captures(&log)
            .map(|captures| captures[1].to_string())
            .unwrap_or_else(|| cell_name.clone());
        let with = seq_metrics(&parse_run_file(&with_file)?);
        let without_file = dir.join("run-headless-without.jsonl");
        let without = if without_file.exists() {
            Some(seq_metrics(&parse_run_file(&without_file)?))
        } else {
            None
        };
        cells.push(MatrixCell {
            repo: repo.clone(),
            files: repo_meta.get(&repo).map(|meta| meta.files),
            with,
            without,
        });
    }
    // 按仓库规模输出，方便直接观察 explore 预算是否随 repo size 单调放大。
    cells.sort_by_key(|cell| cell.files.unwrap_or(0));

    let mut out = String::new();
    writeln!(
        out,
        "\n=== PER-CELL: with-arm codegraph sequence + payload (sorted by repo size) ==="
    )
    .unwrap();
    writeln!(
        out,
        "{} {} trace {} {} turns(w/wo)",
        pad("repo", 22),
        pad("files", 6),
        pad("cg-call sequence", 40),
        pad("cgOutK", 7)
    )
    .unwrap();
    for cell in &cells {
        writeln!(
            out,
            "{} {} {} {} {} {}/{}",
            pad(&cell.repo, 22),
            pad(
                &cell
                    .files
                    .map(|files| files.to_string())
                    .unwrap_or_else(|| "?".to_string()),
                6
            ),
            pad(
                if cell.with.base.trace_used {
                    "YES"
                } else {
                    "no"
                },
                5
            ),
            pad(
                &empty_as_none(&cell.with.base.codegraph_sequence.join(",")),
                40
            ),
            pad(&k_chars(cell.with.base.codegraph_output as f64), 7),
            fmt_option(cell.with.base.turns),
            cell.without
                .as_ref()
                .and_then(|metrics| metrics.base.turns)
                .map(|value| format!("{value:.0}"))
                .unwrap_or_else(|| "?".to_string())
        )
        .unwrap();
    }

    let used = cells
        .iter()
        .filter(|cell| cell.with.base.trace_used)
        .collect::<Vec<_>>();
    // trace 采用率曾经是工具选择质量的核心信号；保留这个段落用于纵向对比旧数据。
    writeln!(
        out,
        "\n=== TRACE ADOPTION (all {} cells are flow questions) ===",
        cells.len()
    )
    .unwrap();
    writeln!(out, "trace called in {}/{} cells", used.len(), cells.len()).unwrap();
    writeln!(
        out,
        "used trace: {}",
        if used.is_empty() {
            "(none)".to_string()
        } else {
            used.iter()
                .map(|cell| cell.repo.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        }
    )
    .unwrap();
    if !used.is_empty() {
        writeln!(
            out,
            "after-trace follow-ups: {}",
            used.iter()
                .map(|cell| format!(
                    "{}[{}]",
                    cell.repo,
                    cell.with
                        .after_trace
                        .as_ref()
                        .map(|items| empty_as_none(&items.join(",")))
                        .unwrap_or_else(|| "none".to_string())
                ))
                .collect::<Vec<_>>()
                .join("  ")
        )
        .unwrap();
    }

    let mut by_tier: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    for cell in &cells {
        by_tier
            .entry(tier(cell.files.unwrap_or(0)).to_string())
            .or_default()
            .push(cell.with.base.codegraph_output as f64);
    }
    // 这里看的是 with-arm 的总 codegraph payload，而不是 token 成本；
    // 目标是发现大仓输出预算过小或过大的行为漂移。
    writeln!(
        out,
        "\n=== with-arm TOTAL codegraph payload by repo-size tier ==="
    )
    .unwrap();
    for name in ["S(<200)", "M(<2000)", "L(>=2000)"] {
        let Some(values) = by_tier.get(name).filter(|values| !values.is_empty()) else {
            continue;
        };
        let average = values.iter().sum::<f64>() / values.len() as f64;
        let min = values.iter().copied().fold(f64::INFINITY, f64::min);
        let max = values.iter().copied().fold(0.0, f64::max);
        writeln!(
            out,
            "  {} n={}  avg cgOut={}K  range {}-{}K",
            pad(name, 10),
            values.len(),
            k_chars_number(average),
            k_chars_number(min),
            k_chars_number(max)
        )
        .unwrap();
    }

    let mut totals: BTreeMap<String, ToolPayload> = BTreeMap::new();
    for cell in &cells {
        for (name, payload) in &cell.with.per_tool {
            let entry = totals.entry(name.clone()).or_default();
            entry.n += payload.n;
            entry.out += payload.out;
        }
    }
    writeln!(
        out,
        "\n=== codegraph tool usage across all cells (n calls, avg payload/call) ==="
    )
    .unwrap();
    let mut totals_vec = totals.into_iter().collect::<Vec<_>>();
    totals_vec.sort_by(|a, b| b.1.n.cmp(&a.1.n).then_with(|| a.0.cmp(&b.0)));
    for (name, payload) in totals_vec {
        writeln!(
            out,
            "  {} calls={} avg={}K/call  total={}K",
            pad(&name, 10),
            pad(&payload.n.to_string(), 4),
            k_chars_number(payload.out as f64 / payload.n.max(1) as f64),
            k_chars_number(payload.out as f64)
        )
        .unwrap();
    }

    let with_turns = cells
        .iter()
        .map(|cell| cell.with.base.turns.unwrap_or(0.0))
        .sum::<f64>();
    let without_turns = cells
        .iter()
        .map(|cell| {
            cell.without
                .as_ref()
                .and_then(|metrics| metrics.base.turns)
                .unwrap_or(0.0)
        })
        .sum::<f64>();
    let with_calls = cells
        .iter()
        .map(|cell| cell.with.base.codegraph_calls)
        .sum::<usize>();
    let toolsearch_all = !cells.is_empty()
        && cells
            .iter()
            .all(|cell| cell.with.sequence.first().is_some_and(|tag| tag == "TS"));
    // ToolSearch 是延迟工具发现带来的固定首轮税；单独报出，避免把它误判成图查询开销。
    writeln!(out, "\n=== ROUND-TRIPS ===").unwrap();
    writeln!(
        out,
        "turns: with={:.0}  without={:.0}  ({:.0}% fewer with)",
        with_turns,
        without_turns,
        if without_turns > 0.0 {
            (1.0 - with_turns / without_turns) * 100.0
        } else {
            0.0
        }
    )
    .unwrap();
    writeln!(
        out,
        "avg turns/cell: with={:.1}  without={:.1}",
        if cells.is_empty() {
            0.0
        } else {
            with_turns / cells.len() as f64
        },
        if cells.is_empty() {
            0.0
        } else {
            without_turns / cells.len() as f64
        }
    )
    .unwrap();
    writeln!(
        out,
        "total rustcodegraph calls={with_calls} (avg {:.1}/cell)",
        if cells.is_empty() {
            0.0
        } else {
            with_calls as f64 / cells.len() as f64
        }
    )
    .unwrap();
    writeln!(
        out,
        "every with-arm opens with a ToolSearch round-trip (deferred tools): {}",
        if toolsearch_all {
            "YES — 1 fixed tax/run"
        } else {
            "no"
        }
    )
    .unwrap();
    Ok(out)
}
