//! Ruby on Rails framework resolver translated from `ruby.ts`.
//!
//! Rails 依赖 routes DSL、控制器命名和 app/ 目录约定。这里把 routes.rb 的
//! 显式/RESTful 路由补成 route -> controller#action 引用。

use regex::Regex;

use crate::resolution::strip_comments::{CommentLang, strip_comments_for_regex};
use crate::resolution::types::{
    FrameworkExtractionResult, FrameworkResolver, ResolutionContext, ResolvedRef, UnresolvedRef,
    make_node, make_reference,
};
use crate::types::{Language, NodeKind, ReferenceKind};

const PLURAL_ACTIONS: &[&str] = &[
    "index", "create", "new", "show", "edit", "update", "destroy",
];
const SINGULAR_ACTIONS: &[&str] = &["create", "new", "show", "edit", "update", "destroy"];

pub struct RailsResolver;

pub const RAILS_RESOLVER: RailsResolver = RailsResolver;

impl FrameworkResolver for RailsResolver {
    fn name(&self) -> &'static str {
        "rails"
    }

    fn languages(&self) -> Option<&'static [Language]> {
        Some(&[Language::Ruby])
    }

    fn claims_reference(&self, name: &str) -> bool {
        let Some((controller, action)) = name.split_once('#') else {
            return false;
        };
        !controller.is_empty() && is_ident(action)
    }

    fn detect(&self, context: &mut dyn ResolutionContext) -> bool {
        context
            .read_file("Gemfile")
            .is_some_and(|gemfile| gemfile.contains("'rails'") || gemfile.contains("\"rails\""))
            || context.file_exists("config/application.rb")
            || context.file_exists("app/controllers/application_controller.rb")
            || context.file_exists("config/routes.rb")
    }

    fn resolve(
        &self,
        reference: &UnresolvedRef,
        context: &mut dyn ResolutionContext,
    ) -> Option<ResolvedRef> {
        if let Some((controller_path, action)) = reference.reference_name.split_once('#') {
            if let Some(target) = resolve_controller_action(controller_path, action, context) {
                return Some(ResolvedRef::framework(reference, target, 0.85));
            }
            return None;
        }
        if is_pascal_word(&reference.reference_name)
            && let Some(target) = resolve_model(&reference.reference_name, context)
        {
            return Some(ResolvedRef::framework(reference, target, 0.8));
        }
        if reference.reference_name.ends_with("Controller")
            && let Some(target) = resolve_controller(&reference.reference_name, context)
        {
            return Some(ResolvedRef::framework(reference, target, 0.85));
        }
        if reference.reference_name.ends_with("Helper")
            && let Some(target) = resolve_helper(&reference.reference_name, context)
        {
            return Some(ResolvedRef::framework(reference, target, 0.8));
        }
        if (reference.reference_name.ends_with("Service")
            || reference.reference_name.ends_with("Job"))
            && let Some(target) = resolve_service(&reference.reference_name, context)
        {
            return Some(ResolvedRef::framework(reference, target, 0.8));
        }
        None
    }

    fn extract(&self, file_path: &str, content: &str) -> FrameworkExtractionResult {
        if !file_path.ends_with(".rb") {
            return FrameworkExtractionResult::default();
        }
        // Rails routes DSL 允许很多元编程写法；这里覆盖最常见的直接 route 和
        // resources，宁可漏掉复杂 DSL，也不生成错误 handler。
        let safe = strip_comments_for_regex(content, CommentLang::Ruby);
        let content = safe.as_str();
        let mut result = FrameworkExtractionResult::default();
        let route_re = Regex::new(r#"\b(get|post|put|patch|delete|match)\s+['"]([^'"]+)['"]\s*(?:,\s*to:\s*|=>\s*)['"]([^#'"]+)#([^'"]+)['"]"#).unwrap();
        for caps in route_re.captures_iter(content) {
            let whole = caps.get(0).unwrap();
            let method = caps.get(1).unwrap().as_str().to_ascii_uppercase();
            let route_path = caps.get(2).unwrap().as_str();
            let ctrl = caps.get(3).unwrap().as_str();
            let action = caps.get(4).unwrap().as_str();
            let line = line_for_offset(content, whole.start());
            let route_id = format!("route:{file_path}:{line}:{method}:{route_path}");
            result.nodes.push(make_node(
                route_id.clone(),
                NodeKind::Route,
                format!("{method} {route_path}"),
                format!("{file_path}::route:{route_path}"),
                file_path,
                Language::Ruby,
                line,
                None,
                None,
            ));
            result.references.push(make_reference(
                route_id,
                format!("{ctrl}#{action}"),
                ReferenceKind::References,
                line,
                0,
                file_path,
                Language::Ruby,
            ));
        }

        let res_re = Regex::new(r"\b(resources?)\s+:(\w+)([^\n]*)").unwrap();
        for caps in res_re.captures_iter(content) {
            let whole = caps.get(0).unwrap();
            let plural = caps.get(1).unwrap().as_str() == "resources";
            let res_name = caps.get(2).unwrap().as_str();
            let tail = caps.get(3).map(|m| m.as_str()).unwrap_or("");
            let mut actions = if plural {
                PLURAL_ACTIONS.to_vec()
            } else {
                SINGULAR_ACTIONS.to_vec()
            };
            if let Some(only) = list_option(tail, "only") {
                actions.retain(|action| only.iter().any(|item| item == action));
            } else if let Some(except) = list_option(tail, "except") {
                actions.retain(|action| !except.iter().any(|item| item == action));
            }
            let ctrl = if plural {
                res_name.to_string()
            } else {
                pluralize(res_name)
            };
            let line = line_for_offset(content, whole.start());
            for action in actions {
                let (method, path) = restful_route(action, res_name);
                let route_id = format!("route:{file_path}:{line}:{method}:{ctrl}#{action}");
                result.nodes.push(make_node(
                    route_id.clone(),
                    NodeKind::Route,
                    format!("{method} {path}"),
                    format!("{file_path}::route:{ctrl}#{action}"),
                    file_path,
                    Language::Ruby,
                    line,
                    None,
                    None,
                ));
                result.references.push(make_reference(
                    route_id,
                    format!("{ctrl}#{action}"),
                    ReferenceKind::References,
                    line,
                    0,
                    file_path,
                    Language::Ruby,
                ));
            }
        }

        result
    }
}

