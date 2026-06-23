//! NestJS framework resolver translated from `nestjs.ts`.
//!
//! NestJS 的路由来自装饰器、GraphQL resolver、WebSocket gateway 和
//! RouterModule 前缀。这里用轻量字符串扫描补出 route 节点，而不是引入
//! TypeScript 语义分析。

use regex::Regex;

use crate::resolution::strip_comments::{CommentLang, strip_comments_for_regex};
use crate::resolution::types::{
    FrameworkExtractionResult, FrameworkResolver, ResolutionContext, ResolvedRef, UnresolvedRef,
    make_node, make_reference,
};
use crate::types::{Language, Node, NodeKind, ReferenceKind};

const HTTP_METHODS: &[&str] = &[
    "Get", "Post", "Put", "Patch", "Delete", "Head", "Options", "All",
];
const GQL_OPS: &[&str] = &["Query", "Mutation", "Subscription"];
const PROVIDER_CONVENTIONS: &[(&str, &str)] = &[
    ("Service", ".service."),
    ("Controller", ".controller."),
    ("Resolver", ".resolver."),
    ("Gateway", ".gateway."),
    ("Repository", ".repository."),
    ("Guard", ".guard."),
    ("Interceptor", ".interceptor."),
    ("Pipe", ".pipe."),
    ("Module", ".module."),
];

/// NestJS 解析入口：提取装饰器路由，并按 Nest 的文件命名约定解析 provider。
pub struct NestJsResolver;

pub const NESTJS_RESOLVER: NestJsResolver = NestJsResolver;

