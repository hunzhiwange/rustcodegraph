//! Django, Flask, and FastAPI framework resolvers translated from `python.ts`.
//!
//! Python Web 框架大量依赖装饰器、URLConf 和目录约定。这里把这些入口统一
//! 提取为 route 节点，并用保守的命名/目录规则解析到 view、router 或 model。

use regex::Regex;

use crate::resolution::strip_comments::{CommentLang, strip_comments_for_regex};
use crate::resolution::types::{
    FrameworkExtractionResult, FrameworkResolver, ResolutionContext, ResolvedRef, UnresolvedRef,
    line_for_byte, make_node, make_reference, resolve_by_name_and_kind,
};
use crate::types::{Language, NodeKind, ReferenceKind};

const MODEL_DIRS: &[&str] = &["models", "app/models", "src/models"];
const VIEW_DIRS: &[&str] = &["views", "app/views", "src/views", "api/views"];
const FORM_DIRS: &[&str] = &["forms", "app/forms", "src/forms"];
const ROUTER_DIRS: &[&str] = &["/routers/", "/api/", "/routes/", "/endpoints/"];
const DEP_DIRS: &[&str] = &["/dependencies/", "/deps/", "/core/"];
const CLASS_KINDS: &[NodeKind] = &[NodeKind::Class];
const VIEW_KINDS: &[NodeKind] = &[NodeKind::Class, NodeKind::Function];
const VARIABLE_KINDS: &[NodeKind] = &[NodeKind::Variable];
const FUNCTION_KINDS: &[NodeKind] = &[NodeKind::Function];

pub struct DjangoResolver;
pub struct FlaskResolver;
pub struct FastApiResolver;

pub const DJANGO_RESOLVER: DjangoResolver = DjangoResolver;
pub const FLASK_RESOLVER: FlaskResolver = FlaskResolver;
pub const FASTAPI_RESOLVER: FastApiResolver = FastApiResolver;

