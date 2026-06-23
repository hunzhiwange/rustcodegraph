//! Graph traversal algorithms.
//!
//! Direct Rust translation of the TypeScript BFS/DFS and graph-neighborhood
//! helpers. The query layer is mutable because it owns prepared statement and
//! node-cache state.
//!
//! traversal 只关心图算法和批量取节点，业务含义尽量由调用方通过 edge/node kind
//! 过滤传入；这让 MCP explore、context 和 SDK 查询能复用同一套遍历行为。

use std::collections::{HashMap, HashSet, VecDeque};

use crate::db::queries::QueryBuilder;
use crate::db::sqlite_adapter::SqliteResult;
use crate::types::{
    Edge, EdgeKind, Node, NodeKind, Subgraph, TraversalDirection, TraversalOptions,
};

#[derive(Debug, Clone)]
struct ResolvedTraversalOptions {
    max_depth: usize,
    edge_kinds: Vec<EdgeKind>,
    node_kinds: Vec<NodeKind>,
    direction: TraversalDirection,
    limit: usize,
    include_start: bool,
}

impl From<TraversalOptions> for ResolvedTraversalOptions {
    fn from(options: TraversalOptions) -> Self {
        // 对外 options 大多是可选字段，内部先解析成确定值，避免每一层递归都处理
        // Option 分支。
        Self {
            max_depth: options
                .max_depth
                .map(|depth| depth as usize)
                .unwrap_or(usize::MAX),
            edge_kinds: options.edge_kinds.unwrap_or_default(),
            node_kinds: options.node_kinds.unwrap_or_default(),
            direction: options.direction.unwrap_or(TraversalDirection::Outgoing),
            limit: options.limit.map(|limit| limit as usize).unwrap_or(1000),
            include_start: options.include_start.unwrap_or(true),
        }
    }
}

#[derive(Debug, Clone)]
struct TraversalStep {
    node: Node,
    edge: Option<Edge>,
    depth: usize,
}

/// Pair returned by caller/callee/usages helpers.
#[derive(Debug, Clone)]
pub struct NodeEdge {
    pub node: Node,
    pub edge: Edge,
}

/// Step in a path result. The first step has no edge.
#[derive(Debug, Clone)]
pub struct PathStep {
    pub node: Node,
    pub edge: Option<Edge>,
}

/// Graph traverser for BFS and DFS traversal.
pub struct GraphTraverser<'a, 'db> {
    queries: &'a mut QueryBuilder<'db>,
}

impl<'a, 'db> GraphTraverser<'a, 'db> {
    pub fn new(queries: &'a mut QueryBuilder<'db>) -> Self {
        Self { queries }
    }

    /// Traverse the graph using breadth-first search.
    pub fn traverse_bfs(
        &mut self,
        start_id: &str,
        options: Option<TraversalOptions>,
    ) -> SqliteResult<Subgraph> {
        // BFS 会优先扩展 contains/calls 边，返回更贴近“代码结构和调用流”的邻域；
        // 节点批量读取减少大图遍历时的 SQLite 往返。
        let opts = ResolvedTraversalOptions::from(options.unwrap_or_else(empty_traversal_options));
        let Some(start_node) = self.queries.get_node_by_id(start_id)? else {
            return Ok(empty_subgraph());
        };

        let mut nodes = HashMap::new();
        let mut edges = Vec::new();
        let mut visited = HashSet::new();
        let mut queue = VecDeque::from([TraversalStep {
            node: start_node.clone(),
            edge: None,
            depth: 0,
        }]);

        if opts.include_start {
            nodes.insert(start_node.id.clone(), start_node);
        }

        while !queue.is_empty() && nodes.len() < opts.limit {
            let step = queue.pop_front().expect("queue checked non-empty");
            let node = step.node;

            if visited.contains(&node.id) {
                continue;
            }
            visited.insert(node.id.clone());

            if let Some(edge) = step.edge {
                edges.push(edge);
            }

            if step.depth >= opts.max_depth {
                continue;
            }

            let mut adjacent_edges =
                self.get_adjacent_edges(&node.id, opts.direction, opts.edge_kinds.as_slice())?;
            adjacent_edges.sort_by_key(edge_priority);

            let want_ids: Vec<String> = adjacent_edges
                .iter()
                .map(|edge| other_endpoint(edge, &node.id))
                .filter(|id| !visited.contains(id))
                .collect();
            let neighbor_nodes = if want_ids.is_empty() {
                HashMap::new()
            } else {
                self.queries.get_nodes_by_ids(&want_ids)?
            };

            for adj_edge in adjacent_edges {
                let next_node_id = other_endpoint(&adj_edge, &node.id);
                if visited.contains(&next_node_id) {
                    continue;
                }

                let Some(next_node) = neighbor_nodes.get(&next_node_id).cloned() else {
                    continue;
                };

                if !opts.node_kinds.is_empty() && !opts.node_kinds.contains(&next_node.kind) {
                    continue;
                }

                nodes.insert(next_node.id.clone(), next_node.clone());
                queue.push_back(TraversalStep {
                    node: next_node,
                    edge: Some(adj_edge),
                    depth: step.depth + 1,
                });
            }
        }

        Ok(make_subgraph(nodes, edges, vec![start_id.to_string()]))
    }

