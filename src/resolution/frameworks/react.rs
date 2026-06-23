//! React and Next.js framework resolver translated from `react.ts`.
//!
//! React/Next 的关键流经常藏在 JSX 组件、hook、context 和文件路由里。
//! 这个 resolver 补出组件/路由节点，并按目录约定解析 PascalCase 引用。

use regex::Regex;

use crate::resolution::types::{
    FrameworkExtractionResult, FrameworkResolver, ResolutionContext, ResolvedRef, UnresolvedRef,
    line_for_byte, make_node, make_reference,
};
use crate::types::{Language, NodeKind, ReferenceKind};

const BUILT_IN_TYPES: &[&str] = &[
    "Array",
    "Boolean",
    "Date",
    "Error",
    "Function",
    "JSON",
    "Math",
    "Number",
    "Object",
    "Promise",
    "RegExp",
    "String",
    "Symbol",
    "Map",
    "Set",
    "WeakMap",
    "WeakSet",
    "React",
    "Component",
    "Fragment",
    "Suspense",
    "StrictMode",
];
const COMPONENT_DIRS: &[&str] = &[
    "/components/",
    "/src/components/",
    "/app/components/",
    "/pages/",
    "/src/pages/",
    "/views/",
    "/src/views/",
];
const HOOK_DIRS: &[&str] = &["/hooks/", "/src/hooks/", "/lib/hooks/", "/utils/hooks/"];
const CONTEXT_DIRS: &[&str] = &[
    "/context/",
    "/contexts/",
    "/src/context/",
    "/src/contexts/",
    "/providers/",
    "/src/providers/",
];

pub struct ReactResolver;

pub const REACT_RESOLVER: ReactResolver = ReactResolver;

impl FrameworkResolver for ReactResolver {
    fn name(&self) -> &'static str {
        "react"
    }

    fn languages(&self) -> Option<&'static [Language]> {
        Some(&[Language::JavaScript, Language::TypeScript])
    }

    fn detect(&self, context: &mut dyn ResolutionContext) -> bool {
        context
            .read_file("package.json")
            .is_some_and(|package_json| {
                ["\"react\"", "\"next\"", "\"react-native\""]
                    .iter()
                    .any(|needle| package_json.contains(needle))
            })
            || context
                .get_all_files()
                .iter()
                .any(|file| file.ends_with(".jsx") || file.ends_with(".tsx"))
    }

    fn resolve(
        &self,
        reference: &UnresolvedRef,
        context: &mut dyn ResolutionContext,
    ) -> Option<ResolvedRef> {
        // JSX 中 PascalCase 调用既可能是组件，也可能是内建类型；先排除常见内建，
        // 再用同目录/组件目录偏好降低重名误连。
        if matches!(reference.language, Language::Tsx | Language::Jsx)
            && is_pascal_case(&reference.reference_name)
            && !BUILT_IN_TYPES.contains(&reference.reference_name.as_str())
            && let Some(target) =
                resolve_component(&reference.reference_name, &reference.file_path, context)
        {
            return Some(ResolvedRef::framework(reference, target, 0.8));
        }
        if reference.reference_name.starts_with("use")
            && reference.reference_name.len() > 3
            && let Some(target) = resolve_hook(&reference.reference_name, context)
        {
            return Some(ResolvedRef::framework(reference, target, 0.85));
        }
        if (reference.reference_name.ends_with("Context")
            || reference.reference_name.ends_with("Provider"))
            && let Some(target) = resolve_context(&reference.reference_name, context)
        {
            return Some(ResolvedRef::framework(reference, target, 0.8));
        }
        None
    }

    fn extract(&self, file_path: &str, content: &str) -> FrameworkExtractionResult {
        let mut result = FrameworkExtractionResult::default();
        extract_components(file_path, content, &mut result);
        extract_hooks(file_path, content, &mut result);
        extract_react_routes(file_path, content, &mut result);
        extract_next_routes(file_path, content, &mut result);
        result
    }
}

fn extract_components(file_path: &str, content: &str, result: &mut FrameworkExtractionResult) {
    // 只把附近确实出现 JSX 的 PascalCase 函数/const 记为组件，避免普通工厂函数
    // 被提升成 Component 节点。
    let patterns = [
        r"(?:export\s+)?function\s+([A-Z][a-zA-Z0-9]*)\s*\(",
        r"(?:export\s+)?(?:const|let)\s+([A-Z][a-zA-Z0-9]*)\s*=\s*(?:\([^)]*\)|[a-zA-Z_][a-zA-Z0-9_]*)\s*=>",
        r"(?:export\s+)?(?:const|let)\s+([A-Z][a-zA-Z0-9]*)\s*=\s*(?:React\.)?forwardRef",
        r"(?:export\s+)?(?:const|let)\s+([A-Z][a-zA-Z0-9]*)\s*=\s*(?:React\.)?memo",
    ];
    for pattern in patterns {
        let re = Regex::new(pattern).unwrap();
        for caps in re.captures_iter(content) {
            let whole = caps.get(0).unwrap();
            let name = caps.get(1).unwrap().as_str();
            let after_end = (whole.end() + 500).min(content.len());
            let after = &content[whole.end()..after_end];
            if !(after.contains('<') && (after.contains("/>") || after.contains("</"))) {
                continue;
            }
            let line = line_for_byte(content, whole.start());
            result.nodes.push(make_node(
                format!("component:{file_path}:{name}:{line}"),
                NodeKind::Component,
                name,
                format!("{file_path}::{name}"),
                file_path,
                if file_path.ends_with(".tsx") {
                    Language::Tsx
                } else {
                    Language::Jsx
                },
                line,
                None,
                None,
            ));
        }
    }
}

