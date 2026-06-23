use super::*;

impl CodeGraph {
    /// facade 方法每次创建短生命周期 QueryBuilder/ContextBuilder，避免在 CodeGraph
    /// 上长期持有可变查询状态，watch/sync 后也能读到最新 DB 内容。
    pub fn get_code(&mut self, node_id: &str) -> Option<String> {
        let mut queries = QueryBuilder::new(self.db.get_db());
        let mut builder =
            crate::context::index::create_context_builder(self.project_root.clone(), &mut queries);
        builder.get_code(node_id).ok().flatten()
    }

    pub fn find_relevant_context(
        &mut self,
        query: &str,
        options: Option<FindRelevantContextOptions>,
    ) -> Subgraph {
        let mut queries = QueryBuilder::new(self.db.get_db());
        let mut builder =
            crate::context::index::create_context_builder(self.project_root.clone(), &mut queries);
        builder
            .find_relevant_context(query, options)
            .unwrap_or_else(|_| empty_subgraph())
    }

    pub fn build_context(
        &mut self,
        input: TaskInput,
        options: Option<BuildContextOptions>,
    ) -> BuildContextResult {
        let mut queries = QueryBuilder::new(self.db.get_db());
        let mut builder =
            crate::context::index::create_context_builder(self.project_root.clone(), &mut queries);
        match builder.build_context(input, options) {
            Ok(crate::context::index::BuildContextResult::Context(context)) => {
                BuildContextResult::Context(context)
            }
            Ok(crate::context::index::BuildContextResult::Formatted(text)) => {
                BuildContextResult::Formatted(text)
            }
            Err(_) => BuildContextResult::Formatted(String::new()),
        }
    }

    pub fn optimize(&mut self) {}

    // 兼容 TypeScript API 的占位方法；Rust facade 当前没有进程内缓存需要清理。
    pub fn clear(&mut self) {}
}
