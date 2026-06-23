use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use crate::db::queries::QueryBuilder;
use crate::extraction::grammars::{
    detect_language, init_grammars, is_language_supported, language_key,
    load_grammars_for_languages,
};
use crate::extraction::tree_sitter::extract_from_source;
use crate::types::{ExtractionError, ExtractionResult, Language};

use super::constants::{
    FILE_IO_BATCH_SIZE, MAX_FILE_SIZE, PARSE_TIMEOUT_MS, WORKER_RECYCLE_INTERVAL,
};
use super::discovery::{get_git_changed_files, scan_directory};
use super::helpers::{extraction_error, hash_content, tracked_files_placeholder};
use super::results::{GitChanges, IndexPhase, IndexProgress, IndexResult, SyncResult};

/// 负责把“发现文件 -> 解析源码 -> 写入图存储 -> 增量同步”串成一个稳定入口。
///
/// 这里持有 QueryBuilder，但解析本身仍委托给 tree-sitter 层；这样 CLI、库 API 和
/// MCP 服务都能复用同一套索引语义。
pub struct ExtractionOrchestrator<'db> {
    root_dir: PathBuf,
    queries: QueryBuilder<'db>,
    detected_framework_names: Option<Vec<String>>,
}

impl<'db> ExtractionOrchestrator<'db> {
    pub fn new(root_dir: impl Into<PathBuf>, queries: QueryBuilder<'db>) -> Self {
        Self {
            root_dir: root_dir.into(),
            queries,
            detected_framework_names: None,
        }
    }

    fn ensure_detected_frameworks(&mut self, files: Option<&[String]>) -> Vec<String> {
        if let Some(names) = &self.detected_framework_names {
            return names.clone();
        }
        let _files = files
            .map(|files| files.to_vec())
            .unwrap_or_else(|| scan_directory(&self.root_dir, None));
        // 框架探测后续会接到这里；现在保留缓存边界，确保同一次索引中每个文件
        // 传给 extract_from_source 的框架集合一致。
        self.detected_framework_names = Some(Vec::new());
        Vec::new()
    }

    pub async fn index_all<F>(&mut self, mut on_progress: Option<F>, verbose: bool) -> IndexResult
    where
        F: FnMut(IndexProgress),
    {
        let _ = init_grammars().await;
        let start = Instant::now();
        let mut errors = Vec::new();
        let mut files_indexed = 0usize;
        let mut files_skipped = 0usize;
        let mut files_errored = 0usize;
        let mut total_nodes = 0usize;
        let mut total_edges = 0usize;

        emit_progress(&mut on_progress, IndexPhase::Scanning, 0, 0, None);
        let mut scan_cb = |current: usize, file: String| {
            emit_progress(
                &mut on_progress,
                IndexPhase::Scanning,
                current,
                0,
                Some(file),
            );
        };
        let files = scan_directory(&self.root_dir, Some(&mut scan_cb));
        self.detected_framework_names = None;
        let framework_names = self.ensure_detected_frameworks(Some(&files));

        emit_progress(&mut on_progress, IndexPhase::Parsing, 0, files.len(), None);
        let mut languages = files
            .iter()
            .map(|file| detect_language(file, None))
            .collect::<Vec<_>>();
        if languages
            .iter()
            .any(|language| language_key(language) == "c")
        {
            // C/C++ 共用若干语法和头文件场景；发现 C 时预热 C++ grammar，
            // 避免后续遇到混合扩展名时重复加载。
            languages.push(Language::Cpp);
        }
        let _ = load_grammars_for_languages(&languages).await;

        // 文件内容读取仍按小批次推进，给进度回调稳定节奏，也避免大仓库里瞬间打开过多文件。
        for batch in files.chunks(FILE_IO_BATCH_SIZE) {
            for file_path in batch {
                match self.index_file_inner(file_path, &framework_names).await {
                    Ok(result) => {
                        if result.errors.is_empty() {
                            files_indexed += 1;
                        } else {
                            files_errored += 1;
                            errors.extend(result.errors.clone());
                        }
                        total_nodes += result.nodes.len();
                        total_edges += result.edges.len();
                    }
                    Err(error) => {
                        files_skipped += 1;
                        errors.push(error);
                    }
                }
                emit_progress(
                    &mut on_progress,
                    IndexPhase::Parsing,
                    files_indexed + files_skipped + files_errored,
                    files.len(),
                    Some(file_path.clone()),
                );
            }
        }

        emit_progress(
            &mut on_progress,
            IndexPhase::Resolving,
            files.len(),
            files.len(),
            None,
        );
        if verbose {
            let _ = (PARSE_TIMEOUT_MS, WORKER_RECYCLE_INTERVAL);
        }

        IndexResult {
            success: errors.is_empty(),
            files_indexed,
            files_skipped,
            files_errored,
            nodes_created: total_nodes,
            edges_created: total_edges,
            errors,
            duration_ms: start.elapsed().as_millis() as u64,
        }
    }