fn extract_hooks(file_path: &str, content: &str, result: &mut FrameworkExtractionResult) {
    let hook_re =
        Regex::new(r"(?:export\s+)?(?:function|const|let)\s+(use[A-Z][a-zA-Z0-9]*)\s*[=(]")
            .unwrap();
    for caps in hook_re.captures_iter(content) {
        let whole = caps.get(0).unwrap();
        let name = caps.get(1).unwrap().as_str();
        let line = line_for_byte(content, whole.start());
        result.nodes.push(make_node(
            format!("hook:{file_path}:{name}:{line}"),
            NodeKind::Function,
            name,
            format!("{file_path}::{name}"),
            file_path,
            if file_path.ends_with(".ts") || file_path.ends_with(".tsx") {
                Language::TypeScript
            } else {
                Language::JavaScript
            },
            line,
            None,
            None,
        ));
    }
}

fn extract_react_routes(file_path: &str, content: &str, result: &mut FrameworkExtractionResult) {
    // React Router 同时支持 JSX <Route> 和对象式 createBrowserRouter；两种写法都
    // 只提取能指向组件的路径，保持 route 节点精度。
    let route_tag_re = Regex::new(r"<Route\b").unwrap();
    let route_component_re = Regex::new(r"\bcomponent\s*=\s*\{\s*([A-Z][A-Za-z0-9_]*)").unwrap();
    let route_element_re = Regex::new(r"\belement\s*=\s*\{\s*<\s*([A-Z][A-Za-z0-9_]*)").unwrap();
    for mat in route_tag_re.find_iter(content) {
        let end = (mat.start() + 400).min(content.len());
        let window = &content[mat.start()..end];
        let Some(route_path) = attr_value(window, "path") else {
            continue;
        };
        let component = route_component_re
            .captures(window)
            .or_else(|| route_element_re.captures(window))
            .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()));
        let line = line_for_byte(content, mat.start());
        let route_id = format!("route:{file_path}:{line}:{route_path}");
        let lang = if file_path.ends_with(".tsx") {
            Language::Tsx
        } else {
            Language::Jsx
        };
        result.nodes.push(make_node(
            route_id.clone(),
            NodeKind::Route,
            route_path.clone(),
            format!("{file_path}::route:{route_path}"),
            file_path,
            lang,
            line,
            None,
            None,
        ));
        if let Some(component) = component {
            result.references.push(make_reference(
                route_id,
                component,
                ReferenceKind::References,
                line,
                0,
                file_path,
                lang,
            ));
        }
    }

    let browser_router_re = Regex::new(
        r"\b(?:createBrowserRouter|createHashRouter|createMemoryRouter|createRoutesFromElements)\b",
    )
    .unwrap();
    if !browser_router_re.is_match(content) {
        return;
    }
    let path_re = Regex::new(r#"\bpath\s*:\s*['"]([^'"]*)['"]"#).unwrap();
    let path_element_re = Regex::new(r"\belement\s*:\s*<\s*([A-Z][A-Za-z0-9_]*)").unwrap();
    let path_component_re = Regex::new(r"\bComponent\s*:\s*([A-Z][A-Za-z0-9_]*)").unwrap();
    for caps in path_re.captures_iter(content) {
        let whole = caps.get(0).unwrap();
        let window = &content[whole.start()..(whole.start() + 300).min(content.len())];
        let component = path_element_re
            .captures(window)
            .or_else(|| path_component_re.captures(window))
            .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()));
        let Some(component) = component else {
            continue;
        };
        let route_path = caps.get(1).map(|m| m.as_str()).unwrap_or("/");
        let route_path = if route_path.is_empty() {
            "/"
        } else {
            route_path
        };
        let line = line_for_byte(content, whole.start());
        let route_id = format!("route:{file_path}:{line}:{route_path}");
        let lang = if file_path.ends_with(".tsx") {
            Language::Tsx
        } else {
            Language::Jsx
        };
        result.nodes.push(make_node(
            route_id.clone(),
            NodeKind::Route,
            route_path,
            format!("{file_path}::route:{route_path}"),
            file_path,
            lang,
            line,
            None,
            None,
        ));
        result.references.push(make_reference(
            route_id,
            component,
            ReferenceKind::References,
            line,
            0,
            file_path,
            lang,
        ));
    }
}

