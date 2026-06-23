//! React Native Fabric / Codegen view resolver translated from `fabric.ts`.
//!
//! Fabric 相关节点来自 TS codegenNativeComponent、ObjC legacy ViewManager 和
//! Java/Kotlin @ReactProp。抽出的 component/prop 节点随后由 synthesizer 跨语言连接。

use regex::Regex;

use crate::resolution::types::{
    FrameworkExtractionResult, FrameworkResolver, ResolutionContext, make_node,
};
use crate::types::{Language, NodeKind};

pub struct FabricViewResolver;

pub const FABRIC_VIEW_RESOLVER: FabricViewResolver = FabricViewResolver;

impl FrameworkResolver for FabricViewResolver {
    fn name(&self) -> &'static str {
        "fabric-view"
    }

    fn languages(&self) -> Option<&'static [Language]> {
        Some(&[
            Language::TypeScript,
            Language::Tsx,
            Language::ObjC,
            Language::Java,
            Language::Kotlin,
        ])
    }

    fn detect(&self, context: &mut dyn ResolutionContext) -> bool {
        // monorepo 常把 react-native 依赖放在 package 子目录，检测时扫一层常见 root。
        if package_has_react_native("package.json", context) {
            return true;
        }
        for root in ["packages", "apps", "modules", "libraries"] {
            for sub in context.list_directories(root) {
                if package_has_react_native(&format!("{root}/{sub}/package.json"), context) {
                    return true;
                }
            }
        }
        false
    }

    fn extract(&self, file_path: &str, source: &str) -> FrameworkExtractionResult {
        let nodes = if file_path.ends_with(".ts") || file_path.ends_with(".tsx") {
            extract_fabric_nodes(file_path, source)
        } else if file_path.ends_with(".m") || file_path.ends_with(".mm") {
            extract_legacy_view_manager_nodes(file_path, source)
        } else if file_path.ends_with(".java") || file_path.ends_with(".kt") {
            extract_jvm_view_manager_nodes(file_path, source)
        } else {
            Vec::new()
        };
        FrameworkExtractionResult {
            nodes,
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

fn package_has_react_native(path: &str, context: &mut dyn ResolutionContext) -> bool {
    context
        .read_file(path)
        .is_some_and(|pkg| pkg.contains("\"react-native\"") || pkg.contains("'react-native'"))
}

fn derive_component_name_from_manager(class_name: &str) -> String {
    // RCTFooViewManager / FooManager 都归约为 JS 侧组件名 Foo。
    let mut name = class_name
        .strip_prefix("RCT")
        .unwrap_or(class_name)
        .to_string();
    if let Some(stripped) = name.strip_suffix("ViewManager") {
        name = stripped.to_string();
    } else if let Some(stripped) = name.strip_suffix("Manager") {
        name = stripped.to_string();
    }
    name
}

fn extract_fabric_nodes(file_path: &str, source: &str) -> Vec<crate::types::Node> {
    // TS/TSX codegen 文件是 Fabric 新架构最直接的 JS component 声明。
    if !source.contains("codegenNativeComponent") {
        return Vec::new();
    }
    let mut nodes = Vec::new();
    let decl_re = Regex::new(
        r#"codegenNativeComponent\s*(?:<[^>]+>)?\s*\(\s*['"]([A-Za-z_][A-Za-z0-9_]*)['"]"#,
    )
    .unwrap();
    let lang = if file_path.ends_with(".tsx") {
        Language::Tsx
    } else {
        Language::TypeScript
    };
    for caps in decl_re.captures_iter(source) {
        let whole = caps.get(0).unwrap();
        let component_name = caps.get(1).unwrap().as_str();
        let line = line_for_offset(source, whole.start());
        let mut node = make_node(
            format!("fabric-component:{file_path}:{component_name}:{line}"),
            NodeKind::Component,
            component_name,
            format!("{file_path}::{component_name}"),
            file_path,
            lang,
            line,
            Some(format!(
                "codegenNativeComponent<NativeProps>('{component_name}')"
            )),
            Some(format!(
                "Fabric/Codegen native component '{component_name}'"
            )),
        );
        node.is_exported = Some(true);
        nodes.push(node);
    }
    if let Some(body) = find_native_props_body(source) {
        // NativeProps 的字段作为 property 节点，便于 inspect bridge surface。
        for prop_name in extract_prop_names(&body) {
            let offset = source.find(&prop_name).unwrap_or(0);
            let line = line_for_offset(source, offset);
            let mut node = make_node(
                format!("fabric-prop:{file_path}:{prop_name}:{line}"),
                NodeKind::Property,
                prop_name.clone(),
                format!("{file_path}::NativeProps.{prop_name}"),
                file_path,
                lang,
                line,
                None,
                Some(format!("Fabric NativeProps prop '{prop_name}'")),
            );
            node.is_exported = Some(true);
            nodes.push(node);
        }
    }
    nodes
}

fn find_native_props_body(source: &str) -> Option<String> {
    Regex::new(r"(?s)export\s+interface\s+NativeProps\b[^{]*\{(.*?)\n\}")
        .unwrap()
        .captures(source)
        .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()))
}

