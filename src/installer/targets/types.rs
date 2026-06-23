//! Agent target abstraction for the Rust installer translation.
//!
//! The TypeScript installer owns real filesystem edits today. This Rust pass
//! preserves the same target ids, method names, result shapes, and user-facing
//! notes, while target implementations return planned file actions instead of
//! mutating agent configuration files.
//!
//! 每个 target 自己拥有路径、detect、install、uninstall 和 print-config；
//! 上层 orchestrator 只消费这个 trait，避免把 agent 专属格式泄漏到入口层。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Location {
    Global,
    Local,
}

impl Location {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Local => "local",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "global" => Some(Self::Global),
            "local" => Some(Self::Local),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TargetId {
    Claude,
    Cursor,
    Codex,
    Opencode,
    Hermes,
    Gemini,
    Antigravity,
    Kiro,
}

impl TargetId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Cursor => "cursor",
            Self::Codex => "codex",
            Self::Opencode => "opencode",
            Self::Hermes => "hermes",
            Self::Gemini => "gemini",
            Self::Antigravity => "antigravity",
            Self::Kiro => "kiro",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "claude" => Some(Self::Claude),
            "cursor" => Some(Self::Cursor),
            "codex" => Some(Self::Codex),
            "opencode" => Some(Self::Opencode),
            "hermes" => Some(Self::Hermes),
            "gemini" => Some(Self::Gemini),
            "antigravity" => Some(Self::Antigravity),
            "kiro" => Some(Self::Kiro),
            _ => None,
        }
    }
}

pub const TARGET_IDS: &[TargetId] = &[
    TargetId::Claude,
    TargetId::Cursor,
    TargetId::Codex,
    TargetId::Opencode,
    TargetId::Hermes,
    TargetId::Gemini,
    TargetId::Antigravity,
    TargetId::Kiro,
];

/// Result of `target.detect(location)`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectionResult {
    pub installed: bool,
    pub already_configured: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WriteAction {
    Created,
    Updated,
    Unchanged,
    Removed,
    NotFound,
    Kept,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileWrite {
    pub path: String,
    pub action: WriteAction,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WriteResult {
    pub files: Vec<FileWrite>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<Vec<String>>,
}

impl WriteResult {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn with_file(file: FileWrite) -> Self {
        Self {
            files: vec![file],
            notes: None,
        }
    }

    pub fn with_notes(mut self, notes: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.notes = Some(notes.into_iter().map(Into::into).collect());
        self
    }

    pub fn push(&mut self, file: FileWrite) {
        self.files.push(file);
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallOptions {
    pub auto_allow: bool,
}

impl Default for InstallOptions {
    fn default() -> Self {
        Self { auto_allow: true }
    }
}

pub trait AgentTarget {
    // `supports_location` 必须先于 install/uninstall 检查；global-only agent
    // 返回空写入加 note，而不是在 target 内部假装写了 local 配置。
    fn id(&self) -> TargetId;
    fn display_name(&self) -> &'static str;
    fn docs_url(&self) -> Option<&'static str> {
        None
    }
    fn supports_location(&self, loc: Location) -> bool;
    fn detect(&self, loc: Location) -> DetectionResult;
    fn install(&self, loc: Location, opts: InstallOptions) -> WriteResult;
    fn uninstall(&self, loc: Location) -> WriteResult;
    fn print_config(&self, loc: Location) -> String;
    fn describe_paths(&self, loc: Location) -> Vec<String>;
}

pub fn file_write(path: impl Into<String>, action: WriteAction) -> FileWrite {
    FileWrite {
        path: path.into(),
        action,
    }
}