fn restful_route(action: &str, resource: &str) -> (&'static str, String) {
    match action {
        "index" => ("GET", format!("/{resource}")),
        "create" => ("POST", format!("/{resource}")),
        "new" => ("GET", format!("/{resource}/new")),
        "show" => ("GET", format!("/{resource}/:id")),
        "edit" => ("GET", format!("/{resource}/:id/edit")),
        "update" => ("PATCH", format!("/{resource}/:id")),
        "destroy" => ("DELETE", format!("/{resource}/:id")),
        _ => ("GET", format!("/{resource}")),
    }
}

fn pluralize(word: &str) -> String {
    if word.ends_with('y')
        && !matches!(word.chars().rev().nth(1), Some('a' | 'e' | 'i' | 'o' | 'u'))
    {
        format!("{}ies", &word[..word.len() - 1])
    } else if ["s", "x", "z", "ch", "sh"]
        .iter()
        .any(|suffix| word.ends_with(suffix))
    {
        format!("{word}es")
    } else {
        format!("{word}s")
    }
}

fn camelize(value: &str) -> String {
    value
        .split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect::<String>()
}

fn resolve_controller_action(
    ctrl_path: &str,
    action: &str,
    context: &mut dyn ResolutionContext,
) -> Option<String> {
    // `users#index` 首先映射到 app/controllers/users_controller.rb；如果项目使用
    // 命名空间或非标准路径，再回退到已索引类名搜索。
    let direct = format!("app/controllers/{ctrl_path}_controller.rb");
    if context.file_exists(&direct)
        && let Some(method) = find_method_in_file(&direct, action, context)
    {
        return Some(method);
    }
    let cls = format!(
        "{}Controller",
        camelize(ctrl_path.rsplit('/').next().unwrap_or(ctrl_path))
    );
    for ctrl in context
        .get_nodes_by_name(&cls)
        .into_iter()
        .filter(|node| node.kind == NodeKind::Class)
    {
        if let Some(method) = find_method_in_file(&ctrl.file_path, action, context) {
            return Some(method);
        }
    }
    None
}

