//! Express/Node.js framework resolver translated from `express.ts`.
//!
//! Express resolver 从 route/middleware/controller/service 的常见约定里抽取 route
//! 节点和 handler 引用。它偏向可命名 handler，避免把所有内联逻辑展开成噪声。

use regex::Regex;

use crate::resolution::strip_comments::{CommentLang, strip_comments_for_regex};
use crate::resolution::types::{
    FrameworkExtractionResult, FrameworkResolver, ResolutionContext, ResolvedRef, UnresolvedRef,
    line_for_byte, make_node, make_reference,
};
use crate::types::{Language, NodeKind, ReferenceKind};

const RESERVED_CALLS: &[&str] = &[
    // 内联 handler 体里这些常见库/response 调用不是业务 callee。
    "json",
    "jsonp",
    "send",
    "sendStatus",
    "sendFile",
    "status",
    "end",
    "redirect",
    "render",
    "set",
    "get",
    "header",
    "type",
    "format",
    "attachment",
    "download",
    "cookie",
    "clearCookie",
    "append",
    "location",
    "vary",
    "links",
    "accepts",
    "is",
    "next",
    "then",
    "catch",
    "finally",
    "resolve",
    "reject",
    "all",
    "race",
    "map",
    "filter",
    "forEach",
    "reduce",
    "find",
    "push",
    "pop",
    "slice",
    "splice",
    "includes",
    "keys",
    "values",
    "entries",
    "assign",
    "parse",
    "stringify",
    "log",
    "error",
    "warn",
    "info",
    "String",
    "Number",
    "Boolean",
    "Array",
    "Object",
    "Date",
    "Math",
    "JSON",
    "Promise",
    "require",
    "fail",
];

pub struct ExpressResolver;

pub const EXPRESS_RESOLVER: ExpressResolver = ExpressResolver;

