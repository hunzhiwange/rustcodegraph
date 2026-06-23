//! agent-eval 报告模块共享的数据模型。
//!
//! 这些结构体只在 `agent_eval` 内部流转，刻意保持扁平，方便不同报告从同一份
//! JSONL 解析结果里复用工具调用、token、矩阵 cell 和 probe 信号。

use std::collections::BTreeMap;

use serde_json::Value;

/// 单次工具调用的归一化记录，保留输出长度用于衡量图工具是否给了足够上下文。
#[derive(Debug, Clone, Default)]
pub(super) struct ToolCall {
    pub(super) id: Option<String>,
    pub(super) name: String,
    pub(super) detail: String,
    pub(super) out_len: usize,
}

/// 一份 JSONL run 解析后的原始事实，尚未折算成报告里的聚合指标。
#[derive(Debug, Clone, Default)]
pub(super) struct ParsedRun {
    pub(super) init_codegraph_tools: Option<usize>,
    pub(super) tool_calls: Vec<ToolCall>,
    pub(super) result: Option<Value>,
    pub(super) assistant_total_tokens: u64,
    pub(super) raced: bool,
}

/// with/without 单臂最常用的指标集合，直接支撑 README 与矩阵报告里的对比口径。
#[derive(Debug, Clone, Default)]
pub(super) struct RunMetrics {
    pub(super) init_codegraph_tools: usize,
    pub(super) reads: usize,
    pub(super) greps: usize,
    pub(super) codegraph_calls: usize,
    pub(super) codegraph_sequence: Vec<String>,
    pub(super) codegraph_output: usize,
    pub(super) trace_used: bool,
    pub(super) turns: Option<f64>,
    pub(super) duration_seconds: Option<f64>,
    pub(super) cost: f64,
    pub(super) ok: bool,
}

/// 在 RunMetrics 之外额外保留调用序列和每个 codegraph 工具的负载拆分。
#[derive(Debug, Clone, Default)]
pub(super) struct SeqMetrics {
    pub(super) base: RunMetrics,
    pub(super) per_tool: BTreeMap<String, ToolPayload>,
    pub(super) sequence: Vec<String>,
    pub(super) after_trace: Option<Vec<String>>,
}

/// 某个工具的调用次数与累计输出字节数，用于发现单工具 payload 漂移。
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct ToolPayload {
    pub(super) n: usize,
    pub(super) out: usize,
}

/// probe-explore 的结构化信号，比纯文本断言更适合跨仓库扫表。
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct ProbeSignals {
    pub(super) has_entry_points: bool,
    pub(super) has_flow_trace: bool,
    pub(super) has_route_manifest: bool,
    pub(super) has_top_handler: bool,
    pub(super) has_small_repo_tail: bool,
}

/// 固定 sweep 目标：id 用于报告行，repo/query 用于实际 probe。
#[derive(Debug, Clone)]
pub(super) struct SweepSubject {
    pub(super) id: &'static str,
    pub(super) repo: &'static str,
    pub(super) query: &'static str,
}

/// 一次 benchmark run 的汇总值；raced 表示 MCP 启动竞态污染了该样本。
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct BenchRun {
    pub(super) duration: f64,
    pub(super) tools: usize,
    pub(super) tokens: u64,
    pub(super) cost: f64,
    pub(super) raced: bool,
}

/// A/B 矩阵中的一个仓库 cell，without 可能缺席以支持只采 with-arm 的诊断跑。
#[derive(Debug, Clone)]
pub(super) struct MatrixCell {
    pub(super) repo: String,
    pub(super) files: Option<usize>,
    pub(super) with: SeqMetrics,
    pub(super) without: Option<SeqMetrics>,
}

/// 从文档矩阵里读取的仓库元数据，目前只需要文件数来分层。
#[derive(Debug, Clone)]
pub(super) struct RepoMeta {
    pub(super) files: usize,
}

/// sweep/probe 输出的一行，error 留在行内而不是提前失败，方便整批报告继续生成。
#[derive(Debug, Clone)]
pub(super) struct SweepRow {
    pub(super) id: String,
    pub(super) ms: u64,
    pub(super) chars: usize,
    pub(super) lines: usize,
    pub(super) signals: ProbeSignals,
    pub(super) error: Option<String>,
}

/// Codex/Claude JSONL 中按用途拆开的 token 合计。
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct TokenTotals {
    pub(super) generated: u64,
    pub(super) fresh: u64,
    pub(super) cached: u64,
}

impl TokenTotals {
    /// 合并主线程与 subagent token；调用方负责保证两边来自同一次会话。
    pub(super) fn add(&mut self, other: TokenTotals) {
        self.generated += other.generated;
        self.fresh += other.fresh;
        self.cached += other.cached;
    }
}
