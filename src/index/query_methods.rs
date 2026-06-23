//! `CodeGraph` 的基础节点、文件和边查询 facade。
//!
//! 这些方法保持同步 API 的宽容语义：打不开 facade 数据库时返回空结果，避免一次状态查询打断宿主流程。

use super::*;

impl CodeGraph {
    pub fn get_node(&mut self, id: &str) -> Option<Node> {
        let conn = open_facade_database(&self.project_root).ok()?;
        conn.query_row("SELECT * FROM nodes WHERE id = ?", [id], row_to_facade_node)
            .optional()
            .ok()
            .flatten()
    }

    pub fn get_nodes_in_file(&mut self, file_path: &str) -> Vec<Node> {
        let Ok(conn) = open_facade_database(&self.project_root) else {
            return Vec::new();
        };
        query_facade_nodes(
            &conn,
            "SELECT * FROM nodes WHERE file_path = ? ORDER BY start_line, name",
            [file_path],
        )
    }

    pub fn get_nodes_by_kind(&mut self, kind: NodeKind) -> Vec<Node> {
        let Ok(conn) = open_facade_database(&self.project_root) else {
            return Vec::new();
        };
        query_facade_nodes(
            &conn,
            "SELECT * FROM nodes WHERE kind = ? ORDER BY file_path, start_line, name",
            [kind_key(kind)],
        )
    }

    pub fn get_nodes_by_name(&mut self, name: &str) -> Vec<Node> {
        let Ok(conn) = open_facade_database(&self.project_root) else {
            return Vec::new();
        };
        query_facade_nodes(
            &conn,
            "SELECT * FROM nodes WHERE name = ? ORDER BY file_path, start_line, kind",
            [name],
        )
    }

    pub fn search_nodes(
        &mut self,
        query: &str,
        options: Option<SearchOptions>,
    ) -> Vec<SearchResult> {
        let mut queries = QueryBuilder::new(self.db.get_db());
        // 搜索默认限制在 100 条，保护 MCP/CLI 输出不会因为宽泛查询而膨胀。
        queries
            .search_nodes(
                query,
                options.unwrap_or(SearchOptions {
                    kinds: None,
                    languages: None,
                    include_patterns: None,
                    exclude_patterns: None,
                    limit: Some(100),
                    offset: None,
                    case_sensitive: None,
                }),
            )
            .unwrap_or_default()
    }

    pub fn get_project_name_tokens(&self) -> HashSet<String> {
        HashSet::new()
    }

    pub fn get_top_route_file(&self) -> Option<TopRouteFile> {
        None
    }

    pub fn get_routing_manifest(&self, _limit: Option<u64>) -> Option<RoutingManifest> {
        None
    }

    pub fn get_outgoing_edges(&mut self, node_id: &str) -> Vec<Edge> {
        let Ok(conn) = open_facade_database(&self.project_root) else {
            return Vec::new();
        };
        query_facade_edges(
            &conn,
            "SELECT * FROM edges WHERE source = ? ORDER BY id",
            [node_id],
        )
    }

    pub fn get_incoming_edges(&mut self, node_id: &str) -> Vec<Edge> {
        let Ok(conn) = open_facade_database(&self.project_root) else {
            return Vec::new();
        };
        query_facade_edges(
            &conn,
            "SELECT * FROM edges WHERE target = ? ORDER BY id",
            [node_id],
        )
    }

    pub fn get_file(&mut self, file_path: &str) -> Option<FileRecord> {
        let conn = open_facade_database(&self.project_root).ok()?;
        conn.query_row(
            "SELECT * FROM files WHERE path = ?",
            [file_path],
            row_to_facade_file,
        )
        .optional()
        .ok()
        .flatten()
    }

    pub fn get_files(&mut self) -> Vec<FileRecord> {
        let Ok(conn) = open_facade_database(&self.project_root) else {
            return Vec::new();
        };
        let Ok(mut stmt) = conn.prepare("SELECT * FROM files ORDER BY path") else {
            return Vec::new();
        };
        stmt.query_map([], row_to_facade_file)
            .map(|rows| rows.filter_map(Result::ok).collect())
            .unwrap_or_default()
    }
}
