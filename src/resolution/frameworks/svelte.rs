//! Svelte / SvelteKit framework resolver translated from `svelte.ts`.
//!
//! Svelte/SvelteKit 的隐式依赖包括 runes、store `$name` 语法、`$lib` alias 和
//! 文件路由。这里把这些框架语义转成可遍历的引用或 route 节点。

use crate::resolution::types::{
    FrameworkExtractionResult, FrameworkResolver, ResolutionContext, ResolvedRef, UnresolvedRef,
    make_node,
};
use crate::types::{Language, NodeKind, ReferenceKind};

const SVELTE_RUNES: &[&str] = &[
    "$state",
    "$state.raw",
    "$state.snapshot",
    "$derived",
    "$derived.by",
    "$effect",
    "$effect.pre",
    "$effect.root",
    "$effect.tracking",
    "$props",
    "$bindable",
    "$inspect",
    "$host",
];
const SVELTEKIT_MODULE_PREFIXES: &[&str] = &[
    "$app/navigation",
    "$app/stores",
    "$app/environment",
    "$app/forms",
    "$app/paths",
    "$env/static/private",
    "$env/static/public",
    "$env/dynamic/private",
    "$env/dynamic/public",
];
const SVELTEKIT_ROUTE_FILES: &[&str] = &[
    "+page.svelte",
    "+page.ts",
    "+page.js",
    "+page.server.ts",
    "+page.server.js",
    "+layout.svelte",
    "+layout.ts",
    "+layout.js",
    "+layout.server.ts",
    "+layout.server.js",
    "+server.ts",
    "+server.js",
    "+error.svelte",
];

pub struct SvelteResolver;

pub const SVELTE_RESOLVER: SvelteResolver = SvelteResolver;

impl FrameworkResolver for SvelteResolver {
    fn name(&self) -> &'static str {
        "svelte"
    }

    fn languages(&self) -> Option<&'static [Language]> {
        Some(&[Language::Svelte])
    }

    fn detect(&self, context: &mut dyn ResolutionContext) -> bool {
        context
            .read_file("package.json")
            .is_some_and(|package_json| {
                ["\"svelte\"", "\"@sveltejs/kit\""]
                    .iter()
                    .any(|needle| package_json.contains(needle))
            })
            || context
                .get_all_files()
                .iter()
                .any(|file| file.ends_with(".svelte"))
    }

    fn resolve(
        &self,
        reference: &UnresolvedRef,
        context: &mut dyn ResolutionContext,
    ) -> Option<ResolvedRef> {
        // Runes 和 SvelteKit 虚拟模块没有项目内定义，解析到自身表示“框架已处理”，
        // 防止后续 name resolver 把它们当成未解析错误。
        if is_rune_reference(&reference.reference_name) {
            return Some(ResolvedRef::framework(
                reference,
                reference.from_node_id.clone(),
                1.0,
            ));
        }
        if reference.reference_name.starts_with('$') && !reference.reference_name.starts_with("$$")
        {
            let store_name = reference.reference_name.trim_start_matches('$');
            if let Some(store) = context
                .get_nodes_by_name(store_name)
                .into_iter()
                .find(|node| matches!(node.kind, NodeKind::Variable | NodeKind::Constant))
            {
                return Some(ResolvedRef::framework(reference, store.id, 0.85));
            }
        }
        if reference.reference_kind == ReferenceKind::Imports
            && reference.reference_name.starts_with('$')
        {
            if let Some(target) = resolve_svelte_import(&reference.reference_name, context) {
                return Some(ResolvedRef::framework(reference, target, 0.9));
            }
            if SVELTEKIT_MODULE_PREFIXES
                .iter()
                .any(|prefix| reference.reference_name.starts_with(prefix))
            {
                return Some(ResolvedRef::framework(
                    reference,
                    reference.from_node_id.clone(),
                    1.0,
                ));
            }
        }
        if is_pascal_case(&reference.reference_name)
            && reference.reference_kind == ReferenceKind::Calls
            && let Some(target) =
                resolve_component(&reference.reference_name, &reference.file_path, context)
        {
            return Some(ResolvedRef::framework(reference, target, 0.8));
        }
        None
    }

    fn extract(&self, file_path: &str, _content: &str) -> FrameworkExtractionResult {
        let mut result = FrameworkExtractionResult::default();
        let file_name = file_path.rsplit(['/', '\\']).next().unwrap_or(file_path);
        if SVELTEKIT_ROUTE_FILES.contains(&file_name)
            && let Some(route_path) = file_path_to_sveltekit_route(file_path)
        {
            result.nodes.push(make_node(
                format!("route:{file_path}:{route_path}:1"),
                NodeKind::Route,
                route_path.clone(),
                format!("{file_path}::route:{route_path}"),
                file_path,
                if file_path.ends_with(".svelte") {
                    Language::Svelte
                } else {
                    Language::TypeScript
                },
                1,
                None,
                None,
            ));
        }
        result
    }
}

fn is_rune_reference(name: &str) -> bool {
    SVELTE_RUNES.contains(&name) || matches!(name, "$state" | "$derived" | "$effect")
}

fn resolve_svelte_import(name: &str, context: &mut dyn ResolutionContext) -> Option<String> {
    // `$lib/foo` 固定映射到 `src/lib/foo`，按 SvelteKit 的扩展名顺序尝试文件
    // 和 index 入口。
    let lib_path = name.strip_prefix("$lib/")?;
    let base = format!("src/lib/{lib_path}");
    for ext in ["", ".ts", ".js", ".svelte", "/index.ts", "/index.js"] {
        let full_path = format!("{base}{ext}");
        if context.file_exists(&full_path)
            && let Some(node) = context.get_nodes_in_file(&full_path).first()
        {
            return Some(node.id.clone());
        }
    }
    None
}

fn resolve_component(
    name: &str,
    from_file: &str,
    context: &mut dyn ResolutionContext,
) -> Option<String> {
    let components = context
        .get_nodes_by_name(name)
        .into_iter()
        .filter(|node| node.kind == NodeKind::Component)
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
    (components.len() == 1).then(|| components[0].id.clone())
}

fn file_path_to_sveltekit_route(file_path: &str) -> Option<String> {
    let normalized = file_path.replace('\\', "/");
    let routes_index = normalized.find("/routes/")?;
    let after_routes = &normalized[routes_index + "/routes/".len()..];
    let dir_path = after_routes
        .rsplit_once('/')
        .map(|(dir, _)| dir)
        .unwrap_or("");
    let route = convert_bracket_route(dir_path);
    Some(if route == "/" {
        route
    } else {
        route.trim_end_matches('/').to_string()
    })
}

fn convert_bracket_route(path: &str) -> String {
    // SvelteKit 的 `[id]`、`[[id]]`、`[...rest]` 分别转成图里统一的
    // `:id`、`:id?`、`*rest` 表达。
    if path.is_empty() {
        return "/".to_string();
    }
    let converted = path
        .split('/')
        .map(|segment| {
            if let Some(rest) = segment
                .strip_prefix("[...")
                .and_then(|s| s.strip_suffix(']'))
            {
                format!("*{rest}")
            } else if let Some(rest) = segment
                .strip_prefix("[[")
                .and_then(|s| s.strip_suffix("]]"))
            {
                format!(":{rest}?")
            } else if let Some(rest) = segment.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                format!(":{rest}")
            } else {
                segment.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("/");
    format!("/{converted}")
}

fn is_pascal_case(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some('A'..='Z')) && chars.all(|ch| ch.is_ascii_alphanumeric())
}
