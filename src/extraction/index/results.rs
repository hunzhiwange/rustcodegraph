/// 索引阶段用于 CLI/UI 展示进度；阶段名保持粗粒度，避免绑定到具体实现细节。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexPhase {
    Scanning,
    Parsing,
    Storing,
    Resolving,
}

/// 进度回调的最小载荷：当前阶段、数量和可选文件名。
#[derive(Debug, Clone)]
pub struct IndexProgress {
    pub phase: IndexPhase,
    pub current: usize,
    pub total: usize,
    pub current_file: Option<String>,
}

/// 全量或部分索引的汇总结果，调用方据此决定是否展示错误和统计信息。
#[derive(Debug, Clone)]
pub struct IndexResult {
    pub success: bool,
    pub files_indexed: usize,
    pub files_skipped: usize,
    pub files_errored: usize,
    pub nodes_created: usize,
    pub edges_created: usize,
    pub errors: Vec<crate::types::ExtractionError>,
    pub duration_ms: u64,
}

/// 增量同步的汇总结果，专门暴露变更文件列表给 watcher/CLI 做后续展示。
#[derive(Debug, Clone)]
pub struct SyncResult {
    pub files_checked: usize,
    pub files_added: usize,
    pub files_modified: usize,
    pub files_removed: usize,
    pub nodes_updated: usize,
    pub duration_ms: u64,
    pub changed_file_paths: Option<Vec<String>>,
}

/// Git 或哈希比较得到的三类文件变化；删除项必须在写库阶段单独清理图数据。
#[derive(Debug, Clone, Default)]
pub struct GitChanges {
    pub added: Vec<String>,
    pub modified: Vec<String>,
    pub deleted: Vec<String>,
}