impl FrameworkResolver for NestJsResolver {
    fn name(&self) -> &'static str {
        "nestjs"
    }

    fn languages(&self) -> Option<&'static [Language]> {
        Some(&[Language::TypeScript, Language::JavaScript])
    }

    fn detect(&self, context: &mut dyn ResolutionContext) -> bool {
        if context
            .read_file("package.json")
            .is_some_and(|pkg| pkg.contains("\"@nestjs/") || pkg.contains("'@nestjs/"))
        {
            return true;
        }
        for file in context.get_all_files() {
            if !(file.ends_with(".controller.ts")
                || file.ends_with(".controller.js")
                || file.ends_with(".module.ts")
                || file.ends_with(".resolver.ts")
                || file.ends_with(".gateway.ts"))
            {
                continue;
            }
            if context.read_file(&file).is_some_and(|content| {
                content.contains("@nestjs/")
                    || content.contains("@Controller")
                    || content.contains("@Module(")
                    || content.contains("@Resolver(")
                    || content.contains("@WebSocketGateway(")
            }) {
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
        for (suffix, convention) in PROVIDER_CONVENTIONS {
            if !reference.reference_name.ends_with(suffix) {
                continue;
            }
            let candidates = context
                .get_nodes_by_name(&reference.reference_name)
                .into_iter()
                .filter(|node| node.kind == NodeKind::Class)
                .collect::<Vec<_>>();
            if candidates.is_empty() {
                return None;
            }
            let preferred = candidates
                .iter()
                .find(|node| node.file_path.contains(convention));
            let target = preferred.unwrap_or(&candidates[0]);
            return Some(ResolvedRef::framework(
                reference,
                target.id.clone(),
                if preferred.is_some() { 0.85 } else { 0.7 },
            ));
        }
        None
    }

    fn extract(&self, file_path: &str, content: &str) -> FrameworkExtractionResult {
        if !is_js_like(file_path) {
            return FrameworkExtractionResult::default();
        }
        let mut result = FrameworkExtractionResult::default();
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
        let scopes = build_class_scopes(content);

        for hit in find_decorators(content, HTTP_METHODS) {
            let prefix = scope_for(&scopes, hit.index)
                .filter(|scope| scope.kind == ClassKind::Controller)
                .map(|scope| scope.prefix.as_str())
                .unwrap_or("");
            let path = join_http_path(prefix, &parse_string_arg(&hit.args));
            add_route(
                &mut result,
                file_path,
                lang,
                hit.index,
                &hit.name.to_ascii_uppercase(),
                &path,
                method_name_after(content, hit.end).as_deref(),
            );
        }

        for hit in find_decorators(content, GQL_OPS) {
            if !scope_for(&scopes, hit.index).is_some_and(|scope| scope.kind == ClassKind::Resolver)
            {
                continue;
            }
            let handler = method_name_after(content, hit.end);
            let name = parse_graphql_name(&hit.args, handler.as_deref());
            add_route(
                &mut result,
                file_path,
                lang,
                hit.index,
                &hit.name.to_ascii_uppercase(),
                &name,
                handler.as_deref(),
            );
        }

        for hit in find_decorators(content, &["MessagePattern", "EventPattern"]) {
            let verb = if hit.name == "EventPattern" {
                "EVENT"
            } else {
                "MESSAGE"
            };
            let handler = method_name_after(content, hit.end);
            let path = parse_string_arg(&hit.args);
            let path = if path.is_empty() {
                handler.clone().unwrap_or_default()
            } else {
                path
            };
            add_route(
                &mut result,
                file_path,
                lang,
                hit.index,
                verb,
                &path,
                handler.as_deref(),
            );
        }

        for hit in find_decorators(content, &["SubscribeMessage"]) {
            let namespace = scope_for(&scopes, hit.index)
                .filter(|scope| scope.kind == ClassKind::Gateway)
                .map(|scope| scope.prefix.as_str())
                .unwrap_or("");
            let handler = method_name_after(content, hit.end);
            let event = parse_string_arg(&hit.args);
            let event = if event.is_empty() {
                handler.clone().unwrap_or_default()
            } else {
                event
            };
            let path = if namespace.is_empty() {
                event
            } else {
                format!("{namespace}:{event}")
            };
            add_route(
                &mut result,
                file_path,
                lang,
                hit.index,
                "WS",
                &path,
                handler.as_deref(),
            );
        }

        result
    }

    fn post_extract(&self, context: &mut dyn ResolutionContext) -> Vec<Node> {
        // RouterModule 的路径前缀和 controller 方法常分散在不同模块文件里。
        // post_extract 在所有文件入库后再回写 route 名称，避免单文件提取阶段
        // 因缺少模块上下文而漏掉父级路径。
        let mut module_to_prefix = std::collections::HashMap::<String, String>::new();
        let mut controller_to_module = std::collections::HashMap::<String, String>::new();
        for file_path in context.get_all_files() {
            if !file_path.contains(".module.") {
                continue;
            }
            if let Some(content) = context.read_file(&file_path) {
                let lang = detect_language(&file_path);
                let safe = strip_comments_for_regex(
                    &content,
                    if lang == Language::TypeScript {
                        CommentLang::TypeScript
                    } else {
                        CommentLang::JavaScript
                    },
                );
                collect_router_module_registrations(&safe, &mut module_to_prefix);
                collect_module_controllers(&safe, &mut controller_to_module);
            }
        }
        let mut updates = Vec::new();
        for (controller, module) in controller_to_module {
            let Some(prefix) = module_to_prefix.get(&module) else {
                continue;
            };
            if prefix.is_empty() || prefix == "/" {
                continue;
            }
            for class_node in context
                .get_nodes_by_name(&controller)
                .into_iter()
                .filter(|node| node.kind == NodeKind::Class)
            {
                for route in context
                    .get_nodes_in_file(&class_node.file_path)
                    .into_iter()
                    .filter(|node| node.kind == NodeKind::Route)
                {
                    if route.start_line < class_node.start_line
                        || route.start_line > class_node.end_line
                    {
                        continue;
                    }
                    if let Some(updated) = apply_module_prefix(&route, prefix)
                        && updated.name != route.name
                    {
                        updates.push(updated);
                    }
                }
            }
        }
        updates
    }
}

#[derive(Clone)]
struct DecoratorHit {
    name: String,
    args: String,
    index: usize,
    end: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ClassKind {
    Controller,
    Resolver,
    Gateway,
    Other,
}

struct ClassScope {
    kind: ClassKind,
    prefix: String,
    start: usize,
    end: usize,
}

fn add_route(
    result: &mut FrameworkExtractionResult,
    file_path: &str,
    language: Language,
    index: usize,
    method: &str,
    path: &str,
    handler: Option<&str>,
) {
    let line = line_at(file_path, index);
    let route_id = format!("route:{file_path}:{line}:{method}:{path}");
    result.nodes.push(make_node(
        route_id.clone(),
        NodeKind::Route,
        format!("{method} {path}"),
        format!("{file_path}::{method}:{path}"),
        file_path,
        language,
        line,
        None,
        None,
    ));
    if let Some(handler) = handler.filter(|value| !value.is_empty()) {
        result.references.push(make_reference(
            route_id,
            handler,
            ReferenceKind::References,
            line,
            0,
            file_path,
            language,
        ));
    }
}

fn find_decorators(source: &str, names: &[&str]) -> Vec<DecoratorHit> {
    // 装饰器参数可能包含嵌套对象或模板字符串，所以先定位 `@Name(`，
    // 再交给 read_args 做括号/引号平衡。
    let mut hits = Vec::new();
    let names_alt = names.join("|");
    let re = Regex::new(&format!(r"@({names_alt})\s*\(")).unwrap();
    for caps in re.captures_iter(source) {
        let whole = caps.get(0).unwrap();
        let open = whole.end() - 1;
        if let Some((args, end)) = read_args(source, open) {
            hits.push(DecoratorHit {
                name: caps.get(1).unwrap().as_str().to_string(),
                args,
                index: whole.start(),
                end,
            });
        }
    }
    hits
}

fn read_args(source: &str, open_index: usize) -> Option<(String, usize)> {
    // 这是小型平衡扫描器：只理解括号和字符串转义，足够覆盖装饰器参数，
    // 同时避免正则在嵌套对象上截断。
    if source.as_bytes().get(open_index) != Some(&b'(') {
        return None;
    }
    let mut depth = 0usize;
    let mut quote = None;
    let mut i = open_index;
    while i < source.len() {
        let ch = source[i..].chars().next()?;
        if let Some(q) = quote {
            if ch == '\\' {
                i += ch.len_utf8();
                if i < source.len() {
                    i += source[i..].chars().next()?.len_utf8();
                }
                continue;
            }
            if ch == q {
                quote = None;
            }
            i += ch.len_utf8();
            continue;
        }
        if matches!(ch, '"' | '\'' | '`') {
            quote = Some(ch);
        } else if ch == '(' {
            depth += 1;
        } else if ch == ')' {
            depth -= 1;
            if depth == 0 {
                return Some((source[open_index + 1..i].to_string(), i + 1));
            }
        }
        i += ch.len_utf8();
    }
    None
}

fn method_name_after(source: &str, start: usize) -> Option<String> {
    let tail = &source[start..(start + 800).min(source.len())];
    Regex::new(r"(?s)^\s*(?:@[\w.]+(?:\([^)]*\))?\s*)*(?:public\s+|private\s+|protected\s+|async\s+|static\s+)*([A-Za-z_$][\w$]*)\s*\(")
        .unwrap()
        .captures(tail)
        .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()))
}

