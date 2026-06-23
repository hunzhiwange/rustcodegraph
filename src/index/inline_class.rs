//! 单行声明和语言特定声明片段的解析工具。
//!
//! 这些函数服务于 fallback 抽取：只从一行源码中提取“足够可靠”的类型、函数、值和成员名，
//! 避免为了少量补洞引入完整语法树依赖。

use super::*;

pub(super) fn inline_class_body(line: &str) -> Option<&str> {
    let start = line.find('{')?;
    let end = line.rfind('}')?;
    if end <= start {
        return None;
    }
    let body = line[start + 1..end].trim();
    (!body.is_empty()).then_some(body)
}

pub(super) fn inline_class_member_segments(body: &str) -> Vec<String> {
    // 单行 class body 可能包含带花括号的方法实现，所以只在顶层 `;` 或完整 block 结束处分段。
    let mut segments = Vec::new();
    let mut start = 0usize;
    let mut depth = 0isize;

    for (idx, ch) in body.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                if depth > 0 {
                    depth -= 1;
                }
                if depth == 0 {
                    let end = idx + ch.len_utf8();
                    let segment = body[start..end].trim();
                    if !segment.is_empty() {
                        segments.push(segment.to_owned());
                    }
                    start = end;
                }
            }
            ';' if depth == 0 => {
                let segment = body[start..idx].trim();
                if !segment.is_empty() {
                    segments.push(segment.to_owned());
                }
                start = idx + ch.len_utf8();
            }
            _ => {}
        }
    }

    let tail = body[start..].trim();
    if !tail.is_empty() {
        segments.push(tail.to_owned());
    }
    segments
}

pub(super) fn type_alias_name_from_line(line: &str) -> Option<&str> {
    let mut rest = line.trim_start();
    if let Some(after_export) = rest.strip_prefix("export ") {
        rest = after_export.trim_start();
    }
    if let Some(after_declare) = rest.strip_prefix("declare ") {
        rest = after_declare.trim_start();
    }
    let after_type = rest.strip_prefix("type ")?;
    first_identifier(after_type).map(trim_identifier)
}

pub(super) fn objc_method_name_from_line(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    if !(trimmed.starts_with("- ") || trimmed.starts_with("+ ")) {
        return None;
    }
    let after_return = trimmed.split_once(')')?.1.trim_start();
    first_identifier(after_return).map(trim_identifier)
}

pub(super) fn go_receiver_method_from_line(line: &str) -> Option<(&str, &str)> {
    let after_func = line.trim_start().strip_prefix("func ")?;
    let after_open = after_func.strip_prefix('(')?;
    let (receiver, after_receiver) = after_open.split_once(')')?;
    let receiver_type_raw = receiver.split_whitespace().last()?;
    let receiver_type = receiver_type_raw
        .trim_start_matches('*')
        .trim_start_matches('[')
        .trim();
    let method_name = method_name_from_signature(after_receiver.trim_start())?;
    Some((trim_identifier(receiver_type), method_name))
}

pub(super) fn cpp_qualified_method_from_line(line: &str) -> Option<(String, String)> {
    if !line.contains("::") || !line.contains('(') {
        return None;
    }
    let before_paren = line.split('(').next()?.trim();
    let (class_part, method_part) = before_paren.rsplit_once("::")?;
    let class_name = identifier_tokens(class_part).into_iter().last()?;
    let method_name = trim_identifier(method_part).to_owned();
    if class_name.is_empty() || method_name.is_empty() {
        return None;
    }
    Some((class_name, method_name))
}

pub(super) fn cpp_base_class_names_from_line(line: &str) -> Vec<String> {
    let Some((_, after_colon)) = line.split_once(':') else {
        return Vec::new();
    };
    let before_body = after_colon.split('{').next().unwrap_or(after_colon);
    before_body
        .split(',')
        .filter_map(|part| {
            identifier_tokens(part).into_iter().rev().find(|token| {
                !matches!(
                    token.as_str(),
                    "public" | "private" | "protected" | "virtual"
                )
            })
        })
        .collect()
}

