//! 索引事务提交后的补充解析。
//!
//! 这一步把 unresolved_refs 交给 resolver，随后运行动态边合成和持久化去重；它发生在数据库已有完整节点集之后，
//! 因为继承、回调和动态分派都需要跨文件视图。

use super::*;

pub(super) fn resolve_facade_reference_queue(project_root: &Path) -> usize {
    let Ok(mut db) = DatabaseConnection::open(facade_database_path(project_root)) else {
        return 0;
    };
    let queries = QueryBuilder::new(db.get_db());
    let mut resolver = crate::resolution::index::create_resolver(
        project_root.to_string_lossy().into_owned(),
        queries,
    );
    let result = resolver.resolve_and_persist_batched(None, 1000);
    let chained = resolver.resolve_chained_calls_via_conformance();
    let inherited_this = resolver.resolve_deferred_this_member_refs();
    let resolved = result.stats.resolved as usize + chained + inherited_this;
    drop(resolver);
    // 合成边依赖最新持久化结果；先释放 resolver，避免同一连接上继续借用导致后续写入受限。
    let synthesized = synthesize_facade_dynamic_edges(project_root);
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

pub(super) fn synthesize_facade_dynamic_edges(project_root: &Path) -> usize {
    let Ok(mut db) = DatabaseConnection::open(facade_database_path(project_root)) else {
        return 0;
    };
    let mut queries = QueryBuilder::new(db.get_db());
    let nodes = queries.get_all_nodes().unwrap_or_default();
    let files = queries
        .get_all_files()
        .unwrap_or_default()
        .into_iter()
        .map(|file| file.path)
        .collect::<Vec<_>>();
    let mut ctx = FacadeSynthesisContext::new(project_root.to_path_buf(), nodes, files);
    synthesize_callback_edges(&mut queries, &mut ctx)
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
