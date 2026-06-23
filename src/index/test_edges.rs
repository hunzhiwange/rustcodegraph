//! 测试文件到被测函数的保守调用边。
//!
//! 当测试文件本身没有抽取出函数节点时，以 file 节点作为 caller，帮助 affected/test 相关查询保留一条弱连接。

use super::*;

pub(super) fn resolve_facade_test_file_edges(
    sources: &[(String, String)],
    nodes: &[Node],
) -> Vec<Edge> {
    let callables = nodes
        .iter()
        .filter(|node| matches!(node.kind, NodeKind::Function | NodeKind::Method))
        .collect::<Vec<_>>();
    if callables.is_empty() {
        return Vec::new();
    }

    let mut callables_by_name: HashMap<&str, Vec<&Node>> = HashMap::new();
    let mut callable_files = HashSet::new();
    for node in &callables {
        callables_by_name.entry(&node.name).or_default().push(*node);
        callable_files.insert(node.file_path.as_str());
    }

    let file_nodes_by_file = nodes
        .iter()
        .filter(|node| node.kind == NodeKind::File)
        .map(|node| (node.file_path.as_str(), node))
        .collect::<HashMap<_, _>>();

    let mut seen = HashSet::new();
    let mut edges = Vec::new();
    for (file_path, source) in sources {
        if !facade_is_test_file(file_path) || callable_files.contains(file_path.as_str()) {
            continue;
        }
        // 如果测试文件已有 callable 节点，就交给正常调用解析，避免 file 节点重复制造 calls 边。
        let Some(caller) = file_nodes_by_file.get(file_path.as_str()) else {
            continue;
        };

        for (idx, line) in source.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with('#') {
                continue;
            }
            let line_number = (idx + 1) as u64;
            for target_name in bare_call_names(trimmed) {
                let Some(candidates) = callables_by_name.get(target_name.as_str()) else {
                    continue;
                };
                let Some(target) = candidates.first().copied() else {
                    continue;
                };
                if caller.id == target.id {
                    continue;
                }
                let key = format!("{}:{}:{line_number}", caller.id, target.id);
                if !seen.insert(key) {
                    continue;
                }
                edges.push(Edge {
                    source: caller.id.clone(),
                    target: target.id.clone(),
                    kind: EdgeKind::Calls,
                    metadata: None,
                    line: Some(line_number),
                    column: Some(0),
                    provenance: None,
                });
            }
        }
    }

    edges
}
