//! 上下文输出格式化。
//!
//! 与 `formatter.ts` 保持契约一致：Markdown 面向 agent 快速阅读，JSON 面向
//! SDK/测试保留稳定的 TypeScript 字段名。

use std::collections::{HashMap, HashSet};

use serde::Serialize;

use crate::extraction::generated_detection::is_generated_file;
use crate::types::{
    CodeBlock, Edge, EdgeKind, Language, Node, NodeKind, Subgraph, TaskContext, Visibility,
};

/// 将上下文格式化为 Markdown。
pub fn format_context_as_markdown(context: &TaskContext) -> String {
    let mut lines = Vec::new();

    lines.push("## Code Context\n".to_string());
    lines.push(format!("**Query:** {}\n", context.query));

    // 生成文件放在后面：agent 优先看到用户维护的源码，必要时仍能看到 generated 入口。
    let mut ordered_entries = context.entry_points.clone();
    ordered_entries.sort_by_key(|a| is_generated_file(&a.file_path));
    if !ordered_entries.is_empty() {
        lines.push("### Entry Points\n".to_string());
        for node in ordered_entries {
            let location = line_location(node.start_line);
            lines.push(format!(
                "- **{}** ({}) - {}{}",
                node.name,
                node_kind_name(node.kind),
                node.file_path,
                location
            ));
            if let Some(signature) = &node.signature {
                lines.push(format!("  `{signature}`"));
            }
        }
        lines.push(String::new());
    }

    let entry_ids = context
        .entry_points
        .iter()
        .map(|node| node.id.as_str())
        .collect::<HashSet<_>>();
    let mut other_symbols = context
        .subgraph
        .nodes
        .values()
        .filter(|node| !entry_ids.contains(node.id.as_str()))
        .filter(|node| !is_generated_file(&node.file_path))
        .cloned()
        .collect::<Vec<_>>();
    // Related Symbols 只展示最多 10 个非 generated 节点，控制上下文体积。
    other_symbols.sort_by_key(node_sort_key);
    other_symbols.truncate(10);

    if !other_symbols.is_empty() {
        lines.push("### Related Symbols\n".to_string());
        let mut by_file = Vec::<(String, Vec<Node>)>::new();
        for node in other_symbols {
            if let Some((_, nodes)) = by_file.iter_mut().find(|(file, _)| file == &node.file_path) {
                nodes.push(node);
            } else {
                by_file.push((node.file_path.clone(), vec![node]));
            }
        }

        for (file, nodes) in by_file {
            let node_list = nodes
                .iter()
                .map(|node| format!("{}:{}", node.name, node.start_line))
                .collect::<Vec<_>>()
                .join(", ");
            lines.push(format!("- {file}: {node_list}"));
        }
        lines.push(String::new());
    }

    if !context.code_blocks.is_empty() {
        let mut ordered_blocks = context.code_blocks.clone();
        ordered_blocks.sort_by_key(|a| is_generated_file(&a.file_path));
        lines.push("### Code\n".to_string());
        for block in ordered_blocks {
            let node_name = block
                .node
                .as_ref()
                .map(|node| node.name.as_str())
                .unwrap_or("Unknown");
            lines.push(format!(
                "#### {} ({}:{})\n",
                node_name, block.file_path, block.start_line
            ));
            lines.push(format!("```{}", language_name(block.language)));
            lines.push(block.content);
            lines.push("```\n".to_string());
        }
    }

    lines.join("\n")
}

/// 将上下文格式化为 JSON。
pub fn format_context_as_json(context: &TaskContext) -> String {
    let mut nodes = context.subgraph.nodes.values().collect::<Vec<_>>();
    nodes.sort_by_key(|a| node_sort_key(a));

    // JSON 输出保留完整节点/边/code block 视图，并用 camelCase 对齐 JS SDK 旧契约。
    let serializable = SerializableContext {
        query: &context.query,
        summary: &context.summary,
        entry_points: context
            .entry_points
            .iter()
            .map(SerializableNode::from)
            .collect(),
        nodes: nodes.into_iter().map(SerializableNode::from).collect(),
        edges: context
            .subgraph
            .edges
            .iter()
            .map(SerializableEdge::from)
            .collect(),
        code_blocks: context
            .code_blocks
            .iter()
            .map(SerializableCodeBlock::from)
            .collect(),
        related_files: &context.related_files,
        stats: &context.stats,
    };

    serde_json::to_string_pretty(&serializable).unwrap_or_else(|error| {
        serde_json::json!({ "error": format!("failed to serialize context: {error}") }).to_string()
    })
}

