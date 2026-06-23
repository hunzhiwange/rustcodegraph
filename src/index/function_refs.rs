//! 函数值引用补边。
//!
//! 这类边表示“把函数当作值传递或保存”，不是直接 calls；用 `fnRef` metadata 标记后，
//! resolver 可以在保持图语义的同时连接回真实函数节点。

use super::*;

pub(super) fn append_facade_function_ref_edges(
    file_path: &str,
    source: &str,
    language: Language,
    file_node_id: &str,
    nodes: &[Node],
    pending_edges: &mut Vec<RichFacadePendingEdge>,
) {
    let lines = source.lines().collect::<Vec<_>>();
    let mut seen = HashSet::new();

    for node in nodes.iter().filter(|node| {
        node.file_path == file_path && matches!(node.kind, NodeKind::Function | NodeKind::Method)
    }) {
        let body = facade_node_source(node, &lines);
        if body.is_empty() {
            continue;
        }
        for target_name in facade_fn_ref_names(&body, language, false) {
            // 在同一 source 上去重，避免一个函数体内重复传递同一 callback 造成边爆炸。
            push_facade_fn_ref_candidate(
                pending_edges,
                &mut seen,
                &node.id,
                target_name,
                node.start_line,
            );
        }
    }

    if matches!(language, Language::C | Language::Cpp) {
        // C/C++ 的静态表常保存函数指针，扫描文件级 initializer 能补上入口表到 handler 的引用。
        for (line, chunk) in facade_static_initializer_chunks(source) {
            for target_name in facade_static_initializer_fn_names(&chunk) {
                push_facade_fn_ref_candidate(
                    pending_edges,
                    &mut seen,
                    file_node_id,
                    target_name,
                    line,
                );
            }
        }
    }

    if language == Language::Ruby {
        append_facade_ruby_hook_refs(source, nodes, pending_edges, &mut seen);
    }
}

pub(super) fn append_facade_react_native_member_call_edges(
    file_path: &str,
    source: &str,
    language: Language,
    nodes: &[Node],
    pending_edges: &mut Vec<RichFacadePendingEdge>,
) {
    if !matches!(
        language,
        Language::TypeScript | Language::Tsx | Language::JavaScript | Language::Jsx
    ) {
        return;
    }
    let lines = source.lines().collect::<Vec<_>>();
    let mut seen = HashSet::new();
    for node in nodes.iter().filter(|node| {
        node.file_path == file_path && matches!(node.kind, NodeKind::Function | NodeKind::Method)
    }) {
        let body = facade_node_source(node, &lines);
        for target_name in member_call_names(&body) {
            if !target_name.starts_with("NativeModules.") {
                continue;
            }
            // React Native 模块方法通常没有静态 import，先保留完整成员名给后续宽松匹配。
            let key = format!("{}|{target_name}", node.id);
            if !seen.insert(key) {
                continue;
            }
            pending_edges.push(RichFacadePendingEdge {
                source: node.id.clone(),
                target_name,
                kind: EdgeKind::Calls,
                metadata: None,
                line: Some(node.start_line),
                column: Some(0),
            });
        }
    }
}

pub(super) fn facade_static_initializer_fn_names(chunk: &str) -> Vec<String> {
    let without_strings = strip_quoted_segments(chunk);
    dedupe_names(
        identifier_tokens(&without_strings)
            .into_iter()
            .filter(|name| !value_decl_keyword(name) && !method_decl_keyword(name))
            .collect(),
    )
}

pub(super) fn push_facade_fn_ref_candidate(
    pending_edges: &mut Vec<RichFacadePendingEdge>,
    seen: &mut HashSet<String>,
    source: &str,
    target_name: String,
    line: u64,
) {
    if target_name.is_empty() || facade_fn_ref_stop_name(&target_name) {
        return;
    }
    let key = format!("{source}|{target_name}");
    if !seen.insert(key) {
        return;
    }
    pending_edges.push(RichFacadePendingEdge {
        source: source.to_owned(),
        target_name,
        kind: EdgeKind::References,
        metadata: Some(HashMap::from([("fnRef".to_owned(), json!(true))])),
        line: Some(line),
        column: Some(0),
    });
}

pub(super) fn facade_node_source(node: &Node, lines: &[&str]) -> String {
    let start = node.start_line.saturating_sub(1) as usize;
    let end = node.end_line.max(node.start_line) as usize;
    if start >= lines.len() {
        return String::new();
    }
    let source = lines[start..end.min(lines.len())].join("\n");
    source
        .find('{')
        .map(|brace| source[brace + 1..].to_owned())
        .unwrap_or(source)
}

