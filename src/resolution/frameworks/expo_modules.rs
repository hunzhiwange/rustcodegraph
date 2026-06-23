//! Expo Modules resolver translated from `expo-modules.ts`.
//!
//! Expo Modules 用 Swift/Kotlin DSL 暴露 JS API。这里把 Function/AsyncFunction/
//! Property/Constants 的 DSL 声明抽成 exported method 节点，供跨平台合成边使用。

use regex::Regex;

use crate::resolution::types::{
    FrameworkExtractionResult, FrameworkResolver, ResolutionContext, make_node,
};
use crate::types::{Language, NodeKind};

pub struct ExpoModulesResolver;

pub const EXPO_MODULES_RESOLVER: ExpoModulesResolver = ExpoModulesResolver;

impl FrameworkResolver for ExpoModulesResolver {
    fn name(&self) -> &'static str {
        "expo-modules"
    }

    fn languages(&self) -> Option<&'static [Language]> {
        Some(&[Language::Swift, Language::Kotlin])
    }

    fn detect(&self, context: &mut dyn ResolutionContext) -> bool {
        // package 依赖是强信号；否则只抽样前 200 个 Swift/Kotlin 文件，避免检测阶段
        // 读完整个大仓。
        if context
            .read_file("package.json")
            .is_some_and(|pkg| pkg.contains("expo-modules-core"))
        {
            return true;
        }
        for file in context.get_all_files().into_iter().take(200) {
            if (file.ends_with(".swift") || file.ends_with(".kt"))
                && context
                    .read_file(&file)
                    .is_some_and(|source| is_expo_module_source(&source))
            {
                return true;
            }
        }
        false
    }

    fn extract(&self, file_path: &str, source: &str) -> FrameworkExtractionResult {
        let language = if file_path.ends_with(".kt") {
            Language::Kotlin
        } else {
            Language::Swift
        };
        FrameworkExtractionResult {
            nodes: extract_expo_methods(file_path, source, language),
            references: Vec::new(),
        }
    }

    fn resolve(
        &self,
        _reference: &crate::resolution::types::UnresolvedRef,
        _context: &mut dyn ResolutionContext,
    ) -> Option<crate::resolution::types::ResolvedRef> {
        None
    }
}

fn is_expo_module_source(source: &str) -> bool {
    // 必须同时看到 Module subclass 和至少一个公开 DSL 声明，减少普通 native 类误判。
    Regex::new(r"\bclass\s+([A-Za-z_][A-Za-z0-9_]*)\s*:\s*Module\b")
        .unwrap()
        .is_match(source)
        && Regex::new(r#"\b(Function|AsyncFunction|Property|Constants)\s*(?:<[^(]*>)?\s*\(\s*["']([A-Za-z_][A-Za-z0-9_]*)["']"#)
            .unwrap()
            .is_match(source)
}

fn extract_expo_methods(
    file_path: &str,
    source: &str,
    language: Language,
) -> Vec<crate::types::Node> {
    if !is_expo_module_source(source) {
        return Vec::new();
    }
    // `Name("Foo")` 是 JS 可见 module 名；没有时退到 class 名。
    let module_name = Regex::new(r#"\bName\s*\(\s*["']([A-Za-z_][A-Za-z0-9_]*)["']"#)
        .unwrap()
        .captures(source)
        .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()))
        .or_else(|| {
            Regex::new(r"\bclass\s+([A-Za-z_][A-Za-z0-9_]*)\s*:\s*Module\b")
                .unwrap()
                .captures(source)
                .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()))
        })
        .unwrap_or_else(|| "ExpoModule".to_string());
    let decl_re = Regex::new(r#"\b(Function|AsyncFunction|Property|Constants)\s*(?:<[^(]*>)?\s*\(\s*["']([A-Za-z_][A-Za-z0-9_]*)["']"#).unwrap();
    let mut nodes = Vec::new();
    let mut seen = Vec::<String>::new();
    for caps in decl_re.captures_iter(source) {
        // 同一行同名 DSL 声明只保留一次，避免泛型/重载 regex 产生重复节点。
        let whole = caps.get(0).unwrap();
        let kind = caps.get(1).unwrap().as_str();
        let method_name = caps.get(2).unwrap().as_str();
        let line = line_for_offset(source, whole.start());
        let key = format!("{method_name}:{line}");
        if seen.iter().any(|item| item == &key) {
            continue;
        }
        seen.push(key);
        let mut node = make_node(
            format!("expo-module:{file_path}:{module_name}:{method_name}:{line}"),
            NodeKind::Method,
            method_name,
            format!("{file_path}::{module_name}.{method_name}"),
            file_path,
            language,
            line,
            Some(format!("{kind}(\"{method_name}\")")),
            Some(format!(
                "Expo Modules {kind}(\"{method_name}\") in {module_name}"
            )),
        );
        node.is_exported = Some(true);
        nodes.push(node);
    }
    nodes
}

fn line_for_offset(source: &str, offset: usize) -> u64 {
    (source[..offset.min(source.len())]
        .bytes()
        .filter(|b| *b == b'\n')
        .count()
        + 1) as u64
}
