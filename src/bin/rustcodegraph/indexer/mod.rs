//! CLI indexing shares the library extraction and resolution pipeline.

use std::io::{self, Write};
use std::path::Path;
use std::time::{Duration, Instant};

use rustcodegraph::ui::glyphs::{Glyphs, get_glyphs};
use rustcodegraph::ui::shimmer_worker::render_bar;

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

pub(crate) struct IndexProgressRenderer {
    enabled: bool,
    glyphs: Glyphs,
    phase: String,
    last_render: Instant,
    frame: usize,
}

impl IndexProgressRenderer {
    pub(crate) fn new(enabled: bool) -> Self {
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

    fn message(&mut self, phase: &str, message: impl Into<String>) {
        if !self.enabled {
            return;
        }
        self.render(phase, None, message.into(), true);
    }

    pub(crate) fn on_progress(&mut self, progress: &rustcodegraph::IndexProgress) {
        if !self.enabled {
            return;
        }
        let suffix = progress
            .current_file
            .as_deref()
            .map(|file| format!("{}/{}  {file}", progress.current, progress.total))
            .unwrap_or_default();
        let percent = (progress.total > 0)
            .then(|| ((progress.current as f64 / progress.total as f64) * 100.0).round() as i32);
        self.render(&progress.phase, percent, suffix, false);
    }

    pub(crate) fn finish(&mut self) {
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
            let progress = format!("{}  {}%", render_bar(&self.glyphs, percent), percent);
            if suffix.is_empty() {
                progress
            } else {
                format!("{progress}  {suffix}")
            }
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
    let mut progress = IndexProgressRenderer::new(show_progress);
    progress.message("Indexing code", "parsing and resolving symbols");
    let mut graph =
        rustcodegraph::CodeGraph::open_sync(project_root).map_err(|error| error.to_string())?;
    let result = graph.index_all(rustcodegraph::IndexOptions::default());
    graph.close();
    progress.finish();
    if !result.success {
        return Err(format!("Indexing failed: {:?}", result.errors));
    }
    Ok(IndexSummary {
        files_indexed: result.files_indexed,
        files_skipped: result.files_skipped,
        nodes_created: result.nodes_created,
        edges_created: result.edges_created,
        duration_ms: result.duration_ms as u128,
    })
}