    /// Traverse the graph using depth-first search.
    pub fn traverse_dfs(
        &mut self,
        start_id: &str,
        options: Option<TraversalOptions>,
    ) -> SqliteResult<Subgraph> {
        let opts = ResolvedTraversalOptions::from(options.unwrap_or_else(empty_traversal_options));
        let Some(start_node) = self.queries.get_node_by_id(start_id)? else {
            return Ok(empty_subgraph());
        };

        let mut nodes = HashMap::new();
        let mut edges = Vec::new();
        let mut visited = HashSet::new();

        if opts.include_start {
            nodes.insert(start_node.id.clone(), start_node.clone());
        }

        self.dfs_recursive(start_node, 0, &opts, &mut nodes, &mut edges, &mut visited)?;
        Ok(make_subgraph(nodes, edges, vec![start_id.to_string()]))
    }

    fn dfs_recursive(
        &mut self,
        node: Node,
        depth: usize,
        opts: &ResolvedTraversalOptions,
        nodes: &mut HashMap<String, Node>,
        edges: &mut Vec<Edge>,
        visited: &mut HashSet<String>,
    ) -> SqliteResult<()> {
        if visited.contains(&node.id) || nodes.len() >= opts.limit || depth >= opts.max_depth {
            return Ok(());
        }

        visited.insert(node.id.clone());

        let adjacent_edges =
            self.get_adjacent_edges(&node.id, opts.direction, opts.edge_kinds.as_slice())?;
        let want_ids: Vec<String> = adjacent_edges
            .iter()
            .map(|edge| other_endpoint(edge, &node.id))
            .filter(|id| !visited.contains(id))
            .collect();
        let neighbor_nodes = if want_ids.is_empty() {
            HashMap::new()
        } else {
            self.queries.get_nodes_by_ids(&want_ids)?
        };

        for edge in adjacent_edges {
            let next_node_id = other_endpoint(&edge, &node.id);
            if visited.contains(&next_node_id) {
                continue;
            }

            let Some(next_node) = neighbor_nodes.get(&next_node_id).cloned() else {
                continue;
            };

            if !opts.node_kinds.is_empty() && !opts.node_kinds.contains(&next_node.kind) {
                continue;
            }

            nodes.insert(next_node.id.clone(), next_node.clone());
            edges.push(edge);
            self.dfs_recursive(next_node, depth + 1, opts, nodes, edges, visited)?;
        }

        Ok(())
    }

    fn get_adjacent_edges(
        &mut self,
        node_id: &str,
        direction: TraversalDirection,
        edge_kinds: &[EdgeKind],
    ) -> SqliteResult<Vec<Edge>> {
        // Both 方向会合并入边和出边；调用方仍可通过 edge_kinds 限制语义范围。
        let kinds = (!edge_kinds.is_empty()).then(|| edge_kinds.to_vec());
        match direction {
            TraversalDirection::Outgoing => self.queries.get_outgoing_edges(node_id, kinds, None),
            TraversalDirection::Incoming => self.queries.get_incoming_edges(node_id, kinds),
            TraversalDirection::Both => {
                let mut outgoing = self
                    .queries
                    .get_outgoing_edges(node_id, kinds.clone(), None)?;
                outgoing.extend(self.queries.get_incoming_edges(node_id, kinds)?);
                Ok(outgoing)
            }
        }
    }

