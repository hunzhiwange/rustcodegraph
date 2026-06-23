//! Go framework resolver translated from `go.ts`.
//!
//! Go resolver 抽取常见 router 方法注册的 route 节点，并用目录惯例辅助解析
//! Handler/Service/Repository/Middleware/Model 这类符号名。

use regex::Regex;

use crate::resolution::strip_comments::{CommentLang, strip_comments_for_regex};
use crate::resolution::types::{
    FrameworkExtractionResult, FrameworkResolver, ResolutionContext, ResolvedRef, UnresolvedRef,
    line_for_byte, make_node, make_reference, resolve_by_name_and_kind,
};
use crate::types::{Language, NodeKind, ReferenceKind};

const HANDLER_DIRS: &[&str] = &[
    "handler",
    "handlers",
    "api",
    "routes",
    "controller",
    "controllers",
];
const SERVICE_DIRS: &[&str] = &["service", "services", "repository", "store", "pkg"];
const MIDDLEWARE_DIRS: &[&str] = &["middleware", "middlewares"];
const MODEL_DIRS: &[&str] = &["model", "models", "entity", "entities", "domain", "pkg"];
const FUNCTION_KINDS: &[NodeKind] = &[NodeKind::Function];
const SERVICE_KINDS: &[NodeKind] = &[NodeKind::Struct, NodeKind::Interface];
const MODEL_KINDS: &[NodeKind] = &[NodeKind::Struct];

pub struct GoResolver;

pub const GO_RESOLVER: GoResolver = GoResolver;

impl FrameworkResolver for GoResolver {
    fn name(&self) -> &'static str {
        "go"
    }

    fn languages(&self) -> Option<&'static [Language]> {
        Some(&[Language::Go])
    }

    fn detect(&self, context: &mut dyn ResolutionContext) -> bool {
        context.read_file("go.mod").is_some()
            || context
                .get_all_files()
                .iter()
                .any(|file| file.ends_with(".go"))
    }

    fn resolve(
        &self,
        reference: &UnresolvedRef,
        context: &mut dyn ResolutionContext,
    ) -> Option<ResolvedRef> {
        // Go 项目缺少框架统一注解，resolver 只在名称和目录都符合惯例时给中等置信度。
        if (reference.reference_name.ends_with("Handler")
            || reference.reference_name.starts_with("Handle"))
            && let Some(target) = resolve_go_name(
                &reference.reference_name,
                FUNCTION_KINDS,
                HANDLER_DIRS,
                context,
            )
        {
            return Some(ResolvedRef::framework(reference, target, 0.8));
        }
        if (reference.reference_name.ends_with("Service")
            || reference.reference_name.ends_with("Repository")
            || reference.reference_name.ends_with("Store"))
            && let Some(target) = resolve_go_name(
                &reference.reference_name,
                SERVICE_KINDS,
                SERVICE_DIRS,
                context,
            )
        {
            return Some(ResolvedRef::framework(reference, target, 0.8));
        }
        if (reference.reference_name.ends_with("Middleware")
            || reference.reference_name.starts_with("Auth")
            || reference.reference_name.starts_with("Log"))
            && let Some(target) = resolve_go_name(
                &reference.reference_name,
                FUNCTION_KINDS,
                MIDDLEWARE_DIRS,
                context,
            )
        {
            return Some(ResolvedRef::framework(reference, target, 0.75));
        }
        if is_pascal_word(&reference.reference_name)
            && let Some(target) =
                resolve_go_name(&reference.reference_name, MODEL_KINDS, MODEL_DIRS, context)
        {
            return Some(ResolvedRef::framework(reference, target, 0.7));
        }
        None
    }

    fn extract(&self, file_path: &str, content: &str) -> FrameworkExtractionResult {
        if !file_path.ends_with(".go") {
            return FrameworkExtractionResult::default();
        }
        let safe = strip_comments_for_regex(content, CommentLang::Go);
        let content = safe.as_str();
        let mut result = FrameworkExtractionResult::default();
        // 覆盖 gin/chi/net/http 一类 `router.GET("/x", handler)` 或 HandleFunc 形式。
        let route_re = Regex::new(r#"\b\w+\.(GET|POST|PUT|PATCH|DELETE|OPTIONS|HEAD|Get|Post|Put|Patch|Delete|Handle|HandleFunc)\s*\(\s*"([^"]+)"\s*,\s*([^)]+)\)"#).unwrap();
        for caps in route_re.captures_iter(content) {
            let whole = caps.get(0).unwrap();
            let raw_method = caps.get(1).unwrap().as_str();
            let route_path = caps.get(2).unwrap().as_str();
            let method = if raw_method == "Handle" || raw_method == "HandleFunc" {
                "ANY".to_string()
            } else {
                raw_method.to_ascii_uppercase()
            };
            let line = line_for_byte(content, whole.start());
            let route_id = format!("route:{file_path}:{line}:{method}:{route_path}");
            result.nodes.push(make_node(
                route_id.clone(),
                NodeKind::Route,
                format!("{method} {route_path}"),
                format!("{file_path}::route:{route_path}"),
                file_path,
                Language::Go,
                line,
                None,
                None,
            ));
            if let Some(handler) = extract_go_tail_ident(caps.get(3).unwrap().as_str()) {
                result.references.push(make_reference(
                    route_id,
                    handler,
                    ReferenceKind::References,
                    line,
                    0,
                    file_path,
                    Language::Go,
                ));
            }
        }
        result
    }
}

fn resolve_go_name(
    name: &str,
    kinds: &[NodeKind],
    dirs: &[&str],
    context: &mut dyn ResolutionContext,
) -> Option<String> {
    // resolve_by_name_and_kind 接受 `/dir/` 片段，这里把 Go 风格目录名统一补斜杠。
    let preferred = dirs
        .iter()
        .map(|dir| format!("/{dir}/"))
        .collect::<Vec<_>>();
    let preferred_refs = preferred.iter().map(String::as_str).collect::<Vec<_>>();
    resolve_by_name_and_kind(name, kinds, &preferred_refs, context)
}

fn extract_go_tail_ident(expr: &str) -> Option<String> {
    // `pkg.Handler`, `Handler()` 都归约到最后的标识符。
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

fn is_ident(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some('_') | Some('A'..='Z') | Some('a'..='z'))
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn is_pascal_word(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some('A'..='Z')) && chars.all(|ch| ch.is_ascii_alphabetic())
}
