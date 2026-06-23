//! 高置信度值引用补边。
//!
//! 这不是完整 def-use 分析，只追踪“足够有区分度”的常量/变量名，并用 shadowing 过滤降低误连风险。

use super::*;

pub(super) fn resolve_facade_value_ref_edges(
    sources: &[(String, String)],
    nodes: &[Node],
) -> Vec<Edge> {
    if std::env::var("RUSTCODEGRAPH_VALUE_REFS").ok().as_deref() == Some("0") {
        // 该启发式可通过环境变量关闭，便于评估误连或控制图规模。
        return Vec::new();
    }

    let mut nodes_by_file: HashMap<&str, Vec<&Node>> = HashMap::new();
    for node in nodes {
        nodes_by_file
            .entry(node.file_path.as_str())
            .or_default()
            .push(node);
    }

    let mut edges = Vec::new();
    let mut seen_edges = HashSet::new();
    for (file_path, source) in sources {
        let Some(file_nodes) = nodes_by_file.get(file_path.as_str()) else {
            continue;
        };
        let lines = source.lines().collect::<Vec<_>>();
        let depths = facade_line_brace_depths(source);

        let mut targets_by_name: HashMap<&str, &Node> = HashMap::new();
        let mut target_locations: HashMap<&str, HashSet<String>> = HashMap::new();
        for node in file_nodes.iter().copied() {
            // 只有文件/类型级、名称有辨识度的值才作为目标，局部变量默认不纳入全文件引用图。
            if !matches!(node.kind, NodeKind::Constant | NodeKind::Variable)
                || !distinctive_value_name(&node.name)
                || !facade_value_target_is_scope(node, &lines, &depths)
            {
                continue;
            }
            targets_by_name.insert(node.name.as_str(), node);
            target_locations
                .entry(node.name.as_str())
                .or_default()
                .insert(facade_value_target_count_key(node));
        }
        if targets_by_name.is_empty() {
            continue;
        }

        let shadowed_target_names = file_nodes
            .iter()
            .copied()
            .filter(|node| {
                matches!(node.kind, NodeKind::Constant | NodeKind::Variable)
                    && distinctive_value_name(&node.name)
                    && !facade_value_target_is_scope(node, &lines, &depths)
            })
            .map(|node| node.name.clone())
            .collect::<Vec<_>>();
        // 只要同名局部声明存在，就移除该目标，避免把局部读取错误连到外层常量。
        for name in shadowed_target_names {
            targets_by_name.remove(name.as_str());
            target_locations.remove(name.as_str());
        }
        if targets_by_name.is_empty() {
            continue;
        }

        let mut declaration_counts: HashMap<String, usize> = HashMap::new();
        for line in &lines {
            for name in facade_declared_value_names(line) {
                if targets_by_name.contains_key(name.as_str()) {
                    *declaration_counts.entry(name).or_default() += 1;
                }
            }
        }
        for (name, declaration_count) in declaration_counts {
            let target_count = target_locations
                .get(name.as_str())
                .map(HashSet::len)
                .unwrap_or(1);
            if declaration_count > target_count {
                targets_by_name.remove(name.as_str());
            }
        }
        if targets_by_name.is_empty() {
            continue;
        }

        let mut readers = file_nodes
            .iter()
            .copied()
            .filter(|node| {
                matches!(
                    node.kind,
                    NodeKind::Function | NodeKind::Method | NodeKind::Constant | NodeKind::Variable
                )
            })
            .collect::<Vec<_>>();
        readers.sort_by(|a, b| {
            a.start_line
                .cmp(&b.start_line)
                .then_with(|| a.end_line.cmp(&b.end_line))
                .then_with(|| a.name.cmp(&b.name))
        });

        for reader in &readers {
            let body = facade_reader_source(reader, &readers, &lines);
            if body.is_empty() {
                continue;
            }
            let mut seen_targets = HashSet::new();
            for token in identifier_tokens(&body) {
                let Some(target) = targets_by_name.get(token.as_str()).copied() else {
                    continue;
                };
                if target.id == reader.id
                    || target.name == reader.name
                    || !seen_targets.insert(target.id.as_str())
                {
                    continue;
                }
                let key = format!("{}:{}:value", reader.id, target.id);
                if !seen_edges.insert(key) {
                    continue;
                }
                edges.push(Edge {
                    source: reader.id.clone(),
                    target: target.id.clone(),
                    kind: EdgeKind::References,
                    metadata: Some(HashMap::from([("valueRef".to_owned(), json!(true))])),
                    line: Some(reader.start_line),
                    column: Some(reader.start_column),
                    provenance: None,
                });
            }
        }
    }

    edges
}

