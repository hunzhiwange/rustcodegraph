//! 语言专属的 fallback 补洞。
//!
//! 这些逻辑覆盖通用逐行扫描难以识别的结构，例如 Ruby 类内方法、Swift 方法和 Java 匿名类。

use super::*;

pub(super) fn append_facade_ruby_methods(
    file_path: &str,
    source: &str,
    language: Language,
    indexed_at: TimestampMs,
    nodes: &mut Vec<Node>,
    direct_edges: &mut Vec<Edge>,
) {
    // Ruby 的 def/end 不靠花括号，使用 class_stack 维护当前类，避免把类方法挂到 file 节点下。
    let source_lines = source.lines().collect::<Vec<_>>();
    let mut class_stack: Vec<(String, String)> = Vec::new();
    let mut seen_nodes = nodes
        .iter()
        .map(|node| node.id.clone())
        .collect::<HashSet<_>>();

    for (idx, line) in source_lines.iter().enumerate() {
        let line_number = (idx + 1) as u64;
        let trimmed = line.trim();
        if let Some((class_name, _)) = container_from_line(trimmed) {
            if let Some(class_node) = nodes
                .iter()
                .find(|node| node.kind == NodeKind::Class && node.name == class_name)
            {
                class_stack.push((class_name.to_owned(), class_node.id.clone()));
            }
            continue;
        }
        if trimmed == "end" {
            class_stack.pop();
            continue;
        }
        let Some(after_def) = trimmed.strip_prefix("def ") else {
            continue;
        };
        let Some((class_name, class_id)) = class_stack.last().cloned() else {
            continue;
        };
        let method_name = trim_identifier(after_def.split(['(', ';']).next().unwrap_or(after_def));
        if method_name.is_empty() {
            continue;
        }
        let mut node = facade_node(
            file_path,
            language,
            method_name,
            NodeKind::Method,
            line,
            line_number,
            Some(trimmed.to_owned()),
            None,
            false,
            indexed_at,
        );
        apply_facade_qualified_name(&mut node, None, Some(&class_name));
        node.end_line = ruby_function_end_line(&source_lines, idx);
        if seen_nodes.insert(node.id.clone()) {
            direct_edges.push(facade_contains_edge(&class_id, &node.id, line_number));
            nodes.push(node);
        }
    }
}

pub(super) fn append_facade_swift_methods(
    file_path: &str,
    source: &str,
    language: Language,
    indexed_at: TimestampMs,
    nodes: &mut Vec<Node>,
    direct_edges: &mut Vec<Edge>,
) {
    // Swift 原生抽取在部分语法组合下会漏方法，这里只按声明行补节点，不解析完整 Swift 语义。
    let source_lines = source.lines().collect::<Vec<_>>();
    let mut seen_nodes = nodes
        .iter()
        .map(|node| node.id.clone())
        .collect::<HashSet<_>>();
    let file_node_id = format!("file:{file_path}");
    for (idx, line) in source_lines.iter().enumerate() {
        let line_number = (idx + 1) as u64;
        let trimmed = line.trim();
        if !(trimmed.starts_with("func ") || trimmed.starts_with("static func ")) {
            continue;
        }
        let Some(name) = function_name_from_line(trimmed) else {
            continue;
        };
        if nodes
            .iter()
            .any(|node| node.name == name && node.start_line == line_number)
        {
            continue;
        }
        let mut node = facade_node(
            file_path,
            language,
            name,
            NodeKind::Method,
            line,
            line_number,
            Some(trimmed.to_owned()),
            None,
            trimmed.starts_with("static "),
            indexed_at,
        );
        if trimmed.contains('{') {
            node.end_line = block_end_line(&source_lines, idx);
        }
        if seen_nodes.insert(node.id.clone()) {
            direct_edges.push(facade_contains_edge(&file_node_id, &node.id, line_number));
            nodes.push(node);
        }
    }
}

