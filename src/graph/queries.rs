//! Higher-level graph query functions built on traversal algorithms.
//!
//! 这一层组合 DB 查询和 traversal 算法，面向 SDK/MCP 提供“上下文、依赖、指标”
//! 这类高层结果；低层 BFS/DFS 细节留在 `traversal.rs`。

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::db::queries::QueryBuilder;
use crate::db::sqlite_adapter::{SqliteError, SqliteResult};
use crate::types::{Context, Edge, EdgeKind, Node, NodeEdgeRef, NodeKind, Subgraph};

use super::traversal::GraphTraverser;

/// Complexity metrics for a node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeMetrics {
    pub incoming_edge_count: usize,
    pub outgoing_edge_count: usize,
    pub call_count: usize,
    pub caller_count: usize,
    pub child_count: usize,
    pub depth: usize,
}

/// Graph query manager for complex queries.
pub struct GraphQueryManager<'a, 'db> {
    queries: &'a mut QueryBuilder<'db>,
}

impl<'a, 'db> GraphQueryManager<'a, 'db> {
    pub fn new(queries: &'a mut QueryBuilder<'db>) -> Self {
        Self { queries }
    }

    /// Get full context for a node.
    pub fn get_context(&mut self, node_id: &str) -> SqliteResult<Context> {
        // context 以 focal node 为中心，包含结构父子、非 contains 引用、类型和文件
        // imports。这里有意排除 contains 引用，避免“结构关系”污染调用/引用面。
        let Some(focal) = self.queries.get_node_by_id(node_id)? else {
            return Err(SqliteError::new(format!("Node not found: {node_id}")));
        };

        let (ancestors, children) = {
            let mut traverser = GraphTraverser::new(self.queries);
            (
                traverser.get_ancestors(node_id)?,
                traverser.get_children(node_id)?,
            )
        };

        let mut incoming_refs = Vec::new();
        for edge in self.queries.get_incoming_edges(node_id, None)? {
            if edge.kind == EdgeKind::Contains {
                continue;
            }
            if let Some(node) = self.queries.get_node_by_id(&edge.source)? {
                incoming_refs.push(NodeEdgeRef { node, edge });
            }
        }

        let mut outgoing_refs = Vec::new();
        for edge in self.queries.get_outgoing_edges(node_id, None, None)? {
            if edge.kind == EdgeKind::Contains {
                continue;
            }
            if let Some(node) = self.queries.get_node_by_id(&edge.target)? {
                outgoing_refs.push(NodeEdgeRef { node, edge });
            }
        }

        let mut types = Vec::new();
        for kind in [EdgeKind::TypeOf, EdgeKind::Returns] {
            for edge in self
                .queries
                .get_outgoing_edges(node_id, Some(vec![kind]), None)?
            {
                if let Some(type_node) = self.queries.get_node_by_id(&edge.target)?
                    && !types.iter().any(|node: &Node| node.id == type_node.id)
                {
                    types.push(type_node);
                }
            }
        }

        let mut imports = Vec::new();
        if let Some(file_node) = ancestors.iter().find(|node| node.kind == NodeKind::File) {
            for edge in self.queries.get_outgoing_edges(
                &file_node.id,
                Some(vec![EdgeKind::Imports]),
                None,
            )? {
                if let Some(import_node) = self.queries.get_node_by_id(&edge.target)? {
                    imports.push(import_node);
                }
            }
        }

        Ok(Context {
            focal,
            ancestors,
            children,
            incoming_refs,
            outgoing_refs,
            types,
            imports,
        })
    }

    /// Get all files that `file_path` depends on.
    pub fn get_file_dependencies(&mut self, file_path: &str) -> SqliteResult<Vec<String>> {
        self.queries.get_dependency_file_paths(file_path)
    }

    /// Get all files that depend on `file_path`.
    pub fn get_file_dependents(&mut self, file_path: &str) -> SqliteResult<Vec<String>> {
        self.queries.get_dependent_file_paths(file_path)
    }

