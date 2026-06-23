//! Agent-eval 的 Rust 侧报告与探针入口。
//!
//! 这些模块解析 Codex JSONL 运行记录、汇总 A/B 指标，并直接调用 MCP 工具实现
//! deterministic probes。它们面向维护者验证 rustcodegraph 是否真的减少 Read/Grep，
//! 因而统计口径需要稳定、保守，不能依赖最后一轮结果里的单点 usage。

mod arms_report;
mod bench_report;
mod cli;
mod formatting;
mod metrics;
mod parser;
mod probes;
mod run_report;
mod seq_matrix_report;
mod session_report;
mod types;

pub use arms_report::parse_arms_report;
pub use bench_report::parse_bench_readme_report;
pub use cli::run_cli;
pub use probes::{probe_explore_text, probe_node_text, probe_sweep_report};
pub use run_report::parse_run_report;
pub use seq_matrix_report::seq_matrix_report;
pub use session_report::{parse_session_report, parse_session_report_with_home};
