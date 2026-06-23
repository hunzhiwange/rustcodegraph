use super::*;

/// TS/JS store 工厂的兜底抽取。某些大文件或新语法会让 tree-sitter 漏掉
/// `export const store = create(() => ({ action() {} }))` 里的 action；这个文本扫描
/// 只在命中特征串时启用，目标是补出关键函数和调用边，而不是完整解析 JS。
pub(super) fn try_ts_js_store_object_fallback(
    file_path: &str,
    source: &str,
    language: &Language,
) -> Option<ExtractionResult> {
    if !matches!(
        language,
        Language::TypeScript | Language::JavaScript | Language::Tsx | Language::Jsx
    ) || !source.contains("export const")
        || !source.contains("create")
    {
        return None;
    }

    let file_id = format!("file:{file_path}");
    let mut nodes = vec![fallback_node(
        &file_id,
        NodeKind::File,
        Path::new(file_path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(file_path),
        file_path,
        file_path,
        *language,
        1,
        0,
        source.lines().count().max(1) as u64,
        0,
        false,
    )];
    let mut edges = Vec::new();
    let mut unresolved_references = Vec::new();
    let mut cursor = 0usize;
    let mut extracted_functions = 0usize;

    while let Some(relative_export_start) = source[cursor..].find("export const") {
        let export_start = cursor + relative_export_start;
        let Some((const_name, name_start, name_end)) =
            exported_const_name(source, export_start + "export const".len())
        else {
            cursor = export_start + "export const".len();
            continue;
        };
        let Some(equal_relative) = source[name_end..].find('=') else {
            cursor = name_end;
            continue;
        };
        let initializer_start = name_end + equal_relative + 1;
        let initializer_end = source[initializer_start..]
            .find("export const")
            .map(|next| initializer_start + next)
            .unwrap_or(source.len());
        let initializer = &source[initializer_start..initializer_end];
        if !initializer.contains("create") {
            cursor = initializer_end;
            continue;
        }
        let Some((object_start, object_end)) =
            find_returned_object_in_initializer(source, initializer_start, initializer_end)
        else {
            cursor = initializer_end;
            continue;
        };

        let (const_line, const_column) = line_col_at(source, name_start);
        let const_id = generate_node_id(
            file_path,
            NodeKind::Constant,
            &const_name,
            const_line as usize,
        );
        nodes.push(fallback_node(
            &const_id,
            NodeKind::Constant,
            &const_name,
            &const_name,
            file_path,
            *language,
            const_line,
            const_column,
            const_line,
            const_column + const_name.len() as u64,
            true,
        ));
        edges.push(edge(
            file_id.clone(),
            const_id,
            EdgeKind::Contains,
            Some(file_path.to_owned()),
        ));

        // 只拆顶层 object 成员，避免把嵌套对象/参数列表里的逗号误当成 action 分隔。
        for (member_start, member_end) in
            split_top_level_segments(source, object_start + 1, object_end)
        {
            let Some((name, value_start, value_end)) =
                object_function_property(source, member_start, member_end)
            else {
                continue;
            };
            let (line, column) = line_col_at(source, member_start);
            let function_id = generate_node_id(file_path, NodeKind::Function, &name, line as usize);
            nodes.push(fallback_node(
                &function_id,
                NodeKind::Function,
                &name,
                &name,
                file_path,
                *language,
                line,
                column,
                line,
                column + name.len() as u64,
                false,
            ));
            edges.push(edge(
                file_id.clone(),
                function_id.clone(),
                EdgeKind::Contains,
                Some(file_path.to_owned()),
            ));
            for reference_name in extract_call_reference_names(&source[value_start..value_end]) {
                unresolved_references.push(unresolved_reference(
                    function_id.clone(),
                    reference_name,
                    ReferenceKind::Calls,
                    line as usize,
                    column as usize,
                ));
            }
            extracted_functions += 1;
        }
        cursor = initializer_end;
    }

    (extracted_functions > 0)
        .then(|| extraction_result_now(nodes, edges, unresolved_references, Vec::new()))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn fallback_node(
    id: &str,
    kind: NodeKind,
    name: &str,
    qualified_name: &str,
    file_path: &str,
    language: Language,
    start_line: u64,
    start_column: u64,
    end_line: u64,
    end_column: u64,
    is_exported: bool,
) -> Node {
    // fallback 产生的节点保持和 tree-sitter 节点同形，便于后续 DB 写入与 resolver
    // 完全复用；updated_at 由外层索引流程统一处理。
    Node {
        id: id.to_owned(),
        kind,
        name: name.to_owned(),
        qualified_name: qualified_name.to_owned(),
        file_path: file_path.to_owned(),
        language,
        start_line,
        end_line,
        start_column,
        end_column,
        docstring: None,
        signature: None,
        visibility: None,
        is_exported: Some(is_exported),
        is_async: Some(false),
        is_static: Some(false),
        is_abstract: Some(false),
        decorators: None,
        type_parameters: None,
        return_type: None,
        updated_at: 0,
    }
}
