//! Shared helpers for callback and dynamic-dispatch edge synthesis.
//!
//! 这些 helper 把 fanout 上限、启发式 edge metadata 和常用节点定位逻辑集中起来，
//! 让各语言 pass 更容易保持同样的保守度。

use std::collections::HashMap;

use serde_json::{Value, json};

use crate::db::queries::QueryBuilder;
use crate::types::{Edge, EdgeKind, EdgeProvenance, Node, NodeKind};

pub(super) const MAX_CALLBACKS_PER_CHANNEL: usize = 40;
pub(super) const EVENT_FANOUT_CAP: usize = 6;
pub(super) const MAX_JSX_CHILDREN: usize = 30;

pub(super) fn edge(
    source: &str,
    target: &str,
    kind: EdgeKind,
    line: Option<u64>,
    synthesized_by: &str,
    metadata: impl IntoIterator<Item = (&'static str, Value)>,
) -> Edge {
    // 启发式边必须标明来源和注册点；MCP 输出会把这些字段展示给 agent，
    // 让它知道这是 synthesized reachability，而不是源码里的直接调用。
    let mut meta = HashMap::from([("synthesizedBy".to_string(), json!(synthesized_by))]);
    for (key, value) in metadata {
        meta.insert(key.to_string(), value);
    }
    Edge {
        source: source.to_string(),
        target: target.to_string(),
        kind,
        metadata: Some(meta),
        line,
        column: None,
        provenance: Some(EdgeProvenance::Heuristic),
    }
}

pub(super) fn static_edge(source: &str, target: &str, kind: EdgeKind, line: Option<u64>) -> Edge {
    // 少数 pass 修复的是确定性的结构边，不能标成 heuristic，否则下游会误以为
    // 它只是运行时猜测。
    Edge {
        source: source.to_string(),
        target: target.to_string(),
        kind,
        metadata: None,
        line,
        column: None,
        provenance: None,
    }
}

pub(super) fn slice_lines(content: &str, start_line: u64, end_line: u64) -> Option<String> {
    // 抽取器的行号是 1-based；无效行号直接放弃，避免 regex 在整文件上误扫。
    if start_line == 0 || end_line == 0 {
        return None;
    }
    with_lines(content, |index| {
        let start = (start_line - 1) as usize;
        let count = end_line.saturating_sub(start_line).saturating_add(1) as usize;
        Some(
            index
                .ranges
                .iter()
                .skip(start)
                .take(count)
                .map(|&(a, b)| &content[a..b])
                .collect::<Vec<_>>()
                .join("\n"),
        )
    })
}

pub(super) fn enclosing_fn(nodes_in_file: &[Node], line: u64) -> Option<Node> {
    // 选最内层 callable/component 作为 dispatch site，避免把事件发送归到外层文件
    // 或类节点。
    nodes_in_file
        .iter()
        .filter(|node| {
            matches!(
                node.kind,
                NodeKind::Method | NodeKind::Function | NodeKind::Component
            ) && node.start_line <= line
                && node.end_line >= line
        })
        .max_by_key(|node| node.start_line)
        .cloned()
}

pub(super) fn method_and_function_nodes(queries: &mut QueryBuilder) -> Vec<Node> {
    let mut nodes = queries
        .get_nodes_by_kind(NodeKind::Method)
        .unwrap_or_default();
    nodes.extend(
        queries
            .get_nodes_by_kind(NodeKind::Function)
            .unwrap_or_default(),
    );
    nodes
}

pub(super) fn children_of_kind(queries: &mut QueryBuilder, id: &str, kind: NodeKind) -> Vec<Node> {
    queries
        .get_outgoing_edges(id, Some(vec![EdgeKind::Contains]), None)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|edge| queries.get_node_by_id(&edge.target).ok().flatten())
        .filter(|node| node.kind == kind)
        .collect()
}

pub(super) fn semantic_duplicate_methods(queries: &mut QueryBuilder, method: &Node) -> Vec<Node> {
    // 某些语言会同时抽到 trait/interface 声明和实现占位；按 qualified_name 找同义
    // 方法，保证 override 边从所有可能入口连到实现。
    let mut duplicates = queries
        .get_nodes_by_qualified_name_exact(&method.qualified_name)
        .unwrap_or_default()
        .into_iter()
        .filter(|node| {
            node.kind == NodeKind::Method
                && node.language == method.language
                && node.name == method.name
        })
        .collect::<Vec<_>>();
    if duplicates.is_empty() {
        duplicates.push(method.clone());
    }
    duplicates
}

// One source per thread: bounded storage, exact text identity (including same-length edits).
#[derive(Default)]
struct SourceLines {
    source: String,
    ranges: Vec<(usize, usize)>,
    newlines: Vec<usize>,
    initialized: bool,
    builds: usize,
}
thread_local! {
    static LINES: std::cell::RefCell<SourceLines> = std::cell::RefCell::new(SourceLines::default());
}
fn with_lines<T>(source: &str, f: impl FnOnce(&SourceLines) -> T) -> T {
    LINES.with(|cache| {
        let mut cache = cache.borrow_mut();
        if !cache.initialized || cache.source != source {
            cache.source = source.to_owned();
            cache.newlines = source
                .bytes()
                .enumerate()
                .filter_map(|(i, b)| (b == b'\n').then_some(i))
                .collect();
            cache.ranges.clear();
            let mut start = 0;
            for line in source.lines() {
                cache.ranges.push((start, start + line.len()));
                start += line.len();
                if source.as_bytes().get(start) == Some(&b'\r') {
                    start += 1;
                }
                if source.as_bytes().get(start) == Some(&b'\n') {
                    start += 1;
                }
            }
            cache.initialized = true;
            cache.builds += 1;
        }
        f(&cache)
    })
}
/// Preserve str::lines().count() on the prefix, including an unfinished final line.
pub(super) fn prefix_line_count(source: &str, offset: usize) -> u64 {
    assert!(source.is_char_boundary(offset));
    with_lines(source, |index| {
        let complete = index.newlines.partition_point(|&i| i < offset);
        (complete + usize::from(offset > 0 && source.as_bytes()[offset - 1] != b'\n')) as u64
    })
}
pub(super) struct LineCacheScope;
impl Drop for LineCacheScope {
    fn drop(&mut self) {
        LINES.with(|cache| *cache.borrow_mut() = SourceLines::default());
    }
}
#[cfg(test)]
// Used by the path-based cache reuse integration test, not the library tests.
#[allow(dead_code)]
pub(super) fn cache_builds() -> usize {
    LINES.with(|cache| cache.borrow().builds)
}