fn extract_prop_names(body: &str) -> Vec<String> {
    let prop_re = Regex::new(r"(?m)^\s*([A-Za-z_][A-Za-z0-9_]*)\??\s*:").unwrap();
    prop_re
        .captures_iter(body)
        .filter_map(|caps| caps.get(1).map(|m| m.as_str().to_string()))
        .collect()
}

fn extract_legacy_view_manager_nodes(file_path: &str, source: &str) -> Vec<crate::types::Node> {
    // ObjC 旧架构 ViewManager 通过 RCT_*_VIEW_PROPERTY 宏暴露 props。
    if !(source.contains("RCT_EXPORT_VIEW_PROPERTY")
        || source.contains("RCT_CUSTOM_VIEW_PROPERTY")
        || source.contains("RCT_REMAP_VIEW_PROPERTY"))
    {
        return Vec::new();
    }
    let Some(class_name) = Regex::new(r"@implementation\s+([A-Za-z_][A-Za-z0-9_]*)")
        .unwrap()
        .captures(source)
        .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()))
    else {
        return Vec::new();
    };
    if !(class_name.ends_with("Manager") || class_name.ends_with("ViewManager")) {
        return Vec::new();
    }
    let component_name = derive_component_name_from_manager(&class_name);
    let mut nodes = vec![fabric_component_node(
        file_path,
        source,
        &component_name,
        &class_name,
        Language::ObjC,
    )];
    let prop_re =
        Regex::new(r"\bRCT_(?:EXPORT|CUSTOM|REMAP)_VIEW_PROPERTY\s*\(\s*([A-Za-z_][A-Za-z0-9_]*)")
            .unwrap();
    push_fabric_props(
        file_path,
        source,
        &component_name,
        Language::ObjC,
        &prop_re,
        &mut nodes,
    );
    nodes
}

fn extract_jvm_view_manager_nodes(file_path: &str, source: &str) -> Vec<crate::types::Node> {
    // Android ViewManager 的 @ReactProp(name="...") 是 native prop 的稳定锚点。
    if !source.contains("@ReactProp") {
        return Vec::new();
    }
    let Some(class_name) = Regex::new(r"\bclass\s+([A-Za-z_][A-Za-z0-9_]*)\b")
        .unwrap()
        .captures(source)
        .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()))
    else {
        return Vec::new();
    };
    if !(class_name.ends_with("Manager") || class_name.ends_with("ViewManager")) {
        return Vec::new();
    }
    let component_name = derive_component_name_from_manager(&class_name);
    let language = if file_path.ends_with(".kt") {
        Language::Kotlin
    } else {
        Language::Java
    };
    let mut nodes = vec![fabric_component_node(
        file_path,
        source,
        &component_name,
        &class_name,
        language,
    )];
    let prop_re = Regex::new(r#"@ReactProp\s*\(\s*(?:name\s*=\s*)?"([^"]+)""#).unwrap();
    push_fabric_props(
        file_path,
        source,
        &component_name,
        language,
        &prop_re,
        &mut nodes,
    );
    nodes
}

fn fabric_component_node(
    file_path: &str,
    source: &str,
    component_name: &str,
    class_name: &str,
    language: Language,
) -> crate::types::Node {
    let line = source
        .find(class_name)
        .map(|idx| line_for_offset(source, idx))
        .unwrap_or(1);
    let mut node = make_node(
        format!("fabric-component:{file_path}:{component_name}:{line}"),
        NodeKind::Component,
        component_name,
        format!("{file_path}::{component_name}"),
        file_path,
        language,
        line,
        Some(format!("class {class_name} : ViewManager")),
        Some(format!(
            "React Native view-manager component '{component_name}'"
        )),
    );
    node.is_exported = Some(true);
    node
}

fn push_fabric_props(
    file_path: &str,
    source: &str,
    component_name: &str,
    language: Language,
    prop_re: &Regex,
    nodes: &mut Vec<crate::types::Node>,
) {
    // 同一个 prop 可能被宏/注解重复声明，只保留首个位置，避免属性列表噪声。
    let mut seen = Vec::<String>::new();
    for caps in prop_re.captures_iter(source) {
        let whole = caps.get(0).unwrap();
        let prop_name = caps.get(1).unwrap().as_str();
        if seen.iter().any(|item| item == prop_name) {
            continue;
        }
        seen.push(prop_name.to_string());
        let line = line_for_offset(source, whole.start());
        let mut node = make_node(
            format!("fabric-prop:{file_path}:{prop_name}:{line}"),
            NodeKind::Property,
            prop_name,
            format!("{file_path}::{component_name}.{prop_name}"),
            file_path,
            language,
            line,
            None,
            Some(format!(
                "React Native view prop '{prop_name}' on {component_name}"
            )),
        );
        node.is_exported = Some(true);
        nodes.push(node);
    }
}

fn line_for_offset(source: &str, offset: usize) -> u64 {
    (source[..offset.min(source.len())]
        .bytes()
        .filter(|b| *b == b'\n')
        .count()
        + 1) as u64
}
