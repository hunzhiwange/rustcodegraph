//! React Native cross-language bridge resolver translated from `react-native.ts`.
//!
//! React Native bridge 把 JS 方法名映射到 ObjC/Java/Kotlin 原生实现。这里用
//! export 宏、`@ReactMethod` 和 TurboModule spec 建一张保守映射表。

use std::collections::HashMap;

use regex::Regex;

use crate::resolution::types::{
    FrameworkExtractionResult, FrameworkResolver, ResolutionContext, ResolvedRef, UnresolvedRef,
    make_node,
};
use crate::types::{Language, NodeKind};

const RN_EMITTER_BUILTINS: &[&str] = &[
    "addListener",
    "removeListeners",
    "remove",
    "invalidate",
    "startObserving",
    "stopObserving",
];

#[derive(Clone)]
struct NativeMethod {
    node: crate::types::Node,
}

/// React Native bridge 解析入口，优先把 JS/TS 调用连到原生方法节点。
pub struct ReactNativeBridgeResolver;

pub const REACT_NATIVE_BRIDGE_RESOLVER: ReactNativeBridgeResolver = ReactNativeBridgeResolver;

impl FrameworkResolver for ReactNativeBridgeResolver {
    fn name(&self) -> &'static str {
        "react-native-bridge"
    }

    fn languages(&self) -> Option<&'static [Language]> {
        Some(&[
            Language::JavaScript,
            Language::TypeScript,
            Language::Tsx,
            Language::Jsx,
            Language::ObjC,
        ])
    }

    fn detect(&self, context: &mut dyn ResolutionContext) -> bool {
        if context
            .read_file("package.json")
            .is_some_and(|pkg| pkg.contains("\"react-native\"") || pkg.contains("'react-native'"))
        {
            return true;
        }
        for file in context.get_all_files().into_iter().take(200) {
            if (file.ends_with(".m") || file.ends_with(".mm"))
                && context
                    .read_file(&file)
                    .is_some_and(|source| source.contains("RCT_EXPORT_MODULE"))
            {
                return true;
            }
            if (file.ends_with(".ts") || file.ends_with(".tsx"))
                && context
                    .read_file(&file)
                    .is_some_and(|source| source.contains("TurboModuleRegistry.get"))
            {
                return true;
            }
        }
        false
    }

    fn extract(&self, file_path: &str, source: &str) -> FrameworkExtractionResult {
        if !(file_path.ends_with(".m") || file_path.ends_with(".mm"))
            || !source.contains("RCT_EXPORT_MODULE")
        {
            return FrameworkExtractionResult::default();
        }
        let exports = parse_objc_rn_exports(source, find_objc_class_name(source).as_deref());
        let mut nodes = Vec::new();
        let mut seen = Vec::<String>::new();
        for export in exports {
            if seen.iter().any(|item| item == &export.js_name) {
                continue;
            }
            seen.push(export.js_name.clone());
            let mut node = make_node(
                format!(
                    "rn-export:{file_path}:{}.{}",
                    export.module_name, export.js_name
                ),
                NodeKind::Method,
                export.js_name.clone(),
                format!("{file_path}::{}.{}", export.module_name, export.js_name),
                file_path,
                Language::ObjC,
                export.line,
                Some(format!(
                    "RCT_EXPORT_METHOD({}:...)",
                    export.native_selector_first_kw
                )),
                Some(format!(
                    "RCT_EXPORT_METHOD {} (module {})",
                    export.native_selector_first_kw, export.module_name
                )),
            );
            node.is_exported = Some(true);
            nodes.push(node);
        }
        FrameworkExtractionResult {
            nodes,
            references: Vec::new(),
        }
    }

    fn resolve(
        &self,
        reference: &UnresolvedRef,
        context: &mut dyn ResolutionContext,
    ) -> Option<ResolvedRef> {
        if !matches!(
            reference.language,
            Language::JavaScript | Language::TypeScript | Language::Tsx | Language::Jsx
        ) {
            return None;
        }
        let name = reference
            .reference_name
            .rsplit('.')
            .next()
            .unwrap_or(&reference.reference_name);
        let mut maps = build_rn_maps(context);
        let entries = maps.remove(name)?;
        let target = entries
            .iter()
            .find(|entry| entry.node.language == Language::ObjC)
            .or_else(|| entries.first())?;
        Some(ResolvedRef::framework(
            reference,
            target.node.id.clone(),
            0.6,
        ))
    }
}

