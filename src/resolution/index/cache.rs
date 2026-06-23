//! `ReferenceResolver` 的生命周期和缓存管理。
//!
//! resolver 会在一次索引/解析批次内重复查询同一批节点、文件内容和 import
//! 映射；这里集中初始化 LRU 缓存、项目级懒加载状态以及 warmed name/file 快照。

use std::collections::HashMap;

use crate::db::queries::QueryBuilder;
use crate::types::Language;

use super::ReferenceResolver;
use super::helpers::resolve_cache_limit;
use crate::resolution::lru_cache::LruCache;

impl<'db> ReferenceResolver<'db> {
    /// 创建 resolver，并按环境变量配置缓存大小。
    ///
    /// 文件内容缓存默认更小：文件字符串通常比节点/映射条目重，过大时会在
    /// 大仓库解析阶段占用明显内存。
    pub fn new(project_root: impl Into<String>, queries: QueryBuilder<'db>) -> Self {
        let limit = resolve_cache_limit();
        let content_limit = 64.max(limit / 5);
        Self {
            project_root: project_root.into(),
            queries,
            frameworks: Vec::new(),
            deferred_chain_refs: Vec::new(),
            deferred_this_member_refs: Vec::new(),
            razor_usings_cache: HashMap::new(),
            node_cache: LruCache::new(limit),
            file_cache: LruCache::new(content_limit),
            import_mapping_cache: LruCache::new(limit),
            re_export_cache: LruCache::new(limit),
            name_cache: LruCache::new(limit),
            lower_name_cache: LruCache::new(limit),
            qualified_name_cache: LruCache::new(limit),
            known_names: None,
            known_files: None,
            caches_warmed: false,
            project_aliases: None,
            project_aliases_loaded: false,
            go_module: None,
            go_module_loaded: false,
            workspace_packages: None,
            workspace_packages_loaded: false,
        }
    }

    pub fn initialize(&mut self) {
        // Framework-specific resolver registration is translated in task 08.
        self.frameworks.clear();
        self.clear_caches();
    }

    pub fn run_post_extract(&mut self) -> usize {
        // The framework hook is intentionally present, but no framework
        // implementations are translated in this task.
        0
    }

    pub fn warm_caches(&mut self) {
        if self.caches_warmed {
            return;
        }
        // `known_*` 是解析前的轻量索引快照，专门服务快速剪枝；它们不是
        // 权威数据源，真正解析仍会通过 QueryBuilder 拉取节点和边。
        self.known_files = Some(
            self.queries
                .get_all_file_paths()
                .unwrap_or_default()
                .into_iter()
                .collect(),
        );
        self.known_names = Some(
            self.queries
                .get_all_node_names()
                .unwrap_or_default()
                .into_iter()
                .collect(),
        );
        self.caches_warmed = true;
    }

    pub fn clear_caches(&mut self) {
        // 插入新边、运行 post-extract 或切换项目级配置后，都必须清掉这些
        // 派生结果，否则后续 resolver 可能看不到刚写入的图结构。
        self.node_cache.clear();
        self.file_cache.clear();
        self.import_mapping_cache.clear();
        self.re_export_cache.clear();
        self.name_cache.clear();
        self.lower_name_cache.clear();
        self.qualified_name_cache.clear();
        self.known_names = None;
        self.known_files = None;
        self.caches_warmed = false;
    }

    pub fn get_detected_frameworks(&self) -> Vec<String> {
        self.frameworks
            .iter()
            .map(|framework| framework.name().to_string())
            .collect()
    }

    pub(super) fn get_file_path_from_node_id(&mut self, node_id: &str) -> String {
        self.queries
            .get_node_by_id(node_id)
            .unwrap_or(None)
            .map(|node| node.file_path)
            .unwrap_or_default()
    }

    pub(super) fn get_language_from_node_id(&mut self, node_id: &str) -> Language {
        self.queries
            .get_node_by_id(node_id)
            .unwrap_or(None)
            .map(|node| node.language)
            .unwrap_or(Language::Unknown)
    }
}
