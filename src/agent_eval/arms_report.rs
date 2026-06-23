//! 多实验 arm 的汇总报告。
//!
//! 输入目录按 repo/arm-run 的形状组织，本模块只消费成功 run，输出每个 repo
//! 和每个 arm 的平均值。它用于判断“工具输出变多/提示变强/移除某工具”到底
//! 是否改变 agent 的真实工具选择，而不是只看单次样例。

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use regex::Regex;

use super::formatting::{arm_label, avg, k_chars, pad};
use super::metrics::run_metrics;
use super::parser::parse_run_file;
use super::types::RunMetrics;

pub fn parse_arms_report(root: &Path) -> Result<String, String> {
    if !root.exists() {
        return Err(format!("no {}", root.display()));
    }
    // 文件名中的 arm 字母是实验设计的一部分；未知文件直接跳过，便于同目录放备注或失败日志。
    let file_re = Regex::new(r"^([A-I])-r(\d+)\.jsonl$").expect("arm regex should compile");
    let mut data: BTreeMap<String, BTreeMap<String, Vec<RunMetrics>>> = BTreeMap::new();
    for repo_entry in fs::read_dir(root).map_err(|err| format!("failed to read root: {err}"))? {
        let repo_entry = repo_entry.map_err(|err| err.to_string())?;
        if !repo_entry
            .file_type()
            .map(|kind| kind.is_dir())
            .unwrap_or(false)
        {
            continue;
        }
        let repo = repo_entry.file_name().to_string_lossy().to_string();
        for entry in fs::read_dir(repo_entry.path()).map_err(|err| err.to_string())? {
            let entry = entry.map_err(|err| err.to_string())?;
            let file_name = entry.file_name().to_string_lossy().to_string();
            let Some(captures) = file_re.captures(&file_name) else {
                continue;
            };
            let metrics = run_metrics(&parse_run_file(&entry.path())?);
            // 失败 run 往往代表 agent/runner 环境异常，混入平均值会误导工具效果判断。
            if metrics.ok {
                data.entry(repo.clone())
                    .or_default()
                    .entry(captures[1].to_string())
                    .or_default()
                    .push(metrics);
            }
        }
    }

    // 顺序按实验叙事排列，而不是字母序，方便直接比较相邻 arm 的假设差异。
    let arms = ["A", "H", "I", "B", "F", "G", "C", "D", "E"];
    let mut out = String::new();
    writeln!(out, "\n=== PER REPO x ARM (avg over runs) ===").unwrap();
    writeln!(
        out,
        "{} {} tools trace {} {} {} dur",
        pad("repo", 22),
        pad("arm", 16),
        pad("reads", 6),
        pad("cgOutK", 7),
        pad("turns", 6)
    )
    .unwrap();
    for (repo, by_arm) in &data {
        for arm in arms {
            let Some(runs) = by_arm.get(arm).filter(|runs| !runs.is_empty()) else {
                continue;
            };
            writeln!(
                out,
                "{} {} {} {} {} {} {:.1} {:.0}s",
                pad(repo, 22),
                pad(arm_label(arm), 16),
                pad(&runs[0].init_codegraph_tools.to_string(), 5),
                pad(
                    &format!(
                        "{}/{}",
                        runs.iter().filter(|run| run.trace_used).count(),
                        runs.len()
                    ),
                    5
                ),
                pad(&format!("{:.1}", avg(runs, |run| run.reads as f64)), 6),
                pad(&k_chars(avg(runs, |run| run.codegraph_output as f64)), 7),
                avg(runs, |run| run.turns.unwrap_or(0.0)),
                avg(runs, |run| run.duration_seconds.unwrap_or(0.0))
            )
            .unwrap();
        }
    }

    writeln!(out, "\n=== AGGREGATE PER ARM (mean across repos) ===").unwrap();
    writeln!(
        out,
        "{} {} {} {} {} {} {} cost",
        pad("arm", 16),
        pad("adoption", 9),
        pad("reads", 7),
        pad("greps", 7),
        pad("cgOutK", 8),
        pad("turns", 7),
        pad("dur", 6)
    )
    .unwrap();
    for arm in arms {
        let mut all = Vec::new();
        let mut repos = BTreeSet::new();
        for (repo, by_arm) in &data {
            if let Some(runs) = by_arm.get(arm) {
                for run in runs {
                    all.push(run.clone());
                    repos.insert(repo.clone());
                }
            }
        }
        if all.is_empty() {
            continue;
        }
        // adoption 用历史 trace 工具是否被实际调用衡量，比初始化时暴露了几个工具更接近效果。
        let adopt = all.iter().filter(|run| run.trace_used).count();
        writeln!(
            out,
            "{} {} {} {} {} {} {} ${:.3}   ({} repos)",
            pad(arm_label(arm), 16),
            pad(&format!("{adopt}/{}", all.len()), 9),
            pad(&format!("{:.2}", avg(&all, |run| run.reads as f64)), 7),
            pad(&format!("{:.2}", avg(&all, |run| run.greps as f64)), 7),
            pad(&k_chars(avg(&all, |run| run.codegraph_output as f64)), 8),
            pad(
                &format!("{:.1}", avg(&all, |run| run.turns.unwrap_or(0.0))),
                7
            ),
            pad(
                &format!(
                    "{:.0}s",
                    avg(&all, |run| run.duration_seconds.unwrap_or(0.0))
                ),
                6
            ),
            avg(&all, |run| run.cost),
            repos.len()
        )
        .unwrap();
    }

    writeln!(
        out,
        "\nRead the signal: B vs A = does steering alone fix adoption + cut payload."
    )
    .unwrap();
    writeln!(
        out,
        "C vs B = is explore redundant (reads should NOT jump). D vs C = is context redundant."
    )
    .unwrap();
    writeln!(
        out,
        "E = non-flow under trace-centric; reads SHOULD jump (proves survey tools are load-bearing)."
    )
    .unwrap();
    Ok(out)
}
