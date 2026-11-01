//! Parse worker behavior.
//!
//! The TypeScript implementation runs this in `worker_threads`. Rust runtime
//! wiring is deferred, but the message protocol, parser-reset cadence, and
//! crash-on-WASM-corruption policy are represented here.

use std::collections::HashMap;

use crate::extraction::grammars::{
    detect_language, language_key, load_grammars_for_languages, reset_parser,
};
use crate::extraction::tree_sitter::extract_from_source;
use crate::types::{ExtractionError, ExtractionResult, ExtractionSeverity, Language};

pub const PARSER_RESET_INTERVAL: usize = 5000;

/// 主线程发给解析 worker 的最小协议；后续接真实线程时保持这个边界即可。
#[derive(Debug, Clone)]
pub enum ParseWorkerMessage {
    LoadGrammars {
        languages: Vec<Language>,
    },
    Parse {
        id: u64,
        file_path: String,
        content: String,
        framework_names: Vec<String>,
    },
    Shutdown,
}

/// worker 回包带上请求 id，保证并发解析时调用方能把结果对应回文件。
#[derive(Debug, Clone)]
pub enum ParseWorkerReply {
    GrammarsLoaded,
    ParseResult { id: u64, result: ExtractionResult },
    ShutdownAck,
}

#[derive(Default)]
pub struct ParseWorker {
    parse_counts: HashMap<String, usize>,
}

impl ParseWorker {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn handle_message(&mut self, msg: ParseWorkerMessage) -> ParseWorkerReply {
        match msg {
            ParseWorkerMessage::LoadGrammars { languages } => {
                let _ = load_grammars_for_languages(&languages).await;
                ParseWorkerReply::GrammarsLoaded
            }
            ParseWorkerMessage::Parse {
                id,
                file_path,
                content,
                framework_names,
            } => {
                let result = self.parse_file(&file_path, &content, &framework_names);
                ParseWorkerReply::ParseResult { id, result }
            }
            ParseWorkerMessage::Shutdown => ParseWorkerReply::ShutdownAck,
        }
    }

    fn parse_file(
        &mut self,
        file_path: &str,
        content: &str,
        framework_names: &[String],
    ) -> ExtractionResult {
        let language = detect_language(file_path, Some(content));
        // tree-sitter/WASM panic 不能让整个索引流程崩掉；普通 panic 转成结构化错误。
        let result = std::panic::catch_unwind(|| {
            extract_from_source(file_path, content, Some(language), Some(framework_names))
        });

        match result {
            Ok(result) => {
                let key = language_key(&language);
                let count = self.parse_counts.entry(key).or_insert(0);
                *count += 1;
                if (*count).is_multiple_of(PARSER_RESET_INTERVAL) {
                    // 长时间复用 parser 可能积累状态/内存碎片，按语言计数定期重置。
                    reset_parser(language);
                }
                result
            }
            Err(err) => {
                let message = panic_message(err);
                if message.contains("memory access out of bounds")
                    || message.contains("out of memory")
                {
                    // 这类 WASM 堆损坏必须让 worker 退出重建；返回错误会让后续解析继续使用坏堆。
                    panic!("{message}");
                }
                ExtractionResult {
                    nodes: Vec::new(),
                    edges: Vec::new(),
                    unresolved_references: Vec::new(),
                    errors: vec![ExtractionError {
                        message: format!("Parse worker error: {message}"),
                        file_path: Some(file_path.to_owned()),
                        line: None,
                        column: None,
                        severity: ExtractionSeverity::Error,
                        code: Some("parse_error".to_owned()),
                    }],
                    duration_ms: 0,
                }
            }
        }
    }
}

fn panic_message(err: Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = err.downcast_ref::<String>() {
        s.clone()
    } else if let Some(s) = err.downcast_ref::<&str>() {
        (*s).to_owned()
    } else {
        "unknown panic".to_owned()
    }
}

/// Run a bounded window of independent jobs, assigning the next job to whichever
/// worker finishes first. Results retain input order; callers bound queued memory.
pub fn schedule_parses<T: Sync, R: Send>(
    jobs: &[T],
    memory_budget: usize,
    bytes_per_worker: usize,
    parse: impl Fn(&T) -> R + Sync,
) -> Vec<R> {
    let workers = jobs
        .len()
        .min(std::thread::available_parallelism().map_or(1, usize::from))
        .min((memory_budget / bytes_per_worker.max(1)).max(1));
    if workers <= 1 {
        return jobs.iter().map(parse).collect();
    }
    let cursor = std::sync::atomic::AtomicUsize::new(0);
    std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                let parse = &parse;
                let cursor = &cursor;
                scope.spawn(move || {
                    let mut results = Vec::new();
                    loop {
                        let i = cursor.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(job) = jobs.get(i) else { break };
                        results.push((i, parse(job)));
                    }
                    results
                })
            })
            .collect();
        let mut results: Vec<_> = handles
            .into_iter()
            .flat_map(|h| h.join().expect("parse worker panicked"))
            .collect();
        results.sort_by_key(|(i, _)| *i);
        results.into_iter().map(|(_, result)| result).collect()
    })
}

/// Snapshot of memory currently available for additional work. Failure is
/// conservative: callers run serially rather than assume unused memory exists.
pub fn available_parse_memory() -> Option<usize> {
    #[cfg(target_os = "linux")]
    {
        parse_available_memory(&std::fs::read_to_string("/proc/meminfo").ok()?, "linux")
    }
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("/usr/bin/vm_stat")
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        parse_available_memory(std::str::from_utf8(&output.stdout).ok()?, "macos")
    }
    #[cfg(target_os = "windows")]
    {
        let output = std::process::Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "(Get-CimInstance Win32_OperatingSystem).FreePhysicalMemory",
            ])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        parse_available_memory(std::str::from_utf8(&output.stdout).ok()?, "windows")
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        None
    }
}

/// Decode OS memory snapshots separately from acquisition for deterministic tests.
#[doc(hidden)]
pub fn parse_available_memory(text: &str, platform: &str) -> Option<usize> {
    match platform {
        "linux" => {
            let value = text
                .lines()
                .find_map(|line| line.strip_prefix("MemAvailable:"))?;
            let mut parts = value.split_whitespace();
            let kb: usize = parts.next()?.parse().ok()?;
            if parts.next()? != "kB" {
                return None;
            }
            kb.checked_mul(1024)
        }
        "windows" => text.trim().parse::<usize>().ok()?.checked_mul(1024),
        "macos" => {
            let page_size: usize = text
                .lines()
                .next()?
                .split("page size of ")
                .nth(1)?
                .split_whitespace()
                .next()?
                .parse()
                .ok()?;
            // Only count free/speculative pages; exclude inactive/compressed pages
            // which may require reclaim or swap before they become usable.
            let pages = |key: &str| -> Option<usize> {
                text.lines()
                    .find_map(|line| line.strip_prefix(key))?
                    .trim()
                    .trim_end_matches('.')
                    .parse()
                    .ok()
            };
            pages("Pages free:")?
                .checked_add(pages("Pages speculative:")?)?
                .checked_mul(page_size)
        }
        _ => None,
    }
}

/// Reserve half of currently available memory for the application and OS, and
/// cap parse concurrency to a modest working set even on very large machines.
pub fn parse_memory_budget(available: Option<usize>) -> usize {
    available.map_or(0, |bytes| (bytes / 2).min(256 * 1024 * 1024))
}