pub(super) fn facade_import_name_from_line(line: &str, language: Language) -> Option<String> {
    if !matches!(
        language,
        Language::Java | Language::Kotlin | Language::Scala | Language::Php
    ) {
        return None;
    }
    let mut rest = line.trim_start().strip_prefix("import ")?;
    if let Some(after_static) = rest.strip_prefix("static ") {
        rest = after_static.trim_start();
    }
    let end = rest
        .find(|ch: char| ch == ';' || ch.is_whitespace())
        .unwrap_or(rest.len());
    let name = rest[..end].trim().trim_matches(';');
    if name.is_empty() || name.ends_with(".*") {
        return None;
    }
    Some(name.to_owned())
}

pub(super) fn function_name_from_line(line: &str) -> Option<&str> {
    // 先剥掉跨语言常见修饰符，再走各语言声明形态；最后才尝试 C-like 启发式。
    let mut rest = line.trim_start();
    if let Some(after_export) = rest.strip_prefix("export ") {
        rest = after_export.trim_start();
    }
    if let Some(after_declare) = rest.strip_prefix("declare ") {
        rest = after_declare.trim_start();
    }
    if let Some(after_pub) = rest.strip_prefix("pub ") {
        rest = after_pub.trim_start();
    } else if rest.starts_with("pub(") {
        let close = rest.find(')')?;
        rest = rest[close + 1..].trim_start();
    }
    if let Some(after_async) = rest.strip_prefix("async ") {
        rest = after_async.trim_start();
    }
    if let Some(after_function) = rest.strip_prefix("function ") {
        if !after_function.contains('(') && after_function.contains(':') {
            return first_identifier(after_function);
        }
        return method_name_from_signature(after_function).or_else(|| {
            first_identifier(after_function.split(':').next().unwrap_or(after_function))
        });
    }
    if let Some(after_func) = rest.strip_prefix("func ") {
        let signature = if after_func.starts_with('(') {
            after_func
                .split_once(')')
                .map(|(_, tail)| tail.trim_start())?
        } else {
            after_func
        };
        return method_name_from_signature(signature);
    }
    if let Some(after_def) = rest.strip_prefix("def ") {
        return method_name_from_signature(after_def)
            .or_else(|| Some(trim_identifier(after_def.split('(').next()?)));
    }
    if let Some(after_procedure) = rest
        .strip_prefix("procedure ")
        .or_else(|| rest.strip_prefix("function "))
    {
        let name = after_procedure
            .split(['(', ';'])
            .next()
            .map(str::trim)
            .and_then(|raw| raw.rsplit('.').next())?;
        return Some(trim_identifier(name));
    }
    if let Some(after_fun) = rest.strip_prefix("fun ") {
        return method_name_from_signature(after_fun);
    }
    if let Some(after_func) = rest.strip_prefix("func ") {
        return method_name_from_signature(after_func);
    }
    if let Some(after_fn) = rest.strip_prefix("fn ") {
        return Some(trim_identifier(after_fn.split('(').next()?));
    }
    c_like_function_name(rest)
}

pub(super) fn top_level_value_from_line(
    line: &str,
    language: Language,
) -> Option<(&str, NodeKind, &str)> {
    // 只识别顶层值声明；如果初始化表达式本身是函数，则提升为 Function 节点用于调用图。
    let mut rest = line.trim_start();
    if let Some(after_export) = rest.strip_prefix("export ") {
        rest = after_export.trim_start();
    }
    if let Some(after_declare) = rest.strip_prefix("declare ") {
        rest = after_declare.trim_start();
    }
    if let Some(after_pub) = rest.strip_prefix("pub ") {
        rest = after_pub.trim_start();
    } else if rest.starts_with("pub(") {
        let close = rest.find(')')?;
        rest = rest[close + 1..].trim_start();
    }

    if let Some((name, kind, body)) = language_value_from_line(rest, language) {
        return Some((name, kind, body));
    }

    let (decl_kind, after_decl) = if let Some(after_const) = rest.strip_prefix("const ") {
        (NodeKind::Constant, after_const)
    } else if let Some(after_let) = rest.strip_prefix("let ") {
        (NodeKind::Variable, after_let)
    } else if let Some(after_var) = rest.strip_prefix("var ") {
        (NodeKind::Variable, after_var)
    } else {
        return None;
    };

    let name = first_identifier(after_decl).map(trim_identifier)?;
    let after_name = after_decl[name.len()..].trim_start();
    let body = after_name
        .find('=')
        .map(|idx| after_name[idx + 1..].trim())
        .unwrap_or_default();
    let kind = if body.contains("=>") || body.starts_with("function") {
        NodeKind::Function
    } else {
        decl_kind
    };
    Some((name, kind, body))
}

