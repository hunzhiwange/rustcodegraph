//! Laravel framework resolver translated from `laravel.ts`.
//!
//! 这个 resolver 负责把 Laravel 路由和常见模型/控制器约定补成图边，避免
//! `Route::get(..., [Controller::class, "..."])` 这类动态入口在静态索引里断开。

use regex::Regex;

use crate::resolution::strip_comments::{CommentLang, strip_comments_for_regex};
use crate::resolution::types::{
    FrameworkExtractionResult, FrameworkResolver, ResolutionContext, ResolvedRef, UnresolvedRef,
    line_for_byte, make_node, make_reference,
};
use crate::types::{Language, NodeKind, ReferenceKind};

pub const FACADE_MAPPINGS: &[(&str, &str)] = &[
    ("Auth", r"Illuminate\Auth\AuthManager"),
    ("Cache", r"Illuminate\Cache\CacheManager"),
    ("Config", r"Illuminate\Config\Repository"),
    ("DB", r"Illuminate\Database\DatabaseManager"),
    ("Event", r"Illuminate\Events\Dispatcher"),
    ("File", r"Illuminate\Filesystem\Filesystem"),
    ("Gate", r"Illuminate\Auth\Access\Gate"),
    ("Hash", r"Illuminate\Hashing\HashManager"),
    ("Log", r"Illuminate\Log\LogManager"),
    ("Mail", r"Illuminate\Mail\Mailer"),
    ("Queue", r"Illuminate\Queue\QueueManager"),
    ("Redis", r"Illuminate\Redis\RedisManager"),
    ("Request", r"Illuminate\Http\Request"),
    ("Response", r"Illuminate\Http\Response"),
    ("Route", r"Illuminate\Routing\Router"),
    ("Session", r"Illuminate\Session\SessionManager"),
    ("Storage", r"Illuminate\Filesystem\FilesystemManager"),
    ("URL", r"Illuminate\Routing\UrlGenerator"),
    ("Validator", r"Illuminate\Validation\Factory"),
    ("View", r"Illuminate\View\Factory"),
];

/// Laravel 解析入口：只处理 PHP 文件，并优先使用框架目录约定做高置信匹配。
pub struct LaravelResolver;

pub const LARAVEL_RESOLVER: LaravelResolver = LaravelResolver;

impl FrameworkResolver for LaravelResolver {
    fn name(&self) -> &'static str {
        "laravel"
    }

    fn languages(&self) -> Option<&'static [Language]> {
        Some(&[Language::Php])
    }

    fn detect(&self, context: &mut dyn ResolutionContext) -> bool {
        context.file_exists("artisan") || context.file_exists("app/Http/Kernel.php")
    }

    fn claims_reference(&self, name: &str) -> bool {
        let Some((controller, method)) = name.split_once('@') else {
            return false;
        };
        controller.ends_with("Controller") && is_ident(controller) && is_ident(method)
    }

    fn resolve(
        &self,
        reference: &UnresolvedRef,
        context: &mut dyn ResolutionContext,
    ) -> Option<ResolvedRef> {
        // `Model::method()` 和 facade 语法长得一样；facade 由框架自身承载，
        // 这里刻意排除，避免把 `Cache::get` 误连到用户模型。
        if let Some((class_name, method_name)) = reference.reference_name.split_once("::") {
            if starts_upper(class_name)
                && !is_laravel_facade(class_name)
                && let Some(target) = resolve_model_call(class_name, method_name, context)
            {
                return Some(ResolvedRef::framework(reference, target, 0.85));
            }
            return None;
        }

        if let Some((controller, method)) = reference.reference_name.split_once('@')
            && controller.ends_with("Controller")
            && let Some(target) = resolve_controller_method(controller, method, context)
        {
            return Some(ResolvedRef::framework(reference, target, 0.9));
        }

        None
    }

    fn extract(&self, file_path: &str, content: &str) -> FrameworkExtractionResult {
        if !file_path.ends_with(".php") {
            return FrameworkExtractionResult::default();
        }
        let safe = strip_comments_for_regex(content, CommentLang::Php);
        let content = safe.as_str();
        let mut result = FrameworkExtractionResult::default();
        let route_re = Regex::new(r#"Route::(get|post|put|patch|delete|options|any)\s*\(\s*['"]([^'"]+)['"]\s*,\s*([^)]+)\)"#).unwrap();
        for caps in route_re.captures_iter(content) {
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
                Language::Php,
                line,
                None,
                None,
            ));
            if let Some(handler) = extract_laravel_handler(caps.get(3).unwrap().as_str()) {
                result.references.push(make_reference(
                    route_id,
                    handler,
                    ReferenceKind::References,
                    line,
                    0,
                    file_path,
                    Language::Php,
                ));
            }
        }

        let resource_re = Regex::new(
            r#"Route::(resource|apiResource)\s*\(\s*['"]([^'"]+)['"]\s*(?:,\s*([^)]+))?\)"#,
        )
        .unwrap();
        for caps in resource_re.captures_iter(content) {
            let whole = caps.get(0).unwrap();
            let resource_name = caps.get(2).unwrap().as_str();
            let line = line_for_byte(content, whole.start());
            let route_id = format!("route:{file_path}:{line}:RESOURCE:{resource_name}");
            result.nodes.push(make_node(
                route_id.clone(),
                NodeKind::Route,
                format!("resource:{resource_name}"),
                format!("{file_path}::route:{resource_name}"),
                file_path,
                Language::Php,
                line,
                None,
                None,
            ));
            if let Some(handler_expr) = caps.get(3)
                && let Some(controller) = extract_laravel_handler(handler_expr.as_str())
            {
                result.references.push(make_reference(
                    route_id,
                    controller,
                    ReferenceKind::Imports,
                    line,
                    0,
                    file_path,
                    Language::Php,
                ));
            }
        }

        result
    }
}