impl FrameworkResolver for DjangoResolver {
    fn name(&self) -> &'static str {
        "django"
    }

    fn languages(&self) -> Option<&'static [Language]> {
        Some(&[Language::Python])
    }

    fn detect(&self, context: &mut dyn ResolutionContext) -> bool {
        ["requirements.txt", "setup.py", "pyproject.toml"]
            .iter()
            .any(|file| {
                context
                    .read_file(file)
                    .is_some_and(|content| content.to_ascii_lowercase().contains("django"))
            })
            || context.file_exists("manage.py")
    }

    fn resolve(
        &self,
        reference: &UnresolvedRef,
        context: &mut dyn ResolutionContext,
    ) -> Option<ResolvedRef> {
        // Django 名称冲突很常见，先按 models/views/forms 目录缩小候选范围，
        // 再返回框架解析结果。
        if (reference.reference_name.ends_with("Model")
            || is_simple_python_class_name(&reference.reference_name))
            && let Some(target) = resolve_by_name_and_kind(
                &reference.reference_name,
                CLASS_KINDS,
                MODEL_DIRS,
                context,
            )
        {
            return Some(ResolvedRef::framework(reference, target, 0.8));
        }
        if (reference.reference_name.ends_with("View")
            || reference.reference_name.ends_with("ViewSet"))
            && let Some(target) =
                resolve_by_name_and_kind(&reference.reference_name, VIEW_KINDS, VIEW_DIRS, context)
        {
            return Some(ResolvedRef::framework(reference, target, 0.8));
        }
        if reference.reference_name.ends_with("Form")
            && let Some(target) =
                resolve_by_name_and_kind(&reference.reference_name, CLASS_KINDS, FORM_DIRS, context)
        {
            return Some(ResolvedRef::framework(reference, target, 0.8));
        }
        if reference.reference_name == "_iterable_class"
            && let Some(target) = resolve_model_iterable_iter(context)
        {
            return Some(ResolvedRef::framework(reference, target, 0.7));
        }
        None
    }

    fn claims_reference(&self, name: &str) -> bool {
        name == "_iterable_class" || name.ends_with(".urls")
    }

    fn extract(&self, file_path: &str, content: &str) -> FrameworkExtractionResult {
        if !file_path.ends_with(".py") {
            return FrameworkExtractionResult::default();
        }
        let safe = strip_comments_for_regex(content, CommentLang::Python);
        let content = safe.as_str();
        let mut result = FrameworkExtractionResult::default();
        let route_re = Regex::new(
            r#"\b(path|re_path|url)\s*\(\s*r?['"]([^'"]+)['"]\s*,\s*([\w.]+(?:\s*\([^)]*\))?)"#,
        )
        .unwrap();
        for caps in route_re.captures_iter(content) {
            let whole = caps.get(0).unwrap();
            let url_path = caps.get(2).unwrap().as_str();
            let handler_expr = caps.get(3).unwrap().as_str().trim();
            let line = line_for_byte(content, whole.start());
            let route_id = format!("route:{file_path}:{line}:{url_path}");
            result.nodes.push(make_node(
                route_id.clone(),
                NodeKind::Route,
                url_path,
                format!("{file_path}::route:{url_path}"),
                file_path,
                Language::Python,
                line,
                None,
                None,
            ));
            if let Some((target_name, kind)) = resolve_handler_name(handler_expr) {
                result.references.push(make_reference(
                    route_id,
                    target_name,
                    kind,
                    line,
                    0,
                    file_path,
                    Language::Python,
                ));
            }
        }

        let router_re =
            Regex::new(r#"\.register\s*\(\s*r?['"]([^'"]+)['"]\s*,\s*([\w.]+)"#).unwrap();
        for caps in router_re.captures_iter(content) {
            let whole = caps.get(0).unwrap();
            let prefix = caps
                .get(1)
                .unwrap()
                .as_str()
                .trim_start_matches('^')
                .trim_end_matches('$')
                .trim_end_matches('/');
            let viewset = caps
                .get(2)
                .unwrap()
                .as_str()
                .rsplit('.')
                .next()
                .unwrap_or("");
            if !(viewset.ends_with("View") || viewset.ends_with("ViewSet")) {
                continue;
            }
            let line = line_for_byte(content, whole.start());
            let route_id = format!("route:{file_path}:{line}:VIEWSET:{prefix}");
            result.nodes.push(make_node(
                route_id.clone(),
                NodeKind::Route,
                format!("VIEWSET /{prefix}"),
                format!("{file_path}::route:{prefix}"),
                file_path,
                Language::Python,
                line,
                None,
                None,
            ));
            result.references.push(make_reference(
                route_id,
                viewset,
                ReferenceKind::References,
                line,
                0,
                file_path,
                Language::Python,
            ));
        }

        result
    }
}

