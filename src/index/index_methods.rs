//! `CodeGraph` 索引入口的同步 facade。
//!
//! 这些方法只维护 facade 实例上的索引状态和版本戳，真正的抽取、解析和持久化在
//! `indexing.rs` / `syncing.rs` 中完成。

use super::*;

impl CodeGraph {
    pub fn index_all(&mut self, _options: IndexOptions) -> IndexResult {
        self.indexing = true;
        let started = Instant::now();
        let result = index_facade_database(&self.project_root, started);
        if result.success && result.files_indexed > 0 {
            // 版本戳用于 `is_index_stale` 判断：只有实际写入文件后才更新，避免空项目误报新索引。
            self.index_build_info = IndexBuildInfo {
                version: Some(env!("CARGO_PKG_VERSION").to_owned()),
                extraction_version: Some(EXTRACTION_VERSION as u64),
            };
        }
        self.indexing = false;
        result
    }

    pub fn index_files(&mut self, _file_paths: &[String]) -> IndexResult {
        self.index_all(IndexOptions::default())
    }

    pub fn sync(&mut self, _options: IndexOptions) -> SyncResult {
        sync_facade_database(&self.project_root, Instant::now())
    }

    pub fn is_indexing(&self) -> bool {
        self.indexing
    }
}
