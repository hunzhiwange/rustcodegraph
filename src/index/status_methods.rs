//! 索引状态、版本戳和解析器兼容入口。
//!
//! 部分方法保留为旧 API 兼容层，返回空集合或 no-op，而不是让宿主因为缺少可选能力失败。

use super::*;

impl CodeGraph {
    pub fn get_changed_files(&self) -> ChangedFiles {
        changed_facade_files(&self.project_root).unwrap_or_default()
    }

    pub fn get_last_indexed_at(&self) -> Option<i64> {
        let conn = open_facade_database(&self.project_root).ok()?;
        read_facade_last_indexed_at(&conn).ok().flatten()
    }

    pub fn get_index_build_info(&self) -> IndexBuildInfo {
        read_facade_index_build_info(&self.project_root)
            .unwrap_or_else(|| self.index_build_info.clone())
    }

    pub fn is_index_stale(&self) -> bool {
        if self.get_last_indexed_at().is_none() {
            return false;
        }
        // 提取器版本递增时，旧索引需要重建；缺少版本戳按旧索引处理。
        self.get_index_build_info()
            .extraction_version
            .map(|version| version < EXTRACTION_VERSION as u64)
            .unwrap_or(true)
    }

    pub fn extract_from_source(&self, file_path: &str, source: &str) -> ExtractionResult {
        extract_source_now(file_path, source, None, None)
    }

    pub fn resolve_references(&mut self) -> crate::resolution::types::ResolutionResult {
        let project_root = self.project_root.to_string_lossy().into_owned();
        let queries = QueryBuilder::new(self.db.get_db());
        let mut resolver = crate::resolution::index::create_resolver(project_root, queries);
        let unresolved_refs = resolver
            .queries
            .get_unresolved_references()
            .unwrap_or_default();
        let result = resolver.resolve_and_persist(&unresolved_refs, None);
        resolver.resolve_chained_calls_via_conformance();
        resolver.resolve_deferred_this_member_refs();
        result
    }

    pub fn resolve_references_batched(&mut self) -> crate::resolution::types::ResolutionResult {
        let project_root = self.project_root.to_string_lossy().into_owned();
        let queries = QueryBuilder::new(self.db.get_db());
        let mut resolver = crate::resolution::index::create_resolver(project_root, queries);
        let result = resolver.resolve_and_persist_batched(None, 1000);
        resolver.resolve_chained_calls_via_conformance();
        resolver.resolve_deferred_this_member_refs();
        result
    }

    pub fn get_detected_frameworks(&self) -> Vec<String> {
        Vec::new()
    }

    pub fn reinitialize_resolver(&mut self) {}
}