#[derive(Clone)]
struct ObjcExport {
    module_name: String,
    js_name: String,
    native_selector_first_kw: String,
    line: u64,
}

fn default_objc_module_name(class_name: &str) -> String {
    class_name
        .strip_prefix("RCT")
        .filter(|rest| !rest.is_empty())
        .unwrap_or(class_name)
        .to_string()
}

fn find_objc_class_name(source: &str) -> Option<String> {
    Regex::new(r"@implementation\s+([A-Za-z_][A-Za-z0-9_]*)")
        .unwrap()
        .captures(source)
        .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()))
}

fn parse_objc_rn_exports(source: &str, class_name: Option<&str>) -> Vec<ObjcExport> {
    // ObjC 可以显式 RCT_EXPORT_MODULE(Name)，也可以省略名称让 RN 从类名推导。
    // 两种形式都要归一成 JS 侧可见的 module/method 名。
    let module_name = Regex::new(r"RCT_EXPORT_MODULE\s*\(\s*([A-Za-z_][A-Za-z0-9_]*)?\s*\)")
        .unwrap()
        .captures(source)
        .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()))
        .or_else(|| class_name.map(default_objc_module_name));
    let Some(module_name) = module_name else {
        return Vec::new();
    };
    let mut results = Vec::new();
    let export_re = Regex::new(r"RCT_EXPORT_METHOD\s*\(\s*([A-Za-z_][A-Za-z0-9_]*)").unwrap();
    for caps in export_re.captures_iter(source) {
        let whole = caps.get(0).unwrap();
        let name = caps.get(1).unwrap().as_str().to_string();
        results.push(ObjcExport {
            module_name: module_name.clone(),
            js_name: name.clone(),
            native_selector_first_kw: name,
            line: line_for_offset(source, whole.start()),
        });
    }
    let remap_re = Regex::new(
        r"RCT_REMAP_METHOD\s*\(\s*([A-Za-z_][A-Za-z0-9_]*)\s*,\s*([A-Za-z_][A-Za-z0-9_]*)",
    )
    .unwrap();
    for caps in remap_re.captures_iter(source) {
        let whole = caps.get(0).unwrap();
        results.push(ObjcExport {
            module_name: module_name.clone(),
            js_name: caps.get(1).unwrap().as_str().to_string(),
            native_selector_first_kw: caps.get(2).unwrap().as_str().to_string(),
            line: line_for_offset(source, whole.start()),
        });
    }
    results
}

fn parse_jvm_rn_exports(source: &str) -> Vec<(String, String)> {
    let module_name = Regex::new(
        r#"\bgetName\s*\([^)]*\)\s*(?::\s*String)?\s*(?:=\s*|\{[^}]*return\s*)"([^"]+)""#,
    )
    .unwrap()
    .captures(source)
    .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()))
    .or_else(|| {
        Regex::new(
            r"\bclass\s+([A-Za-z_][A-Za-z0-9_]*)\b[^{]*(?:ReactContextBaseJavaModule|ReactPackage)",
        )
        .unwrap()
        .captures(source)
        .and_then(|caps| {
            caps.get(1)
                .map(|m| m.as_str().trim_end_matches("Module").to_string())
        })
    });
    let Some(module_name) = module_name else {
        return Vec::new();
    };
    let method_re = Regex::new(r"@ReactMethod\b[^{]*?(?:\bfun\s+|\bvoid\s+|\bpublic\s+\w[\w<>\[\]]*\s+)([A-Za-z_][A-Za-z0-9_]*)\s*\(").unwrap();
    method_re
        .captures_iter(source)
        .filter_map(|caps| {
            caps.get(1)
                .map(|m| (module_name.clone(), m.as_str().to_string()))
        })
        .collect()
}

