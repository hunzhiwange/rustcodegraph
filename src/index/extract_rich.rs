//! facade fallback 抽取状态机。
//!
//! 当原生 tree-sitter 抽取不可用、文件过大或某些语言需要补洞时，这里用轻量文本启发式补出节点、
//! contains 边和待解析边；它追求“足够阻止 agent 回退 Read/Grep”，不尝试成为完整解析器。
// Kept just over 500 lines because this is the fallback extractor state machine; splitting it further would make this mechanical facade split harder to audit.
use super::*;

pub(super) fn extract_facade_symbols_rich(
    file_path: &str,
    source: &str,
    language: Language,
    indexed_at: TimestampMs,
    framework_resolvers: &[ResolverRef],
) -> (Vec<Node>, Vec<RichFacadePendingEdge>, Vec<Edge>) {
    let mut nodes = Vec::new();
    let mut pending_edges = Vec::new();
    let mut direct_edges = Vec::new();
    let file_node_id = format!("file:{file_path}");
    let package_prefix = package_prefix_from_source(language, source);
    let mut in_class = false;
    let mut class_depth = 0isize;
    let mut current_class_id: Option<String> = None;
    let mut current_class_name: Option<String> = None;
    let mut current_container_kind: Option<FacadeContainerKind> = None;
    let mut current_objc_class_id: Option<String> = None;
    let mut current_objc_class_name: Option<String> = None;
    let mut in_function = false;
    let mut function_depth = 0isize;
    let mut current_function: Option<(String, String)> = None;
    let source_lines = source.lines().collect::<Vec<_>>();

    for (idx, line) in source_lines.iter().copied().enumerate() {
        let line_number = (idx + 1) as u64;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }
        if let Some(import_name) = facade_import_name_from_line(trimmed, language) {
            let node = facade_node(
                file_path,
                language,
                &import_name,
                NodeKind::Import,
                line,
                line_number,
                Some(trimmed.to_owned()),
                None,
                false,
                indexed_at,
            );
            direct_edges.push(facade_contains_edge(&file_node_id, &node.id, line_number));
            pending_edges.push(RichFacadePendingEdge {
                source: node.id.clone(),
                target_name: import_name,
                kind: EdgeKind::Imports,
                metadata: None,
                line: Some(line_number),
                column: Some(0),
            });
            nodes.push(node);
            continue;
        }
        if language == Language::ObjC && trimmed == "@end" {
            current_objc_class_id = None;
            current_objc_class_name = None;
            continue;
        }

        if in_function {
            if let Some((function_id, function_name)) = current_function.as_ref() {
                // 多行函数体中的调用边必须边扫描边记录；只看声明行会漏掉大多数 fallback 场景。
                push_facade_executable_edges(
                    &mut pending_edges,
                    function_id,
                    trimmed,
                    Some(function_name),
                    line_number,
                );
            }
            function_depth += brace_delta(trimmed);
            if function_depth <= 0 {
                in_function = false;
                function_depth = 0;
                current_function = None;
            }
            continue;
        }

        if let Some((class_name, container_kind)) = container_from_line(trimmed) {
            let node_kind = match container_kind {
                FacadeContainerKind::Class | FacadeContainerKind::Object => NodeKind::Class,
                FacadeContainerKind::Struct => NodeKind::Struct,
                FacadeContainerKind::Enum => NodeKind::Enum,
                FacadeContainerKind::Trait => NodeKind::Trait,
            };
            let mut node = facade_node(
                file_path,
                language,
                class_name,
                node_kind,
                line,
                line_number,
                None,
                None,
                false,
                indexed_at,
            );
            node.end_line = block_end_line(&source_lines, idx);
            apply_facade_qualified_name(&mut node, package_prefix.as_deref(), None);
            let class_id = node.id.clone();
            // 继承/实现关系先以名称保存，等全文件节点汇总后再统一解析，避免前向引用丢失。
            for target_name in class_relation_names_from_line(trimmed, "implements") {
                pending_edges.push(RichFacadePendingEdge {
                    source: class_id.clone(),
                    target_name,
                    kind: EdgeKind::Implements,
                    metadata: None,
                    line: Some(line_number),
                    column: Some(0),
                });
            }
            for target_name in class_relation_names_from_line(trimmed, "extends") {
                pending_edges.push(RichFacadePendingEdge {
                    source: class_id.clone(),
                    target_name,
                    kind: EdgeKind::Extends,
                    metadata: None,
                    line: Some(line_number),
                    column: Some(0),
                });
            }
            if language == Language::Dart {
                for target_name in class_relation_names_from_line(trimmed, "with") {
                    pending_edges.push(RichFacadePendingEdge {
                        source: class_id.clone(),
                        target_name,
                        kind: EdgeKind::Implements,
                        metadata: None,
                        line: Some(line_number),
                        column: Some(0),
                    });
                }
            }
            if matches!(language, Language::C | Language::Cpp) {
                for target_name in cpp_base_class_names_from_line(trimmed) {
                    pending_edges.push(RichFacadePendingEdge {
                        source: class_id.clone(),
                        target_name,
                        kind: EdgeKind::Extends,
                        metadata: None,
                        line: Some(line_number),
                        column: Some(0),
                    });
                }
            }
            if language == Language::Ruby
                && let Some(target_name) = ruby_superclass_from_line(trimmed)
            {
                pending_edges.push(RichFacadePendingEdge {
                    source: class_id.clone(),
                    target_name,
                    kind: EdgeKind::Extends,
                    metadata: None,
                    line: Some(line_number),
                    column: Some(0),
                });
            }
            direct_edges.push(facade_contains_edge(&file_node_id, &class_id, line_number));
            nodes.push(node);
            if language == Language::ObjC && trimmed.starts_with("@implementation ") {
                current_objc_class_id = Some(class_id.clone());
                current_objc_class_name = Some(class_name.to_owned());
            }
            if let Some(inline_body) = inline_class_body(trimmed) {
                // 单行 class/struct 在 generated 或压缩代码里很常见，不能依赖后续 in_class 状态继续扫描。
                for member in inline_class_member_segments(inline_body) {
                    if let Some((node, mut refs)) = class_member_from_line(
                        file_path,
                        language,
                        Some(class_name),
                        Some(container_kind),
                        &member,
                        line_number,
                        indexed_at,
                        package_prefix.as_deref(),
                    ) {
                        direct_edges.push(facade_contains_edge(&class_id, &node.id, line_number));
                        pending_edges.append(&mut refs);
                        nodes.push(node);
                    }
                }
            }
            let delta = brace_delta(trimmed);
            if trimmed.contains('{') && delta > 0 {
                in_class = true;
                class_depth = delta;
                current_class_id = Some(class_id);
                current_class_name = Some(class_name.to_owned());
                current_container_kind = Some(container_kind);
            }
            continue;
        }

        if let Some(interface_name) = interface_name_from_line(trimmed) {
            let mut node = facade_node(
                file_path,
                language,
                interface_name,
                NodeKind::Interface,
                line,
                line_number,
                Some(trimmed.to_owned()),
                None,
                false,
                indexed_at,
            );
            node.end_line = block_end_line(&source_lines, idx);
            apply_facade_qualified_name(&mut node, package_prefix.as_deref(), None);
            let interface_id = node.id.clone();
            for target_name in class_relation_names_from_line(trimmed, "extends") {
                pending_edges.push(RichFacadePendingEdge {
                    source: interface_id.clone(),
                    target_name,
                    kind: EdgeKind::Extends,
                    metadata: None,
                    line: Some(line_number),
                    column: Some(0),
                });
            }
            direct_edges.push(facade_contains_edge(
                &file_node_id,
                &interface_id,
                line_number,
            ));
            nodes.push(node);
            continue;
        }

        if let Some(type_name) = type_alias_name_from_line(trimmed) {
            let mut node = facade_node(
                file_path,
                language,
                type_name,
                NodeKind::TypeAlias,
                line,
                line_number,
                Some(trimmed.to_owned()),
                None,
                false,
                indexed_at,
            );
            apply_facade_qualified_name(&mut node, package_prefix.as_deref(), None);
            direct_edges.push(facade_contains_edge(&file_node_id, &node.id, line_number));
            nodes.push(node);
            continue;
        }

        if in_class {
            if let Some((mut node, mut refs)) = class_member_from_line(
                file_path,
                language,
                current_class_name.as_deref(),
                current_container_kind,
                line,
                line_number,
                indexed_at,
                package_prefix.as_deref(),
            ) {
                if trimmed.contains('{') {
                    node.end_line = block_end_line(&source_lines, idx);
                }
                if let Some(class_id) = current_class_id.as_deref() {
                    direct_edges.push(facade_contains_edge(class_id, &node.id, line_number));
                }
                // 成员内部引用暂时绑定到临时节点；后续关系补扫会把 source 修正为去重后的真实节点。
                pending_edges.append(&mut refs);
                nodes.push(node);
            }
            class_depth += brace_delta(trimmed);
            if class_depth <= 0 {
                in_class = false;
                class_depth = 0;
                current_class_id = None;
                current_class_name = None;
                current_container_kind = None;
            }
            continue;
        }

        if language == Language::ObjC
            && let Some(name) = objc_method_name_from_line(trimmed)
        {
            let mut node = facade_node(
                file_path,
                language,
                name,
                NodeKind::Method,
                line,
                line_number,
                Some(trimmed.to_owned()),
                None,
                trimmed.starts_with('+'),
                indexed_at,
            );
            if trimmed.contains('{') {
                node.end_line = block_end_line(&source_lines, idx);
            }
            apply_facade_qualified_name(
                &mut node,
                package_prefix.as_deref(),
                current_objc_class_name.as_deref(),
            );
            let parent_id = current_objc_class_id.as_deref().unwrap_or(&file_node_id);
            direct_edges.push(facade_contains_edge(parent_id, &node.id, line_number));
            nodes.push(node);
            continue;
        }

        if language == Language::Go
            && let Some((receiver_type, name)) = go_receiver_method_from_line(trimmed)
        {
            let mut node = facade_node(
                file_path,
                language,
                name,
                NodeKind::Method,
                line,
                line_number,
                Some(trimmed.to_owned()),
                None,
                false,
                indexed_at,
            );
            node.qualified_name = format!("{receiver_type}::{name}");
            if trimmed.contains('{') {
                node.end_line = block_end_line(&source_lines, idx);
            }
            push_facade_executable_edges(
                &mut pending_edges,
                &node.id,
                trimmed,
                Some(name),
                line_number,
            );
            let parent_id = nodes
                .iter()
                .find(|candidate| {
                    candidate.file_path == file_path
                        && candidate.name == receiver_type
                        && matches!(candidate.kind, NodeKind::Struct | NodeKind::Class)
                })
                .map(|candidate| candidate.id.as_str())
                .unwrap_or(&file_node_id);
            direct_edges.push(facade_contains_edge(parent_id, &node.id, line_number));
            nodes.push(node);
            continue;
        }

        if matches!(language, Language::C | Language::Cpp)
            && let Some((class_name, name)) = cpp_qualified_method_from_line(trimmed)
        {
            let mut node = facade_node(
                file_path,
                language,
                &name,
                NodeKind::Method,
                line,
                line_number,
                Some(trimmed.to_owned()),
                None,
                false,
                indexed_at,
            );
            node.qualified_name = format!("{class_name}::{name}");
            if trimmed.contains('{') {
                node.end_line = block_end_line(&source_lines, idx);
            }
            push_facade_executable_edges(
                &mut pending_edges,
                &node.id,
                trimmed,
                Some(&name),
                line_number,
            );
            for target_name in member_call_names(trimmed) {
                pending_edges.push(RichFacadePendingEdge {
                    source: node.id.clone(),
                    target_name,
                    kind: EdgeKind::Calls,
                    metadata: None,
                    line: Some(line_number),
                    column: Some(0),
                });
            }
            let parent_id = nodes
                .iter()
                .find(|candidate| {
                    candidate.file_path == file_path
                        && candidate.name == class_name
                        && facade_supertype_bearing(candidate.kind)
                })
                .map(|candidate| candidate.id.as_str())
                .unwrap_or(&file_node_id);
            direct_edges.push(facade_contains_edge(parent_id, &node.id, line_number));
            nodes.push(node);
            continue;
        }

        if let Some(name) = function_name_from_line(trimmed) {
            if matches!(language, Language::Pascal) && !pascal_has_body(&source_lines, idx) {
                continue;
            }
            let mut node = facade_node(
                file_path,
                language,
                name,
                NodeKind::Function,
                line,
                line_number,
                Some(trimmed.to_owned()),
                None,
                false,
                indexed_at,
            );
            if trimmed.contains('{') {
                node.end_line = block_end_line(&source_lines, idx);
            } else if matches!(language, Language::Pascal) {
                node.end_line = pascal_function_end_line(&source_lines, idx);
            } else if matches!(language, Language::Python)
                && (trimmed.starts_with("def ") || trimmed.starts_with("async def "))
            {
                node.end_line = python_function_end_line(&source_lines, idx);
            } else if matches!(language, Language::Ruby) && trimmed.starts_with("def ") {
                node.end_line = ruby_function_end_line(&source_lines, idx);
            }
            apply_facade_qualified_name(&mut node, package_prefix.as_deref(), None);
            push_facade_executable_edges(
                &mut pending_edges,
                &node.id,
                trimmed,
                Some(name),
                line_number,
            );
            let delta = brace_delta(trimmed);
            if trimmed.contains('{') && delta > 0 {
                in_function = true;
                function_depth = delta;
                current_function = Some((node.id.clone(), name.to_owned()));
            }
            direct_edges.push(facade_contains_edge(&file_node_id, &node.id, line_number));
            nodes.push(node);
            continue;
        }

        if let Some((name, kind, body)) = top_level_value_from_line(trimmed, language) {
            let mut node = facade_node(
                file_path,
                language,
                name,
                kind,
                line,
                line_number,
                Some(trimmed.to_owned()),
                None,
                false,
                indexed_at,
            );
            apply_facade_qualified_name(&mut node, package_prefix.as_deref(), None);
            if kind == NodeKind::Function {
                push_facade_executable_edges(
                    &mut pending_edges,
                    &node.id,
                    body,
                    Some(name),
                    line_number,
                );
            }
            direct_edges.push(facade_contains_edge(&file_node_id, &node.id, line_number));
            nodes.push(node);
        }
    }

    if matches!(
        language,
        Language::TypeScript | Language::JavaScript | Language::Tsx | Language::Jsx
    ) {
        // Zustand/Jotai 类 store 常把 action 定义在返回的对象字面量中，tree-sitter fallback 的逐行扫描看不到。
        append_facade_store_object_functions(
            file_path,
            source,
            language,
            indexed_at,
            &file_node_id,
            &mut nodes,
            &mut pending_edges,
            &mut direct_edges,
        );
    }

    append_facade_framework_extraction(
        file_path,
        source,
        language,
        framework_resolvers,
        &file_node_id,
        &mut nodes,
        &mut pending_edges,
        &mut direct_edges,
    );
    if language == Language::Ruby {
        append_facade_ruby_methods(
            file_path,
            source,
            language,
            indexed_at,
            &mut nodes,
            &mut direct_edges,
        );
    }
    if language == Language::Swift {
        append_facade_swift_methods(
            file_path,
            source,
            language,
            indexed_at,
            &mut nodes,
            &mut direct_edges,
        );
    }
    if language == Language::Java {
        append_facade_java_anonymous_classes(
            file_path,
            source,
            language,
            indexed_at,
            &mut nodes,
            &mut pending_edges,
            &mut direct_edges,
        );
    }
    append_facade_function_ref_edges(
        file_path,
        source,
        language,
        &file_node_id,
        &nodes,
        &mut pending_edges,
    );

    (nodes, pending_edges, direct_edges)
}