    /// Get all symbols exported by a file.
    pub fn get_exported_symbols(&mut self, file_path: &str) -> SqliteResult<Vec<Node>> {
        Ok(self
            .queries
            .get_nodes_by_file(file_path)?
            .into_iter()
            .filter(node_is_exported)
            .collect())
    }

    /// Find symbols by qualified name pattern. Supports `*` and `?` wildcards.
    pub fn find_by_qualified_name(&mut self, pattern: &str) -> SqliteResult<Vec<Node>> {
        let mut all_nodes = Vec::new();
        let kinds = [
            NodeKind::Class,
            NodeKind::Function,
            NodeKind::Method,
            NodeKind::Interface,
            NodeKind::TypeAlias,
            NodeKind::Variable,
            NodeKind::Constant,
        ];

        for kind in kinds {
            for node in self.queries.get_nodes_by_kind(kind)? {
                if wildcard_match(pattern, &node.qualified_name) {
                    all_nodes.push(node);
                }
            }
        }

        Ok(all_nodes)
    }

    /// Get files organized by directory path.
    pub fn get_module_structure(&mut self) -> SqliteResult<BTreeMap<String, Vec<String>>> {
        let files = self.queries.get_all_files()?;
        let mut structure: BTreeMap<String, Vec<String>> = BTreeMap::new();

        for file in files {
            let dir = file
                .path
                .rsplit_once('/')
                .map(|(dir, _)| if dir.is_empty() { "." } else { dir })
                .unwrap_or(".");
            structure
                .entry(dir.to_string())
                .or_default()
                .push(file.path);
        }

        Ok(structure)
    }

    /// Find circular file dependencies.
    pub fn find_circular_dependencies(&mut self) -> SqliteResult<Vec<Vec<String>>> {
        // cycle 检测按文件依赖图做 DFS，使用 recursion_stack 区分“已完成访问”
        // 和“当前路径上”，否则共享依赖会被误报为环。
        let files = self.queries.get_all_files()?;
        let mut cycles = Vec::new();
        let mut visited = HashSet::new();
        let mut recursion_stack = HashSet::new();

        for file in files {
            if !visited.contains(&file.path) {
                self.find_cycles_dfs(
                    &file.path,
                    Vec::new(),
                    &mut cycles,
                    &mut visited,
                    &mut recursion_stack,
                )?;
            }
        }

        Ok(cycles)
    }

    fn find_cycles_dfs(
        &mut self,
        file_path: &str,
        path: Vec<String>,
        cycles: &mut Vec<Vec<String>>,
        visited: &mut HashSet<String>,
        recursion_stack: &mut HashSet<String>,
    ) -> SqliteResult<()> {
        if recursion_stack.contains(file_path) {
            if let Some(cycle_start) = path.iter().position(|p| p == file_path) {
                cycles.push(path[cycle_start..].to_vec());
            }
            return Ok(());
        }

        if visited.contains(file_path) {
            return Ok(());
        }

        visited.insert(file_path.to_string());
        recursion_stack.insert(file_path.to_string());

        for dep in self.get_file_dependencies(file_path)? {
            let mut next_path = path.clone();
            next_path.push(file_path.to_string());
            self.find_cycles_dfs(&dep, next_path, cycles, visited, recursion_stack)?;
        }

        recursion_stack.remove(file_path);
        Ok(())
    }

    /// Get complexity metrics for a node.
    pub fn get_node_metrics(&mut self, node_id: &str) -> SqliteResult<NodeMetrics> {
        // calls/callers 按 source/target/位置去重，避免同一调用被多条启发式边重复
        // 计入复杂度指标。
        let incoming_edges = self.queries.get_incoming_edges(node_id, None)?;
        let outgoing_edges = self.queries.get_outgoing_edges(node_id, None, None)?;

        let call_count = outgoing_edges
            .iter()
            .filter(|edge| edge.kind == EdgeKind::Calls)
            .map(|edge| (edge.target.as_str(), edge.line, edge.column))
            .collect::<HashSet<_>>()
            .len();
        let caller_count = incoming_edges
            .iter()
            .filter(|edge| edge.kind == EdgeKind::Calls)
            .map(|edge| (edge.source.as_str(), edge.line, edge.column))
            .collect::<HashSet<_>>()
            .len();
        let child_count = outgoing_edges
            .iter()
            .filter(|edge| edge.kind == EdgeKind::Contains)
            .count();
        let depth = {
            let mut traverser = GraphTraverser::new(self.queries);
            traverser.get_ancestors(node_id)?.len()
        };

        Ok(NodeMetrics {
            incoming_edge_count: incoming_edges.len(),
            outgoing_edge_count: outgoing_edges.len(),
            call_count,
            caller_count,
            child_count,
            depth,
        })
    }

