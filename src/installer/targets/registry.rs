//! Registry of all known agent targets.
//!
//! 注册表是 installer 的唯一 target 顺序来源。新增 agent 时先在这里接入，
//! 再补 contract tests，避免 CLI auto/all 和文档列出的顺序不一致。

use super::antigravity::AntigravityTarget;
use super::claude::ClaudeTarget;
use super::codex::CodexTarget;
use super::cursor::CursorTarget;
use super::gemini::GeminiTarget;
use super::hermes::HermesTarget;
use super::kiro::KiroTarget;
use super::opencode::OpencodeTarget;
use super::types::{AgentTarget, DetectionResult, Location, TARGET_IDS, TargetId};

pub fn all_targets() -> Vec<Box<dyn AgentTarget>> {
    vec![
        Box::new(ClaudeTarget),
        Box::new(CursorTarget),
        Box::new(CodexTarget),
        Box::new(OpencodeTarget),
        Box::new(HermesTarget),
        Box::new(GeminiTarget),
        Box::new(AntigravityTarget),
        Box::new(KiroTarget),
    ]
}

pub fn get_target(id: &str) -> Option<Box<dyn AgentTarget>> {
    all_targets()
        .into_iter()
        .find(|target| target.id().as_str() == id)
}

pub fn list_target_ids() -> Vec<TargetId> {
    TARGET_IDS.to_vec()
}

pub struct DetectedTarget {
    pub target: Box<dyn AgentTarget>,
    pub detection: DetectionResult,
}

pub fn detect_all(loc: Location) -> Vec<DetectedTarget> {
    all_targets()
        .into_iter()
        .map(|target| {
            let detection = target.detect(loc);
            DetectedTarget { target, detection }
        })
        .collect()
}

pub fn resolve_target_flag(
    value: &str,
    loc: Location,
) -> Result<Vec<Box<dyn AgentTarget>>, String> {
    match value {
        "none" => return Ok(Vec::new()),
        "all" => return Ok(all_targets()),
        "auto" => {
            // auto 优先安装已检测到的 agent；全新机器上没有信号时回落 Claude，
            // 保持旧 installer 的默认体验。
            let detected = detect_all(loc)
                .into_iter()
                .filter(|entry| entry.detection.installed)
                .map(|entry| entry.target)
                .collect::<Vec<_>>();
            if !detected.is_empty() {
                return Ok(detected);
            }
            return Ok(get_target("claude").into_iter().collect());
        }
        _ => {}
    }

    let mut resolved = Vec::new();
    let mut unknown = Vec::new();
    for id in value.split(',').map(str::trim).filter(|id| !id.is_empty()) {
        if let Some(target) = get_target(id) {
            resolved.push(target);
        } else {
            unknown.push(id.to_owned());
        }
    }
    if unknown.is_empty() {
        Ok(resolved)
    } else {
        let known = list_target_ids()
            .into_iter()
            .map(TargetId::as_str)
            .collect::<Vec<_>>()
            .join(", ");
        Err(format!(
            "Unknown --target id(s): {}. Known: {}, plus 'auto' / 'all' / 'none'.",
            unknown.join(", "),
            known
        ))
    }
}