impl FrameworkResolver for FlaskResolver {
    fn name(&self) -> &'static str {
        "flask"
    }

    fn languages(&self) -> Option<&'static [Language]> {
        Some(&[Language::Python])
    }

    fn detect(&self, context: &mut dyn ResolutionContext) -> bool {
        for file in ["requirements.txt", "pyproject.toml", "Pipfile", "setup.py"] {
            if context
                .read_file(file)
                .is_some_and(|content| content.to_ascii_lowercase().contains("flask"))
            {
                return true;
            }
        }
        for file in context
            .get_all_files()
            .into_iter()
            .filter(|file| is_python_entrypoint(file))
            .take(50)
        {
            if let Some(content) = context.read_file(&file)
                && content.contains("Flask(")
                && (content.contains("import flask") || content.contains("from flask"))
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
        if (reference.reference_name.ends_with("_bp")
            || reference.reference_name.ends_with("_blueprint"))
            && let Some(target) =
                resolve_by_name_and_kind(&reference.reference_name, VARIABLE_KINDS, &[], context)
        {
            return Some(ResolvedRef::framework(reference, target, 0.8));
        }
        None
    }

    fn extract(&self, file_path: &str, content: &str) -> FrameworkExtractionResult {
        if !file_path.ends_with(".py") {
            return FrameworkExtractionResult::default();
        }
        let safe = strip_comments_for_regex(content, CommentLang::Python);
        let content = safe.as_str();
        let mut decorator = extract_decorator_routes(
            file_path,
            content,
            Regex::new(r#"@(\w+)\.route\s*\(\s*['"]([^'"]*)['"](?:\s*,\s*methods\s*=\s*[\[(]([^\])]+)[\])])?\s*\)"#).unwrap(),
            "GET",
            None,
            Some(3),
            2,
        );
        let restful = extract_flask_restful(file_path, content);
        decorator.nodes.extend(restful.nodes);
        decorator.references.extend(restful.references);
        decorator
    }
}

impl FrameworkResolver for FastApiResolver {
    fn name(&self) -> &'static str {
        "fastapi"
    }

    fn languages(&self) -> Option<&'static [Language]> {
        Some(&[Language::Python])
    }

    fn detect(&self, context: &mut dyn ResolutionContext) -> bool {
        context
            .read_file("requirements.txt")
            .is_some_and(|content| content.to_ascii_lowercase().contains("fastapi"))
            || context
                .read_file("pyproject.toml")
                .is_some_and(|content| content.to_ascii_lowercase().contains("fastapi"))
            || ["app.py", "main.py", "api.py"].iter().any(|file| {
                context
                    .read_file(file)
                    .is_some_and(|content| content.contains("FastAPI("))
            })
    }

    fn resolve(
        &self,
        reference: &UnresolvedRef,
        context: &mut dyn ResolutionContext,
    ) -> Option<ResolvedRef> {
        if (reference.reference_name.ends_with("_router") || reference.reference_name == "router")
            && let Some(target) = resolve_by_name_and_kind(
                &reference.reference_name,
                VARIABLE_KINDS,
                ROUTER_DIRS,
                context,
            )
        {
            return Some(ResolvedRef::framework(reference, target, 0.8));
        }
        if (reference.reference_name.starts_with("get_")
            || reference.reference_name.starts_with("Depends"))
            && let Some(target) = resolve_by_name_and_kind(
                &reference.reference_name,
                FUNCTION_KINDS,
                DEP_DIRS,
                context,
            )
        {
            return Some(ResolvedRef::framework(reference, target, 0.75));
        }
        None
    }

    fn extract(&self, file_path: &str, content: &str) -> FrameworkExtractionResult {
        if !file_path.ends_with(".py") {
            return FrameworkExtractionResult::default();
        }
        let safe = strip_comments_for_regex(content, CommentLang::Python);
        extract_decorator_routes(
            file_path,
            safe.as_str(),
            Regex::new(
                r#"@(\w+)\.(get|post|put|patch|delete|options|head)\s*\(\s*['"]([^'"]*)['"]"#,
            )
            .unwrap(),
            "",
            Some(2),
            None,
            3,
        )
    }
}

fn resolve_model_iterable_iter(context: &mut dyn ResolutionContext) -> Option<String> {
    // Django QuerySet 的迭代边不是源码里的直接调用；把 `_iterable_class`
    // 这个抽象引用落到 ModelIterable.__iter__，让 ORM descriptor flow 能继续。
    let class_node = context
        .get_nodes_by_name("ModelIterable")
        .into_iter()
        .find(|node| node.kind == NodeKind::Class)?;
    context
        .get_nodes_by_name("__iter__")
        .into_iter()
        .find_map(|node| {
            (node.file_path == class_node.file_path
                && node.start_line >= class_node.start_line
                && node.start_line <= class_node.end_line)
                .then_some(node.id)
        })
}

