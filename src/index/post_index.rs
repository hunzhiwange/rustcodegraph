//! 索引事务提交后的补充解析。
//!
//! 这一步把 unresolved_refs 交给 resolver，随后运行动态边合成和持久化去重；它发生在数据库已有完整节点集之后，
//! 因为继承、回调和动态分派都需要跨文件视图。

use std::sync::atomic::{AtomicU64, Ordering};

use super::*;

/// 累计构造动态边合成上下文时一次性 load 进内存的节点数。
/// 用于观测增量同步的内存修复（OOM）：增量路径应按需查 DB（计数为 0），
/// 只有全量路径才允许把整库节点一次性 load 进 `Vec<Node>`。
static FACADE_SYNTHESIS_NODES_LOADED: AtomicU64 = AtomicU64::new(0);

/// 读取自进程启动以来动态边合成上下文一次性 load 进内存的累计节点数。
pub fn facade_synthesis_nodes_loaded() -> u64 {
    FACADE_SYNTHESIS_NODES_LOADED.load(Ordering::Relaxed)
}

pub(super) fn resolve_facade_reference_queue(project_root: &Path, full_rebuild: bool) -> usize {
    let Ok(mut db) = DatabaseConnection::open(facade_database_path(project_root)) else {
        return 0;
    };
    let queries = QueryBuilder::new(db.get_db());
    let mut resolver = crate::resolution::index::create_resolver(
        project_root.to_string_lossy().into_owned(),
        queries,
    );
    crate::utils::debug_rss("resolve:before resolve_and_persist_batched");
    let result = resolver.resolve_and_persist_batched(None, 1000);
    crate::utils::debug_rss("resolve:after resolve_and_persist_batched");
    let chained = resolver.resolve_chained_calls_via_conformance();
    crate::utils::debug_rss("resolve:after chained");
    let inherited_this = resolver.resolve_deferred_this_member_refs();
    crate::utils::debug_rss("resolve:after deferred_this");
    let resolved = result.stats.resolved as usize + chained + inherited_this;
    drop(resolver);
    crate::utils::debug_rss("resolve:after drop(resolver)");
    // 合成边依赖最新持久化结果；先释放 resolver，避免同一连接上继续借用导致后续写入受限。
    let synthesized = synthesize_facade_dynamic_edges(project_root, full_rebuild);
    crate::utils::debug_rss("resolve:after synthesize");
    let removed = dedupe_facade_persisted_edges(project_root);
    resolved.saturating_add(synthesized).saturating_sub(removed)
}

pub(super) fn dedupe_facade_persisted_edges(project_root: &Path) -> usize {
    let Ok(conn) = open_facade_database(project_root) else {
        return 0;
    };
    conn.execute(
        r#"
        DELETE FROM edges
        WHERE id NOT IN (
            SELECT MIN(id)
            FROM edges
            GROUP BY
                source,
                target,
                kind,
                COALESCE(provenance, ''),
                CASE
                    WHEN kind = 'calls' THEN ''
                    ELSE COALESCE(metadata, '')
                END
        )
        "#,
        [],
    )
    .unwrap_or_default()
}

