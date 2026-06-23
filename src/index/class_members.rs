use super::*;

// facade 的轻量解析按行识别类成员，用于 tree-sitter 结果不足或富抽取补边时。
// 这里宁可保守漏掉复杂语法，也避免把表达式调用误建成方法节点。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FacadeContainerKind {
    Class,
    Object,
    Struct,
    Enum,
    Trait,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn class_member_from_line(
    file_path: &str,
    language: Language,
    class_name: Option<&str>,
    container_kind: Option<FacadeContainerKind>,
    line: &str,
    line_number: u64,
    indexed_at: TimestampMs,
    package_prefix: Option<&str>,
) -> Option<(Node, Vec<RichFacadePendingEdge>)> {
    // 一行内同时判断 method/property/constant，并收集待解析引用；真正 target
    // 等所有节点建完后再统一 resolve，避免顺序依赖。
    let mut rest = line.trim().trim_end_matches(';').trim();
    if rest.is_empty() || rest.starts_with('}') {
        return None;
    }

    rest = strip_leading_annotations(rest);
    let (visibility, after_visibility) = strip_visibility(rest);
    rest = after_visibility;
    let (is_static, after_static) = strip_static(rest);
    rest = after_static;

    let (name, kind, type_annotation, body) = if let Some((name, kind, body)) =
        class_value_from_line(rest, language, container_kind, is_static)
    {
        (name, kind, None, body)
    } else if let Some(getter_rest) = rest.strip_prefix("get ") {
        let name = method_name_from_signature(getter_rest)?;
        (name, NodeKind::Method, None, rest)
    } else if looks_like_method_declaration(rest) {
        if matches!(language, Language::C | Language::Cpp) && !rest.contains('{') {
            return None;
        }
        let name = facade_member_method_name(rest, language)?;
        (name, NodeKind::Method, None, rest)
    } else {
        let name = if matches!(
            language,
            Language::TypeScript | Language::Tsx | Language::JavaScript | Language::Jsx
        ) {
            ts_js_class_field_name(rest)
        } else {
            value_decl_name_before_eq(rest).or_else(|| first_identifier(rest))
        }?;
        let after_name = rest
            .find(name)
            .map(|idx| rest[idx + name.len()..].trim_start())
            .unwrap_or_default();
        let eq_index = after_name.find('=');
        let colon_index = after_name
            .find(':')
            .filter(|colon| eq_index.map(|eq| *colon < eq).unwrap_or(true));
        let type_annotation = colon_index
            .map(|colon| {
                let end = eq_index.unwrap_or(after_name.len());
                after_name[colon + 1..end].trim().to_owned()
            })
            .or_else(|| facade_prefix_type_annotation(rest, name, language));
        let initializer = eq_index
            .map(|eq| after_name[eq + 1..].trim())
            .unwrap_or_default();
        let kind = if initializer.contains("=>") || initializer.contains("function") {
            NodeKind::Method
        } else {
            NodeKind::Property
        };
        (name, kind, type_annotation, initializer)
    };

    let signature = if kind == NodeKind::Property {
        type_annotation
            .as_deref()
            .filter(|ty| !ty.is_empty())
            .map(|ty| format!("{ty} {name}"))
    } else {
        None
    };

    let mut node = facade_node(
        file_path,
        language,
        name,
        kind,
        line,
        line_number,
        signature,
        visibility,
        is_static,
        indexed_at,
    );
    apply_facade_qualified_name(&mut node, package_prefix, class_name);

    let mut references = Vec::new();
    if kind == NodeKind::Property {
        if let Some(type_annotation) = type_annotation {
            for target_name in type_identifiers(&type_annotation) {
                references.push(RichFacadePendingEdge {
                    source: node.id.clone(),
                    target_name,
                    kind: EdgeKind::References,
                    metadata: None,
                    line: Some(line_number),
                    column: Some(0),
                });
            }
        }

        if body.contains('{') && body.contains('}') {
            for target_name in facade_fn_ref_names(body, language, false) {
                references.push(RichFacadePendingEdge {
                    source: node.id.clone(),
                    target_name,
                    kind: EdgeKind::References,
                    metadata: Some(HashMap::from([("fnRef".to_owned(), json!(true))])),
                    line: Some(line_number),
                    column: Some(0),
                });
            }
        }
    }

    if kind == NodeKind::Method {
        for target_name in call_names(body) {
            references.push(RichFacadePendingEdge {
                source: node.id.clone(),
                target_name,
                kind: EdgeKind::Calls,
                metadata: None,
                line: Some(line_number),
                column: Some(0),
            });
        }
        for target_name in member_call_names(body) {
            references.push(RichFacadePendingEdge {
                source: node.id.clone(),
                target_name,
                kind: EdgeKind::Calls,
                metadata: None,
                line: Some(line_number),
                column: Some(0),
            });
        }
    }

    Some((node, references))
}

