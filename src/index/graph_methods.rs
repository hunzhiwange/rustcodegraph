//! `CodeGraph` 图遍历与依赖查询 API 的同步 facade。
//!
//! 这里故意把底层查询错误折叠为空结果或空子图，保持 JS/CLI 侧调用“可继续”的体验；
//! 真正需要诊断的数据库错误仍由索引、初始化和状态接口负责暴露。

use super::*;

impl CodeGraph {
    pub fn get_context(&mut self, node_id: &str) -> Option<Context> {
        let mut queries = QueryBuilder::new(self.db.get_db());
        let mut manager = crate::graph::queries::GraphQueryManager::new(&mut queries);
        match manager.get_context(node_id) {
            Ok(context) => Some(context),
            Err(error) => panic!("{error}"),
        }
    }

    pub fn traverse(&mut self, start_id: &str, options: Option<TraversalOptions>) -> Subgraph {
        let mut queries = QueryBuilder::new(self.db.get_db());
        let mut traverser = crate::graph::traversal::GraphTraverser::new(&mut queries);
        traverser
            .traverse_bfs(start_id, options)
            .unwrap_or_else(|_| empty_subgraph())
    }

    pub fn get_call_graph(&mut self, node_id: &str, depth: u64) -> Subgraph {
        let mut queries = QueryBuilder::new(self.db.get_db());
        let mut traverser = crate::graph::traversal::GraphTraverser::new(&mut queries);
        traverser
            .get_call_graph(node_id, Some(depth as usize))
            .unwrap_or_else(|_| empty_subgraph())
    }

    pub fn get_type_hierarchy(&mut self, node_id: &str) -> Subgraph {
        let mut queries = QueryBuilder::new(self.db.get_db());
        let mut traverser = crate::graph::traversal::GraphTraverser::new(&mut queries);
        traverser
            .get_type_hierarchy(node_id)
            .unwrap_or_else(|_| empty_subgraph())
    }

    pub fn find_usages(&mut self, node_id: &str) -> Vec<NodeEdgeRef> {
        let mut queries = QueryBuilder::new(self.db.get_db());
        let mut traverser = crate::graph::traversal::GraphTraverser::new(&mut queries);
        traverser
            .find_usages(node_id)
            .unwrap_or_default()
            .into_iter()
            .map(|node_edge| NodeEdgeRef {
                node: node_edge.node,
                edge: node_edge.edge,
            })
            .collect()
    }

    pub fn get_callers(&mut self, node_id: &str, max_depth: u64) -> Vec<NodeEdgeRef> {
        let mut queries = QueryBuilder::new(self.db.get_db());
        let mut traverser = crate::graph::traversal::GraphTraverser::new(&mut queries);
        traverser
            .get_callers(node_id, Some(max_depth as usize))
            .unwrap_or_default()
            .into_iter()
            .map(|node_edge| NodeEdgeRef {
                node: node_edge.node,
                edge: node_edge.edge,
            })
            .collect()
    }

    pub fn get_callees(&mut self, node_id: &str, max_depth: u64) -> Vec<NodeEdgeRef> {
        let mut queries = QueryBuilder::new(self.db.get_db());
        let mut traverser = crate::graph::traversal::GraphTraverser::new(&mut queries);
        traverser
            .get_callees(node_id, Some(max_depth as usize))
            .unwrap_or_default()
            .into_iter()
            .map(|node_edge| NodeEdgeRef {
                node: node_edge.node,
                edge: node_edge.edge,
            })
            .collect()
    }

    pub fn get_impact_radius(&mut self, node_id: &str, max_depth: u64) -> Subgraph {
        let mut queries = QueryBuilder::new(self.db.get_db());
        let mut traverser = crate::graph::traversal::GraphTraverser::new(&mut queries);
        traverser
            .get_impact_radius(node_id, Some(max_depth as usize))
            .unwrap_or_else(|_| empty_subgraph())
    }

    pub fn find_path(
        &mut self,
        from_id: &str,
        to_id: &str,
        edge_kinds: Option<Vec<EdgeKind>>,
    ) -> Option<Vec<PathStep>> {
        let mut queries = QueryBuilder::new(self.db.get_db());
        let mut traverser = crate::graph::traversal::GraphTraverser::new(&mut queries);
        traverser
            .find_path(from_id, to_id, edge_kinds.as_deref().unwrap_or(&[]))
            .ok()
            .flatten()
            .map(|steps| {
                steps
                    .into_iter()
                    .map(|step| PathStep {
                        node: step.node,
                        edge: step.edge,
                    })
                    .collect()
            })
    }

    pub fn get_ancestors(&mut self, node_id: &str) -> Vec<Node> {
        let mut queries = QueryBuilder::new(self.db.get_db());
        let mut traverser = crate::graph::traversal::GraphTraverser::new(&mut queries);
        traverser.get_ancestors(node_id).unwrap_or_default()
    }

    pub fn get_children(&mut self, node_id: &str) -> Vec<Node> {
        let mut queries = QueryBuilder::new(self.db.get_db());
        let mut traverser = crate::graph::traversal::GraphTraverser::new(&mut queries);
        traverser.get_children(node_id).unwrap_or_default()
    }

    pub fn get_file_dependencies(&mut self, file_path: &str) -> Vec<String> {
        let mut queries = QueryBuilder::new(self.db.get_db());
        let mut manager = crate::graph::queries::GraphQueryManager::new(&mut queries);
        manager.get_file_dependencies(file_path).unwrap_or_default()
    }

    pub fn get_file_dependents(&mut self, file_path: &str) -> Vec<String> {
        let mut queries = QueryBuilder::new(self.db.get_db());
        let mut manager = crate::graph::queries::GraphQueryManager::new(&mut queries);
        manager.get_file_dependents(file_path).unwrap_or_default()
    }

    pub fn find_circular_dependencies(&mut self) -> Vec<Vec<String>> {
        let mut queries = QueryBuilder::new(self.db.get_db());
        let mut manager = crate::graph::queries::GraphQueryManager::new(&mut queries);
        manager.find_circular_dependencies().unwrap_or_default()
    }

    pub fn find_dead_code(&mut self, kinds: Option<Vec<NodeKind>>) -> Vec<Node> {
        let mut queries = QueryBuilder::new(self.db.get_db());
        let mut manager = crate::graph::queries::GraphQueryManager::new(&mut queries);
        manager.find_dead_code(kinds.as_deref()).unwrap_or_default()
    }

    pub fn get_node_metrics(&mut self, node_id: &str) -> NodeMetrics {
        let mut queries = QueryBuilder::new(self.db.get_db());
        let mut manager = crate::graph::queries::GraphQueryManager::new(&mut queries);
        manager
            .get_node_metrics(node_id)
            .map(|metrics| NodeMetrics {
                incoming_edge_count: metrics.incoming_edge_count as u64,
                outgoing_edge_count: metrics.outgoing_edge_count as u64,
                call_count: metrics.call_count as u64,
                caller_count: metrics.caller_count as u64,
                child_count: metrics.child_count as u64,
                depth: metrics.depth as u64,
            })
            .unwrap_or(NodeMetrics {
                incoming_edge_count: 0,
                outgoing_edge_count: 0,
                call_count: 0,
                caller_count: 0,
                child_count: 0,
                depth: 0,
            })
    }
}