    /// Find all callers of a function/method.
    pub fn get_callers(
        &mut self,
        node_id: &str,
        max_depth: Option<usize>,
    ) -> SqliteResult<Vec<NodeEdge>> {
        let mut result = Vec::new();
        let mut visited = HashSet::new();
        self.get_callers_recursive(
            node_id,
            max_depth.unwrap_or(1),
            0,
            &mut result,
            &mut visited,
        )?;
        Ok(result)
    }

    fn get_callers_recursive(
        &mut self,
        node_id: &str,
        max_depth: usize,
        current_depth: usize,
        result: &mut Vec<NodeEdge>,
        visited: &mut HashSet<String>,
    ) -> SqliteResult<()> {
        if current_depth >= max_depth || visited.contains(node_id) {
            return Ok(());
        }
        visited.insert(node_id.to_string());

        let incoming_edges = self.queries.get_incoming_edges(
            node_id,
            Some(vec![
                EdgeKind::Calls,
                EdgeKind::References,
                EdgeKind::Imports,
                EdgeKind::Instantiates,
            ]),
        )?;
        if incoming_edges.is_empty() {
            return Ok(());
        }

        let source_ids: Vec<String> = incoming_edges
            .iter()
            .map(|edge| edge.source.clone())
            .collect();
        let caller_nodes = self.queries.get_nodes_by_ids(&source_ids)?;

        for edge in incoming_edges {
            if let Some(caller_node) = caller_nodes.get(&edge.source).cloned()
                && !visited.contains(&caller_node.id)
            {
                result.push(NodeEdge {
                    node: caller_node.clone(),
                    edge: edge.clone(),
                });
                self.get_callers_recursive(
                    &caller_node.id,
                    max_depth,
                    current_depth + 1,
                    result,
                    visited,
                )?;
            }
        }

        Ok(())
    }

    /// Find all functions/methods called by a function.
    pub fn get_callees(
        &mut self,
        node_id: &str,
        max_depth: Option<usize>,
    ) -> SqliteResult<Vec<NodeEdge>> {
        let mut result = Vec::new();
        let mut visited = HashSet::new();
        self.get_callees_recursive(
            node_id,
            max_depth.unwrap_or(1),
            0,
            &mut result,
            &mut visited,
        )?;
        Ok(result)
    }

    fn get_callees_recursive(
        &mut self,
        node_id: &str,
        max_depth: usize,
        current_depth: usize,
        result: &mut Vec<NodeEdge>,
        visited: &mut HashSet<String>,
    ) -> SqliteResult<()> {
        if current_depth >= max_depth || visited.contains(node_id) {
            return Ok(());
        }
        visited.insert(node_id.to_string());

        let outgoing_edges = self.queries.get_outgoing_edges(
            node_id,
            Some(vec![
                EdgeKind::Calls,
                EdgeKind::References,
                EdgeKind::Imports,
                EdgeKind::Instantiates,
            ]),
            None,
        )?;
        if outgoing_edges.is_empty() {
            return Ok(());
        }

        let target_ids: Vec<String> = outgoing_edges
            .iter()
            .map(|edge| edge.target.clone())
            .collect();
        let callee_nodes = self.queries.get_nodes_by_ids(&target_ids)?;

        for edge in outgoing_edges {
            if let Some(callee_node) = callee_nodes.get(&edge.target).cloned()
                && !visited.contains(&callee_node.id)
            {
                result.push(NodeEdge {
                    node: callee_node.clone(),
                    edge: edge.clone(),
                });
                self.get_callees_recursive(
                    &callee_node.id,
                    max_depth,
                    current_depth + 1,
                    result,
                    visited,
                )?;
            }
        }

        Ok(())
    }

