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
