//! `rustcodegraph agent-eval` 子命令分发层。
//!
//! CLI 只负责参数路由和默认路径，具体解析、统计、探针执行都委托给 sibling 模块。
//! 这样脚本和测试可以直接调用报告函数，避免通过 stdout 再反解析。

use std::path::{Path, PathBuf};

use regex::Regex;

use super::arms_report::parse_arms_report;
use super::bench_report::parse_bench_readme_report;
use super::formatting::required_arg;
use super::probes::{probe_explore_text, probe_node_text, probe_sweep_report};
use super::run_report::parse_run_report;
use super::seq_matrix_report::seq_matrix_report;
use super::session_report::parse_session_report;

pub fn run_cli(args: &[String]) -> Result<(), String> {
    match args.get(1).map(String::as_str) {
        Some("parse-run") => {
            let file = required_arg(args, 2, "usage: rustcodegraph agent-eval parse-run <jsonl>")?;
            print!("{}", parse_run_report(Path::new(file))?);
            Ok(())
        }
        Some("parse-session") => {
            let project = required_arg(
                args,
                2,
                "usage: rustcodegraph agent-eval parse-session <project-dir>",
            )?;
            print!("{}", parse_session_report(Path::new(project))?);
            Ok(())
        }
        Some("parse-arms") => {
            // 默认路径对应 scripts/agent-eval 的输出，手工跑命令时通常不需要再传参。
            let root = args
                .get(2)
                .map(PathBuf::from)
                .unwrap_or_else(|| "/tmp/arms".into());
            print!("{}", parse_arms_report(&root)?);
            Ok(())
        }
        Some("parse-bench-readme") => {
            // README benchmark 使用固定的 ab-readme 输出根目录；允许覆盖以便比较旧实验。
            let root = args
                .get(2)
                .map(PathBuf::from)
                .unwrap_or_else(|| "/tmp/ab-readme".into());
            print!("{}", parse_bench_readme_report(&root)?);
            Ok(())
        }
        Some("seq-matrix") => {
            // seq-matrix 支持后续 flag，因此根目录只在第二个参数不是 flag 时读取。
            let root = args
                .get(2)
                .filter(|arg| !arg.starts_with("--"))
                .map(PathBuf::from)
                .unwrap_or_else(|| "/tmp/ab-matrix".into());
            print!("{}", seq_matrix_report(&root, Path::new("."))?);
            Ok(())
        }
        Some("probe-explore") => {
            let repo = required_arg(
                args,
                2,
                "usage: rustcodegraph agent-eval probe-explore <repo> <query>",
            )?;
            let query = args.get(3..).unwrap_or(&[]).join(" ");
            if query.trim().is_empty() {
                return Err(
                    "usage: rustcodegraph agent-eval probe-explore <repo> <query>".to_string(),
                );
            }
            let text = probe_explore_text(Path::new(repo), &query)?;
            println!("{text}");
            eprintln!();
            eprintln!("--- PROBE STATS ---");
            eprintln!("output chars: {}", text.chars().count());
            // 这些信号是 Excalidraw 回归检查的烟雾测试，输出到 stderr 避免污染 probe 正文。
            eprintln!(
                "triggerRender body present (-> setState({{}})): {}",
                Regex::new(r"triggerRender[\s\S]{0,400}setState\(\{\}\)")
                    .expect("probe regex should compile")
                    .is_match(&text)
            );
            eprintln!(
                "App.tsx in source section: {}",
                Regex::new(r"(?m)^#### .*App\.tsx")
                    .expect("probe regex should compile")
                    .is_match(&text)
            );
            Ok(())
        }
        Some("probe-node") => {
            let repo = required_arg(
                args,
                2,
                "usage: rustcodegraph agent-eval probe-node <repo> <symbol> [code]",
            )?;
            let symbol = required_arg(
                args,
                3,
                "usage: rustcodegraph agent-eval probe-node <repo> <symbol> [code]",
            )?;
            let include_code = args.get(4).is_some_and(|arg| arg == "code");
            println!(
                "{}",
                probe_node_text(Path::new(repo), symbol, include_code)?
            );
            Ok(())
        }
        Some("probe-sweep") => {
            print!("{}", probe_sweep_report(args.get(2..).unwrap_or(&[]))?);
            Ok(())
        }
        Some("probe-context") | Some("probe-trace") => {
            // 保留明确错误能防止旧脚本静默调用已退役工具，导致误以为新工具无输出。
            Err("context and trace probes are retired; use probe-explore instead".to_string())
        }
        Some("-h" | "--help") | None => {
            println!("{}", agent_eval_help());
            Ok(())
        }
        Some(other) => Err(format!("unknown agent-eval command '{other}'")),
    }
}

fn agent_eval_help() -> &'static str {
    "Usage: rustcodegraph agent-eval <command>

Commands:
  parse-run <jsonl>
  parse-session <project-dir>
  parse-arms [root]
  parse-bench-readme [root]
  seq-matrix [root]
  probe-explore <repo> <query>
  probe-node <repo> <symbol> [code]
  probe-sweep [--tool=explore] [--repos=a,b]"
}