impl FrameworkResolver for ExpressResolver {
    fn name(&self) -> &'static str {
        "express"
    }

    fn languages(&self) -> Option<&'static [Language]> {
        Some(&[Language::JavaScript, Language::TypeScript])
    }

    fn detect(&self, context: &mut dyn ResolutionContext) -> bool {
        // package.json 是强信号；目录名 + route 语法作为轻量兜底。
        if let Some(package_json) = context.read_file("package.json")
            && ["\"express\"", "\"fastify\"", "\"koa\"", "\"hapi\""]
                .iter()
                .any(|needle| package_json.contains(needle))
        {
            return true;
        }
        for file in context.get_all_files() {
            if !(file.contains("routes")
                || file.contains("controllers")
                || file.contains("middleware"))
            {
                continue;
            }
            if let Some(content) = context.read_file(&file)
                && (content.contains("express")
                    || content.contains("app.get")
                    || content.contains("router.get"))
            {
                return true;
            }
        }
        false
    }

    fn resolve(
        &self,
        reference: &UnresolvedRef,
        context: &mut dyn ResolutionContext,
    ) -> Option<ResolvedRef> {
        // 这些解析只处理框架惯例名，普通 import/name matching 仍由核心 resolver 负责。
        if is_middleware_name(&reference.reference_name)
            && let Some(target) = resolve_middleware(&reference.reference_name, context)
        {
            return Some(ResolvedRef::framework(reference, target, 0.8));
        }

        if let Some((controller, method)) = split_controller_ref(&reference.reference_name)
            && let Some(target) = resolve_controller_method(&controller, &method, context)
        {
            return Some(ResolvedRef::framework(reference, target, 0.85));
        }

        if let Some((service, method)) = split_service_ref(&reference.reference_name)
            && let Some(target) = resolve_service_method(&service, &method, context)
        {
            return Some(ResolvedRef::framework(reference, target, 0.8));
        }

        None
    }

    fn extract(&self, file_path: &str, content: &str) -> FrameworkExtractionResult {
        if !is_js_like(file_path) {
            return FrameworkExtractionResult::default();
        }
        let lang = detect_language(file_path);
        let safe = strip_comments_for_regex(
            content,
            if lang == Language::TypeScript {
                CommentLang::TypeScript
            } else {
                CommentLang::JavaScript
            },
        );
        let content = safe.as_str();
        let mut result = FrameworkExtractionResult::default();
        // 只抽 `app/router.METHOD("path", ...)` 这类高置信 route 头。
        let head_re = Regex::new(
            r#"\b(app|router)\.(get|post|put|patch|delete|all|use)\s*\(\s*['"]([^'"]+)['"]\s*,"#,
        )
        .unwrap();
        let call_re = Regex::new(r"\b([A-Za-z_$][\w$]*)\s*\(").unwrap();

        for caps in head_re.captures_iter(content) {
            let whole = caps.get(0).unwrap();
            let method = caps.get(2).unwrap().as_str();
            let route_path = caps.get(3).unwrap().as_str();
            if method == "use" && !route_path.starts_with('/') {
                continue;
            }
            let upper = method.to_ascii_uppercase();
            let line = line_for_byte(content, whole.start());
            let route_id = format!("route:{file_path}:{line}:{upper}:{route_path}");
            result.nodes.push(make_node(
                route_id.clone(),
                NodeKind::Route,
                format!("{upper} {route_path}"),
                format!("{file_path}::{upper}:{route_path}"),
                file_path,
                lang,
                line,
                None,
                None,
            ));

            let open_paren = content[whole.start()..]
                .find('(')
                .map(|idx| whole.start() + idx)
                .unwrap_or(whole.start());
            let close_paren = match_delim(content, open_paren, '(', ')').unwrap_or(open_paren);
            let args = if close_paren > open_paren {
                &content[open_paren + 1..close_paren]
            } else {
                ""
            };

            if let Some(arrow_at) = args.find("=>") {
                // 对内联箭头 handler 只抽体内业务调用名，跳过 res/json/Promise 等保留调用。
                let after_arrow = &args[arrow_at + 2..];
                let body = if let Some(brace_at) = after_arrow.find('{') {
                    if after_arrow[..brace_at].trim().is_empty() {
                        match_delim(after_arrow, brace_at, '{', '}')
                            .map(|end| &after_arrow[brace_at + 1..end])
                            .unwrap_or(after_arrow)
                    } else {
                        after_arrow
                    }
                } else {
                    after_arrow
                };
                let mut seen = Vec::<String>::new();
                for call_caps in call_re.captures_iter(body) {
                    let name = call_caps.get(1).unwrap().as_str();
                    if RESERVED_CALLS.contains(&name)
                        || seen.iter().any(|existing| existing == name)
                    {
                        continue;
                    }
                    seen.push(name.to_string());
                    result.references.push(make_reference(
                        route_id.clone(),
                        name,
                        ReferenceKind::Calls,
                        line,
                        0,
                        file_path,
                        lang,
                    ));
                }
            } else {
                // 非内联形式取最后一个参数，支持 middleware 链后面的最终 handler。
                let handler_name = args
                    .split(',')
                    .map(str::trim)
                    .rfind(|part| !part.is_empty())
                    .and_then(extract_tail_ident);
                if let Some(handler_name) = handler_name {
                    result.references.push(make_reference(
                        route_id.clone(),
                        handler_name,
                        ReferenceKind::References,
                        line,
                        0,
                        file_path,
                        lang,
                    ));
                }
            }
        }
        result
    }
}

fn extract_tail_ident(expr: &str) -> Option<String> {
    // `controller.show`, `handlers.show()` 都归约为 `show`，交给后续 name resolver。
    let cleaned = expr
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>()
        .trim_end_matches("()")
        .to_string();
    cleaned
        .rsplit('.')
        .next()
        .filter(|part| is_ident(part))
        .map(ToOwned::to_owned)
}

fn match_delim(s: &str, open: usize, open_char: char, close_char: char) -> Option<usize> {
    // route 参数可能包含 nested function/array/object 和字符串；括号匹配要跳过引号。
    let mut depth = 0usize;
    let mut i = open;
    while i < s.len() {
        let ch = s[i..].chars().next()?;
        if matches!(ch, '"' | '\'' | '`') {
            let quote = ch;
            i += ch.len_utf8();
            while i < s.len() {
                let qch = s[i..].chars().next()?;
                if qch == '\\' {
                    i += qch.len_utf8();
                    if i < s.len() {
                        i += s[i..].chars().next()?.len_utf8();
                    }
                    continue;
                }
                i += qch.len_utf8();
                if qch == quote {
                    break;
                }
            }
            continue;
        }
        if ch == open_char {
            depth += 1;
        } else if ch == close_char {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return Some(i);
            }
        }
        i += ch.len_utf8();
    }
    None
}