pub(super) fn class_value_from_line(
    line: &str,
    language: Language,
    container_kind: Option<FacadeContainerKind>,
    is_static: bool,
) -> Option<(&str, NodeKind, &str)> {
    // 各语言 class/object 内的常量规则差异很大，只识别非常明确的形态。
    let rest = line.trim().trim_end_matches(';').trim();

    if let Some(after_companion) = rest.strip_prefix("companion object") {
        let body = inline_class_body(after_companion)?;
        return class_value_from_line(body, language, Some(FacadeContainerKind::Object), true);
    }

    let name = value_decl_name_before_eq(rest)?;
    let body = rest
        .find('=')
        .map(|idx| rest[idx + 1..].trim())
        .unwrap_or_default();

    match language {
        Language::Java if is_static && rest.starts_with("final ") => {
            Some((name, NodeKind::Constant, body))
        }
        Language::CSharp if rest.starts_with("const ") => Some((name, NodeKind::Constant, body)),
        Language::CSharp if is_static && rest.starts_with("readonly ") => {
            Some((name, NodeKind::Constant, body))
        }
        Language::Php if rest.starts_with("const ") => Some((name, NodeKind::Constant, body)),
        Language::Ruby if distinctive_value_name(name) => Some((name, NodeKind::Constant, body)),
        Language::Scala => {
            if matches!(container_kind, Some(FacadeContainerKind::Object))
                && (rest.starts_with("val ") || rest.starts_with("var "))
            {
                Some((name, NodeKind::Constant, body))
            } else {
                None
            }
        }
        Language::Kotlin => {
            if rest.starts_with("const val ") {
                Some((name, NodeKind::Constant, body))
            } else if matches!(container_kind, Some(FacadeContainerKind::Object))
                && rest.starts_with("val ")
            {
                Some((name, NodeKind::Variable, body))
            } else {
                None
            }
        }
        Language::Swift if is_static && (rest.starts_with("let ") || rest.starts_with("var ")) => {
            Some((name, NodeKind::Constant, body))
        }
        Language::Dart if is_static && rest.starts_with("const ") => {
            Some((name, NodeKind::Constant, body))
        }
        Language::Dart if is_static && rest.starts_with("final ") => {
            Some((name, NodeKind::Variable, body))
        }
        _ => None,
    }
}

pub(super) fn ts_js_class_field_name(input: &str) -> Option<&str> {
    let before_eq = input.split('=').next()?.trim();
    let before_type = before_eq.split(':').next()?.trim();
    let token = identifier_tokens(before_type)
        .into_iter()
        .rev()
        .find(|token| !value_decl_keyword(token) && !method_decl_keyword(token))?;
    let start = before_type.rfind(&token)?;
    Some(before_type[start..start + token.len()].trim_start_matches('#'))
}

pub(super) fn strip_leading_annotations(mut line: &str) -> &str {
    // 行级 facade 解析先剥离注解，避免 `@Get() foo()` 被误认为方法名是注解。
    loop {
        let trimmed = line.trim_start();
        let Some(after_at) = trimmed.strip_prefix('@') else {
            return trimmed;
        };
        let Some(name) = first_identifier(after_at) else {
            return trimmed;
        };
        let mut rest = after_at[name.len()..].trim_start();
        if rest.starts_with('(') {
            if let Some(close) = facade_find_matching_delim(rest, 0, '(', ')', rest.len()) {
                rest = rest[close + 1..].trim_start();
            } else {
                return trimmed;
            }
        }
        line = rest;
    }
}

pub(super) fn facade_prefix_type_annotation(
    rest: &str,
    name: &str,
    language: Language,
) -> Option<String> {
    if !matches!(
        language,
        Language::Java | Language::Kotlin | Language::Scala | Language::C | Language::Cpp
    ) {
        return None;
    }
    let name_pos = rest.find(name)?;
    let before_name = rest[..name_pos].trim();
    if before_name.is_empty() || before_name.contains('(') {
        return None;
    }
    identifier_tokens(before_name)
        .into_iter()
        .rev()
        .find(|token| !method_decl_keyword(token) && !value_decl_keyword(token))
}

pub(super) fn facade_member_method_name(input: &str, language: Language) -> Option<&str> {
    // 排除 `obj.method()` / `ptr->call()` / C 函数指针声明，保守识别真正的成员声明。
    let before_paren = input.split('(').next()?.trim();
    if before_paren.contains('.') || before_paren.contains("->") || before_paren.contains("(*") {
        return None;
    }
    let tokens = identifier_tokens(before_paren);
    let declaration_keyword = tokens.iter().any(|token| {
        method_decl_keyword(token) || matches!(token.as_str(), "fun" | "func" | "function")
    });
    if !input.contains('{')
        && tokens.len() < 2
        && !declaration_keyword
        && !matches!(language, Language::Swift | Language::Kotlin)
    {
        return None;
    }
    method_name_from_signature(input)
}