    pub async fn index_files(&mut self, file_paths: &[String]) -> IndexResult {
        let start = Instant::now();
        let framework_names = self.ensure_detected_frameworks(Some(file_paths));
        let mut out = IndexResult {
            success: true,
            files_indexed: 0,
            files_skipped: 0,
            files_errored: 0,
            nodes_created: 0,
            edges_created: 0,
            errors: Vec::new(),
            duration_ms: 0,
        };

        for file_path in file_paths {
            match self.index_file_inner(file_path, &framework_names).await {
                Ok(result) => {
                    out.files_indexed += 1;
                    out.nodes_created += result.nodes.len();
                    out.edges_created += result.edges.len();
                    out.errors.extend(result.errors);
                }
                Err(error) => {
                    out.files_errored += 1;
                    out.errors.push(error);
                }
            }
        }
        out.success = out.errors.is_empty();
        out.duration_ms = start.elapsed().as_millis() as u64;
        out
    }

    pub async fn index_file(&mut self, relative_path: &str) -> ExtractionResult {
        let framework_names = self.ensure_detected_frameworks(None);
        self.index_file_inner(relative_path, &framework_names)
            .await
            .unwrap_or_else(|error| ExtractionResult {
                nodes: Vec::new(),
                edges: Vec::new(),
                unresolved_references: Vec::new(),
                errors: vec![error],
                duration_ms: 0,
            })
    }

    pub async fn index_file_with_content(
        &mut self,
        relative_path: &str,
        content: &str,
    ) -> ExtractionResult {
        let framework_names = self.ensure_detected_frameworks(None);
        let language = detect_language(relative_path, Some(content));
        if !is_language_supported(language) {
            return ExtractionResult {
                nodes: Vec::new(),
                edges: Vec::new(),
                unresolved_references: Vec::new(),
                errors: vec![extraction_error(
                    format!("Unsupported language: {}", language_key(&language)),
                    Some(relative_path.to_owned()),
                    "unsupported_language",
                )],
                duration_ms: 0,
            };
        }

        let result = extract_from_source(
            relative_path,
            content,
            Some(language),
            Some(&framework_names),
        );
        // 即便内容来自 watcher/调用方内存，也通过同一写入路径保持图数据一致。
        self.store_extraction_result(relative_path, content, &result);
        result
    }

    async fn index_file_inner(
        &mut self,
        relative_path: &str,
        framework_names: &[String],
    ) -> Result<ExtractionResult, ExtractionError> {
        let full_path = self.root_dir.join(relative_path);
        let metadata = fs::metadata(&full_path).map_err(|err| {
            extraction_error(
                format!("Unable to stat file: {err}"),
                Some(relative_path.to_owned()),
                "file_error",
            )
        })?;
        if metadata.len() > MAX_FILE_SIZE {
            return Err(extraction_error(
                "File exceeds maximum indexable size".to_owned(),
                Some(relative_path.to_owned()),
                "file_too_large",
            ));
        }
        let content = fs::read_to_string(&full_path).map_err(|err| {
            extraction_error(
                format!("Unable to read file: {err}"),
                Some(relative_path.to_owned()),
                "file_error",
            )
        })?;
        let language = detect_language(relative_path, Some(&content));
        // 不在这里拒绝 unsupported language：调用者传入的文件通常已由 discovery 过滤，
        // 直接让 extractor 返回结构化错误可以保留更完整的诊断信息。
        let result = extract_from_source(
            relative_path,
            &content,
            Some(language),
            Some(framework_names),
        );
        self.store_extraction_result(relative_path, &content, &result);
        Ok(result)
    }