    /// Find dead code: nodes with no incoming non-containment references.
    pub fn find_dead_code(&mut self, kinds: Option<&[NodeKind]>) -> SqliteResult<Vec<Node>> {
        let default_kinds = [NodeKind::Function, NodeKind::Method, NodeKind::Class];
        let target_kinds = kinds.unwrap_or(&default_kinds);
        let mut dead_code = Vec::new();

        for kind in target_kinds {
            for node in self.queries.get_nodes_by_kind(*kind)? {
                if node_is_exported(&node) {
                    continue;
                }

                let references: Vec<Edge> = self
                    .queries
                    .get_incoming_edges(&node.id, None)?
                    .into_iter()
                    .filter(|edge| edge.kind != EdgeKind::Contains)
                    .collect();

                if references.is_empty() {
                    dead_code.push(node);
                }
            }
        }

        Ok(dead_code)
    }

    /// Get subgraph containing nodes matching a filter.
    pub fn get_filtered_subgraph<F>(
        &mut self,
        filter: F,
        include_edges: Option<bool>,
    ) -> SqliteResult<Subgraph>
    where
        F: Fn(&Node) -> bool,
    {
        let mut nodes = HashMap::new();
        let mut edges = Vec::new();
        let kinds = [
            NodeKind::File,
            NodeKind::Module,
            NodeKind::Class,
            NodeKind::Struct,
            NodeKind::Interface,
            NodeKind::Trait,
            NodeKind::Function,
            NodeKind::Method,
            NodeKind::Variable,
            NodeKind::Constant,
            NodeKind::Enum,
            NodeKind::TypeAlias,
        ];

        for kind in kinds {
            for node in self.queries.get_nodes_by_kind(kind)? {
                if filter(&node) {
                    nodes.insert(node.id.clone(), node);
                }
            }
        }

        if include_edges.unwrap_or(true) {
            for node_id in nodes.keys() {
                for edge in self.queries.get_outgoing_edges(node_id, None, None)? {
                    if nodes.contains_key(&edge.target) {
                        edges.push(edge);
                    }
                }
            }
        }

        Ok(Subgraph {
            nodes,
            edges,
            roots: Vec::new(),
            confidence: None,
        })
    }

    /// Access the underlying traverser.
    pub fn get_traverser(&mut self) -> GraphTraverser<'_, 'db> {
        GraphTraverser::new(self.queries)
    }

    /// Rust-style alias for [`Self::get_traverser`].
    pub fn traverser(&mut self) -> GraphTraverser<'_, 'db> {
        self.get_traverser()
    }
}

fn node_is_exported(node: &Node) -> bool {
    node.is_exported.unwrap_or(false)
}

fn wildcard_match(pattern: &str, text: &str) -> bool {
    // 简单 glob 匹配只支持 `*`/`?`，用于 qualified_name 搜索；不引入 regex，
    // 避免把用户输入当正则解析带来转义和性能问题。
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();
    let (mut p, mut t) = (0, 0);
    let mut star: Option<usize> = None;
    let mut match_after_star = 0;

    while t < text.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == text[t]) {
            p += 1;
            t += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some(p);
            match_after_star = t;
            p += 1;
        } else if let Some(star_idx) = star {
            p = star_idx + 1;
            match_after_star += 1;
            t = match_after_star;
        } else {
            return false;
        }
    }

    while p < pattern.len() && pattern[p] == '*' {
        p += 1;
    }

    p == pattern.len()
}
