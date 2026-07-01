//! Shared MCP engine translated from `engine.ts`.
//!
//! engine 持有 session 级的 ToolHandler 和项目路径提示，但尽量延迟打开
//! SQLite。未索引工作区必须表现为空工具列表，而不是一组会失败的工具。

use std::path::{Path, PathBuf};

use crate::directory::find_nearest_code_graph_root;

use super::tools::ToolHandler;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MCPEngineOptions {
    pub watch: bool,
}

impl Default for MCPEngineOptions {
    fn default() -> Self {
        Self { watch: true }
    }
}

#[derive(Debug, Clone)]
pub struct MCPEngine {
    project_path: Option<PathBuf>,
    tool_handler: ToolHandler,
    watcher_started: bool,
    opts: MCPEngineOptions,
    closed: bool,
}

impl MCPEngine {
    pub fn new(opts: Option<MCPEngineOptions>) -> Self {
        Self {
            project_path: None,
            tool_handler: ToolHandler::new(false),
            watcher_started: false,
            opts: opts.unwrap_or_default(),
            closed: false,
        }
    }

    pub fn set_project_path_hint(&mut self, project_path: impl Into<PathBuf>) {
        // rootUri/workspaceFolders 可能先于索引发现到达；先保存 hint，
        // 之后工具报未索引时可以给出准确路径。
        let path = project_path.into();
        self.tool_handler
            .set_default_project_hint(path.to_string_lossy().to_string());
        self.project_path = Some(path);
    }

    pub fn get_project_path(&self) -> Option<&Path> {
        self.project_path.as_deref()
    }

    pub fn get_tool_handler(&mut self) -> &mut ToolHandler {
        &mut self.tool_handler
    }

    pub fn has_default_code_graph(&self) -> bool {
        self.tool_handler.has_default_code_graph()
    }

    /// Resolve the nearest index root, but defer actually opening SQLite.
    pub fn ensure_initialized(&mut self, search_from: impl AsRef<Path>) {
        if self.closed || self.has_default_code_graph() {
            return;
        }
        self.tool_handler
            .set_default_project_hint(search_from.as_ref().to_string_lossy().to_string());
        if let Some(root) = find_nearest_code_graph_root(search_from.as_ref()) {
            self.project_path = Some(root);
            // Runtime backend open is deferred; keep loaded=false so tools/list
            // can still model the unindexed/empty-tools behavior until wired.
        } else {
            self.project_path = Some(search_from.as_ref().to_path_buf());
        }
    }

    pub fn retry_initialize_sync(&mut self, search_from: impl AsRef<Path>) {
        self.ensure_initialized(search_from);
    }

    pub fn stop(&mut self) {
        if self.closed {
            return;
        }
        self.closed = true;
        self.tool_handler.close_all();
    }

    pub fn watcher_started(&self) -> bool {
        self.watcher_started
    }

    pub fn watch_enabled(&self) -> bool {
        self.opts.watch
    }
}

pub fn parse_debounce_env(raw: Option<&str>) -> Option<u64> {
    parse_watch_duration_env(raw)
}

pub fn parse_watch_policy_from_env() -> crate::WatchOptions {
    parse_watch_policy_env(
        std::env::var("RUSTCODEGRAPH_WATCH_DEBOUNCE_MS")
            .ok()
            .as_deref(),
        std::env::var("RUSTCODEGRAPH_WATCH_MAX_DEBOUNCE_MS")
            .ok()
            .as_deref(),
        std::env::var("RUSTCODEGRAPH_WATCH_MIN_SYNC_INTERVAL_MS")
            .ok()
            .as_deref(),
    )
}

pub fn parse_watch_policy_env(
    debounce_ms: Option<&str>,
    max_debounce_ms: Option<&str>,
    min_sync_interval_ms: Option<&str>,
) -> crate::WatchOptions {
    crate::WatchOptions {
        debounce_ms: parse_watch_duration_env(debounce_ms),
        max_debounce_ms: parse_watch_duration_env(max_debounce_ms),
        min_sync_interval_ms: parse_watch_duration_env(min_sync_interval_ms),
    }
}

fn parse_watch_duration_env(raw: Option<&str>) -> Option<u64> {
    let raw = raw?;
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let parsed = raw.parse::<f64>().ok()?;
    if parsed.is_finite() && parsed.fract() == 0.0 && (100.0..=60_000.0).contains(&parsed) {
        Some(parsed as u64)
    } else {
        None
    }
}