fn build_class_scopes(source: &str) -> Vec<ClassScope> {
    // Nest 装饰器作用于紧随其后的 class。用相邻装饰器之间的范围近似 class
    // scope，足以把方法装饰器归到 Controller/Resolver/Gateway。
    let mut raw = Vec::<(ClassKind, String, usize)>::new();
    for hit in find_decorators(source, &["Controller"]) {
        raw.push((
            ClassKind::Controller,
            parse_controller_prefix(&hit.args),
            hit.index,
        ));
    }
    for hit in find_decorators(source, &["Resolver"]) {
        raw.push((ClassKind::Resolver, String::new(), hit.index));
    }
    for hit in find_decorators(source, &["WebSocketGateway"]) {
        raw.push((
            ClassKind::Gateway,
            parse_gateway_namespace(&hit.args),
            hit.index,
        ));
    }
    for hit in find_decorators(source, &["Injectable", "Module", "Catch"]) {
        raw.push((ClassKind::Other, String::new(), hit.index));
    }
    raw.sort_by_key(|(_, _, idx)| *idx);
    raw.iter()
        .enumerate()
        .map(|(idx, (kind, prefix, start))| ClassScope {
            kind: *kind,
            prefix: prefix.clone(),
            start: *start,
            end: raw
                .get(idx + 1)
                .map(|(_, _, next)| *next)
                .unwrap_or(source.len()),
        })
        .collect()
}

fn scope_for(scopes: &[ClassScope], index: usize) -> Option<&ClassScope> {
    scopes
        .iter()
        .find(|scope| index >= scope.start && index < scope.end)
}