pub(super) fn synthesize_facade_dynamic_edges(project_root: &Path, full_rebuild: bool) -> usize {
    let Ok(mut db) = DatabaseConnection::open(facade_database_path(project_root)) else {
        return 0;
    };

    if full_rebuild {
        // 全量索引保持原行为：一次性 load 整库节点供合成上下文做 O(1) 内存查询。
        let mut queries = QueryBuilder::new(db.get_db());
        let nodes = queries.get_all_nodes().unwrap_or_default();
        FACADE_SYNTHESIS_NODES_LOADED.fetch_add(nodes.len() as u64, Ordering::Relaxed);
        let files = queries
            .get_all_files()
            .unwrap_or_default()
            .into_iter()
            .map(|file| file.path)
            .collect::<Vec<_>>();
        let mut ctx = FacadeSynthesisContext::new(project_root.to_path_buf(), nodes, files);
        return synthesize_callback_edges(&mut queries, &mut ctx);
    }

    // 增量同步：每次单文件改动都全量 load 节点是 watch OOM 的主因。改为按需查 DB，
    // 让内存占用从 O(全库) 降到 O(单次查询命中)。合成上下文用一条独立连接直查，
    // 与负责写入的 `queries` 连接互不借用。两条连接读同一 WAL 库，产出与全量等价。
    let mut queries = QueryBuilder::new(db.get_db());
    let mut ctx = match DbBackedSynthesisContext::open(project_root) {
        Some(ctx) => ctx,
        // 第二条连接打不开时退回全量内存上下文，宁可慢/吃内存也不漏合成边。
        None => {
            let nodes = queries.get_all_nodes().unwrap_or_default();
            FACADE_SYNTHESIS_NODES_LOADED.fetch_add(nodes.len() as u64, Ordering::Relaxed);
            let files = queries
                .get_all_files()
                .unwrap_or_default()
                .into_iter()
                .map(|file| file.path)
                .collect::<Vec<_>>();
            let mut ctx = FacadeSynthesisContext::new(project_root.to_path_buf(), nodes, files);
            return synthesize_callback_edges(&mut queries, &mut ctx);
        }
    };
    synthesize_callback_edges(&mut queries, &mut ctx)
}

/// 增量同步用的按需查询合成上下文。
///
/// 持有一条独立 `DatabaseConnection`，每个 `get_nodes_by_*` 直接查 DB，不在内存里
/// 保留整库 `Vec<Node>`。产出与 `FacadeSynthesisContext` 等价（读同一已提交索引），
/// 区别只在内存占用：从 O(全库) 降到 O(单次命中)。
struct DbBackedSynthesisContext {
    project_root: PathBuf,
    db: DatabaseConnection,
}

impl DbBackedSynthesisContext {
    fn open(project_root: &Path) -> Option<Self> {
        let db = DatabaseConnection::open(facade_database_path(project_root)).ok()?;
        Some(Self {
            project_root: project_root.to_path_buf(),
            db,
        })
    }

    fn queries(&mut self) -> QueryBuilder<'_> {
        QueryBuilder::new(self.db.get_db())
    }
}

impl ResolutionContext for DbBackedSynthesisContext {
    fn get_nodes_in_file(&mut self, file_path: &str) -> Vec<Node> {
        self.queries()
            .get_nodes_by_file(file_path)
            .unwrap_or_default()
    }

    fn get_nodes_by_name(&mut self, name: &str) -> Vec<Node> {
        self.queries().get_nodes_by_name(name).unwrap_or_default()
    }

    fn get_nodes_by_qualified_name(&mut self, qualified_name: &str) -> Vec<Node> {
        self.queries()
            .get_nodes_by_qualified_name_exact(qualified_name)
            .unwrap_or_default()
    }

    fn get_nodes_by_kind(&mut self, kind: NodeKind) -> Vec<Node> {
        self.queries().get_nodes_by_kind(kind).unwrap_or_default()
    }

    fn file_exists(&mut self, file_path: &str) -> bool {
        if self
            .queries()
            .get_all_file_paths()
            .unwrap_or_default()
            .iter()
            .any(|path| path == file_path)
        {
            return true;
        }
        self.project_root.join(file_path).is_file()
    }

    fn read_file(&mut self, file_path: &str) -> Option<String> {
        fs::read_to_string(self.project_root.join(file_path)).ok()
    }

    fn get_project_root(&self) -> String {
        self.project_root.to_string_lossy().into_owned()
    }

    fn get_all_files(&mut self) -> Vec<String> {
        self.queries().get_all_file_paths().unwrap_or_default()
    }

    fn get_nodes_by_lower_name(&mut self, lower_name: &str) -> Vec<Node> {
        self.queries()
            .get_nodes_by_lower_name(lower_name)
            .unwrap_or_default()
    }