pub(super) fn language_value_from_line(
    line: &str,
    language: Language,
) -> Option<(&str, NodeKind, &str)> {
    let body = line
        .find('=')
        .map(|idx| line[idx + 1..].trim())
        .unwrap_or_default();
    match language {
        Language::Rust => {
            let after_decl = line
                .strip_prefix("const ")
                .or_else(|| line.strip_prefix("static "))?;
            let name = first_identifier(after_decl)?;
            Some((name, NodeKind::Constant, body))
        }
        Language::Python | Language::Ruby => {
            if !line.contains('=') {
                return None;
            }
            let name = value_decl_name_before_eq(line)?;
            distinctive_value_name(name).then_some((name, NodeKind::Constant, body))
        }
        Language::C | Language::Cpp => {
            c_static_const_name(line).map(|name| (name, NodeKind::Constant, body))
        }
        Language::Php => {
            let after_const = line.strip_prefix("const ")?;
            let name = first_identifier(after_const)?;
            Some((name, NodeKind::Constant, body))
        }
        Language::Scala => {
            let after_val = line
                .strip_prefix("val ")
                .or_else(|| line.strip_prefix("var "))?;
            let name = first_identifier(after_val)?;
            Some((name, NodeKind::Constant, body))
        }
        Language::Kotlin => {
            let after_val = line
                .strip_prefix("const val ")
                .or_else(|| line.strip_prefix("val "))?;
            let name = first_identifier(after_val)?;
            Some((name, NodeKind::Constant, body))
        }
        Language::Swift => {
            let after_decl = line
                .strip_prefix("let ")
                .or_else(|| line.strip_prefix("var "))?;
            let name = first_identifier(after_decl)?;
            Some((name, NodeKind::Constant, body))
        }
        Language::Dart => {
            let after_decl = line
                .strip_prefix("const ")
                .or_else(|| line.strip_prefix("final "))?;
            let name = first_identifier(after_decl)?;
            Some((name, NodeKind::Constant, body))
        }
        Language::Pascal => {
            if line.contains(":=") {
                return None;
            }
            let name = value_decl_name_before_eq(line)?;
            distinctive_value_name(name).then_some((name, NodeKind::Constant, body))
        }
        _ => None,
    }
}

pub(super) fn method_name_from_signature(input: &str) -> Option<&str> {
    let before_paren = input.split('(').next()?.trim();
    identifier_tokens(before_paren)
        .into_iter()
        .rev()
        .find(|token| !method_decl_keyword(token))
        .map(|token| {
            let start = before_paren.rfind(&token).unwrap_or(0);
            &before_paren[start..start + token.len()]
        })
}

pub(super) fn c_like_function_name(input: &str) -> Option<&str> {
    if !looks_like_method_declaration(input)
        || input.starts_with('#')
        || input.starts_with("typedef ")
    {
        return None;
    }
    let before_paren = input.split('(').next()?.trim();
    if identifier_tokens(before_paren).len() < 2 {
        return None;
    }
    let name = method_name_from_signature(input)?;
    (!is_call_keyword(name) && !value_decl_keyword(name)).then_some(name)
}

pub(super) fn value_decl_name_before_eq(input: &str) -> Option<&str> {
    let before_eq = input.split('=').next()?.trim();
    let token = identifier_tokens(before_eq)
        .into_iter()
        .rev()
        .find(|token| !value_decl_keyword(token))?;
    let start = before_eq.rfind(&token)?;
    Some(before_eq[start..start + token.len()].trim_start_matches('$'))
}

pub(super) fn c_static_const_name(input: &str) -> Option<&str> {
    if !input.contains("const") || !input.contains('=') || input.contains('(') {
        return None;
    }
    value_decl_name_before_eq(input)
}