pub(super) fn filter_facade_shadowed_value_ref_edges(
    sources: &[(String, String)],
    nodes: &[Node],
    edges: &mut Vec<Edge>,
) {
    // 在所有补边合并后再过滤一遍，覆盖其他解析路径生成的 valueRef 边。
    let nodes_by_id = nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect::<HashMap<_, _>>();
    let mut nodes_by_file: HashMap<&str, Vec<&Node>> = HashMap::new();
    for node in nodes {
        nodes_by_file
            .entry(node.file_path.as_str())
            .or_default()
            .push(node);
    }
    let shadowed_by_file = sources
        .iter()
        .filter_map(|(file_path, source)| {
            let file_nodes = nodes_by_file.get(file_path.as_str())?;
            let shadowed = facade_shadowed_value_names(file_nodes, source);
            (!shadowed.is_empty()).then_some((file_path.as_str(), shadowed))
        })
        .collect::<HashMap<_, _>>();

    edges.retain(|edge| {
        if !facade_edge_is_value_ref(edge) {
            return true;
        }
        let Some(target) = nodes_by_id.get(edge.target.as_str()).copied() else {
            return true;
        };
        !shadowed_by_file
            .get(target.file_path.as_str())
            .is_some_and(|names| names.contains(&target.name))
    });
}

pub(super) fn facade_shadowed_value_names(file_nodes: &[&Node], source: &str) -> HashSet<String> {
    let lines = source.lines().collect::<Vec<_>>();
    let depths = facade_line_brace_depths(source);
    let mut target_locations: HashMap<&str, HashSet<String>> = HashMap::new();
    let mut shadowed = HashSet::new();

    for node in file_nodes.iter().copied() {
        if !matches!(node.kind, NodeKind::Constant | NodeKind::Variable)
            || !distinctive_value_name(&node.name)
        {
            continue;
        }
        if facade_value_target_is_scope(node, &lines, &depths) {
            target_locations
                .entry(node.name.as_str())
                .or_default()
                .insert(facade_value_target_count_key(node));
        } else {
            shadowed.insert(node.name.clone());
        }
    }

    let mut declaration_counts: HashMap<String, usize> = HashMap::new();
    for line in &lines {
        for name in facade_declared_value_names(line) {
            if target_locations.contains_key(name.as_str()) {
                *declaration_counts.entry(name).or_default() += 1;
            }
        }
    }
    for (name, declaration_count) in declaration_counts {
        let target_count = target_locations
            .get(name.as_str())
            .map(HashSet::len)
            .unwrap_or(1);
        if declaration_count > target_count {
            shadowed.insert(name);
        }
    }

    shadowed
}

pub(super) fn facade_value_target_count_key(node: &Node) -> String {
    format!("{}:{}:{}", node.file_path, node.start_line, node.name)
}

pub(super) fn facade_edge_is_value_ref(edge: &Edge) -> bool {
    edge.kind == EdgeKind::References
        && edge
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("valueRef"))
            .and_then(Value::as_bool)
            == Some(true)
}

pub(super) fn resolve_facade_python_include_edges(
    sources: &[(String, String)],
    nodes: &[Node],
) -> Vec<Edge> {
    // Django/Flask URL include 字符串是文件依赖而不是普通 symbol 引用，直接连 file imports 边。
    let file_nodes = nodes
        .iter()
        .filter(|node| node.kind == NodeKind::File)
        .map(|node| (node.file_path.as_str(), node))
        .collect::<HashMap<_, _>>();
    let mut edges = Vec::new();
    let mut seen = HashSet::new();
    for (file_path, source) in sources {
        if !file_path.ends_with(".py") || !source.contains("include(") {
            continue;
        }
        let Some(source_file) = file_nodes.get(file_path.as_str()).copied() else {
            continue;
        };
        for (line, module_name) in facade_python_include_modules(source) {
            let module_file = format!("{}.py", module_name.replace('.', "/"));
            let package_file = format!("{}/__init__.py", module_name.replace('.', "/"));
            let Some(target_file) = file_nodes
                .get(module_file.as_str())
                .or_else(|| file_nodes.get(package_file.as_str()))
                .copied()
            else {
                continue;
            };
            if target_file.id == source_file.id {
                continue;
            }
            let key = format!(
                "{}|{}|{:?}",
                source_file.id,
                target_file.id,
                EdgeKind::Imports
            );
            if !seen.insert(key) {
                continue;
            }
            edges.push(Edge {
                source: source_file.id.clone(),
                target: target_file.id.clone(),
                kind: EdgeKind::Imports,
                metadata: None,
                line: Some(line),
                column: Some(0),
                provenance: None,
            });
        }
    }
    edges
}