pub(super) fn append_facade_java_anonymous_classes(
    file_path: &str,
    source: &str,
    language: Language,
    indexed_at: TimestampMs,
    nodes: &mut Vec<Node>,
    pending_edges: &mut Vec<RichFacadePendingEdge>,
    direct_edges: &mut Vec<Edge>,
) {
    // 匿名类没有稳定源码名称，使用 enclosing method + 行号生成合成类型名，并把 override 方法挂进去。
    let lines = source.lines().collect::<Vec<_>>();
    let mut idx = 0usize;
    while idx < lines.len() {
        let line = lines[idx];
        let trimmed = line.trim();
        let Some((base_type, new_pos)) = java_anonymous_base_type(trimmed) else {
            idx += 1;
            continue;
        };
        let line_number = (idx + 1) as u64;
        let Some(enclosing_method) =
            facade_enclosing_method_at(nodes, file_path, line_number, language).cloned()
        else {
            idx += 1;
            continue;
        };
        let anon_name = format!("{base_type}$anon@{line_number}");
        let mut anon = facade_node(
            file_path,
            language,
            &anon_name,
            NodeKind::Class,
            line,
            line_number,
            Some(trimmed.to_owned()),
            None,
            false,
            indexed_at,
        );
        anon.qualified_name = format!("{}::{anon_name}", enclosing_method.qualified_name);
        let anon_qn = anon.qualified_name.clone();
        let anon_id = anon.id.clone();
        let block_end = block_end_line(&lines, idx);
        anon.end_line = block_end;
        direct_edges.push(facade_contains_edge(
            &enclosing_method.id,
            &anon_id,
            line_number,
        ));
        pending_edges.push(RichFacadePendingEdge {
            source: anon_id.clone(),
            target_name: base_type.clone(),
            kind: EdgeKind::Extends,
            metadata: None,
            line: Some(line_number),
            column: Some(new_pos as u64),
        });
        nodes.push(anon);

        let mut inner = idx + 1;
        while inner < lines.len() && (inner + 1) as u64 <= block_end {
            let inner_line = lines[inner];
            let inner_trimmed = strip_leading_annotations(inner_line.trim());
            if let Some(method_name) = java_anonymous_method_name(inner_trimmed) {
                let mut method = facade_node(
                    file_path,
                    language,
                    method_name,
                    NodeKind::Method,
                    inner_line,
                    (inner + 1) as u64,
                    Some(inner_trimmed.to_owned()),
                    None,
                    false,
                    indexed_at,
                );
                method.id = format!(
                    "{}:{}:method:{}:{}",
                    file_path,
                    inner + 1,
                    sanitize_id_part(&anon_name),
                    sanitize_id_part(method_name)
                );
                method.qualified_name = format!("{anon_qn}::{method_name}");
                if inner_trimmed.contains('{') {
                    method.end_line = block_end_line(&lines, inner);
                }
                push_facade_executable_edges(
                    pending_edges,
                    &method.id,
                    inner_trimmed,
                    Some(method_name),
                    (inner + 1) as u64,
                );
                direct_edges.push(facade_contains_edge(
                    &anon_id,
                    &method.id,
                    (inner + 1) as u64,
                ));
                nodes.push(method);
            }
            inner += 1;
        }
        idx = inner;
    }
}

pub(super) fn java_anonymous_base_type(line: &str) -> Option<(String, usize)> {
    let new_idx = line.find("new ")?;
    let after_new = &line[new_idx + 4..];
    let base = first_identifier(after_new).map(trim_identifier)?;
    let after_base = after_new[base.len()..].trim_start();
    if after_base.starts_with("()") && after_base.contains('{') {
        Some((base.to_owned(), new_idx))
    } else {
        None
    }
}

pub(super) fn facade_enclosing_method_at<'a>(
    nodes: &'a [Node],
    file_path: &str,
    line_number: u64,
    language: Language,
) -> Option<&'a Node> {
    nodes
        .iter()
        .filter(|node| {
            node.file_path == file_path
                && node.language == language
                && node.kind == NodeKind::Method
                && node.start_line < line_number
                && node.end_line >= line_number
        })
        .max_by_key(|node| node.start_line)
}

pub(super) fn java_anonymous_method_name(line: &str) -> Option<&str> {
    if line.is_empty() || line.starts_with('@') || !line.contains('(') {
        return None;
    }
    if line.contains(" new ") || line.starts_with("return ") {
        return None;
    }
    facade_member_method_name(line, Language::Java)
}

pub(super) fn ruby_hook_method_names(line: &str) -> Vec<String> {
    // Rails/Ruby hook 常以符号传入方法名，例如 `before_action :load_user`。
    let Some(call) = first_identifier(line) else {
        return Vec::new();
    };
    if !is_ruby_hook_call(call) {
        return Vec::new();
    }
    line.split(|ch: char| ch == ',' || ch.is_whitespace())
        .filter_map(|part| part.trim().strip_prefix(':'))
        .filter(|name| is_simple_identifier(name))
        .map(str::to_owned)
        .collect()
}

pub(super) fn is_ruby_hook_call(name: &str) -> bool {
    name == "validate"
        || name == "set_callback"
        || name == "helper_method"
        || name == "rescue_from"
        || ((name.starts_with("before_")
            || name.starts_with("after_")
            || name.starts_with("around_")
            || name.starts_with("skip_before_")
            || name.starts_with("skip_after_")
            || name.starts_with("skip_around_"))
            && name.chars().all(|ch| ch.is_ascii_lowercase() || ch == '_'))
}
