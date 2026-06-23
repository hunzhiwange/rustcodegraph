//! facade 数据库索引管线。
//!
//! 这里把文件发现、原生抽取、fallback 补洞、边解析和 SQLite 持久化串成一次事务。增量索引复用同一管线，
//! 只是在写入前清理选中文件关联的旧节点、边和 unresolved refs。

use super::*;

pub(super) fn index_facade_database(project_root: &Path, started: Instant) -> IndexResult {
    index_facade_database_inner(project_root, started, None)
}

pub(super) fn index_facade_changed_files(
    project_root: &Path,
    started: Instant,
    changes: &ChangedFiles,
) -> IndexResult {
    let mut selected_paths = HashSet::new();
    selected_paths.extend(changes.added.iter().cloned());
    selected_paths.extend(changes.modified.iter().cloned());
    selected_paths.extend(changes.removed.iter().cloned());
    index_facade_database_inner(project_root, started, Some(selected_paths))
}

pub(super) fn index_facade_database_inner(
    project_root: &Path,
    started: Instant,
    selected_paths: Option<HashSet<String>>,
) -> IndexResult {
    let mut conn = match ensure_facade_database(project_root) {
        Ok(conn) => conn,
        Err(err) => return index_failure(err, started),
    };
    let full_rebuild = selected_paths.is_none();
    let mut files = existing_source_files(project_root);
    if let Some(selected_paths) = selected_paths.as_ref() {
        files.retain(|path| selected_paths.contains(path));
    }
    let indexed_at = now_ms();
    let mut files_indexed = 0usize;
    let mut files_skipped = 0usize;
    let mut file_records = Vec::new();
    let mut nodes = Vec::new();
    let mut sources = Vec::new();
    let mut pending_edges = Vec::new();
    let mut direct_edges = Vec::new();
    let mut unresolved_refs = Vec::new();
    let framework_resolvers = get_all_framework_resolvers();

    for file_path in files {
        let abs = project_root.join(&file_path);
        let source = match fs::read_to_string(&abs) {
            Ok(source) => source,
            Err(_) => {
                files_skipped += 1;
                continue;
            }
        };
        let metadata = fs::metadata(&abs).ok();
        let language = detect_language(&file_path, Some(&source));
        let mut file_errors = Vec::new();
        let (mut file_nodes, mut file_pending_edges, mut file_edges, mut file_unresolved_refs) =
            if should_index_with_native_parser(language, &source) {
                // 小文件优先走原生 tree-sitter，之后再用 facade fallback 针对语言缺口补齐节点和边。
                let mut result = extract_source_now(&file_path, &source, Some(language), None);
                let mut pending_edges = result
                    .unresolved_references
                    .iter()
                    .filter_map(pending_edge_from_unresolved_reference)
                    .collect::<Vec<_>>();
                if matches!(language, Language::C | Language::Cpp) {
                    pending_edges.retain(|edge| !facade_pending_is_fn_ref(edge));
                }
                for reference in &mut result.unresolved_references {
                    reference.file_path.get_or_insert_with(|| file_path.clone());
                    reference.language.get_or_insert(language);
                }
                let mut nodes = result.nodes;
                apply_native_source_namespace(&file_path, &source, language, &mut nodes);
                apply_native_ts_js_class_field_kinds(&file_path, &source, language, &mut nodes);
                let mut edges = result.edges;
                let file_node_id = format!("file:{file_path}");
                if !nodes.iter().any(|node| node.id == file_node_id) {
                    nodes.insert(
                        0,
                        facade_file_node(&file_path, &source, language, indexed_at),
                    );
                }
                if language == Language::CSharp
                    && !nodes.iter().any(|node| {
                        node.file_path == file_path && facade_supertype_bearing(node.kind)
                    })
                {
                    append_facade_csharp_inline_namespace_types(
                        &file_path,
                        &source,
                        indexed_at,
                        &file_node_id,
                        &mut nodes,
                        &mut edges,
                    );
                }
                let needs_csharp_fallback = language == Language::CSharp
                    && !nodes.iter().any(|node| {
                        node.file_path == file_path && facade_supertype_bearing(node.kind)
                    });
                let needs_objc_fallback = language == Language::ObjC
                    && !nodes.iter().any(|node| {
                        node.file_path == file_path && facade_supertype_bearing(node.kind)
                    });
                let has_cpp_pointer_class = (source.contains("class ")
                    || source.contains("struct "))
                    && source.contains('*');
                let needs_cpp_fallback = matches!(language, Language::C | Language::Cpp)
                    && (has_cpp_pointer_class
                        || (source.contains("::")
                            && !nodes.iter().any(|node| {
                                node.file_path == file_path
                                    && matches!(node.kind, NodeKind::Method | NodeKind::Function)
                                    && node.qualified_name.contains("::")
                            })));
                let needs_value_ref_fallback = needs_facade_value_ref_fallback(language, &source);
                if language == Language::Java
                    || language == Language::Php
                    || needs_csharp_fallback
                    || needs_objc_fallback
                    || needs_cpp_fallback
                    || needs_value_ref_fallback
                    || (language == Language::Go && go_facade_fallback_file(&file_path, &source))
                {
                    // fallback 节点放在前面，后续去重会保留首个 id；这让补洞结果能覆盖原生抽取漏掉的结构。
                    let (mut fallback_nodes, mut fallback_pending_edges, mut fallback_edges) =
                        extract_facade_symbols_rich(&file_path, &source, language, indexed_at, &[]);
                    fallback_nodes.append(&mut nodes);
                    nodes = fallback_nodes;
                    pending_edges.append(&mut fallback_pending_edges);
                    edges.append(&mut fallback_edges);
                }
                let framework_pending_start = pending_edges.len();
                append_facade_framework_extraction(
                    &file_path,
                    &source,
                    language,
                    &framework_resolvers,
                    &file_node_id,
                    &mut nodes,
                    &mut pending_edges,
                    &mut edges,
                );
                result.unresolved_references.extend(
                    // framework resolver 产出的引用也进入 unresolved_refs，复用统一 resolver 的命名解析规则。
                    pending_edges[framework_pending_start..]
                        .iter()
                        .map(|edge| facade_unresolved_reference(edge, &file_path, language)),
                );
                file_errors = result.errors;
                (nodes, pending_edges, edges, result.unresolved_references)
            } else {
                let (mut nodes, pending_edges, edges) = extract_facade_symbols_rich(
                    &file_path,
                    &source,
                    language,
                    indexed_at,
                    &framework_resolvers,
                );
                nodes.insert(
                    0,
                    facade_file_node(&file_path, &source, language, indexed_at),
                );
                let unresolved_refs = pending_edges
                    .iter()
                    .map(|edge| facade_unresolved_reference(edge, &file_path, language))
                    .collect::<Vec<_>>();
                (nodes, pending_edges, edges, unresolved_refs)
            };
        if language == Language::CSharp {
            append_facade_csharp_inline_namespace_types(
                &file_path,
                &source,
                indexed_at,
                &format!("file:{file_path}"),
                &mut file_nodes,
                &mut file_edges,
            );
        }
        append_facade_function_ref_edges(
            &file_path,
            &source,
            language,
            &format!("file:{file_path}"),
            &file_nodes,
            &mut file_pending_edges,
        );
        append_facade_react_native_member_call_edges(
            &file_path,
            &source,
            language,
            &file_nodes,
            &mut file_pending_edges,
        );
        append_facade_class_member_reference_edges(
            &file_path,
            &source,
            language,
            indexed_at,
            &file_nodes,
            &mut file_pending_edges,
        );
        append_facade_relation_edges(&source, language, &file_nodes, &mut file_pending_edges);
        let node_count = file_nodes.len();
        sources.push((file_path.clone(), source.clone()));
        file_records.push(FileRecord {
            path: file_path.clone(),
            content_hash: hash_content(&source),
            language,
            size: metadata
                .as_ref()
                .map(|metadata| metadata.len())
                .unwrap_or(0) as ByteSize,
            modified_at: metadata
                .as_ref()
                .and_then(|metadata| metadata.modified().ok())
                .map(system_time_ms)
                .unwrap_or(0),
            indexed_at,
            node_count: node_count as u64,
            errors: (!file_errors.is_empty()).then_some(file_errors.clone()),
        });
        nodes.append(&mut file_nodes);
        pending_edges.append(&mut file_pending_edges);
        direct_edges.append(&mut file_edges);
        file_unresolved_refs
            .retain(|reference| reference.reference_kind != ReferenceKind::FunctionRef);
        unresolved_refs.append(&mut file_unresolved_refs);
        files_indexed += 1;
    }

    dedupe_facade_nodes(&mut nodes);
    let mut edges = resolve_facade_edges_rich(&nodes, pending_edges, &direct_edges);
    edges.append(&mut direct_edges);
    // 这些补充边都依赖全项目节点集合，必须等所有文件抽取完成后再统一运行。
    edges.extend(resolve_facade_property_type_edges(&nodes));
    edges.extend(resolve_facade_value_ref_edges(&sources, &nodes));
    filter_facade_shadowed_value_ref_edges(&sources, &nodes, &mut edges);
    edges.extend(resolve_facade_python_include_edges(&sources, &nodes));
    edges.extend(resolve_facade_test_file_edges(&sources, &nodes));
    dedupe_facade_edges(&mut edges);
    dedupe_facade_edges_by_node_names(&mut edges, &nodes);

    let tx = match conn.transaction() {
        Ok(tx) => tx,
        Err(err) => {
            return index_failure(format!("failed to start index transaction: {err}"), started);
        }
    };
    if full_rebuild {
        if let Err(err) = tx.execute_batch(
            r#"
            DELETE FROM unresolved_refs;
            DELETE FROM edges;
            DELETE FROM nodes;
            DELETE FROM files;
            "#,
        ) {
            return index_failure(format!("failed to clear existing index: {err}"), started);
        }
    } else if let Some(selected_paths) = selected_paths.as_ref() {
        // 增量模式清理与文件相连的边，避免被删除或重命名的节点继续影响图遍历结果。
        for path in selected_paths {
            if let Err(err) = tx.execute(
                "DELETE FROM unresolved_refs WHERE file_path = ?",
                params![path],
            ) {
                return index_failure(
                    format!("failed to clear unresolved refs for {path}: {err}"),
                    started,
                );
            }
            if let Err(err) = tx.execute(
                r#"
                DELETE FROM edges
                WHERE source IN (SELECT id FROM nodes WHERE file_path = ?)
                   OR target IN (SELECT id FROM nodes WHERE file_path = ?)
                "#,
                params![path, path],
            ) {
                return index_failure(format!("failed to clear edges for {path}: {err}"), started);
            }
            if let Err(err) = tx.execute("DELETE FROM nodes WHERE file_path = ?", params![path]) {
                return index_failure(format!("failed to clear nodes for {path}: {err}"), started);
            }
            if let Err(err) = tx.execute("DELETE FROM files WHERE path = ?", params![path]) {
                return index_failure(
                    format!("failed to clear file row for {path}: {err}"),
                    started,
                );
            }
        }
    }

    {
        let mut stmt = match tx.prepare(
            r#"
            INSERT INTO files
                (path, content_hash, language, size, modified_at, indexed_at, node_count, errors)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        ) {
            Ok(stmt) => stmt,
            Err(err) => {
                return index_failure(format!("failed to prepare file insert: {err}"), started);
            }
        };

        for file in &file_records {
            if let Err(err) = stmt.execute(params![
                file.path,
                file.content_hash,
                language_key(&file.language),
                file.size,
                file.modified_at,
                file.indexed_at,
                file.node_count,
                json_string_option(file.errors.as_ref()),
            ]) {
                return index_failure(format!("failed to insert file row: {err}"), started);
            }
        }
    }

    {
        let mut stmt = match tx.prepare(
            r#"
            INSERT INTO nodes (
                id, kind, name, qualified_name, file_path, language,
                start_line, end_line, start_column, end_column,
                docstring, signature, visibility,
                is_exported, is_async, is_static, is_abstract,
                decorators, type_parameters, return_type, updated_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        ) {
            Ok(stmt) => stmt,
            Err(err) => {
                return index_failure(format!("failed to prepare node insert: {err}"), started);
            }
        };

        for node in &nodes {
            if let Err(err) = stmt.execute(params![
                node.id,
                kind_key(node.kind),
                node.name,
                node.qualified_name,
                node.file_path,
                language_key(&node.language),
                node.start_line,
                node.end_line,
                node.start_column,
                node.end_column,
                node.docstring,
                node.signature,
                node.visibility.map(visibility_key),
                bool_int(node.is_exported),
                bool_int(node.is_async),
                bool_int(node.is_static),
                bool_int(node.is_abstract),
                json_string_option(node.decorators.as_ref()),
                json_string_option(node.type_parameters.as_ref()),
                node.return_type,
                node.updated_at,
            ]) {
                return index_failure(format!("failed to insert node {}: {err}", node.id), started);
            }
        }
    }

    {
        let mut stmt = match tx.prepare(
            r#"
            INSERT INTO edges (source, target, kind, metadata, line, col, provenance)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        ) {
            Ok(stmt) => stmt,
            Err(err) => {
                return index_failure(format!("failed to prepare edge insert: {err}"), started);
            }
        };

        for edge in &edges {
            if let Err(err) = stmt.execute(params![
                edge.source,
                edge.target,
                edge_kind_key(edge.kind),
                json_string_option(edge.metadata.as_ref()),
                edge.line,
                edge.column,
                edge.provenance.map(edge_provenance_key),
            ]) {
                return index_failure(
                    format!(
                        "failed to insert edge {} -> {}: {err}",
                        edge.source, edge.target
                    ),
                    started,
                );
            }
        }
    }

    {
        let mut stmt = match tx.prepare(
            r#"
            INSERT INTO unresolved_refs
                (from_node_id, reference_name, reference_kind, line, col, candidates, file_path, language)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        ) {
            Ok(stmt) => stmt,
            Err(err) => {
                return index_failure(
                    format!("failed to prepare unresolved ref insert: {err}"),
                    started,
                );
            }
        };

        for reference in &unresolved_refs {
            if let Err(err) = stmt.execute(params![
                reference.from_node_id,
                reference.reference_name,
                reference_kind_key(reference.reference_kind),
                reference.line,
                reference.column,
                json_string_option(reference.candidates.as_ref()),
                reference.file_path.as_deref().unwrap_or_default(),
                reference
                    .language
                    .as_ref()
                    .map(language_key)
                    .unwrap_or_else(|| "unknown".to_owned()),
            ]) {
                return index_failure(
                    format!(
                        "failed to insert unresolved reference {} -> {}: {err}",
                        reference.from_node_id, reference.reference_name
                    ),
                    started,
                );
            }
        }
    }

    if let Err(err) = tx.commit() {
        return index_failure(
            format!("failed to commit index transaction: {err}"),
            started,
        );
    }

    if files_indexed > 0 {
        let conn = match open_facade_database(project_root) {
            Ok(conn) => conn,
            Err(err) => return index_failure(err, started),
        };
        if let Err(err) = conn.execute(
            "INSERT INTO project_metadata (key, value, updated_at) VALUES (?, ?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params!["indexed_with_version", env!("CARGO_PKG_VERSION"), now_ms()],
        ) {
            return index_failure(format!("failed to stamp package version: {err}"), started);
        }
        if let Err(err) = conn.execute(
            "INSERT INTO project_metadata (key, value, updated_at) VALUES (?, ?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![
                "indexed_with_extraction_version",
                EXTRACTION_VERSION.to_string(),
                now_ms()
            ],
        ) {
            return index_failure(format!("failed to stamp extraction version: {err}"), started);
        }
    }

    let post_resolution_edges = if files_indexed > 0 {
        // resolver 和动态边合成在事务提交后运行，因为它们需要查询刚写入的完整索引。
        resolve_facade_reference_queue(project_root)
    } else {
        0
    };
    IndexResult {
        success: true,
        files_indexed,
        files_skipped,
        files_errored: 0,
        nodes_created: nodes.len(),
        edges_created: edges.len() + post_resolution_edges,
        errors: Vec::new(),
        duration_ms: started.elapsed().as_millis() as u64,
    }
}
