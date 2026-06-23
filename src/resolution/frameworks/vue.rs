//! Vue / Nuxt framework resolver translated from `vue.ts`.
//!
//! Vue/Nuxt 的隐式边主要来自 compiler macro、auto import、虚拟模块、组件文件名
//! 和 pages/server/api 目录。这里把这些约定转成可查询节点。

use crate::resolution::types::{
    FrameworkExtractionResult, FrameworkResolver, ResolutionContext, ResolvedRef, UnresolvedRef,
    make_node,
};
use crate::types::{Language, NodeKind, ReferenceKind};

const VUE_COMPILER_MACROS: &[&str] = &[
    "defineProps",
    "defineEmits",
    "defineExpose",
    "defineOptions",
    "defineSlots",
    "defineModel",
    "withDefaults",
];
const NUXT_AUTO_IMPORTS: &[&str] = &[
    "useRoute",
    "useRouter",
    "navigateTo",
    "abortNavigation",
    "useFetch",
    "useAsyncData",
    "useLazyFetch",
    "useLazyAsyncData",
    "refreshNuxtData",
    "useState",
    "clearNuxtState",
    "useHead",
    "useSeoMeta",
    "useServerSeoMeta",
    "useRuntimeConfig",
    "useAppConfig",
    "useNuxtApp",
    "useCookie",
    "useError",
    "createError",
    "showError",
    "clearError",
    "definePageMeta",
    "defineNuxtConfig",
    "defineNuxtPlugin",
    "defineNuxtRouteMiddleware",
    "useRequestHeaders",
    "useRequestEvent",
    "useRequestFetch",
    "useRequestURL",
];
const NUXT_VIRTUAL_MODULES: &[&str] = &["#imports", "#components", "#app", "#build", "#head"];

pub struct VueResolver;

pub const VUE_RESOLVER: VueResolver = VueResolver;