/// 将子图格式化为 ASCII 树。
pub fn format_subgraph_tree(subgraph: &Subgraph, entry_points: &[Node]) -> String {
    let mut lines = Vec::new();
    let mut printed = HashSet::new();
    let mut outgoing: HashMap<&str, Vec<&Edge>> = HashMap::new();

    // 先按 source 建邻接表，后续递归只沿重要边展示，避免 contains 边淹没调用关系。
    for edge in &subgraph.edges {
        outgoing.entry(edge.source.as_str()).or_default().push(edge);
    }

    for entry in entry_points {
        format_node_tree(entry, subgraph, &outgoing, &mut printed, &mut lines, 0, "");
        lines.push(String::new());
    }

    let mut remaining = subgraph
        .nodes
        .values()
        .filter(|node| !printed.contains(node.id.as_str()))
        .collect::<Vec<_>>();
    remaining.sort_by_key(|a| node_sort_key(a));

    if !remaining.is_empty() && remaining.len() <= 10 {
        lines.push("Other relevant symbols:".to_string());
        for node in remaining {
            let location = line_location(node.start_line);
            lines.push(format!(
                "  {}: {} ({}{})",
                node_kind_name(node.kind),
                node.name,
                node.file_path,
                location
            ));
        }
    } else if remaining.len() > 10 {
        lines.push(format!("... and {} more related symbols", remaining.len()));
    }

    lines.join("\n").trim().to_string()
}

