use super::*;

/// Extract nodes and edges from source code.
pub fn extract_from_source(
    file_path: &str,
    source: &str,
    language: Option<Language>,
    framework_names: Option<&[String]>,
) -> ExtractionResult {
    // 特殊混合文件优先走专用抽取器；普通代码文件才进入通用 tree-sitter。
    // 这个顺序保证 SFC/模板文件能保留 component 语义和嵌入脚本行号。
    let detected_language = language.unwrap_or_else(|| detect_language(file_path, Some(source)));
    let file_extension = Path::new(file_path)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| format!(".{}", ext.to_ascii_lowercase()))
        .unwrap_or_default();

    let result = if matches!(detected_language, Language::Svelte) {
        SvelteExtractor::new(file_path, source).extract()
    } else if matches!(detected_language, Language::Vue) {
        VueExtractor::new(file_path, source).extract()
    } else if matches!(detected_language, Language::Astro) {
        AstroExtractor::new(file_path, source).extract()
    } else if matches!(detected_language, Language::Liquid) {
        LiquidExtractor::new(file_path, source).extract()
    } else if matches!(detected_language, Language::Razor) {
        RazorExtractor::new(file_path, source).extract()
    } else if matches!(detected_language, Language::Xml) {
        MyBatisExtractor::new(file_path, source).extract()
    } else if is_file_level_only_language(detected_language) {
        extraction_result_now(Vec::new(), Vec::new(), Vec::new(), Vec::new())
    } else if matches!(language_key(&detected_language).as_str(), "pascal")
        && matches!(file_extension.as_str(), ".dfm" | ".fmx")
    {
        DfmExtractor::new(file_path, source).extract()
    } else if let Some(fallback) =
        try_ts_js_store_object_fallback(file_path, source, &detected_language)
    {
        fallback
    } else {
        TreeSitterExtractor::new(file_path, source, Some(detected_language)).extract()
    };

    if framework_names.is_some_and(|names| !names.is_empty()) {
        // Framework-specific extraction is intentionally represented as a
        // merge point only. Resolvers/templates are Task 06/08.
    }

    result
}

pub(super) fn fn_ref_spec_for_language(language: &Language) -> Option<FnRefSpec> {
    FN_REF_SPECS.get(language_key(language).as_str()).cloned()
}

pub(super) fn language_extractor_for(
    language: &Language,
) -> Option<&'static dyn LanguageExtractor> {
    extractor_for(language_key(language).as_str())
}

pub(super) fn extraction_result(
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    unresolved_references: Vec<UnresolvedReference>,
    errors: Vec<ExtractionError>,
    start: Instant,
) -> ExtractionResult {
    ExtractionResult {
        nodes,
        edges,
        unresolved_references,
        errors,
        duration_ms: start.elapsed().as_millis() as u64,
    }
}

pub(super) fn java_decl_names_from_text(text: &str) -> Vec<String> {
    let mut names = Vec::new();
    for segment in text.split(['{', '}', ';']) {
        if !segment.contains('=') {
            continue;
        }
        let before_eq = segment.split('=').next().unwrap_or_default();
        let mut tokens = before_eq
            .split(|ch: char| !(ch == '_' || ch == '$' || ch.is_ascii_alphanumeric()))
            .filter(|token| !token.is_empty())
            .collect::<Vec<_>>();
        while matches!(
            tokens.last().copied(),
            Some(
                "int"
                    | "long"
                    | "short"
                    | "byte"
                    | "char"
                    | "float"
                    | "double"
                    | "boolean"
                    | "final"
                    | "static"
                    | "return"
            )
        ) {
            tokens.pop();
        }
        if let Some(name) = tokens.last().copied() {
            names.push(name.trim_start_matches('$').to_owned());
        }
    }
    names.sort();
    names.dedup();
    names
}

