//! Shimmer worker protocol types.
//!
//! progress facade 和渲染端只通过这些消息耦合；测试也直接断言消息序列，
//! 所以新增状态时要同步考虑两端处理。

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShimmerWorkerMessage {
    /// percent 为负数表示 total 未知，应展示 count 或省略进度值。
    Update {
        phase: String,
        phase_name: String,
        percent: i32,
        count: usize,
    },
    FinishPhase,
    Stop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShimmerMainMessage {
    Stopped,
}
