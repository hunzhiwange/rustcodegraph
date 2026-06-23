//! Rust web-framework resolver translated from `rust.ts`.
//!
//! Rust Web 框架把路由挂在属性宏或 builder 链上。这里覆盖 actix/axum 常见写法，
//! 并按 handler/service/model 目录约定补静态引用。

use regex::Regex;

use crate::resolution::frameworks::cargo_workspace::get_cargo_workspace_crate_map;
use crate::resolution::strip_comments::{CommentLang, strip_comments_for_regex};
use crate::resolution::types::{
    FrameworkExtractionResult, FrameworkResolver, ResolutionContext, ResolvedRef, UnresolvedRef,
    line_for_byte, make_node, make_reference, resolve_by_name_and_kind,
};
use crate::types::{Language, NodeKind, ReferenceKind};

const HANDLER_DIRS: &[&str] = &[
    "/handlers/",
    "/handler/",
    "/api/",
    "/routes/",
    "/controllers/",
];
const SERVICE_DIRS: &[&str] = &["/services/", "/service/", "/repository/", "/domain/"];
const MODEL_DIRS: &[&str] = &[
    "/models/",
    "/model/",
    "/entities/",
    "/entity/",
    "/domain/",
    "/types/",
];
const FUNCTION_KINDS: &[NodeKind] = &[NodeKind::Function];
const SERVICE_KINDS: &[NodeKind] = &[NodeKind::Struct, NodeKind::Trait];
const STRUCT_KINDS: &[NodeKind] = &[NodeKind::Struct];

pub struct RustResolver;

pub const RUST_RESOLVER: RustResolver = RustResolver;

impl FrameworkResolver for RustResolver {
    fn name(&self) -> &'static str {
        "rust"
    }

    fn languages(&self) -> Option<&'static [Language]> {
        Some(&[Language::Rust])
    }

    fn detect(&self, context: &mut dyn ResolutionContext) -> bool {
        context.file_exists("Cargo.toml")
    }

    fn resolve(
        &self,
        reference: &UnresolvedRef,
        context: &mut dyn ResolutionContext,
    ) -> Option<ResolvedRef> {
        if (reference.reference_name.ends_with("_handler")
            || reference.reference_name.starts_with("handle_"))
            && let Some(target) = resolve_by_name_and_kind(
                &reference.reference_name,
                FUNCTION_KINDS,
                HANDLER_DIRS,
                context,
            )
        {
            return Some(ResolvedRef::framework(reference, target, 0.8));
        }
        if (reference.reference_name.ends_with("Service")
            || reference.reference_name.ends_with("Repository"))
            && let Some(target) = resolve_by_name_and_kind(
                &reference.reference_name,
                SERVICE_KINDS,
                SERVICE_DIRS,
                context,
            )
        {
            return Some(ResolvedRef::framework(reference, target, 0.8));
        }
        if is_pascal_word(&reference.reference_name)
            && let Some(target) = resolve_by_name_and_kind(
                &reference.reference_name,
                STRUCT_KINDS,
                MODEL_DIRS,
                context,
            )
        {
            return Some(ResolvedRef::framework(reference, target, 0.7));
        }
        if is_snake_module(&reference.reference_name)
            && let Some(module) = resolve_module(&reference.reference_name, context)
        {
            return Some(ResolvedRef::framework(
                reference,
                module.target_id,
                if module.from_workspace { 0.95 } else { 0.6 },
            ));
        }
        None
    }

    fn extract(&self, file_path: &str, content: &str) -> FrameworkExtractionResult {
        if !file_path.ends_with(".rs") {
            return FrameworkExtractionResult::default();
        }
        // 先处理 actix-web 属性宏，再扫描 axum/actix builder 链；三种入口都会
        // 生成同一种 route -> handler 边。
        let safe = strip_comments_for_regex(content, CommentLang::Rust);
        let content = safe.as_str();
        let mut result = FrameworkExtractionResult::default();

        let attr_re = Regex::new(
            r#"#\[(get|post|put|patch|delete|head|options)\s*\(\s*["']([^"']+)["'][^\]]*\)\]"#,
        )
        .unwrap();
        let fn_re = Regex::new(r"\n\s*(?:pub\s+)?(?:async\s+)?fn\s+(\w+)").unwrap();
        for caps in attr_re.captures_iter(content) {
            let whole = caps.get(0).unwrap();
            let method = caps.get(1).unwrap().as_str().to_ascii_uppercase();
            let route_path = caps.get(2).unwrap().as_str();
            let line = line_for_byte(content, whole.start());
            let route_id = format!("route:{file_path}:{line}:{method}:{route_path}");
            result.nodes.push(make_node(
                route_id.clone(),
                NodeKind::Route,
                format!("{method} {route_path}"),
                format!("{file_path}::route:{route_path}"),
                file_path,
                Language::Rust,
                line,
                None,
                None,
            ));
            if let Some(fn_caps) = fn_re.captures(&content[whole.end()..]) {
                result.references.push(make_reference(
                    route_id,
                    fn_caps.get(1).unwrap().as_str(),
                    ReferenceKind::References,
                    line,
                    0,
                    file_path,
                    Language::Rust,
                ));
            }
        }

        extract_axum_routes(file_path, content, &mut result);
        extract_actix_routes(file_path, content, &mut result);
        result
    }
}