fn format_node_tree(
    node: &Node,
    subgraph: &Subgraph,
    outgoing: &HashMap<&str, Vec<&Edge>>,
    printed: &mut HashSet<String>,
    lines: &mut Vec<String>,
    depth: usize,
    prefix: &str,
) {
    if !printed.insert(node.id.clone()) {
        // 子图可能有环；同一节点只打印一次，避免 ASCII 树无限递归。
        return;
    }

    let location = line_location(node.start_line);
    let signature = node
        .signature
        .as_ref()
        .map(|signature| format!(" - {}", truncate(signature, 50)))
        .unwrap_or_default();
    lines.push(format!(
        "{}{}: {} ({}{}){}",
        prefix,
        node_kind_name(node.kind),
        node.name,
        node.file_path,
        location,
        signature
    ));

    let significant_edges = outgoing
        .get(node.id.as_str())
        .map(|edges| {
            edges
                .iter()
                .copied()
                .filter(|edge| {
                    matches!(
                        edge.kind,
                        EdgeKind::Calls
                            | EdgeKind::Extends
                            | EdgeKind::Implements
                            | EdgeKind::Imports
                            | EdgeKind::References
                    )
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    // 保留原始边种类出现顺序分组，比按 enum 排序更贴近 ContextBuilder 的排序结果。
    let mut edges_by_kind = Vec::<(EdgeKind, Vec<&Edge>)>::new();
    for edge in &significant_edges {
        if let Some((_, edges)) = edges_by_kind
            .iter_mut()
            .find(|(kind, _)| kind == &edge.kind)
        {
            edges.push(*edge);
        } else {
            edges_by_kind.push((edge.kind, vec![*edge]));
        }
    }

    let new_prefix = format!("{prefix}  ");
    for (kind, kind_edges) in edges_by_kind {
        if kind_edges.len() > 3 {
            let names = kind_edges
                .iter()
                .take(3)
                .map(|edge| {
                    subgraph
                        .nodes
                        .get(&edge.target)
                        .map(|target| target.name.as_str())
                        .unwrap_or("unknown")
                })
                .collect::<Vec<_>>()
                .join(", ");
            lines.push(format!(
                "{}├── {}: {} and {} more",
                new_prefix,
                edge_kind_name(kind),
                names,
                kind_edges.len() - 3
            ));
        } else {
            for (idx, edge) in kind_edges.iter().enumerate() {
                let target_name = subgraph
                    .nodes
                    .get(&edge.target)
                    .map(|target| target.name.as_str())
                    .unwrap_or("unknown");
                let connector = if idx == kind_edges.len() - 1 {
                    "└──"
                } else {
                    "├──"
                };
                lines.push(format!(
                    "{}{} {} → {}",
                    new_prefix,
                    connector,
                    edge_kind_name(kind),
                    target_name
                ));
            }
        }
    }

    if depth < 1 {
        // 树只展开一层，再多会让 agent 面对重复和噪音；完整节点仍在 JSON/markdown 中。
        for edge in significant_edges.into_iter().take(3) {
            if let Some(target) = subgraph.nodes.get(&edge.target)
                && !printed.contains(target.id.as_str())
            {
                format_node_tree(
                    target,
                    subgraph,
                    outgoing,
                    printed,
                    lines,
                    depth + 1,
                    &new_prefix,
                );
            }
        }
    }
}

fn truncate(value: &str, max_length: usize) -> String {
    // 按 char 截断，避免 UTF-8 中间截断；签名过长时宁愿丢尾部而不破坏输出。
    if value.chars().count() <= max_length {
        return value.to_string();
    }
    value
        .chars()
        .take(max_length.saturating_sub(3))
        .collect::<String>()
        + "..."
}

/// 将字节数格式化为人类可读字符串。
pub fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} bytes")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SerializableContext<'a> {
    query: &'a str,
    summary: &'a str,
    entry_points: Vec<SerializableNode<'a>>,
    nodes: Vec<SerializableNode<'a>>,
    edges: Vec<SerializableEdge<'a>>,
    code_blocks: Vec<SerializableCodeBlock<'a>>,
    related_files: &'a [String],
    stats: &'a crate::types::TaskContextStats,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SerializableNode<'a> {
    id: &'a str,
    kind: NodeKind,
    name: &'a str,
    qualified_name: &'a str,
    file_path: &'a str,
    language: Language,
    start_line: u64,
    end_line: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    signature: Option<&'a String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    docstring: Option<&'a String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    visibility: Option<Visibility>,
    #[serde(skip_serializing_if = "Option::is_none")]
    is_exported: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    is_async: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    is_static: Option<bool>,
}

impl<'a> From<&'a Node> for SerializableNode<'a> {
    fn from(node: &'a Node) -> Self {
        // Serializable* 类型借用原始 context，避免格式化大上下文时复制源码块内容。
        Self {
            id: &node.id,
            kind: node.kind,
            name: &node.name,
            qualified_name: &node.qualified_name,
            file_path: &node.file_path,
            language: node.language,
            start_line: node.start_line,
            end_line: node.end_line,
            signature: node.signature.as_ref(),
            docstring: node.docstring.as_ref(),
            visibility: node.visibility,
            is_exported: node.is_exported,
            is_async: node.is_async,
            is_static: node.is_static,
        }
    }
}

#[derive(Serialize)]
struct SerializableEdge<'a> {
    source: &'a str,
    target: &'a str,
    kind: EdgeKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    line: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    column: Option<u64>,
}

impl<'a> From<&'a Edge> for SerializableEdge<'a> {
    fn from(edge: &'a Edge) -> Self {
        Self {
            source: &edge.source,
            target: &edge.target,
            kind: edge.kind,
            line: edge.line,
            column: edge.column,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SerializableCodeBlock<'a> {
    file_path: &'a str,
    start_line: u64,
    end_line: u64,
    language: Language,
    content: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    node_name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    node_kind: Option<NodeKind>,
}

impl<'a> From<&'a CodeBlock> for SerializableCodeBlock<'a> {
    fn from(block: &'a CodeBlock) -> Self {
        Self {
            file_path: &block.file_path,
            start_line: block.start_line,
            end_line: block.end_line,
            language: block.language,
            content: &block.content,
            node_name: block.node.as_ref().map(|node| node.name.as_str()),
            node_kind: block.node.as_ref().map(|node| node.kind),
        }
    }
}

fn line_location(line: u64) -> String {
    if line > 0 {
        format!(":{line}")
    } else {
        String::new()
    }
}

fn node_sort_key(node: &Node) -> (String, u64, String, String) {
    (
        node.file_path.clone(),
        node.start_line,
        node.name.clone(),
        node.id.clone(),
    )
}

fn enum_json_name<T: Serialize>(value: T) -> String {
    // NodeKind/EdgeKind/Language 的用户可见名字以 serde JSON 字符串为准。
    match serde_json::to_value(value) {
        Ok(serde_json::Value::String(name)) => name,
        _ => "unknown".to_string(),
    }
}

pub(crate) fn node_kind_name(kind: NodeKind) -> String {
    enum_json_name(kind)
}

pub(crate) fn edge_kind_name(kind: EdgeKind) -> String {
    enum_json_name(kind)
}

pub(crate) fn language_name(language: Language) -> String {
    enum_json_name(language)
}