    /// Get the call graph for a function.
    pub fn get_call_graph(
        &mut self,
        node_id: &str,
        depth: Option<usize>,
    ) -> SqliteResult<Subgraph> {
        // call graph 是 callers + callees 的并集，不做路径压缩；调用方可用 roots
        // 把焦点节点重新突出显示。
        let Some(focal_node) = self.queries.get_node_by_id(node_id)? else {
            return Ok(empty_subgraph());
        };
        let depth = depth.unwrap_or(2);

        let mut nodes = HashMap::new();
        let mut edges = Vec::new();
        nodes.insert(focal_node.id.clone(), focal_node);

        for NodeEdge { node, edge } in self.get_callers(node_id, Some(depth))? {
            nodes.insert(node.id.clone(), node);
            edges.push(edge);
        }
        for NodeEdge { node, edge } in self.get_callees(node_id, Some(depth))? {
            nodes.insert(node.id.clone(), node);
            edges.push(edge);
        }

        Ok(make_subgraph(nodes, edges, vec![node_id.to_string()]))
    }

    /// Get the type hierarchy for a class/interface.
    pub fn get_type_hierarchy(&mut self, node_id: &str) -> SqliteResult<Subgraph> {
        let Some(focal_node) = self.queries.get_node_by_id(node_id)? else {
            return Ok(empty_subgraph());
        };

        let mut nodes = HashMap::new();
        let mut edges = Vec::new();
        let mut visited = HashSet::new();
        nodes.insert(focal_node.id.clone(), focal_node);

        self.get_type_ancestors(node_id, &mut nodes, &mut edges, &mut visited)?;
        self.get_type_descendants(node_id, &mut nodes, &mut edges, &mut visited)?;

        Ok(make_subgraph(nodes, edges, vec![node_id.to_string()]))
    }

    fn get_type_ancestors(
        &mut self,
        node_id: &str,
        nodes: &mut HashMap<String, Node>,
        edges: &mut Vec<Edge>,
        visited: &mut HashSet<String>,
    ) -> SqliteResult<()> {
        if visited.contains(node_id) {
            return Ok(());
        }
        visited.insert(node_id.to_string());

        let outgoing_edges = self.queries.get_outgoing_edges(
            node_id,
            Some(vec![EdgeKind::Extends, EdgeKind::Implements]),
            None,
        )?;
        if outgoing_edges.is_empty() {
            return Ok(());
        }
        let parent_ids = outgoing_edges
            .iter()
            .map(|edge| edge.target.clone())
            .collect::<Vec<_>>();
        let parents = self.queries.get_nodes_by_ids(&parent_ids)?;

        for edge in outgoing_edges {
            if let Some(parent_node) = parents.get(&edge.target).cloned()
                && !nodes.contains_key(&parent_node.id)
            {
                nodes.insert(parent_node.id.clone(), parent_node.clone());
                edges.push(edge);
                self.get_type_ancestors(&parent_node.id, nodes, edges, visited)?;
            }
        }

        Ok(())
    }

    fn get_type_descendants(
        &mut self,
        node_id: &str,
        nodes: &mut HashMap<String, Node>,
        edges: &mut Vec<Edge>,
        visited: &mut HashSet<String>,
    ) -> SqliteResult<()> {
        if visited.contains(node_id) {
            return Ok(());
        }
        visited.insert(node_id.to_string());

        let incoming_edges = self
            .queries
            .get_incoming_edges(node_id, Some(vec![EdgeKind::Extends, EdgeKind::Implements]))?;
        if incoming_edges.is_empty() {
            return Ok(());
        }
        let child_ids = incoming_edges
            .iter()
            .map(|edge| edge.source.clone())
            .collect::<Vec<_>>();
        let children = self.queries.get_nodes_by_ids(&child_ids)?;

        for edge in incoming_edges {
            if let Some(child_node) = children.get(&edge.source).cloned()
                && !nodes.contains_key(&child_node.id)
            {
                nodes.insert(child_node.id.clone(), child_node.clone());
                edges.push(edge);
                self.get_type_descendants(&child_node.id, nodes, edges, visited)?;
            }
        }

        Ok(())
    }

    /// Find all usages of a symbol.
    pub fn find_usages(&mut self, node_id: &str) -> SqliteResult<Vec<NodeEdge>> {
        let mut result = Vec::new();
        let incoming_edges = self.queries.get_incoming_edges(node_id, None)?;
        if incoming_edges.is_empty() {
            return Ok(result);
        }

        let source_ids: Vec<String> = incoming_edges
            .iter()
            .map(|edge| edge.source.clone())
            .collect();
        let sources = self.queries.get_nodes_by_ids(&source_ids)?;
        for edge in incoming_edges {
            if let Some(source_node) = sources.get(&edge.source).cloned() {
                result.push(NodeEdge {
                    node: source_node,
                    edge,
                });
            }
        }

        Ok(result)
    }