fn parse_string_arg(args: &str) -> String {
    Regex::new(r#"['"`]([^'"`]*)['"`]"#)
        .unwrap()
        .captures(args)
        .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()))
        .unwrap_or_default()
}

fn parse_controller_prefix(args: &str) -> String {
    Regex::new(r#"path\s*:\s*['"`]([^'"`]*)['"`]"#)
        .unwrap()
        .captures(args)
        .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()))
        .unwrap_or_else(|| parse_string_arg(args))
}

fn parse_gateway_namespace(args: &str) -> String {
    Regex::new(r#"namespace\s*:\s*['"`]([^'"`]*)['"`]"#)
        .unwrap()
        .captures(args)
        .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()))
        .unwrap_or_default()
}

fn parse_graphql_name(args: &str, handler: Option<&str>) -> String {
    Regex::new(r#"name\s*:\s*['"`]([^'"`]*)['"`]"#)
        .unwrap()
        .captures(args)
        .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()))
        .or_else(|| {
            Regex::new(r#"^\s*['"`]([^'"`]*)['"`]"#)
                .unwrap()
                .captures(args)
                .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()))
        })
        .unwrap_or_else(|| handler.unwrap_or("").to_string())
}

fn join_http_path(prefix: &str, sub: &str) -> String {
    let parts = [prefix, sub]
        .into_iter()
        .map(|part| part.trim().trim_matches('/'))
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    format!("/{}", parts.join("/"))
}

fn line_at(source: &str, index: usize) -> u64 {
    (source[..index.min(source.len())]
        .bytes()
        .filter(|b| *b == b'\n')
        .count()
        + 1) as u64
}

fn detect_language(file_path: &str) -> Language {
    if file_path.ends_with(".ts") || file_path.ends_with(".tsx") {
        Language::TypeScript
    } else {
        Language::JavaScript
    }
}

fn is_js_like(file_path: &str) -> bool {
    [".js", ".mjs", ".cjs", ".ts", ".tsx"]
        .iter()
        .any(|ext| file_path.ends_with(ext))
}

fn collect_router_module_registrations(
    source: &str,
    out: &mut std::collections::HashMap<String, String>,
) {
    // RouterModule 支持嵌套路由树；这里只记录 module -> prefix，
    // route 节点仍由 controller 文件本身提取。
    let re = Regex::new(r"\bRouterModule\s*\.\s*(?:register|forRoot|forChild)\s*\(").unwrap();
    for mat in re.find_iter(source) {
        let open = mat.end() - 1;
        let Some((args, _)) = read_args(source, open) else {
            continue;
        };
        let items = parse_routes_array(&args);
        walk_routes_tree(&items, "", out);
    }
}

struct RouteItem {
    path: String,
    module_name: Option<String>,
    children: Vec<RouteItem>,
}

fn parse_routes_array(args: &str) -> Vec<RouteItem> {
    let trimmed = args.trim();
    if !trimmed.starts_with('[') {
        return Vec::new();
    }
    let Some(close) = matching_close(trimmed, 0) else {
        return Vec::new();
    };
    parse_route_objects(&trimmed[1..close])
}

fn parse_route_objects(source: &str) -> Vec<RouteItem> {
    split_top_level_objects(source)
        .into_iter()
        .map(|object| {
            let children = parse_array_field(&object, "children")
                .map(|inner| parse_route_objects(&inner))
                .unwrap_or_default();
            RouteItem {
                path: parse_string_field(&object, "path"),
                module_name: parse_ident_field(&object, "module"),
                children,
            }
        })
        .collect()
}

fn walk_routes_tree(
    items: &[RouteItem],
    parent_prefix: &str,
    out: &mut std::collections::HashMap<String, String>,
) {
    for item in items {
        let prefix = join_http_path(parent_prefix, &item.path);
        if let Some(module_name) = &item.module_name {
            out.entry(module_name.clone()).or_insert(prefix.clone());
        }
        if !item.children.is_empty() {
            walk_routes_tree(&item.children, &prefix, out);
        }
    }
}

fn collect_module_controllers(source: &str, out: &mut std::collections::HashMap<String, String>) {
    for hit in find_decorators(source, &["Module"]) {
        let Some(class_name) = class_name_after(source, hit.end) else {
            continue;
        };
        if let Some(inner) = parse_array_field(&hit.args, "controllers") {
            for controller in inner
                .split(',')
                .map(str::trim)
                .filter(|item| is_ident(item))
            {
                out.entry(controller.to_string())
                    .or_insert(class_name.clone());
            }
        }
    }
}

