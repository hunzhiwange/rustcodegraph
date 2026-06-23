//! facade 层的轻量公共辅助函数。
//!
//! 这些函数服务于 `CodeGraph` 的同步 API：路径统一、空图兜底和默认遍历选项都在这里保持一致，
//! 避免上层方法因为数据库查询失败或相对路径输入而暴露不同的行为。

use super::*;

pub(super) fn empty_subgraph() -> Subgraph {
    Subgraph {
        nodes: HashMap::new(),
        edges: Vec::new(),
        roots: Vec::new(),
        confidence: None,
    }
}

pub(super) fn resolve_root(project_root: impl AsRef<Path>) -> PathBuf {
    let path = project_root.as_ref();
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    }
}

#[allow(dead_code)]
pub(super) fn default_traversal_options() -> TraversalOptions {
    TraversalOptions {
        max_depth: None,
        edge_kinds: None,
        node_kinds: None,
        direction: Some(TraversalDirection::Both),
        limit: None,
        include_start: Some(true),
    }
}
