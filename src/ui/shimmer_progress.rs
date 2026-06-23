//! Main-thread shimmer progress facade.
//!
//! 这里不直接渲染终端，而是把索引进度翻译成 worker 消息，方便 CLI 和测试复用
//! 同一套阶段切换语义。

use super::types::ShimmerWorkerMessage;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexProgress {
    pub phase: String,
    pub current: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Default)]
pub struct ShimmerProgress {
    pub messages: Vec<ShimmerWorkerMessage>,
    last_phase: String,
    stopped: bool,
}

fn phase_name(phase: &str) -> String {
    match phase {
        "scanning" => "Scanning files",
        "parsing" => "Parsing code",
        "storing" => "Storing data",
        "resolving" => "Resolving refs",
        other => other,
    }
    .to_owned()
}

pub fn create_shimmer_progress() -> ShimmerProgress {
    ShimmerProgress::default()
}

impl ShimmerProgress {
    pub fn on_progress(&mut self, progress: IndexProgress) {
        if progress.phase != self.last_phase && !self.last_phase.is_empty() {
            // 阶段变化时先收尾上一阶段，渲染端才能清掉旧进度行。
            self.messages.push(ShimmerWorkerMessage::FinishPhase);
        }
        self.last_phase = progress.phase.clone();
        let (percent, count) = if progress.total > 0 {
            (
                ((progress.current as f64 / progress.total as f64) * 100.0).round() as i32,
                0,
            )
        } else if progress.current > 0 {
            // total 未知时用 percent=-1 表示计数模式，避免展示虚假的百分比。
            (-1, progress.current)
        } else {
            (-1, 0)
        };
        self.messages.push(ShimmerWorkerMessage::Update {
            phase: progress.phase.clone(),
            phase_name: phase_name(&progress.phase),
            percent,
            count,
        });
    }

    pub fn stop(&mut self) {
        self.stopped = true;
        self.messages.push(ShimmerWorkerMessage::Stop);
    }

    pub fn is_stopped(&self) -> bool {
        self.stopped
    }
}