fn extract_axum_routes(file_path: &str, content: &str, result: &mut FrameworkExtractionResult) {
    // axum 的 `.route("...", get(handler).post(...))` 需要先找到完整参数列表，
    // 再在第二个参数里提取 HTTP method 与 handler。
    let route_open_re = Regex::new(r"\.route\s*\(").unwrap();
    let method_re =
        Regex::new(r"\b(get|post|put|patch|delete|head|options|trace)\s*\(\s*([A-Za-z_][\w:]*)")
            .unwrap();
    for mat in route_open_re.find_iter(content) {
        let open_idx = content[mat.start()..]
            .find('(')
            .map(|idx| mat.start() + idx)
            .unwrap_or(mat.start());
        let Some(close_idx) = find_matching_paren(content, open_idx) else {
            continue;
        };
        let args = &content[open_idx + 1..close_idx];
        let Some(path_end) = args
            .trim_start()
            .strip_prefix('"')
            .and_then(|rest| rest.find('"').map(|idx| idx + 1))
        else {
            continue;
        };
        let trimmed = args.trim_start();
        let route_path = &trimmed[1..path_end];
        let line = line_for_byte(content, mat.start());
        let method_body =
            trimmed[path_end + 1..].trim_start_matches(|ch| ch == ',' || char::is_whitespace(ch));
        for caps in method_re.captures_iter(method_body) {
            let method = caps.get(1).unwrap().as_str().to_ascii_uppercase();
            let handler = caps
                .get(2)
                .unwrap()
                .as_str()
                .split("::")
                .filter(|part| !part.is_empty())
                .last()
                .unwrap()
                .to_string();
            push_route(result, file_path, &method, route_path, &handler, line);
        }
    }
}