fn resolve_model(name: &str, context: &mut dyn ResolutionContext) -> Option<String> {
    let snake = snake_case(name);
    for path in [
        format!("app/models/{snake}.rb"),
        format!("app/models/concerns/{snake}.rb"),
    ] {
        if context.file_exists(&path)
            && let Some(node) = context
                .get_nodes_in_file(&path)
                .into_iter()
                .find(|node| node.kind == NodeKind::Class && node.name == name)
        {
            return Some(node.id);
        }
    }
    context
        .get_nodes_by_name(name)
        .into_iter()
        .find_map(|node| {
            (node.kind == NodeKind::Class && node.file_path.contains("app/models/"))
                .then_some(node.id)
        })
}

fn resolve_controller(name: &str, context: &mut dyn ResolutionContext) -> Option<String> {
    let snake = snake_case(name);
    for path in [
        format!("app/controllers/{snake}.rb"),
        format!("app/controllers/api/{snake}.rb"),
        format!("app/controllers/api/v1/{snake}.rb"),
    ] {
        if context.file_exists(&path)
            && let Some(node) = context
                .get_nodes_in_file(&path)
                .into_iter()
                .find(|node| node.kind == NodeKind::Class && node.name == name)
        {
            return Some(node.id);
        }
    }
    context
        .get_nodes_by_name(name)
        .into_iter()
        .find_map(|node| {
            (node.kind == NodeKind::Class && node.file_path.contains("controllers/"))
                .then_some(node.id)
        })
}

fn resolve_helper(name: &str, context: &mut dyn ResolutionContext) -> Option<String> {
    let path = format!("app/helpers/{}.rb", snake_case(name));
    context
        .get_nodes_in_file(&path)
        .into_iter()
        .find_map(|node| {
            (context.file_exists(&path) && node.kind == NodeKind::Module && node.name == name)
                .then_some(node.id)
        })
}

fn resolve_service(name: &str, context: &mut dyn ResolutionContext) -> Option<String> {
    let snake = snake_case(name);
    for path in [
        format!("app/services/{snake}.rb"),
        format!("app/jobs/{snake}.rb"),
        format!("app/workers/{snake}.rb"),
    ] {
        if !context.file_exists(&path) {
            continue;
        }
        if let Some(node) = context
            .get_nodes_in_file(&path)
            .into_iter()
            .find(|node| node.kind == NodeKind::Class && node.name == name)
        {
            return Some(node.id);
        }
    }
    None
}

fn find_method_in_file(
    path: &str,
    action: &str,
    context: &mut dyn ResolutionContext,
) -> Option<String> {
    context
        .get_nodes_in_file(path)
        .into_iter()
        .find_map(|node| {
            (matches!(node.kind, NodeKind::Method | NodeKind::Function) && node.name == action)
                .then_some(node.id)
        })
}

fn snake_case(value: &str) -> String {
    let mut out = String::new();
    for (idx, ch) in value.chars().enumerate() {
        if ch.is_ascii_uppercase() && idx > 0 {
            out.push('_');
        }
        out.push(ch.to_ascii_lowercase());
    }
    out
}

fn list_option(tail: &str, key: &str) -> Option<Vec<String>> {
    // 只解析 `only: [...]` / `except: [...]` 的显式数组形式；复杂 Ruby 表达式
    // 无法可靠静态求值，保持默认 RESTful action 更安全。
    let needle = format!("{key}:");
    let start = tail.find(&needle)? + needle.len();
    let bracket_start = tail[start..].find('[')? + start;
    let bracket_end = tail[bracket_start..].find(']')? + bracket_start;
    Some(
        tail[bracket_start + 1..bracket_end]
            .split(',')
            .map(|item| item.trim().trim_start_matches(':').to_string())
            .filter(|item| !item.is_empty())
            .collect(),
    )
}

fn line_for_offset(content: &str, offset: usize) -> u64 {
    (content[..offset.min(content.len())]
        .bytes()
        .filter(|b| *b == b'\n')
        .count()
        + 1) as u64
}

fn is_pascal_word(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some('A'..='Z')) && chars.all(|ch| ch.is_ascii_alphabetic())
}

fn is_ident(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some('_') | Some('A'..='Z') | Some('a'..='z'))
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}