fn resolve_handler_name(expr: &str) -> Option<(String, ReferenceKind)> {
    // URLConf handler 可能是 include(...)、Class.as_view() 或装饰器包装后的调用。
    // 只保留最后的可命名对象，后续由普通 name resolver 再做精确定位。
    let include_re = Regex::new(r#"^include\s*\(\s*['"]([^'"]+)['"]"#).unwrap();
    if let Some(caps) = include_re.captures(expr) {
        return Some((
            caps.get(1).unwrap().as_str().to_string(),
            ReferenceKind::Imports,
        ));
    }
    let mut head = Regex::new(r"\.as_view\s*\([^)]*\)\s*$")
        .unwrap()
        .replace(expr, "")
        .to_string();
    head = Regex::new(r"\.\w+\s*\([^)]*\)\s*$")
        .unwrap()
        .replace(&head, "")
        .to_string();
    let last = head.split('.').rfind(|part| !part.is_empty())?;
    is_ident(last).then(|| (last.to_string(), ReferenceKind::References))
}

fn extract_decorator_routes(
    file_path: &str,
    content: &str,
    decorator_re: Regex,
    default_method: &str,
    method_group: Option<usize>,
    method_from_group: Option<usize>,
    path_group: usize,
) -> FrameworkExtractionResult {
    // Flask/FastAPI 装饰器后通常紧跟函数定义。这里从装饰器窗口往后找第一个
    // def，用 route 节点引用它，而不是尝试解析完整 Python AST。
    let mut result = FrameworkExtractionResult::default();
    let def_re = Regex::new(r"\n\s*(?:async\s+)?def\s+(\w+)").unwrap();
    let method_literal_re = Regex::new(r#"['"]([A-Z]+)['"]"#).unwrap();
    for caps in decorator_re.captures_iter(content) {
        let whole = caps.get(0).unwrap();
        let route_path = caps.get(path_group).map(|m| m.as_str()).unwrap_or("");
        let mut method = default_method.to_string();
        if let Some(group) = method_group.and_then(|group| caps.get(group)) {
            method = group.as_str().to_ascii_uppercase();
        } else if let Some(group) = method_from_group.and_then(|group| caps.get(group))
            && let Some(method_caps) = method_literal_re.captures(group.as_str())
        {
            method = method_caps.get(1).unwrap().as_str().to_ascii_uppercase();
        }
        let line = line_for_byte(content, whole.start());
        let name = if method.is_empty() {
            if route_path.is_empty() {
                "/"
            } else {
                route_path
            }
            .to_string()
        } else {
            format!(
                "{method} {}",
                if route_path.is_empty() {
                    "/"
                } else {
                    route_path
                }
            )
        };
        let route_id = format!("route:{file_path}:{line}:{method}:{route_path}");
        result.nodes.push(make_node(
            route_id.clone(),
            NodeKind::Route,
            name,
            format!("{file_path}::{method}:{route_path}"),
            file_path,
            Language::Python,
            line,
            None,
            None,
        ));
        if let Some(def_caps) = def_re.captures(&content[whole.end()..]) {
            result.references.push(make_reference(
                route_id,
                def_caps.get(1).unwrap().as_str(),
                ReferenceKind::References,
                line,
                0,
                file_path,
                Language::Python,
            ));
        }
    }
    result
}

fn extract_flask_restful(file_path: &str, content: &str) -> FrameworkExtractionResult {
    let mut result = FrameworkExtractionResult::default();
    let re = Regex::new(r#"\.add\w*[Rr]esource\s*\(\s*(\w+)\s*,\s*((?:['"][^'"]+['"]\s*,?\s*)+)"#)
        .unwrap();
    let path_re = Regex::new(r#"['"]([^'"]+)['"]"#).unwrap();
    for caps in re.captures_iter(content) {
        let whole = caps.get(0).unwrap();
        let class_name = caps.get(1).unwrap().as_str();
        let line = line_for_byte(content, whole.start());
        for path_caps in path_re.captures_iter(caps.get(2).unwrap().as_str()) {
            let route_path = path_caps.get(1).unwrap().as_str();
            let route_id = format!("route:{file_path}:{line}:ANY:{route_path}");
            result.nodes.push(make_node(
                route_id.clone(),
                NodeKind::Route,
                format!("ANY {route_path}"),
                format!("{file_path}::ANY:{route_path}"),
                file_path,
                Language::Python,
                line,
                None,
                None,
            ));
            result.references.push(make_reference(
                route_id,
                class_name,
                ReferenceKind::References,
                line,
                0,
                file_path,
                Language::Python,
            ));
        }
    }
    result
}

fn is_python_entrypoint(file: &str) -> bool {
    [
        "app.py",
        "application.py",
        "main.py",
        "wsgi.py",
        "__init__.py",
    ]
    .iter()
    .any(|suffix| file.ends_with(suffix))
}

fn is_simple_python_class_name(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some('A'..='Z'))
        && chars.next().is_some_and(|ch| ch.is_ascii_lowercase())
        && chars.all(|ch| ch.is_ascii_lowercase())
}

fn is_ident(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some('_') | Some('A'..='Z') | Some('a'..='z'))
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}