    fn store_extraction_result(
        &mut self,
        relative_path: &str,
        content: &str,
        result: &ExtractionResult,
    ) {
        let _content_hash = hash_content(content);
        let _ = (relative_path, result, &mut self.queries);
        // 后续接入 QueryBuilder 写库时要保持“先清旧图，再事务性写入新文件记录、
        // 节点、边和 unresolved references”的顺序，避免部分失败留下混合版本图。
    }

    pub async fn sync<F>(&mut self, mut on_progress: Option<F>) -> SyncResult
    where
        F: FnMut(IndexProgress),
    {
        let start = Instant::now();
        let changes = self.get_changed_files();
        let mut changed_file_paths = Vec::new();
        changed_file_paths.extend(changes.added.iter().cloned());
        changed_file_paths.extend(changes.modified.iter().cloned());
        changed_file_paths.extend(changes.deleted.iter().cloned());

        emit_progress(
            &mut on_progress,
            IndexPhase::Parsing,
            0,
            changes.added.len() + changes.modified.len(),
            None,
        );

        let mut nodes_updated = 0usize;
        // 删除单独处理：增改文件可以重建节点，删除文件需要清理已存在的图行。
        for file in changes.added.iter().chain(changes.modified.iter()) {
            let result = self.index_file(file).await;
            nodes_updated += result.nodes.len();
        }

        for removed in &changes.deleted {
            let _ = (removed, &mut self.queries);
            // 后续由 QueryBuilder 删除文件记录以及依赖它的节点/边。
        }

        SyncResult {
            files_checked: changed_file_paths.len(),
            files_added: changes.added.len(),
            files_modified: changes.modified.len(),
            files_removed: changes.deleted.len(),
            nodes_updated,
            duration_ms: start.elapsed().as_millis() as u64,
            changed_file_paths: Some(changed_file_paths),
        }
    }

    pub fn get_changed_files(&self) -> GitChanges {
        if let Some(git_changes) = get_git_changed_files(&self.root_dir) {
            return git_changes;
        }

        // 非 Git 项目只能用“当前扫描结果 vs 已入库文件哈希”判断变化；
        // 这条路径比 Git 慢，但覆盖临时目录、测试夹具和未初始化仓库。
        let current_files = scan_directory(&self.root_dir, None)
            .into_iter()
            .collect::<HashSet<_>>();
        let tracked_files = tracked_files_placeholder(&self.queries);
        let tracked_map = tracked_files
            .iter()
            .map(|file| (file.path.clone(), file.clone()))
            .collect::<HashMap<_, _>>();

        let mut changes = GitChanges::default();
        for tracked in &tracked_files {
            if !current_files.contains(&tracked.path) {
                changes.deleted.push(tracked.path.clone());
            }
        }
        for file_path in current_files {
            let full_path = self.root_dir.join(&file_path);
            let Ok(content) = fs::read_to_string(full_path) else {
                continue;
            };
            let content_hash = hash_content(&content);
            match tracked_map.get(&file_path) {
                None => changes.added.push(file_path),
                Some(tracked) if tracked.content_hash != content_hash => {
                    changes.modified.push(file_path)
                }
                _ => {}
            }
        }
        changes
    }
}

fn emit_progress<F>(
    callback: &mut Option<F>,
    phase: IndexPhase,
    current: usize,
    total: usize,
    current_file: Option<String>,
) where
    F: FnMut(IndexProgress),
{
    // 所有入口复用同一个轻量回调包装，避免在主流程里散落 Option 判断。
    if let Some(callback) = callback.as_mut() {
        callback(IndexProgress {
            phase,
            current,
            total,
            current_file,
        });
    }
}