    /// Calculate the impact radius of a node.
    pub fn get_impact_radius(
        &mut self,
        node_id: &str,
        max_depth: Option<usize>,
    ) -> SqliteResult<Subgraph> {
        let Some(focal_node) = self.queries.get_node_by_id(node_id)? else {
            return Ok(empty_subgraph());
        };
        let max_depth = max_depth.unwrap_or(3);

        let mut nodes = HashMap::new();
        let mut edges = Vec::new();
        let mut visited = HashSet::new();
        nodes.insert(focal_node.id.clone(), focal_node);

        self.get_impact_recursive(node_id, max_depth, 0, &mut nodes, &mut edges, &mut visited)?;
        Ok(make_subgraph(nodes, edges, vec![node_id.to_string()]))
    }

    fn get_impact_recursive(
        &mut self,
        node_id: &str,
        max_depth: usize,
        current_depth: usize,
        nodes: &mut HashMap<String, Node>,
        edges: &mut Vec<Edge>,
        visited: &mut HashSet<String>,
    ) -> SqliteResult<()> {
        // impact 从“谁引用我”向外扩散；若起点是容器，先纳入子节点，让修改 class/
        // module 时能看到成员级影响。
        if current_depth >= max_depth || visited.contains(node_id) {
            return Ok(());
        }
        visited.insert(node_id.to_string());

        if let Some(focal_node) = self.queries.get_node_by_id(node_id)?
            && is_container_kind(&focal_node.kind)
        {
            let contains_edges =
                self.queries
                    .get_outgoing_edges(node_id, Some(vec![EdgeKind::Contains]), None)?;
            if !contains_edges.is_empty() {
                let child_ids = contains_edges
                    .iter()
                    .map(|edge| edge.target.clone())
                    .collect::<Vec<_>>();
                let children = self.queries.get_nodes_by_ids(&child_ids)?;
                for edge in contains_edges {
                    if let Some(child_node) = children.get(&edge.target).cloned()
                        && !visited.contains(&child_node.id)
                    {
                        nodes.insert(child_node.id.clone(), child_node.clone());
                        edges.push(edge);
                        self.get_impact_recursive(
                            &child_node.id,
                            max_depth,
                            current_depth,
                            nodes,
                            edges,
                            visited,
                        )?;
                    }
                }
            }
        }

        let incoming_edges: Vec<Edge> = self
            .queries
            .get_incoming_edges(node_id, None)?
            .into_iter()
            .filter(|edge| edge.kind != EdgeKind::Contains)
            .collect();
        if incoming_edges.is_empty() {
            return Ok(());
        }
        let source_ids = incoming_edges
            .iter()
            .map(|edge| edge.source.clone())
            .collect::<Vec<_>>();
        let sources = self.queries.get_nodes_by_ids(&source_ids)?;

        for edge in incoming_edges {
            if let Some(source_node) = sources.get(&edge.source).cloned()
                && !nodes.contains_key(&source_node.id)
            {
                nodes.insert(source_node.id.clone(), source_node.clone());
                edges.push(edge);
                self.get_impact_recursive(
                    &source_node.id,
                    max_depth,
                    current_depth + 1,
                    nodes,
                    edges,
                    visited,
                )?;
            }
        }

        Ok(())
    }

