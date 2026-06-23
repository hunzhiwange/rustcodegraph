//! standalone Rust CLI 路径使用的快速 SQLite 索引构建器。
//!
//! 完整库索引走 tree-sitter orchestration；这里的实现偏轻量，目标是为 CLI/MCP
//! shim 快速生成 files/nodes/calls 表，保证基础查询可用。

mod calls;
mod symbols;

use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::time::{Duration, Instant};

use rustcodegraph::extraction::grammars::detect_language;
use rustcodegraph::extraction::index::{hash_content, scan_directory};
use rustcodegraph::types::{ByteSize, FileRecord};
use rustcodegraph::ui::glyphs::{Glyphs, get_glyphs};
use rustcodegraph::ui::shimmer_worker::{format_number, render_bar};

use super::args::{now_ms, system_time_ms};
use super::storage::{open_sqlite_database, write_sqlite_index};
use calls::extract_lightweight_call_edges;
use symbols::{extract_lightweight_symbols, file_node};

#[derive(Debug, Clone)]
pub(crate) struct IndexSummary {
    /// 成功写入 SQLite `files` 表的源码文件数量。
    pub(crate) files_indexed: usize,
    /// 已发现但无法按文本读取而跳过的文件数量。
    pub(crate) files_skipped: usize,
    pub(crate) nodes_created: usize,
    pub(crate) edges_created: usize,
    pub(crate) duration_ms: u128,
}

pub(super) struct IndexProgressRenderer {
    enabled: bool,
    glyphs: Glyphs,
    phase: String,
    last_render: Instant,
    frame: usize,
}

impl IndexProgressRenderer {
    fn new(enabled: bool) -> Self {
        Self {
            enabled,
            glyphs: get_glyphs(),
            phase: String::new(),
            last_render: Instant::now()
                .checked_sub(Duration::from_secs(1))
                .unwrap_or_else(Instant::now),
            frame: 0,
        }
    }

    fn count(&mut self, phase: &str, current: usize) {
        if !self.enabled {
            return;
        }
        let suffix = if current > 0 {
            format!("{} found", format_number(current))
        } else {
            String::new()
        };
        self.render(phase, None, suffix, false);
    }

    fn percent(&mut self, phase: &str, current: usize, total: usize) {
        if !self.enabled {
            return;
        }
        let percent = if total > 0 {
            ((current as f64 / total as f64) * 100.0).round() as i32
        } else {
            0
        };
        self.render(phase, Some(percent), String::new(), false);
    }

    fn message(&mut self, phase: &str, message: impl Into<String>) {
        if !self.enabled {
            return;
        }
        self.render(phase, None, message.into(), true);
    }

    fn finish(&mut self) {
        if !self.enabled || self.phase.is_empty() {
            return;
        }
        eprint!("\r\x1b[2K");
        let _ = io::stderr().flush();
        self.phase.clear();
    }

    fn render(&mut self, phase: &str, percent: Option<i32>, suffix: String, force: bool) {
        // 进度条直接写 stderr，并节流到约 80ms；索引快时不会用大量刷新拖慢终端。
        if self.phase != phase {
            if !self.phase.is_empty() {
                eprintln!(
                    "\r\x1b[2K{}  {} {}",
                    self.glyphs.rail, self.glyphs.phase_done, self.phase
                );
            }
            self.phase = phase.to_owned();
            self.last_render = Instant::now()
                .checked_sub(Duration::from_secs(1))
                .unwrap_or_else(Instant::now);
        }
        if !force && self.last_render.elapsed() < Duration::from_millis(80) {
            return;
        }
        self.last_render = Instant::now();
        self.frame = self.frame.wrapping_add(1);
        let spinner = self
            .glyphs
            .spinner
            .get(self.frame % self.glyphs.spinner.len())
            .copied()
            .unwrap_or(".");
        let detail = if let Some(percent) = percent {
            format!("{}  {}%", render_bar(&self.glyphs, percent), percent)
        } else if suffix.is_empty() {
            String::new()
        } else {
            suffix
        };
        if detail.is_empty() {
            eprint!("\r\x1b[2K{}  {} {}...", self.glyphs.rail, spinner, phase);
        } else {
            eprint!(
                "\r\x1b[2K{}  {} {} {}",
                self.glyphs.rail, spinner, phase, detail
            );
        }
        let _ = io::stderr().flush();
    }
}

pub(crate) fn build_sqlite_index(
    project_root: &Path,
    show_progress: bool,
) -> Result<IndexSummary, String> {
    // 索引时间戳在一次 run 内固定，方便 status 读取“最后索引时间”且避免每行不同。
    let started = std::time::Instant::now();
    let mut conn = open_sqlite_database(project_root)?;
    let mut progress = IndexProgressRenderer::new(show_progress);
    let mut scan_progress = |count: usize, _file: String| {
        progress.count("Scanning files", count);
    };
    let files = scan_directory(project_root, Some(&mut scan_progress));
    progress.message(
        "Scanning files",
        format!("{} found", format_number(files.len())),
    );
    let mut file_records = Vec::new();
    let mut nodes = Vec::new();
    let mut sources = Vec::new();
    let mut files_skipped = 0usize;
    let indexed_at = now_ms();
    let total_files = files.len();

    for (idx, file_path) in files.into_iter().enumerate() {
        progress.percent("Parsing code", idx + 1, total_files);
        let abs = project_root.join(&file_path);
        let Ok(source) = fs::read_to_string(&abs) else {
            // 二进制或权限受限文件只计入 skipped；索引继续前进，避免一个坏文件阻断项目初始化。
            files_skipped += 1;
            continue;
        };
        sources.push((file_path.clone(), source.clone()));
        let metadata = fs::metadata(&abs).ok();
        let language = detect_language(&file_path, Some(&source));
        let mut file_nodes = extract_lightweight_symbols(&file_path, &source, language, indexed_at);
        let file_node = file_node(&file_path, &source, language, indexed_at);
        let node_count = file_nodes.len() + 1;
        nodes.push(file_node);
        nodes.append(&mut file_nodes);
        file_records.push(FileRecord {
            path: file_path,
            content_hash: hash_content(&source),
            language,
            size: metadata.as_ref().map(|m| m.len()).unwrap_or(0) as ByteSize,
            modified_at: metadata
                .and_then(|m| m.modified().ok())
                .map(system_time_ms)
                .unwrap_or(0),
            indexed_at,
            node_count: node_count as u64,
            errors: None,
        });
    }

    // 调用边依赖所有文件的符号表，因此必须等 nodes 收齐后统一解析。
    let edges = extract_lightweight_call_edges(&sources, &nodes, &mut progress);
    progress.message("Writing database", "storing rows");
    write_sqlite_index(&mut conn, &file_records, &nodes, &edges)?;
    progress.finish();
    Ok(IndexSummary {
        files_indexed: file_records.len(),
        files_skipped,
        nodes_created: nodes.len(),
        edges_created: edges.len(),
        duration_ms: started.elapsed().as_millis(),
    })
}