pub(super) fn value_ref_decl_is_target_scope(
    source: &str,
    language: Language,
    name: &str,
    node: &SyntaxNode,
    parent_id: &str,
) -> bool {
    // value-ref 只接受文件/类型级声明；内联 block、Pascal implementation 区和
    // 已进入花括号的 file scope 都视为局部，避免被全文件引用误连。
    let row = node.start_position.row;
    let line = source.lines().nth(row).unwrap_or_default();
    if value_ref_decl_after_inline_block(line, name) {
        return false;
    }
    if parent_id.starts_with("file:") && value_ref_brace_depth_before_line(source, row) > 0 {
        return false;
    }
    if language == Language::Pascal {
        return !source.lines().take(row).any(|line| {
            let lower = line.trim().to_ascii_lowercase();
            lower == "implementation"
                || lower.starts_with("function ")
                || lower.starts_with("procedure ")
        });
    }
    true
}

pub(super) fn value_ref_decl_after_inline_block(line: &str, name: &str) -> bool {
    let Some(name_pos) = line.find(name) else {
        return false;
    };
    let before_name = &line[..name_pos];
    let Some(open_brace) = before_name.rfind('{') else {
        return false;
    };
    before_name[..open_brace].contains('(')
}

pub(super) fn value_ref_brace_depth_before_line(source: &str, row: usize) -> isize {
    source
        .lines()
        .take(row)
        .map(|line| {
            line.chars().filter(|ch| *ch == '{').count() as isize
                - line.chars().filter(|ch| *ch == '}').count() as isize
        })
        .sum()
}

pub(super) fn count_value_declarations_in_source(source: &str, target_name: &str) -> usize {
    source
        .split(['\n', ';', '{', '}'])
        .filter(|segment| segment.contains('=') || segment.contains(":="))
        .filter(|segment| {
            let before_assignment = segment
                .split('=')
                .next()
                .unwrap_or_default()
                .split(":=")
                .next()
                .unwrap_or_default();
            before_assignment
                .split(|ch: char| !(ch == '_' || ch == '$' || ch.is_ascii_alphanumeric()))
                .any(|token| token == target_name)
        })
        .count()
}

pub(super) fn extraction_result_now(
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    unresolved_references: Vec<UnresolvedReference>,
    errors: Vec<ExtractionError>,
) -> ExtractionResult {
    ExtractionResult {
        nodes,
        edges,
        unresolved_references,
        errors,
        duration_ms: 0,
    }
}

pub(super) fn extraction_error(
    message: String,
    file_path: Option<String>,
    severity: ExtractionSeverity,
    code: Option<&str>,
) -> ExtractionError {
    ExtractionError {
        message,
        file_path,
        line: None,
        column: None,
        severity,
        code: code.map(str::to_owned),
    }
}

pub(super) fn unresolved_reference(
    from_node_id: String,
    reference_name: String,
    reference_kind: ReferenceKind,
    line: usize,
    column: usize,
) -> UnresolvedReference {
    UnresolvedReference {
        from_node_id,
        reference_name,
        reference_kind,
        line: line as u64,
        column: column as u64,
        file_path: None,
        language: None,
        candidates: None,
    }
}

pub(super) fn edge(
    from_node_id: String,
    to_node_id: String,
    kind: EdgeKind,
    _file_path: Option<String>,
) -> Edge {
    Edge {
        source: from_node_id,
        target: to_node_id,
        kind,
        metadata: None,
        line: None,
        column: None,
        provenance: Some(EdgeProvenance::TreeSitter),
    }
}

pub(super) fn node_from_parts(
    id: String,
    kind: NodeKind,
    name: String,
    qualified_name: String,
    file_path: String,
    language: Language,
    syntax_node: &SyntaxNode,
) -> Option<Node> {
    Some(Node {
        id,
        kind,
        name,
        qualified_name,
        file_path,
        language,
        start_line: (syntax_node.start_position.row + 1) as u64,
        end_line: (syntax_node.end_position.row + 1) as u64,
        start_column: syntax_node.start_position.column as u64,
        end_column: syntax_node.end_position.column as u64,
        docstring: None,
        signature: None,
        visibility: None,
        is_exported: Some(false),
        is_async: Some(false),
        is_static: Some(false),
        is_abstract: Some(false),
        decorators: None,
        type_parameters: None,
        return_type: None,
        updated_at: 0,
    })
}

pub(super) fn node_id(node: &Node) -> String {
    node.id.clone()
}

pub(super) fn node_name(node: &Node) -> String {
    node.name.clone()
}

#[allow(dead_code)]
pub(super) fn node_kind(node: &Node) -> NodeKind {
    node.kind
}