fn extract_next_routes(file_path: &str, content: &str, result: &mut FrameworkExtractionResult) {
    if !(file_path.contains("pages/") || file_path.contains("app/"))
        || !content.contains("export default")
    {
        return;
    }
    let Some(route_path) = file_path_to_route(file_path) else {
        return;
    };
    let line = content
        .find("export default")
        .map(|idx| line_for_byte(content, idx))
        .unwrap_or(1);
    result.nodes.push(make_node(
        format!("route:{file_path}:{route_path}:{line}"),
        NodeKind::Route,
        route_path.clone(),
        format!("{file_path}::route:{route_path}"),
        file_path,
        if file_path.ends_with(".tsx") {
            Language::Tsx
        } else if file_path.ends_with(".ts") {
            Language::TypeScript
        } else {
            Language::JavaScript
        },
        line,
        None,
        None,
    ));
}

fn resolve_component(
    name: &str,
    from_file: &str,
    context: &mut dyn ResolutionContext,
) -> Option<String> {
    let components = context
        .get_nodes_by_name(name)
        .into_iter()
        .filter(|node| {
            matches!(
                node.kind,
                NodeKind::Component | NodeKind::Function | NodeKind::Class
            )
        })
        .collect::<Vec<_>>();
    if components.is_empty() {
        return None;
    }
    let from_dir = from_file
        .rfind('/')
        .map(|idx| &from_file[..idx])
        .unwrap_or("");
    if let Some(same_dir) = components
        .iter()
        .find(|node| node.file_path.starts_with(from_dir))
    {
        return Some(same_dir.id.clone());
    }
    if let Some(preferred) = components.iter().find(|node| {
        COMPONENT_DIRS
            .iter()
            .any(|dir| node.file_path.contains(dir))
    }) {
        return Some(preferred.id.clone());
    }
    (components.len() == 1).then(|| components[0].id.clone())
}

fn resolve_hook(name: &str, context: &mut dyn ResolutionContext) -> Option<String> {
    let hooks = context
        .get_nodes_by_name(name)
        .into_iter()
        .filter(|node| node.kind == NodeKind::Function && node.name.starts_with("use"))
        .collect::<Vec<_>>();
    if hooks.is_empty() {
        return None;
    }
    hooks
        .iter()
        .find(|node| HOOK_DIRS.iter().any(|dir| node.file_path.contains(dir)))
        .or_else(|| hooks.first())
        .map(|node| node.id.clone())
}

fn resolve_context(name: &str, context: &mut dyn ResolutionContext) -> Option<String> {
    let candidates = context.get_nodes_by_name(name);
    if candidates.is_empty() {
        let base = name
            .trim_end_matches("Context")
            .trim_end_matches("Provider");
        if base != name {
            return context
                .get_nodes_by_name(base)
                .first()
                .map(|node| node.id.clone());
        }
        return None;
    }
    candidates
        .iter()
        .find(|node| CONTEXT_DIRS.iter().any(|dir| node.file_path.contains(dir)))
        .or_else(|| candidates.first())
        .map(|node| node.id.clone())
}

fn file_path_to_route(file_path: &str) -> Option<String> {
    // Next.js pages/app 路由来自文件名。忽略 `_app`、配置文件和非 page 文件，
    // 防止框架壳文件污染业务路由图。
    let base = file_path.rsplit('/').next().unwrap_or(file_path);
    if ![".ts", ".tsx", ".js", ".jsx"]
        .iter()
        .any(|ext| base.ends_with(ext))
    {
        return None;
    }
    if base.starts_with('_') || base.contains(".config.") {
        return None;
    }
    if let Some(idx) = file_path.find("pages/") {
        let mut route = format!("/{}", &file_path[idx + "pages/".len()..]);
        route = strip_js_ext(&route);
        route = route.trim_end_matches("/index").to_string();
        route = route.replace('[', ":").replace(']', "");
        return Some(if route.is_empty() {
            "/".to_string()
        } else {
            route
        });
    }
    if let Some(idx) = file_path.find("app/") {
        if !file_path.contains("page.") {
            return None;
        }
        let mut route = format!("/{}", &file_path[idx + "app/".len()..]);
        route = strip_js_ext(&route);
        route = route.trim_end_matches("/page").to_string();
        route = route.replace('[', ":").replace(']', "");
        return Some(if route.is_empty() {
            "/".to_string()
        } else {
            route
        });
    }
    None
}

fn strip_js_ext(value: &str) -> String {
    [".tsx", ".ts", ".jsx", ".js"]
        .iter()
        .find_map(|ext| value.strip_suffix(ext))
        .unwrap_or(value)
        .to_string()
}

fn attr_value(window: &str, attr: &str) -> Option<String> {
    let re = Regex::new(&format!(
        r#"\b{}\s*=\s*["']([^"']+)["']"#,
        regex::escape(attr)
    ))
    .unwrap();
    re.captures(window)
        .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()))
}

fn is_pascal_case(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some('A'..='Z')) && chars.all(|ch| ch.is_ascii_alphanumeric())
}