pub(super) fn facade_python_include_modules(source: &str) -> Vec<(u64, String)> {
    let mut out = Vec::new();
    for (idx, line) in source.lines().enumerate() {
        let line_number = (idx + 1) as u64;
        for quote in ['"', '\''] {
            let needle = format!("include({quote}");
            let mut rest = line;
            while let Some(start) = rest.find(&needle) {
                let after = &rest[start + needle.len()..];
                let Some(end) = after.find(quote) else {
                    break;
                };
                let module_name = after[..end].trim();
                if module_name.ends_with(".urls")
                    && module_name
                        .chars()
                        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.'))
                {
                    out.push((line_number, module_name.to_owned()));
                }
                rest = &after[end + quote.len_utf8()..];
            }
        }
    }
    out
}

pub(super) fn facade_value_target_is_scope(node: &Node, lines: &[&str], depths: &[isize]) -> bool {
    let idx = node.start_line.saturating_sub(1) as usize;
    if node.language == Language::Pascal {
        // Pascal implementation 段里的 const/var 更接近局部实现细节，不作为跨作用域引用目标。
        return !lines.iter().take(idx).any(|line| {
            let lower = line.trim().to_ascii_lowercase();
            lower == "implementation"
                || lower.starts_with("function ")
                || lower.starts_with("procedure ")
        });
    }
    let depth = depths.get(idx).copied().unwrap_or_default();
    let raw_line = lines.get(idx).copied().unwrap_or_default();
    if facade_value_decl_after_inline_block(raw_line, &node.name) {
        return false;
    }
    if depth <= 0 {
        return true;
    }
    // 对 class body 语言，类型字段/常量虽然 brace depth > 0，仍应作为稳定引用目标。
    let line = raw_line.trim();
    matches!(
        node.language,
        Language::Java
            | Language::CSharp
            | Language::Php
            | Language::Ruby
            | Language::Scala
            | Language::Kotlin
            | Language::Swift
            | Language::Dart
    ) && !line.starts_with("var ")
        && !line.starts_with("let ")
}

pub(super) fn facade_value_decl_after_inline_block(line: &str, name: &str) -> bool {
    let Some(name_pos) = line.find(name) else {
        return false;
    };
    let before_name = &line[..name_pos];
    let Some(open_brace) = before_name.rfind('{') else {
        return false;
    };
    before_name[..open_brace].contains('(')
}

pub(super) fn facade_reader_source(reader: &Node, readers: &[&Node], lines: &[&str]) -> String {
    if lines.is_empty() || reader.start_line == 0 {
        return String::new();
    }
    let next_reader_line = readers
        .iter()
        .filter(|candidate| candidate.file_path == reader.file_path)
        .filter(|candidate| candidate.start_line > reader.start_line)
        .map(|candidate| candidate.start_line)
        .min();
    let mut end_line = if reader.end_line > reader.start_line {
        reader.end_line
    } else {
        next_reader_line
            .and_then(|line| line.checked_sub(1))
            .unwrap_or(reader.start_line)
    };
    end_line = end_line.min(lines.len() as u64).max(reader.start_line);
    let start = reader.start_line.saturating_sub(1) as usize;
    let end = end_line as usize;
    lines[start..end].join("\n")
}

pub(super) fn facade_line_brace_depths(source: &str) -> Vec<isize> {
    let mut depth = 0isize;
    source
        .lines()
        .map(|line| {
            let before = depth;
            depth += brace_delta(line);
            before
        })
        .collect()
}

pub(super) fn facade_declared_value_names(line: &str) -> Vec<String> {
    let mut names = Vec::new();
    for segment in line.split(['{', '}', ';']) {
        if !(segment.contains('=') || segment.contains(":=")) {
            continue;
        }
        if let Some(name) = value_decl_name_before_eq(segment) {
            names.push(name.to_owned());
        }
        if let Some(colon_eq) = segment.find(":=")
            && let Some(name) = value_decl_name_before_eq(&segment[..colon_eq])
        {
            names.push(name.to_owned());
        }
    }
    names.sort();
    names.dedup();
    names
}
