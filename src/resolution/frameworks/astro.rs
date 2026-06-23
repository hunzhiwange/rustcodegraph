//! Astro framework resolver translated from `astro.ts`.
//!
//! Astro 解析器负责三件事：识别虚拟 `astro:*` import、把 `.astro` 组件名引用
//! 连到组件节点，以及根据 `src/pages` 文件约定抽取 route 节点。

use crate::resolution::types::{
    FrameworkExtractionResult, FrameworkResolver, ResolutionContext, ResolvedRef, UnresolvedRef,
    language_for_path, line_for_byte, make_node,
};
use crate::types::{NodeKind, ReferenceKind};

const ASTRO_VIRTUAL_MODULES: &[&str] = &[
    "astro:content",
    "astro:assets",
    "astro:actions",
    "astro:env",
    "astro:i18n",
    "astro:middleware",
    "astro:transitions",
    "astro:components",
    "astro:schema",
];

pub struct AstroResolver;

pub const ASTRO_RESOLVER: AstroResolver = AstroResolver;

impl FrameworkResolver for AstroResolver {
    fn name(&self) -> &'static str {
        "astro"
    }

    fn detect(&self, context: &mut dyn ResolutionContext) -> bool {
        // package.json 是强信号；没有依赖声明时，只要项目里有 .astro 文件也启用。
        if let Some(package_json) = context.read_file("package.json")
            && package_json.contains("\"astro\"")
        {
            return true;
        }
        context
            .get_all_files()
            .iter()
            .any(|file| file.ends_with(".astro"))
    }

    fn resolve(
        &self,
        reference: &UnresolvedRef,
        context: &mut dyn ResolutionContext,
    ) -> Option<ResolvedRef> {
        if reference.reference_name == "Astro" || reference.reference_name.starts_with("Astro.") {
            // `Astro` 是编译器注入的全局对象，不应被当成未解析符号。
            return Some(ResolvedRef::framework(
                reference,
                reference.from_node_id.clone(),
                1.0,
            ));
        }

        if reference.reference_kind == ReferenceKind::Imports
            && reference.reference_name.starts_with("astro:")
            && ASTRO_VIRTUAL_MODULES
                .iter()
                .any(|prefix| reference.reference_name.starts_with(prefix))
        {
            // 虚拟模块没有磁盘文件，解析到自身即可表示“框架已处理”。
            return Some(ResolvedRef::framework(
                reference,
                reference.from_node_id.clone(),
                1.0,
            ));
        }

        if is_pascal_case(&reference.reference_name)
            && matches!(
                reference.reference_kind,
                ReferenceKind::References | ReferenceKind::Calls
            )
            && let Some(target) =
                resolve_component(&reference.reference_name, &reference.file_path, context)
        {
            return Some(ResolvedRef::framework(reference, target, 0.8));
        }

        None
    }

    fn extract(&self, file_path: &str, _content: &str) -> FrameworkExtractionResult {
        let mut result = FrameworkExtractionResult::default();
        let normalized = file_path.replace('\\', "/");
        let Some(pages_start) = normalized.find("src/pages/") else {
            return result;
        };
        if ![".astro", ".ts", ".js", ".mjs"]
            .iter()
            .any(|ext| normalized.ends_with(ext))
        {
            return result;
        }

        let after_pages = &normalized[pages_start + "src/pages/".len()..];
        let base = after_pages.rsplit('/').next().unwrap_or("");
        if after_pages
            .split('/')
            .any(|segment| segment.starts_with('_'))
            || base.contains(".config.")
        {
            // `_` 前缀和 config 文件不是公开页面路由。
            return result;
        }

        let route_path = file_path_to_astro_route(after_pages);
        result.nodes.push(make_node(
            format!("route:{file_path}:{route_path}:1"),
            NodeKind::Route,
            route_path.clone(),
            format!("{file_path}::route:{route_path}"),
            file_path,
            language_for_path(file_path),
            line_for_byte("", 0),
            None,
            None,
        ));
        result
    }
}

fn is_pascal_case(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some('A'..='Z')) && chars.all(|ch| ch.is_ascii_alphanumeric())
}

fn resolve_component(
    name: &str,
    from_file: &str,
    context: &mut dyn ResolutionContext,
) -> Option<String> {
    // 同名组件优先选同目录/子目录，只有全项目唯一时才跨目录解析。
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

fn file_path_to_astro_route(after_pages: &str) -> String {
    // Astro 文件路由：index 折叠为空段，`[id]` 变 `:id`，`[...rest]` 变 `*rest`。
    let without_ext = [".astro", ".ts", ".js", ".mjs"]
        .iter()
        .find_map(|ext| after_pages.strip_suffix(ext))
        .unwrap_or(after_pages);
    let mut segments = without_ext
        .split('/')
        .filter(|segment| *segment != "index")
        .map(|segment| {
            if let Some(rest) = segment
                .strip_prefix("[...")
                .and_then(|s| s.strip_suffix(']'))
            {
                format!("*{rest}")
            } else if let Some(rest) = segment.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                format!(":{rest}")
            } else {
                segment.to_string()
            }
        })
        .collect::<Vec<_>>();
    while segments.last().is_some_and(|segment| segment.is_empty()) {
        segments.pop();
    }
    if segments.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", segments.join("/"))
    }
}