pub(super) fn facade_fn_ref_names(code: &str, language: Language, file_scope: bool) -> Vec<String> {
    let mut names = Vec::new();
    if language == Language::Php {
        // PHP callback 形式更依赖字符串/数组语法，交给专门的 callable 收集器处理。
        collect_php_callable_names(code, &mut names);
        return dedupe_names(names);
    }

    collect_scoped_fn_ref_names(code, language, &mut names);
    collect_assignment_fn_ref_names(code, language, &mut names);
    collect_call_arg_fn_ref_names(code, language, &mut names);

    if file_scope
        || matches!(
            language,
            Language::TypeScript | Language::Tsx | Language::JavaScript | Language::Jsx
        )
    {
        // JS/TS 对象字面量和数组配置经常保存 callback，需要额外扫描 literal 值。
        collect_literal_fn_ref_names(code, language, &mut names);
    }

    dedupe_names(names)
}

pub(super) fn collect_scoped_fn_ref_names(code: &str, language: Language, names: &mut Vec<String>) {
    for part in code.split_whitespace() {
        let trimmed = part.trim_matches(|ch: char| {
            !(ch.is_ascii_alphanumeric() || ch == '_' || ch == ':' || ch == '&' || ch == '@')
        });
        if let Some(name) = facade_candidate_from_expr(trimmed, language)
            && (name.contains("::") || name.starts_with("this."))
        {
            names.push(name);
        }
    }
}

pub(super) fn collect_assignment_fn_ref_names(
    code: &str,
    language: Language,
    names: &mut Vec<String>,
) {
    for line in code.lines() {
        let Some(rhs) = facade_assignment_rhs(line) else {
            continue;
        };
        if let Some(name) = facade_candidate_from_expr(rhs, language) {
            names.push(name);
        }
    }
}

pub(super) fn collect_call_arg_fn_ref_names(
    code: &str,
    language: Language,
    names: &mut Vec<String>,
) {
    let mut cursor = 0usize;
    while let Some(relative_open) = code[cursor..].find('(') {
        let open = cursor + relative_open;
        let Some(close) = facade_find_matching_delim(code, open, '(', ')', code.len()) else {
            cursor = open + 1;
            continue;
        };
        let callee = facade_callee_before_paren(&code[..open]);
        if callee.as_deref().is_some_and(is_call_keyword) {
            cursor = close + 1;
            continue;
        }
        for (arg_start, arg_end) in facade_split_top_level_segments(code, open + 1, close) {
            let arg = code[arg_start..arg_end].trim();
            if language == Language::Cpp && !facade_expr_is_explicit_fn_ref(arg) {
                continue;
            }
            if let Some(name) = facade_candidate_from_expr(arg, language) {
                names.push(name);
            }
        }
        cursor = close + 1;
    }
}

pub(super) fn collect_literal_fn_ref_names(
    code: &str,
    language: Language,
    names: &mut Vec<String>,
) {
    for line in code.lines() {
        let mut cursor = 0usize;
        while let Some(relative_colon) = line[cursor..].find(':') {
            let colon = cursor + relative_colon;
            let value = line[colon + 1..]
                .split([',', ';', '}'])
                .next()
                .unwrap_or_default()
                .trim();
            if let Some(name) = facade_candidate_from_expr(value, language) {
                names.push(name);
            }
            cursor = colon + 1;
        }
    }
    for delimiter in ['[', '{'] {
        let close_delimiter = if delimiter == '[' { ']' } else { '}' };
        let mut cursor = 0usize;
        while let Some(relative_open) = code[cursor..].find(delimiter) {
            let open = cursor + relative_open;
            let Some(close) =
                facade_find_matching_delim(code, open, delimiter, close_delimiter, code.len())
            else {
                cursor = open + 1;
                continue;
            };
            for (start, end) in facade_split_top_level_segments(code, open + 1, close) {
                let segment = code[start..end].trim();
                let value = facade_find_top_level_char(code, start, end, ':')
                    .map(|colon| code[colon + 1..end].trim())
                    .unwrap_or(segment);
                if let Some(name) = facade_candidate_from_expr(value, language) {
                    names.push(name);
                }
            }
            cursor = close + 1;
        }
    }
}