fn extract_laravel_handler(expr: &str) -> Option<String> {
    // Laravel 接受数组 handler、"Controller@method" 和 class-only 三种写法。
    // 统一成短类名引用后，后续 resolver 才能按控制器文件定位方法。
    let trimmed = expr.trim();
    let tuple_re =
        Regex::new(r#"^\[\s*([A-Za-z_\\][\w\\]*)::class\s*,\s*['"]([^'"]+)['"]\s*\]"#).unwrap();
    if let Some(caps) = tuple_re.captures(trimmed) {
        return Some(format!(
            "{}@{}",
            short_php_name(caps.get(1).unwrap().as_str()),
            caps.get(2).unwrap().as_str()
        ));
    }

    let at_re = Regex::new(r#"^['"]([^'"@]+)@([^'"]+)['"]$"#).unwrap();
    if let Some(caps) = at_re.captures(trimmed) {
        return Some(format!(
            "{}@{}",
            short_php_name(caps.get(1).unwrap().as_str()),
            caps.get(2).unwrap().as_str()
        ));
    }

    let class_re = Regex::new(r"^([A-Za-z_\\][\w\\]*)::class").unwrap();
    class_re
        .captures(trimmed)
        .map(|caps| short_php_name(caps.get(1).unwrap().as_str()))
}

fn short_php_name(value: &str) -> String {
    value.rsplit('\\').next().unwrap_or(value).to_string()
}

fn resolve_model_call(
    class_name: &str,
    method_name: &str,
    context: &mut dyn ResolutionContext,
) -> Option<String> {
    for model_path in [
        format!("app/Models/{class_name}.php"),
        format!("app/{class_name}.php"),
    ] {
        if !context.file_exists(&model_path) {
            continue;
        }
        let nodes = context.get_nodes_in_file(&model_path);
        if let Some(method) = nodes
            .iter()
            .find(|node| node.kind == NodeKind::Method && node.name == method_name)
        {
            return Some(method.id.clone());
        }
        if let Some(class_node) = nodes
            .iter()
            .find(|node| node.kind == NodeKind::Class && node.name == class_name)
        {
            return Some(class_node.id.clone());
        }
    }
    None
}

fn resolve_controller_method(
    controller: &str,
    method: &str,
    context: &mut dyn ResolutionContext,
) -> Option<String> {
    // 先命中默认目录，失败后再按类名搜索；这样保留 Rails/Laravel 风格项目
    // 中最常见路径的确定性，同时兼容自定义命名空间。
    let controller_path = format!("app/Http/Controllers/{controller}.php");
    if context.file_exists(&controller_path)
        && let Some(method_node) = context
            .get_nodes_in_file(&controller_path)
            .into_iter()
            .find(|node| node.kind == NodeKind::Method && node.name == method)
    {
        return Some(method_node.id);
    }
    for ctrl in context.get_nodes_by_name(controller) {
        if ctrl.kind == NodeKind::Class
            && ctrl.file_path.contains("Controllers")
            && let Some(method_node) = context
                .get_nodes_in_file(&ctrl.file_path)
                .into_iter()
                .find(|node| node.kind == NodeKind::Method && node.name == method)
        {
            return Some(method_node.id);
        }
    }
    None
}

fn is_laravel_facade(name: &str) -> bool {
    FACADE_MAPPINGS.iter().any(|(facade, _)| *facade == name)
}

fn starts_upper(value: &str) -> bool {
    value
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_uppercase())
}

fn is_ident(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some('_') | Some('A'..='Z') | Some('a'..='z'))
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}