    /// Find the shortest path between two nodes.
    pub fn find_path(
        &mut self,
        from_id: &str,
        to_id: &str,
        edge_kinds: &[EdgeKind],
    ) -> SqliteResult<Option<Vec<PathStep>>> {
        // 这里用 BFS 保证第一条命中的路径最短；PathStep 保留进入节点的边，
        // 方便渲染时解释每一跳的来源。
        let Some(from_node) = self.queries.get_node_by_id(from_id)? else {
            return Ok(None);
        };
        if self.queries.get_node_by_id(to_id)?.is_none() {
            return Ok(None);
        }

        let mut visited = HashSet::new();
        let mut queue = VecDeque::from([(
            from_id.to_string(),
            vec![PathStep {
                node: from_node,
                edge: None,
            }],
        )]);

        while let Some((node_id, path)) = queue.pop_front() {
            if node_id == to_id {
                return Ok(Some(path));
            }

            if visited.contains(&node_id) {
                continue;
            }
            visited.insert(node_id.clone());

            let kinds = (!edge_kinds.is_empty()).then(|| edge_kinds.to_vec());
            let outgoing_edges = self.queries.get_outgoing_edges(&node_id, kinds, None)?;
            if outgoing_edges.is_empty() {
                continue;
            }

            let want_ids: Vec<String> = outgoing_edges
                .iter()
                .map(|edge| edge.target.clone())
                .filter(|id| !visited.contains(id))
                .collect();
            let next_nodes = if want_ids.is_empty() {
                HashMap::new()
            } else {
                self.queries.get_nodes_by_ids(&want_ids)?
            };

            for edge in outgoing_edges {
                if visited.contains(&edge.target) {
                    continue;
                }
                if let Some(next_node) = next_nodes.get(&edge.target).cloned() {
                    let mut next_path = path.clone();
                    next_path.push(PathStep {
                        node: next_node,
                        edge: Some(edge.clone()),
                    });
                    queue.push_back((edge.target, next_path));
                }
            }
        }

        Ok(None)
    }

    /// Get the containment hierarchy for a node, immediate parent first.
    pub fn get_ancestors(&mut self, node_id: &str) -> SqliteResult<Vec<Node>> {
        let mut ancestors = Vec::new();
        let mut visited = HashSet::new();
        let mut current_id = node_id.to_string();

        loop {
            if visited.contains(&current_id) {
                break;
            }
            visited.insert(current_id.clone());

            let containing_edges = self
                .queries
                .get_incoming_edges(&current_id, Some(vec![EdgeKind::Contains]))?;
            let Some(first_edge) = containing_edges.first() else {
                break;
            };

            if let Some(parent_node) = self.queries.get_node_by_id(&first_edge.source)? {
                current_id = parent_node.id.clone();
                ancestors.push(parent_node);
            } else {
                break;
            }
        }

        Ok(ancestors)
    }

    /// Get immediate children of a node.
    pub fn get_children(&mut self, node_id: &str) -> SqliteResult<Vec<Node>> {
        let contains_edges =
            self.queries
                .get_outgoing_edges(node_id, Some(vec![EdgeKind::Contains]), None)?;
        if contains_edges.is_empty() {
            return Ok(Vec::new());
        }

        let child_ids = contains_edges
            .iter()
            .map(|edge| edge.target.clone())
            .collect::<Vec<_>>();
        let child_nodes = self.queries.get_nodes_by_ids(&child_ids)?;
        let mut children = Vec::new();
        for edge in contains_edges {
            if let Some(child_node) = child_nodes.get(&edge.target).cloned() {
                children.push(child_node);
            }
        }
        Ok(children)
    }
}

fn edge_priority(edge: &Edge) -> u8 {
    // 遍历邻域时先走结构边再走调用边，输出更稳定，也更符合 agent 阅读顺序。
    match edge.kind {
        EdgeKind::Contains => 0,
        EdgeKind::Calls => 1,
        _ => 2,
    }
}

fn other_endpoint(edge: &Edge, node_id: &str) -> String {
    if edge.source == node_id {
        edge.target.clone()
    } else {
        edge.source.clone()
    }
}

fn is_container_kind(kind: &NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::Class
            | NodeKind::Interface
            | NodeKind::Struct
            | NodeKind::Trait
            | NodeKind::Protocol
            | NodeKind::Module
            | NodeKind::Enum
    )
}

fn empty_subgraph() -> Subgraph {
    make_subgraph(HashMap::new(), Vec::new(), Vec::new())
}

fn empty_traversal_options() -> TraversalOptions {
    TraversalOptions {
        max_depth: None,
        edge_kinds: None,
        node_kinds: None,
        direction: None,
        limit: None,
        include_start: None,
    }
}

fn make_subgraph(nodes: HashMap<String, Node>, edges: Vec<Edge>, roots: Vec<String>) -> Subgraph {
    Subgraph {
        nodes,
        edges,
        roots,
        confidence: None,
    }
}