fn is_middleware_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "auth"
            | "authenticate"
            | "authorization"
            | "cors"
            | "helmet"
            | "logger"
            | "errorhandler"
            | "notfound"
    ) || lower.starts_with("validate")
        || lower.starts_with("sanitize")
        || lower.starts_with("ratelimit")
        || lower.ends_with("middleware")
}

fn resolve_middleware(name: &str, context: &mut dyn ResolutionContext) -> Option<String> {
    // FooMiddleware 可退到 Foo，并优先 middleware 目录下的节点。
    let lower = name.to_ascii_lowercase();
    if let Some(node) = context.get_nodes_by_name(name).into_iter().find(|node| {
        node.name.eq_ignore_ascii_case(name)
            || node
                .name
                .eq_ignore_ascii_case(lower.trim_end_matches("middleware"))
    }) {
        return Some(node.id);
    }
    let base = name.trim_end_matches("Middleware");
    if base != name {
        let candidates = context.get_nodes_by_name(base);
        if let Some(preferred) = candidates.iter().find(|node| {
            node.file_path.contains("/middleware/") || node.file_path.contains("/middlewares/")
        }) {
            return Some(preferred.id.clone());
        }
        return candidates.first().map(|node| node.id.clone());
    }
    None
}

fn resolve_controller_method(
    controller: &str,
    method: &str,
    context: &mut dyn ResolutionContext,
) -> Option<String> {
    // controller.method 字符串不一定对应 class 节点；先按文件路径包含 controller 名
    // 找同名函数/方法，再退到 Controller class 的同文件方法。
    let controller_lower = controller.to_ascii_lowercase();
    if let Some(method_node) = context.get_nodes_by_name(method).into_iter().find(|node| {
        matches!(node.kind, NodeKind::Method | NodeKind::Function)
            && node
                .file_path
                .to_ascii_lowercase()
                .contains(&controller_lower)
    }) {
        return Some(method_node.id);
    }
    let controller_name = format!("{controller}Controller");
    for ctrl in context.get_nodes_by_name(&controller_name) {
        if let Some(method_node) =
            context
                .get_nodes_in_file(&ctrl.file_path)
                .into_iter()
                .find(|node| {
                    matches!(node.kind, NodeKind::Method | NodeKind::Function)
                        && node.name == method
                })
        {
            return Some(method_node.id);
        }
    }
    None
}

fn resolve_service_method(
    service_name: &str,
    method: &str,
    context: &mut dyn ResolutionContext,
) -> Option<String> {
    let stripped = service_name
        .trim_end_matches("Service")
        .trim_end_matches("Helper")
        .trim_end_matches("Utils")
        .trim_end_matches("Util")
        .to_ascii_lowercase();
    context
        .get_nodes_by_name(method)
        .into_iter()
        .find_map(|node| {
            (matches!(node.kind, NodeKind::Method | NodeKind::Function)
                && node.file_path.to_ascii_lowercase().contains(&stripped))
            .then_some(node.id)
        })
}

fn split_controller_ref(value: &str) -> Option<(String, String)> {
    let (left, right) = value.split_once('.')?;
    left.strip_suffix("Controller")
        .filter(|controller| is_ident(controller) && is_ident(right))
        .map(|controller| (controller.to_string(), right.to_string()))
}

fn split_service_ref(value: &str) -> Option<(String, String)> {
    let (left, right) = value.split_once('.')?;
    ["Service", "Helper", "Utils", "Util"]
        .iter()
        .any(|suffix| left.ends_with(suffix))
        .then(|| (left.to_string(), right.to_string()))
}

fn is_js_like(file_path: &str) -> bool {
    [".js", ".mjs", ".cjs", ".ts", ".tsx"]
        .iter()
        .any(|ext| file_path.ends_with(ext))
}

fn detect_language(file_path: &str) -> Language {
    if file_path.ends_with(".ts") || file_path.ends_with(".tsx") {
        Language::TypeScript
    } else {
        Language::JavaScript
    }
}

fn is_ident(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(
        chars.next(),
        Some('_') | Some('$') | Some('A'..='Z') | Some('a'..='z')
    ) && chars.all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
}