impl FrameworkResolver for VueResolver {
    fn name(&self) -> &'static str {
        "vue"
    }

    fn detect(&self, context: &mut dyn ResolutionContext) -> bool {
        context
            .read_file("package.json")
            .is_some_and(|package_json| {
                ["\"vue\"", "\"nuxt\"", "\"@nuxt/kit\""]
                    .iter()
                    .any(|needle| package_json.contains(needle))
            })
            || context
                .get_all_files()
                .iter()
                .any(|file| file.ends_with(".vue"))
    }

    fn resolve(
        &self,
        reference: &UnresolvedRef,
        context: &mut dyn ResolutionContext,
    ) -> Option<ResolvedRef> {
        // Vue compiler macro 和 Nuxt auto import 没有项目内定义，解析到自身即可
        // 表示已消费，避免干扰真正的 unresolved reference。
        if VUE_COMPILER_MACROS.contains(&reference.reference_name.as_str())
            || NUXT_AUTO_IMPORTS.contains(&reference.reference_name.as_str())
        {
            return Some(ResolvedRef::framework(
                reference,
                reference.from_node_id.clone(),
                1.0,
            ));
        }
        if reference.reference_kind == ReferenceKind::Imports
            && reference.reference_name.starts_with('#')
            && NUXT_VIRTUAL_MODULES
                .iter()
                .any(|prefix| reference.reference_name.starts_with(prefix))
        {
            return Some(ResolvedRef::framework(
                reference,
                reference.from_node_id.clone(),
                1.0,
            ));
        }
        if reference.reference_kind == ReferenceKind::Imports
            && (reference.reference_name.starts_with("@/")
                || reference.reference_name.starts_with("~/"))
            && let Some(target) = resolve_alias_import(&reference.reference_name, context)
        {
            return Some(ResolvedRef::framework(reference, target, 0.9));
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
        let normalized = file_path.replace('\\', "/");
        if let Some(pages_index) = normalized.find("/pages/")
            && normalized.ends_with(".vue")
            && let Some(route_path) =
                file_path_to_nuxt_route(&normalized, pages_index + "/pages/".len())
        {
            result.nodes.push(make_node(
                format!("route:{file_path}:{route_path}:1"),
                NodeKind::Route,
                route_path.clone(),
                format!("{file_path}::route:{route_path}"),
                file_path,
                Language::Vue,
                1,
                None,
                None,
            ));
        }
        if let Some(api_index) = normalized.find("/server/api/") {
            let after_api = &normalized[api_index + "/server/api/".len()..];
            let route_name = strip_extension(after_api)
                .trim_end_matches("/index")
                .to_string();
            let api_route = format!("/api/{route_name}");
            result.nodes.push(make_node(
                format!("route:{file_path}:{api_route}:1"),
                NodeKind::Route,
                api_route.clone(),
                format!("{file_path}::route:{api_route}"),
                file_path,
                if normalized.ends_with(".vue") {
                    Language::Vue
                } else {
                    Language::TypeScript
                },
                1,
                None,
                None,
            ));
        }
        if let Some(middleware_index) = normalized.find("/middleware/") {
            let after = &normalized[middleware_index + "/middleware/".len()..];
            let middleware_name = strip_extension(after).to_string();
            result.nodes.push(make_node(
                format!("middleware:{file_path}:{middleware_name}:1"),
                NodeKind::Function,
                middleware_name.clone(),
                format!("{file_path}::middleware:{middleware_name}"),
                file_path,
                if normalized.ends_with(".vue") {
                    Language::Vue
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

fn resolve_alias_import(name: &str, context: &mut dyn ResolutionContext) -> Option<String> {
    let alias_path = name
        .strip_prefix("@/")
        .or_else(|| name.strip_prefix("~/"))
        .map(|rest| format!("src/{rest}"))?;
    for ext in [
        "",
        ".ts",
        ".js",
        ".vue",
        "/index.ts",
        "/index.js",
        "/index.vue",
    ] {
        let full_path = format!("{alias_path}{ext}");
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
    // Nuxt/Vue 常按文件名自动注册组件。先找同目录组件，再接受唯一匹配，
    // 不在多重候选时猜测跨目录优先级。
    let matches = context
        .get_all_files()
        .into_iter()
        .filter(|file| file.ends_with(".vue"))
        .filter(|file| {
            file.rsplit(['/', '\\'])
                .next()
                .and_then(|base| base.strip_suffix(".vue"))
                == Some(name)
        })
        .collect::<Vec<_>>();
    if matches.is_empty() {
        return None;
    }
    let mut component_in = |file: &str| -> Option<String> {
        context
            .get_nodes_in_file(file)
            .into_iter()
            .find(|node| node.kind == NodeKind::Component && node.name == name)
            .map(|node| node.id)
    };
    let from_dir = from_file
        .rfind('/')
        .map(|idx| &from_file[..idx])
        .unwrap_or("");
    if let Some(same_dir) = matches.iter().find(|file| file.starts_with(from_dir)) {
        return component_in(same_dir);
    }
    (matches.len() == 1)
        .then(|| component_in(&matches[0]))
        .flatten()
}

fn file_path_to_nuxt_route(normalized: &str, after_pages_start: usize) -> Option<String> {
    let after_pages = &normalized[after_pages_start..];
    let without_ext = after_pages.strip_suffix(".vue").unwrap_or(after_pages);
    let without_index = without_ext.trim_end_matches("/index");
    let route = convert_bracket_route(without_index);
    Some(if route == "/" {
        route
    } else {
        route.trim_end_matches('/').to_string()
    })
}

fn convert_bracket_route(path: &str) -> String {
    // 与 SvelteKit 保持一致的路由参数表示，方便 MCP 输出和后续 flow 展示统一。
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

fn strip_extension(path: &str) -> &str {
    path.rsplit_once('.').map(|(stem, _)| stem).unwrap_or(path)
}

fn is_pascal_case(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some('A'..='Z')) && chars.all(|ch| ch.is_ascii_alphanumeric())
}
