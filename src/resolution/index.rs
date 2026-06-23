//! Reference resolution orchestrator.
//!
//! Coordinates import matching, framework hooks, name matching, deferred
//! conformance passes, and callback synthesis. Framework-specific resolver
//! implementations intentionally remain out of this task; the shared
//! `FrameworkResolver` trait and orchestration hook are present for task 08.
//!
//! 这是引用解析的总状态对象：缓存、framework resolver、deferred refs 和项目级
//! alias/module 信息都汇聚在这里，供 `resolve` 子模块按阶段消费。

use std::collections::{HashMap, HashSet};

use crate::db::queries::QueryBuilder;
use crate::types::Node;

use super::go_module::GoModule;
use super::lru_cache::LruCache;
use super::path_aliases::AliasMap;
use super::workspace_packages::WorkspacePackages;

pub use super::types::*;

mod builtins;
mod builtins_core;
mod builtins_native;
mod cache;
mod context;
mod edges;
mod helpers;
mod razor;
mod resolve;
mod this_member;

pub struct ReferenceResolver<'db> {
    // 缓存都挂在 resolver 生命周期上，保证一次索引/同步内复用查询结果；
    // 索引重建时创建新的 resolver，自然丢弃旧快照。
    project_root: String,
    pub queries: QueryBuilder<'db>,
    frameworks: Vec<Box<dyn FrameworkResolver>>,
    deferred_chain_refs: Vec<UnresolvedRef>,
    deferred_this_member_refs: Vec<UnresolvedRef>,
    razor_usings_cache: HashMap<String, Vec<String>>,
    node_cache: LruCache<String, Vec<Node>>,
    file_cache: LruCache<String, Option<String>>,
    import_mapping_cache: LruCache<String, Vec<ImportMapping>>,
    re_export_cache: LruCache<String, Vec<ReExport>>,
    name_cache: LruCache<String, Vec<Node>>,
    lower_name_cache: LruCache<String, Vec<Node>>,
    qualified_name_cache: LruCache<String, Vec<Node>>,
    known_names: Option<HashSet<String>>,
    known_files: Option<HashSet<String>>,
    caches_warmed: bool,
    project_aliases: Option<AliasMap>,
    project_aliases_loaded: bool,
    go_module: Option<GoModule>,
    go_module_loaded: bool,
    workspace_packages: Option<WorkspacePackages>,
    workspace_packages_loaded: bool,
}

pub fn create_resolver<'db>(
    project_root: impl Into<String>,
    queries: QueryBuilder<'db>,
) -> ReferenceResolver<'db> {
    // 所有外部入口都通过 create_resolver，确保 framework 列表和缓存预热逻辑
    // 始终在第一次解析前完成初始化。
    let mut resolver = ReferenceResolver::new(project_root, queries);
    resolver.initialize();
    resolver
}
