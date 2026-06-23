//! Shimmer worker rendering logic translated without spawning a worker thread.
//!
//! TypeScript 版使用 worker 推动动画；Rust CLI 目前同步渲染，因此这里保留同样的
//! 消息协议和格式化逻辑，但不真正创建后台 worker。

use super::glyphs::{Glyphs, get_glyphs};
use super::types::{ShimmerMainMessage, ShimmerWorkerMessage};

#[derive(Debug, Clone)]
pub struct ShimmerWorkerState {
    pub current_message: String,
    pub current_percent: i32,
    pub current_count: usize,
    pub stopped: bool,
    glyphs: Glyphs,
}

impl Default for ShimmerWorkerState {
    fn default() -> Self {
        Self {
            current_message: String::new(),
            current_percent: -1,
            current_count: 0,
            stopped: false,
            glyphs: get_glyphs(),
        }
    }
}

pub fn format_number(value: usize) -> String {
    let raw = value.to_string();
    let mut out = String::new();
    for (idx, ch) in raw.chars().rev().enumerate() {
        if idx > 0 && idx % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out.chars().rev().collect()
}

pub fn render_bar(glyphs: &Glyphs, percent: i32) -> String {
    let width = 25usize;
    // percent 可能来自外部进度事件，渲染层只负责夹紧，避免条形宽度越界。
    let bounded = percent.clamp(0, 100) as usize;
    let filled = width * bounded / 100;
    format!(
        "{}{}",
        glyphs.bar_filled.repeat(filled),
        glyphs.bar_empty.repeat(width - filled)
    )
}

impl ShimmerWorkerState {
    pub fn handle_message(&mut self, msg: ShimmerWorkerMessage) -> Option<ShimmerMainMessage> {
        match msg {
            ShimmerWorkerMessage::Update {
                phase_name,
                percent,
                count,
                ..
            } => {
                self.current_message = phase_name;
                self.current_percent = percent;
                self.current_count = count;
                None
            }
            ShimmerWorkerMessage::FinishPhase => {
                self.finish_phase();
                None
            }
            ShimmerWorkerMessage::Stop => {
                // Stop 同时清理当前阶段，让调用方最后一帧不会残留旧文案。
                self.finish_phase();
                self.stopped = true;
                Some(ShimmerMainMessage::Stopped)
            }
        }
    }

    pub fn render_line(&self, frame: usize) -> Option<String> {
        if self.current_message.is_empty() {
            return None;
        }
        // frame/3 放慢 spinner 变化，避免快速循环时终端闪烁过密。
        let spinner = self
            .glyphs
            .spinner
            .get((frame / 3) % self.glyphs.spinner.len())
            .copied()
            .unwrap_or(".");
        if self.current_percent >= 0 {
            Some(format!(
                "{}  {} {}  {}  {}%",
                self.glyphs.rail,
                spinner,
                self.current_message,
                render_bar(&self.glyphs, self.current_percent),
                self.current_percent
            ))
        } else if self.current_count > 0 {
            Some(format!(
                "{}  {} {}... {} found",
                self.glyphs.rail,
                spinner,
                self.current_message,
                format_number(self.current_count)
            ))
        } else {
            Some(format!(
                "{}  {} {}...",
                self.glyphs.rail, spinner, self.current_message
            ))
        }
    }

    fn finish_phase(&mut self) {
        self.current_message.clear();
        self.current_percent = -1;
        self.current_count = 0;
    }
}
