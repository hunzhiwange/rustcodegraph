//! C/C++ 调用链和 receiver type 的保守推断。
//!
//! 这里不是完整 C++ 类型系统，只处理局部变量声明、`auto = new/make_*` 和简单
//! factory 返回类型，目的是补上常见 `obj.method`/`factory().method` 流。

use crate::resolution::types::{ResolutionContext, ResolvedBy, ResolvedRef, UnresolvedRef};
use crate::types::NodeKind;

use super::common::split_chain_shape;
use super::typed::{lookup_callee_return_type, resolve_method_on_type};

fn cpp_last_segment(name: &str) -> &str {
    name.split("::")
        .filter(|part| !part.is_empty())
        .last()
        .unwrap_or(name)
}

fn cpp_class_exists(
    name: &str,
    reference: &UnresolvedRef,
    context: &mut dyn ResolutionContext,
) -> bool {
    let last = cpp_last_segment(name);
    context.get_nodes_by_name(last).into_iter().any(|node| {
        (node.kind == NodeKind::Class || node.kind == NodeKind::Struct)
            && node.language == reference.language
    })
}

fn resolve_cpp_call_result_type(
    inner: &str,
    reference: &UnresolvedRef,
    context: &mut dyn ResolutionContext,
    depth: usize,
) -> Option<String> {
    // 限制递归深度，避免复杂表达式或递归 factory 链导致解析阶段长时间卡住。
    if depth > 3 {
        return None;
    }
    let expr = inner.trim();
    if let Some(start) = expr
        .find("make_unique<")
        .or_else(|| expr.find("make_shared<"))
    {
        let after = &expr[start..];
        if let Some(open) = after.find('<') {
            let tail = &after[open + 1..];
            let name = tail
                .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
                .next()
                .unwrap_or("");
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
    }

    if let Some(dot_idx) = expr.rfind('.') {
        let recv = &expr[..dot_idx];
        let method = &expr[dot_idx + 1..];
        if recv.contains('.') || recv.contains('(') || recv.contains("::") {
            return None;
        }
        let recv_type = infer_cpp_receiver_type(recv, reference, context, depth + 1)?;
        return lookup_callee_return_type(&format!("{recv_type}::{method}"), reference, context);
    }

    if let Some(ret) = lookup_callee_return_type(expr, reference, context) {
        return Some(ret);
    }
    cpp_class_exists(expr, reference, context).then(|| cpp_last_segment(expr).to_string())
}

pub(super) fn infer_cpp_receiver_type(
    receiver_name: &str,
    reference: &UnresolvedRef,
    context: &mut dyn ResolutionContext,
    depth: usize,
) -> Option<String> {
    // 只向调用点之前回扫同一文件的局部声明；跨文件数据流和宏展开不在这里处理，
    // 避免把启发式推断做成昂贵且不稳定的语义分析。
    let source = context.read_file(&reference.file_path)?;
    let lines = source.lines().collect::<Vec<_>>();
    let call_line_index = reference
        .line
        .saturating_sub(1)
        .min(lines.len().saturating_sub(1) as u64) as usize;

    for line in lines[..=call_line_index].iter().rev() {
        if !line.contains(receiver_name) {
            continue;
        }
        if let Some(before) = line.split(receiver_name).next() {
            let ty = before
                .split([';', '=', ',', ')', '[', '{', '('])
                .next_back()
                .unwrap_or(before)
                .trim()
                .trim_end_matches(['*', '&', ' '])
                .split_whitespace()
                .last()
                .unwrap_or("");
            let normalized = normalize_cpp_type_name(ty);
            if normalized.as_deref() == Some("auto") {
                if let Some(init) =
                    infer_cpp_auto_initializer_type(line, receiver_name, reference, context, depth)
                {
                    return Some(init);
                }
            } else if normalized.is_some() {
                return normalized;
            }
        }
    }

    None
}

fn normalize_cpp_type_name(type_name: &str) -> Option<String> {
    // 去掉限定词、指针/引用和模板参数后只保留最后一段类型名，便于与抽取出的
    // class/struct 节点名称匹配。
    let mut normalized = type_name
        .replace("const", " ")
        .replace("volatile", " ")
        .replace("mutable", " ")
        .replace("typename", " ")
        .replace("class", " ")
        .replace("struct", " ")
        .replace(['&', '*'], " ");
    if let Some(start) = normalized.find('<')
        && let Some(end) = normalized.rfind('>')
    {
        normalized.replace_range(start..=end, " ");
    }
    let last = normalized
        .split_whitespace()
        .last()
        .and_then(|part| part.split("::").last())
        .unwrap_or("")
        .trim();
    if last.is_empty() || CPP_NON_TYPE_TOKENS.contains(&last) {
        None
    } else {
        Some(last.to_string())
    }
}

const CPP_NON_TYPE_TOKENS: &[&str] = &[
    "return",
    "if",
    "else",
    "for",
    "while",
    "do",
    "switch",
    "case",
    "default",
    "break",
    "continue",
    "goto",
    "throw",
    "new",
    "delete",
    "co_await",
    "co_yield",
    "co_return",
    "static_cast",
    "const_cast",
    "dynamic_cast",
    "reinterpret_cast",
    "sizeof",
    "alignof",
    "typeid",
    "and",
    "or",
    "not",
    "xor",
];

fn infer_cpp_auto_initializer_type(
    line: &str,
    receiver_name: &str,
    reference: &UnresolvedRef,
    context: &mut dyn ResolutionContext,
    depth: usize,
) -> Option<String> {
    // `auto x = new Foo` 和 `auto x = make_unique<Foo>` 是最常见的可静态恢复形状；
    // 其它复杂 initializer 继续交给返回类型 lookup。
    let rhs = line.split(receiver_name).nth(1)?.split_once('=')?.1.trim();
    if let Some(rest) = rhs.strip_prefix("new ") {
        return Some(
            cpp_last_segment(rest.split(['(', ' ', ';']).next().unwrap_or("")).to_string(),
        );
    }
    let callee = rhs
        .split('(')
        .next()?
        .trim()
        .replace(char::is_whitespace, "");
    resolve_cpp_call_result_type(&callee, reference, context, depth + 1)
}

pub fn match_cpp_call_chain(
    reference: &UnresolvedRef,
    context: &mut dyn ResolutionContext,
) -> Option<ResolvedRef> {
    let (inner, method) = split_chain_shape(&reference.reference_name)?;
    let cls = resolve_cpp_call_result_type(inner, reference, context, 0)?;
    resolve_method_on_type(
        &cls,
        method,
        reference,
        context,
        0.85,
        ResolvedBy::InstanceMethod,
        None,
        0,
    )
}
