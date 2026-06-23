//! Rust CLI 轻量索引器的调用边合成。
//!
//! 这不是完整 resolver：它只在已抽取的函数/方法符号之间做保守连边，用于让
//! CLI/MCP shim 在没有完整 tree-sitter 管线时仍能回答基本 callers/callees。

use std::collections::{HashMap, HashSet};

use rustcodegraph::types::{Edge, EdgeKind, EdgeProvenance, LineNumber, Node, NodeKind};

use super::super::storage::is_test_file;
use super::IndexProgressRenderer;

pub(super) fn extract_lightweight_call_edges(
    sources: &[(String, String)],
    nodes: &[Node],
    progress: &mut IndexProgressRenderer,
) -> Vec<Edge> {
    let callables = nodes
        .iter()
        .filter(|node| matches!(node.kind, NodeKind::Function | NodeKind::Method))
        .collect::<Vec<_>>();
    if callables.is_empty() {
        return Vec::new();
    }

    // 同文件同名优先，因为局部 helper 比跨文件同名符号更可能是正确目标。
    // 跨文件只在全局唯一时连边，避免把常见名字如 `new`、`run` 错连成噪音。
    let mut callables_by_name: HashMap<&str, Vec<&Node>> = HashMap::new();
    let mut callables_by_file_and_name: HashMap<(&str, &str), Vec<&Node>> = HashMap::new();
    for node in &callables {
        if node.name.len() >= 2 {
            callables_by_name
                .entry(node.name.as_str())
                .or_default()
                .push(node);
            callables_by_file_and_name
                .entry((node.file_path.as_str(), node.name.as_str()))
                .or_default()
                .push(node);
        }
    }
    let mut callables_by_file: HashMap<&str, Vec<&Node>> = HashMap::new();
    for node in &callables {
        callables_by_file
            .entry(node.file_path.as_str())
            .or_default()
            .push(node);
    }
    for nodes in callables_by_file.values_mut() {
        nodes.sort_by_key(|node| node.start_line);
    }
    let file_nodes_by_file = nodes
        .iter()
        .filter(|node| matches!(node.kind, NodeKind::File))
        .map(|node| (node.file_path.as_str(), node))
        .collect::<HashMap<_, _>>();

    let mut edge_keys = HashSet::new();
    let mut edges = Vec::new();
    let total_sources = sources.len();
    for (source_idx, (file_path, source)) in sources.iter().enumerate() {
        progress.percent("Resolving calls", source_idx + 1, total_sources);
        let mut file_callers = callables_by_file
            .get(file_path.as_str())
            .cloned()
            .unwrap_or_default();
        if file_callers.is_empty()
            && is_test_file(file_path)
            && let Some(file_node) = file_nodes_by_file.get(file_path.as_str())
        {
            // 测试文件常常是顶层断言脚本，没有函数声明；用 file 节点作为 caller
            // 能让 affected/test 查询仍然看到“测试文件依赖了某符号”。
            file_callers.push(*file_node);
        }
        if file_callers.is_empty() {
            continue;
        }
        let lines = source.lines().collect::<Vec<_>>();
        for (index, caller) in file_callers.iter().enumerate() {
            let start_line = caller.start_line.max(1) as usize;
            let end_line = file_callers
                .get(index + 1)
                .map(|next| next.start_line.saturating_sub(1) as usize)
                .unwrap_or(lines.len())
                .max(start_line);
            for (offset, line) in lines
                .iter()
                .enumerate()
                .skip(start_line.saturating_sub(1))
                .take(end_line.saturating_sub(start_line).saturating_add(1))
            {
                let trimmed = line.trim();
                if trimmed.starts_with("//") || trimmed.starts_with('#') {
                    continue;
                }
                let line_number = (offset + 1) as LineNumber;
                // 轻量扫描按 caller 起始行到下一个 callable 起始行切片；它不理解嵌套作用域，
                // 但足够生成低成本、低风险的启发式 calls 边。
                for call_name in call_names_in_line(trimmed) {
                    let same_file_key = (file_path.as_str(), call_name);
                    if let Some(callees) = callables_by_file_and_name.get(&same_file_key) {
                        push_call_edges(caller, callees, line_number, &mut edge_keys, &mut edges);
                        continue;
                    }
                    if let Some(callees) = callables_by_name.get(call_name)
                        && callees.len() == 1
                    {
                        push_call_edges(caller, callees, line_number, &mut edge_keys, &mut edges);
                    }
                }
            }
        }
    }
    edges
}

fn push_call_edges(
    caller: &Node,
    callees: &[&Node],
    line_number: LineNumber,
    edge_keys: &mut HashSet<(String, String, LineNumber)>,
    edges: &mut Vec<Edge>,
) {
    for callee in callees {
        if caller.id == callee.id {
            continue;
        }
        // 同一行重复出现的调用只保留一条边，避免链式调用或宏展开样式放大图规模。
        let key = (caller.id.clone(), callee.id.clone(), line_number);
        if !edge_keys.insert(key) {
            continue;
        }
        edges.push(Edge {
            source: caller.id.clone(),
            target: callee.id.clone(),
            kind: EdgeKind::Calls,
            metadata: None,
            line: Some(line_number),
            column: None,
            provenance: Some(EdgeProvenance::Heuristic),
        });
    }
}

fn call_names_in_line(line: &str) -> Vec<&str> {
    // 找到 `(` 前最近的标识符，覆盖 foo() / obj.foo() 的常见尾部名字；
    // 不追踪接收者类型，因此调用方必须继续做唯一性过滤。
    let bytes = line.as_bytes();
    let mut names = Vec::new();
    let mut seen = HashSet::new();
    for idx in 0..bytes.len() {
        if bytes[idx] != b'(' {
            continue;
        }
        let mut name_end = idx;
        while name_end > 0 && bytes[name_end - 1].is_ascii_whitespace() {
            name_end -= 1;
        }
        let mut name_start = name_end;
        while name_start > 0 && is_identifier_byte(bytes[name_start - 1]) {
            name_start -= 1;
        }
        if name_end.saturating_sub(name_start) < 2 {
            continue;
        }
        let name = &line[name_start..name_end];
        if seen.insert(name) {
            names.push(name);
        }
    }
    names
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$'
}
