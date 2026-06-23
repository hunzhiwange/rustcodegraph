use super::*;

pub(super) struct CoreExtractorContext<'a> {
    pub(super) inner: &'a mut TreeSitterExtractor,
}

// 语言适配器的自定义 visit 通过这个薄包装回调核心抽取器。这样既能复用
// create_node/visit_node 的 scope 维护，又不会把 TreeSitterExtractor 全部暴露出去。
impl ExtractorContext for CoreExtractorContext<'_> {
    fn create_node(
        &mut self,
        kind: NodeKind,
        name: &str,
        node: &SyntaxNode,
        extra: NodeExtra,
    ) -> Option<Node> {
        let created = self.inner.create_node(kind, name, node, extra)?;
        self.inner.nodes.push(created.clone());
        Some(created)
    }

    fn visit_node(&mut self, node: &SyntaxNode) {
        self.inner.visit_node(node);
    }

    fn visit_function_body(&mut self, body: &SyntaxNode, function_id: &str) {
        self.inner.visit_function_body(body, function_id);
    }

    fn add_unresolved_reference(&mut self, reference: UnresolvedReferenceInput) {
        self.inner.unresolved_references.push(UnresolvedReference {
            from_node_id: reference.from_node_id,
            reference_name: reference.reference_name,
            reference_kind: reference.reference_kind,
            line: reference.line.unwrap_or(0) as u64,
            column: reference.column.unwrap_or(0) as u64,
            file_path: reference.file_path,
            language: None,
            candidates: None,
        });
    }

    fn push_scope(&mut self, node_id: String) {
        self.inner.node_stack.push(node_id);
    }

    fn pop_scope(&mut self) {
        self.inner.node_stack.pop();
    }

    fn file_path(&self) -> &str {
        &self.inner.file_path
    }

    fn source(&self) -> &str {
        &self.inner.source
    }

    fn node_stack(&self) -> &[String] {
        &self.inner.node_stack
    }

    fn nodes(&self) -> &[Node] {
        &self.inner.nodes
    }
}