fn class_name_after(source: &str, start: usize) -> Option<String> {
    let tail = &source[start..(start + 800).min(source.len())];
    Regex::new(r"(?s)^\s*(?:@[\w.]+(?:\([^)]*\))?\s*)*(?:export\s+)?(?:default\s+)?(?:abstract\s+)?class\s+([A-Za-z_$][\w$]*)")
        .unwrap()
        .captures(tail)
        .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()))
}

fn parse_array_field(object: &str, name: &str) -> Option<String> {
    let re = Regex::new(&format!(r"(?:^|[,{{\s]){}\s*:\s*\[", regex::escape(name))).unwrap();
    let mat = re.find(object)?;
    let open = mat.end() - 1;
    let close = matching_close(object, open)?;
    Some(object[open + 1..close].to_string())
}

fn split_top_level_objects(source: &str) -> Vec<String> {
    // routes 数组里可能有 children 嵌套对象；只在顶层切对象，避免把子路由
    // 提前拆碎。
    let chars = source.char_indices().collect::<Vec<_>>();
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut object_start: Option<usize> = None;
    let mut quote: Option<char> = None;
    let mut pos = 0usize;
    while pos < chars.len() {
        let (idx, ch) = chars[pos];
        if let Some(q) = quote {
            if ch == '\\' {
                pos += 2;
                continue;
            }
            if ch == q {
                quote = None;
            }
            pos += 1;
            continue;
        }
        if matches!(ch, '"' | '\'' | '`') {
            quote = Some(ch);
            pos += 1;
            continue;
        }
        if depth == 0 && ch == '{' {
            depth = 1;
            object_start = Some(idx);
            pos += 1;
            continue;
        }
        if matches!(ch, '{' | '[' | '(') {
            depth += 1;
        } else if matches!(ch, '}' | ']' | ')') {
            depth = depth.saturating_sub(1);
            if depth == 0
                && ch == '}'
                && let Some(start) = object_start.take()
            {
                out.push(source[start + 1..idx].to_string());
            }
        }
        pos += 1;
    }
    out
}

fn parse_string_field(object: &str, name: &str) -> String {
    Regex::new(&format!(
        r#"(?:^|[,{{\s]){}\s*:\s*['"`]([^'"`]*)['"`]"#,
        regex::escape(name)
    ))
    .unwrap()
    .captures(object)
    .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()))
    .unwrap_or_default()
}

fn parse_ident_field(object: &str, name: &str) -> Option<String> {
    Regex::new(&format!(
        r"(?:^|[,{{\s]){}\s*:\s*([A-Za-z_$][\w$]*)",
        regex::escape(name)
    ))
    .unwrap()
    .captures(object)
    .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()))
}

fn matching_close(source: &str, open: usize) -> Option<usize> {
    let opener = source.as_bytes().get(open).copied()?;
    if !matches!(opener, b'[' | b'{' | b'(') {
        return None;
    }
    let mut depth = 0usize;
    let mut quote = None;
    let mut i = open;
    while i < source.len() {
        let ch = source[i..].chars().next()?;
        if let Some(q) = quote {
            if ch == '\\' {
                i += ch.len_utf8();
                if i < source.len() {
                    i += source[i..].chars().next()?.len_utf8();
                }
                continue;
            }
            if ch == q {
                quote = None;
            }
            i += ch.len_utf8();
            continue;
        }
        if matches!(ch, '"' | '\'' | '`') {
            quote = Some(ch);
        } else if matches!(ch, '[' | '{' | '(') {
            depth += 1;
        } else if matches!(ch, ']' | '}' | ')') {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
        i += ch.len_utf8();
    }
    None
}

fn apply_module_prefix(route: &Node, prefix: &str) -> Option<Node> {
    let (_, tail) = route.qualified_name.split_once("::")?;
    let (method, original) = tail.split_once(':')?;
    let mut updated = route.clone();
    updated.name = format!("{method} {}", join_http_path(prefix, original));
    updated.updated_at = crate::resolution::types::now_ms();
    Some(updated)
}

fn is_ident(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(
        chars.next(),
        Some('_') | Some('$') | Some('A'..='Z') | Some('a'..='z')
    ) && chars.all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
}