fn parse_turbo_module_spec(source: &str) -> Option<(String, Vec<String>)> {
    // TurboModule spec 是 JS 侧声明，未必有直接调用点；读取 interface 方法后，
    // 再和已索引的 ObjC/JVM 方法名相交，避免凭空生成目标。
    let module_name = Regex::new(
        r#"TurboModuleRegistry\.(?:getEnforcing|get)\s*<[^>]*>\s*\(\s*['"]([^'"]+)['"]\s*\)"#,
    )
    .unwrap()
    .captures(source)
    .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()))?;
    let body = Regex::new(r"(?s)export\s+interface\s+Spec\b[^{]*\{(.*?)\n\}")
        .unwrap()
        .captures(source)
        .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()))?;
    let method_re = Regex::new(r"(?m)^\s*([A-Za-z_][A-Za-z0-9_]*)\s*\(").unwrap();
    let methods = method_re
        .captures_iter(&body)
        .filter_map(|caps| caps.get(1).map(|m| m.as_str().to_string()))
        .collect::<Vec<_>>();
    Some((module_name, methods))
}

fn build_rn_maps(context: &mut dyn ResolutionContext) -> HashMap<String, Vec<NativeMethod>> {
    // 每次解析时从索引重建映射，保证增量同步后不复用旧 bridge；同时跳过 RN
    // emitter 生命周期方法，避免把框架噪音当成业务 flow。
    let mut by_js_name = HashMap::<String, Vec<NativeMethod>>::new();
    let mut objc_methods = HashMap::<String, Vec<crate::types::Node>>::new();
    let mut jvm_methods = HashMap::<String, Vec<crate::types::Node>>::new();
    for node in context.get_nodes_by_kind(NodeKind::Method) {
        if node.language == Language::ObjC {
            let first = node
                .name
                .split(':')
                .next()
                .unwrap_or(&node.name)
                .to_string();
            objc_methods.entry(first).or_default().push(node);
        } else if matches!(node.language, Language::Java | Language::Kotlin) {
            jvm_methods.entry(node.name.clone()).or_default().push(node);
        }
    }

    for file in context.get_all_files() {
        if (file.ends_with(".m") || file.ends_with(".mm"))
            && let Some(source) = context.read_file(&file)
        {
            for export in parse_objc_rn_exports(&source, find_objc_class_name(&source).as_deref()) {
                if RN_EMITTER_BUILTINS.contains(&export.js_name.as_str()) {
                    continue;
                }
                if let Some(node) =
                    objc_methods
                        .get(&export.native_selector_first_kw)
                        .and_then(|nodes| {
                            nodes
                                .iter()
                                .find(|node| node.file_path == file)
                                .or_else(|| nodes.first())
                        })
                {
                    by_js_name
                        .entry(export.js_name.clone())
                        .or_default()
                        .push(NativeMethod { node: node.clone() });
                }
            }
        }
        if (file.ends_with(".java") || file.ends_with(".kt"))
            && let Some(source) = context.read_file(&file)
        {
            for (_module_name, js_name) in parse_jvm_rn_exports(&source) {
                if RN_EMITTER_BUILTINS.contains(&js_name.as_str()) {
                    continue;
                }
                if let Some(node) = jvm_methods.get(&js_name).and_then(|nodes| {
                    nodes
                        .iter()
                        .find(|node| node.file_path == file)
                        .or_else(|| nodes.first())
                }) {
                    by_js_name
                        .entry(js_name.clone())
                        .or_default()
                        .push(NativeMethod { node: node.clone() });
                }
            }
        }
        if (file.ends_with(".ts") || file.ends_with(".tsx"))
            && let Some(source) = context.read_file(&file)
            && let Some((_module_name, methods)) = parse_turbo_module_spec(&source)
        {
            for method in methods {
                if RN_EMITTER_BUILTINS.contains(&method.as_str()) {
                    continue;
                }
                for node in objc_methods
                    .get(&method)
                    .into_iter()
                    .flat_map(|nodes| nodes.iter())
                    .chain(
                        jvm_methods
                            .get(&method)
                            .into_iter()
                            .flat_map(|nodes| nodes.iter()),
                    )
                {
                    by_js_name
                        .entry(method.clone())
                        .or_default()
                        .push(NativeMethod { node: node.clone() });
                }
            }
        }
    }
    by_js_name
}

fn line_for_offset(source: &str, offset: usize) -> u64 {
    (source[..offset.min(source.len())]
        .bytes()
        .filter(|b| *b == b'\n')
        .count()
        + 1) as u64
}
