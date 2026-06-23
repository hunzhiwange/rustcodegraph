//! Stable context-output sentinel strings.
//!
//! This is intentionally dependency-free so the MCP layer can recognize
//! low-confidence context responses without pulling in the full context module.
//!
//! 这里放跨模块共享的输出哨兵，避免 MCP/tool 层为了判断低置信度响应而依赖
//! `context::index` 的构建器和数据库相关类型。

/// Heading appended to markdown context when retrieval confidence is low.
/// Markdown 文案本身是公开契约；改动时要同步检查 MCP 侧识别逻辑和相关测试。
pub const LOW_CONFIDENCE_MARKER: &str = "### ⚠️ Low-confidence match";
