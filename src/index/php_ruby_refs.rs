//! PHP callable 和 Ruby hook callback 的补充提取。
//!
//! 两种语言都大量用字符串/符号表示回调；这些 helper 只在已知高阶函数或框架 hook 语境中提升为函数引用。

use super::*;

pub(super) fn collect_php_callable_names(code: &str, names: &mut Vec<String>) {
    // 只扫描 PHP 标准 callable 高阶函数，避免普通字符串字面量被误认为函数名。
    for hof in PHP_CALLABLE_HOFS {
        let mut cursor = 0usize;
        let needle = format!("{hof}(");
        while let Some(relative) = code[cursor..].find(&needle) {
            let open = cursor + relative + hof.len();
            let Some(close) = facade_find_matching_delim(code, open, '(', ')', code.len()) else {
                cursor = open + 1;
                continue;
            };
            for (start, end) in facade_split_top_level_segments(code, open + 1, close) {
                let arg = code[start..end].trim();
                if let Some(name) = php_callable_from_arg(arg) {
                    names.push(name);
                }
            }
            cursor = close + 1;
        }
    }
}

pub(super) fn php_callable_from_arg(arg: &str) -> Option<String> {
    let trimmed = arg.trim();
    if let Some(content) = quoted_string_content(trimmed) {
        return is_simple_identifier(&content).then_some(content);
    }
    if !(trimmed.starts_with('[') && trimmed.ends_with(']')) {
        return None;
    }
    let inner = &trimmed[1..trimmed.len().saturating_sub(1)];
    let parts = facade_split_top_level_segments(inner, 0, inner.len());
    if parts.len() != 2 {
        return None;
    }
    let receiver = inner[parts[0].0..parts[0].1].trim();
    let member = quoted_string_content(inner[parts[1].0..parts[1].1].trim())?;
    if !is_simple_identifier(&member) {
        return None;
    }
    if receiver == "$this" {
        return Some(format!("this.{member}"));
    }
    let class_name = receiver.strip_suffix("::class")?.trim();
    is_simple_identifier(class_name).then(|| format!("{class_name}::{member}"))
}

pub(super) fn append_facade_ruby_hook_refs(
    source: &str,
    nodes: &[Node],
    pending_edges: &mut Vec<RichFacadePendingEdge>,
    seen: &mut HashSet<String>,
) {
    // Ruby hook 行通常位于类体中，source 需要绑定到当前类的同名方法节点而不是 file 节点。
    let class_nodes = nodes
        .iter()
        .filter(|node| matches!(node.kind, NodeKind::Class | NodeKind::Struct))
        .map(|node| (node.name.as_str(), node))
        .collect::<HashMap<_, _>>();
    let mut current_class: Option<&Node> = None;
    for (idx, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        if let Some(after_class) = trimmed.strip_prefix("class ") {
            current_class =
                first_identifier(after_class).and_then(|name| class_nodes.get(name).copied());
            continue;
        }
        if trimmed == "end" {
            current_class = None;
            continue;
        }
        let Some(class_node) = current_class else {
            continue;
        };
        for name in ruby_hook_method_names(trimmed) {
            push_facade_fn_ref_candidate(
                pending_edges,
                seen,
                &class_node.id,
                format!("this.{name}"),
                (idx + 1) as u64,
            );
        }
    }
}

pub(super) fn ruby_superclass_from_line(line: &str) -> Option<String> {
    let after_lt = line.split_once('<')?.1.trim();
    first_identifier(after_lt).map(|name| trim_identifier(name).to_owned())
}