    fn get_import_mappings(&mut self, file_path: &str, language: Language) -> Vec<ImportMapping> {
        self.read_file(file_path)
            .map(|content| extract_import_mappings(file_path, &content, language))
            .unwrap_or_default()
    }

    fn get_node_by_id(&mut self, id: &str) -> Option<Node> {
        self.queries().get_node_by_id(id).unwrap_or(None)
    }

    fn get_re_exports(&mut self, _file_path: &str, _language: Language) -> Vec<ReExport> {
        Vec::new()
    }

    fn list_directories(&mut self, relative_path: &str) -> Vec<String> {
        let path = self.project_root.join(relative_path);
        fs::read_dir(path)
            .ok()
            .into_iter()
            .flat_map(|entries| entries.filter_map(Result::ok))
            .filter(|entry| entry.file_type().map(|ty| ty.is_dir()).unwrap_or(false))
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect()
    }
}

pub(super) struct FacadeSynthesisContext {
    project_root: PathBuf,
    nodes: Vec<Node>,
    files: Vec<String>,
}

impl FacadeSynthesisContext {
    fn new(project_root: PathBuf, nodes: Vec<Node>, files: Vec<String>) -> Self {
        Self {
            project_root,
            nodes,
            files,
        }
    }
}

impl ResolutionContext for FacadeSynthesisContext {
    fn get_nodes_in_file(&mut self, file_path: &str) -> Vec<Node> {
        self.nodes
            .iter()
            .filter(|node| node.file_path == file_path)
            .cloned()
            .collect()
    }

    fn get_nodes_by_name(&mut self, name: &str) -> Vec<Node> {
        self.nodes
            .iter()
            .filter(|node| node.name == name)
            .cloned()
            .collect()
    }

    fn get_nodes_by_qualified_name(&mut self, qualified_name: &str) -> Vec<Node> {
        self.nodes
            .iter()
            .filter(|node| node.qualified_name == qualified_name)
            .cloned()
            .collect()
    }

    fn get_nodes_by_kind(&mut self, kind: NodeKind) -> Vec<Node> {
        self.nodes
            .iter()
            .filter(|node| node.kind == kind)
            .cloned()
            .collect()
    }

    fn file_exists(&mut self, file_path: &str) -> bool {
        // 优先使用索引中的文件列表，回退到磁盘检查以支持 resolver 读取尚未建节点的辅助文件。
        self.files.iter().any(|path| path == file_path)
            || self.project_root.join(file_path).is_file()
    }

    fn read_file(&mut self, file_path: &str) -> Option<String> {
        fs::read_to_string(self.project_root.join(file_path)).ok()
    }

    fn get_project_root(&self) -> String {
        self.project_root.to_string_lossy().into_owned()
    }

    fn get_all_files(&mut self) -> Vec<String> {
        self.files.clone()
    }

    fn get_nodes_by_lower_name(&mut self, lower_name: &str) -> Vec<Node> {
        self.nodes
            .iter()
            .filter(|node| node.name.eq_ignore_ascii_case(lower_name))
            .cloned()
            .collect()
    }

    fn get_import_mappings(&mut self, file_path: &str, language: Language) -> Vec<ImportMapping> {
        self.read_file(file_path)
            .map(|content| extract_import_mappings(file_path, &content, language))
            .unwrap_or_default()
    }

    fn get_node_by_id(&mut self, id: &str) -> Option<Node> {
        self.nodes.iter().find(|node| node.id == id).cloned()
    }

    fn get_re_exports(&mut self, _file_path: &str, _language: Language) -> Vec<ReExport> {
        Vec::new()
    }

    fn list_directories(&mut self, relative_path: &str) -> Vec<String> {
        let path = self.project_root.join(relative_path);
        fs::read_dir(path)
            .ok()
            .into_iter()
            .flat_map(|entries| entries.filter_map(Result::ok))
            .filter(|entry| entry.file_type().map(|ty| ty.is_dir()).unwrap_or(false))
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect()
    }
}
