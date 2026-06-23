//! README benchmark 表格的生成逻辑。
//!
//! 这里的输出是面向发布文案的稳定摘要：每个仓库取 with/without rustcodegraph
//! 多次运行的中位数，再汇总平均节省比例。中位数比均值更能抵抗 agent 单次跑偏。

use std::collections::HashMap;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use super::formatting::{avg_i64, fmt_time, fmt_tokens, median, pct};
use super::metrics::bench_run_metrics;

pub fn parse_bench_readme_report(root: &Path) -> Result<String, String> {
    // README 只展示这组固定 corpus，避免本地临时目录中的实验仓库污染公开数字。
    let repos = [
        "vscode",
        "excalidraw",
        "django",
        "tokio",
        "okhttp",
        "gin",
        "alamofire",
    ];
    // raced 表示 Codex 在 MCP 启动前先尝试了工具；默认排除，因为那是 attach 时序噪声。
    let include_raced = std::env::var("CG_INCLUDE_RACED").ok().as_deref() == Some("1");
    let mut savings: HashMap<&str, Vec<i64>> = HashMap::new();
    let mut out = String::new();
    writeln!(
        out,
        "repo        n(w/wo)  time WITH->WITHOUT      tools W->WO   tokens W->WO (saved)     cost W->WO (saved)"
    )
    .unwrap();
    for repo in repos {
        let dir = root.join(repo);
        let run_dirs = if dir.exists() {
            fs::read_dir(&dir)
                .map_err(|err| format!("failed to read {}: {err}", dir.display()))?
                .filter_map(Result::ok)
                .filter(|entry| {
                    entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false)
                        && entry.file_name().to_string_lossy().starts_with("run")
                })
                .map(|entry| entry.path())
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let mut with_runs = Vec::new();
        let mut without_runs = Vec::new();
        let mut raced_excluded = 0usize;
        for run_dir in run_dirs {
            if let Some(run) = bench_run_metrics(&run_dir.join("run-headless-with.jsonl"))? {
                if run.raced && !include_raced {
                    raced_excluded += 1;
                } else {
                    with_runs.push(run);
                }
            }
            if let Some(run) = bench_run_metrics(&run_dir.join("run-headless-without.jsonl"))? {
                without_runs.push(run);
            }
        }
        if with_runs.is_empty() || without_runs.is_empty() {
            writeln!(
                out,
                "{:<11} (incomplete: w={} wo={})",
                repo,
                with_runs.len(),
                without_runs.len()
            )
            .unwrap();
            continue;
        }

        // 每个仓库先独立取中位数，再计算节省比例；这样大仓库不会因绝对耗时/成本吞掉小仓库信号。
        let w_time = median(with_runs.iter().map(|run| run.duration).collect());
        let wo_time = median(without_runs.iter().map(|run| run.duration).collect());
        let w_tokens = median(with_runs.iter().map(|run| run.tokens as f64).collect());
        let wo_tokens = median(without_runs.iter().map(|run| run.tokens as f64).collect());
        let w_cost = median(with_runs.iter().map(|run| run.cost).collect());
        let wo_cost = median(without_runs.iter().map(|run| run.cost).collect());
        let w_tools = median(with_runs.iter().map(|run| run.tools as f64).collect());
        let wo_tools = median(without_runs.iter().map(|run| run.tools as f64).collect());

        for (key, value) in [
            ("time", pct(w_time, wo_time)),
            ("tokens", pct(w_tokens, wo_tokens)),
            ("cost", pct(w_cost, wo_cost)),
            ("tools", pct(w_tools, wo_tools)),
        ] {
            savings.entry(key).or_default().push(value);
        }

        writeln!(
            out,
            "{:<11} {}/{}      {:<22}{:<12}{:<24}${:.2}->${:.2} ({}%){}",
            repo,
            with_runs.len(),
            without_runs.len(),
            format!("{}->{}", fmt_time(w_time), fmt_time(wo_time)),
            format!("{}->{}", w_tools.round() as i64, wo_tools.round() as i64),
            format!(
                "{}->{} ({}%)",
                fmt_tokens(w_tokens),
                fmt_tokens(wo_tokens),
                pct(w_tokens, wo_tokens)
            ),
            w_cost,
            wo_cost,
            pct(w_cost, wo_cost),
            if raced_excluded > 0 {
                format!(
                    "  [{raced_excluded} raced run{} excluded]",
                    if raced_excluded == 1 { "" } else { "s" }
                )
            } else {
                String::new()
            }
        )
        .unwrap();
    }
    writeln!(
        out,
        "\nAVERAGE saved:  cost {}%  ·  tokens {}%  ·  time {}%  ·  tool calls {}%",
        avg_i64(savings.get("cost")),
        avg_i64(savings.get("tokens")),
        avg_i64(savings.get("time")),
        avg_i64(savings.get("tools"))
    )
    .unwrap();
    Ok(out)
}