fn extract_actix_routes(file_path: &str, content: &str, result: &mut FrameworkExtractionResult) {
    // actix 同时支持 `web::resource(...).route(...to(handler))` 和
    // `.route(path, web::get().to(handler))`，这里分别扫描两种链式形态。
    let resource_re = Regex::new(r#"web::resource\s*\(\s*"([^"]+)"\s*\)"#).unwrap();
    let method_to_re = Regex::new(
        r"web::(get|post|put|patch|delete|head)\s*\(\s*\)\s*\.to\s*\(\s*([A-Za-z_][\w:]*)",
    )
    .unwrap();
    let direct_re = Regex::new(r"^\s*\.to\s*\(\s*([A-Za-z_][\w:]*)").unwrap();
    for caps in resource_re.captures_iter(content) {
        let whole = caps.get(0).unwrap();
        let route_path = caps.get(1).unwrap().as_str();
        let start_line = line_for_byte(content, whole.start());
        let after = whole.end();
        let next_res = content[after..]
            .find("web::resource")
            .map(|idx| after + idx)
            .unwrap_or(content.len());
        let end = (after + 500).min(next_res);
        let chain = &content[after..end];
        let mut found = false;
        for method_caps in method_to_re.captures_iter(chain) {
            let line = start_line + line_for_byte(chain, method_caps.get(0).unwrap().start()) - 1;
            push_route(
                result,
                file_path,
                &method_caps.get(1).unwrap().as_str().to_ascii_uppercase(),
                route_path,
                method_caps
                    .get(2)
                    .unwrap()
                    .as_str()
                    .rsplit("::")
                    .next()
                    .unwrap_or(""),
                line,
            );
            found = true;
        }
        if !found && let Some(direct) = direct_re.captures(chain) {
            push_route(
                result,
                file_path,
                "ANY",
                route_path,
                direct
                    .get(1)
                    .unwrap()
                    .as_str()
                    .rsplit("::")
                    .next()
                    .unwrap_or(""),
                start_line,
            );
        }
    }

    let app_route_re = Regex::new(r#"\.route\s*\(\s*"([^"]+)"\s*,\s*web::(get|post|put|patch|delete|head)\s*\(\s*\)\s*\.to\s*\(\s*([A-Za-z_][\w:]*)"#).unwrap();
    for caps in app_route_re.captures_iter(content) {
        let line = line_for_byte(content, caps.get(0).unwrap().start());
        push_route(
            result,
            file_path,
            &caps.get(2).unwrap().as_str().to_ascii_uppercase(),
            caps.get(1).unwrap().as_str(),
            caps.get(3)
                .unwrap()
                .as_str()
                .rsplit("::")
                .next()
                .unwrap_or(""),
            line,
        );
    }
}

fn push_route(
    result: &mut FrameworkExtractionResult,
    file_path: &str,
    method: &str,
    route_path: &str,
    handler: &str,
    line: u64,
) {
    if handler.is_empty() {
        return;
    }
    let route_id = format!("route:{file_path}:{line}:{method}:{route_path}");
    result.nodes.push(make_node(
        route_id.clone(),
        NodeKind::Route,
        format!("{method} {route_path}"),
        format!("{file_path}::route:{route_path}"),
        file_path,
        Language::Rust,
        line,
        None,
        None,
    ));
    result.references.push(make_reference(
        route_id,
        handler,
        ReferenceKind::References,
        line,
        0,
        file_path,
        Language::Rust,
    ));
}

fn find_matching_paren(source: &str, open_idx: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (idx, ch) in source.char_indices().skip_while(|(idx, _)| *idx < open_idx) {
        if ch == '(' {
            depth += 1;
        } else if ch == ')' {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return Some(idx);
            }
        }
    }
    None
}

struct ModuleResolution {
    target_id: String,
    from_workspace: bool,
}

fn resolve_module(name: &str, context: &mut dyn ResolutionContext) -> Option<ModuleResolution> {
    // 模块名解析既要看当前 crate 的 `src/foo.rs` / `src/foo/mod.rs`，也要看
    // workspace member crate；workspace 命中给更高置信度。
    let workspace_crates = get_cargo_workspace_crate_map(context);
    let crate_path = workspace_crates.get(name);
    let mut candidates = vec![
        (format!("src/{name}.rs"), false),
        (format!("src/{name}/mod.rs"), false),
    ];
    if let Some(crate_path) = crate_path {
        candidates.push((format!("{crate_path}/src/lib.rs"), true));
        candidates.push((format!("{crate_path}/src/main.rs"), true));
    }
    for (path, from_workspace) in candidates {
        if !context.file_exists(&path) {
            continue;
        }
        let nodes = context.get_nodes_in_file(&path);
        if let Some(module) = nodes.iter().find(|node| node.kind == NodeKind::Module) {
            return Some(ModuleResolution {
                target_id: module.id.clone(),
                from_workspace,
            });
        }
        if let Some(first) = nodes.first() {
            return Some(ModuleResolution {
                target_id: first.id.clone(),
                from_workspace,
            });
        }
    }
    None
}

fn is_pascal_word(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some('A'..='Z')) && chars.all(|ch| ch.is_ascii_alphabetic())
}

fn is_snake_module(value: &str) -> bool {
    !value.is_empty() && value.chars().all(|ch| ch == '_' || ch.is_ascii_lowercase())
}
