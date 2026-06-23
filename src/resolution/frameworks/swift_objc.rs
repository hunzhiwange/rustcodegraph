//! Swift ↔ Objective-C bridge resolver translated from `swift-objc.ts`.
//!
//! Swift/Objective-C 混编项目会通过 selector 和 `@objc` 暴露跨语言调用。
//! 这里只做低置信但有用的桥接，优先避免把通用方法名误连。

use crate::resolution::types::{FrameworkResolver, ResolutionContext, ResolvedRef, UnresolvedRef};
use crate::types::{Language, Node, NodeKind};

const GENERIC_NAMES: &[&str] = &[
    "init",
    "description",
    "debugDescription",
    "hash",
    "isEqual",
    "isEqualTo",
    "copy",
    "mutableCopy",
    "class",
    "self",
    "count",
    "length",
    "value",
    "name",
    "data",
    "string",
    "object",
    "add",
    "remove",
    "update",
    "load",
    "save",
    "reload",
    "cancel",
    "start",
    "stop",
    "pause",
    "resume",
    "close",
    "open",
    "show",
    "hide",
    "toString",
    "dealloc",
    "release",
    "retain",
    "autorelease",
];
const SOURCE_PROBE_LINES: usize = 3;

pub struct SwiftObjcBridgeResolver;

pub const SWIFT_OBJC_BRIDGE_RESOLVER: SwiftObjcBridgeResolver = SwiftObjcBridgeResolver;

impl FrameworkResolver for SwiftObjcBridgeResolver {
    fn name(&self) -> &'static str {
        "swift-objc-bridge"
    }

    fn languages(&self) -> Option<&'static [Language]> {
        Some(&[Language::Swift, Language::ObjC])
    }

    fn detect(&self, context: &mut dyn ResolutionContext) -> bool {
        let mut has_swift = false;
        let mut has_objc = false;
        for file in context.get_all_files() {
            if file.ends_with(".swift") {
                has_swift = true;
            } else if file.ends_with(".m") || file.ends_with(".mm") {
                has_objc = true;
            }
            if has_swift && has_objc {
                return true;
            }
        }
        false
    }

    fn claims_reference(&self, name: &str) -> bool {
        name.contains(':')
    }

    fn resolve(
        &self,
        reference: &UnresolvedRef,
        context: &mut dyn ResolutionContext,
    ) -> Option<ResolvedRef> {
        match reference.language {
            Language::Swift => resolve_swift_call_to_objc(reference, context),
            Language::ObjC => resolve_objc_call_to_swift(reference, context),
            _ => None,
        }
    }
}

fn resolve_swift_call_to_objc(
    reference: &UnresolvedRef,
    context: &mut dyn ResolutionContext,
) -> Option<ResolvedRef> {
    // Swift 侧通常看到的是 selector 的 base name；跳过 init/count 等通用名称，
    // 防止任何 ObjC 类的常见方法都被当成目标。
    let raw_name = reference
        .reference_name
        .rsplit('.')
        .next()
        .unwrap_or(&reference.reference_name);
    let target = context
        .get_nodes_by_kind(NodeKind::Method)
        .into_iter()
        .filter(|node| node.language == Language::ObjC)
        .find(|node| {
            swift_base_names_for_objc_selector(&node.name)
                .into_iter()
                .any(|candidate| {
                    candidate == raw_name && !GENERIC_NAMES.contains(&candidate.as_str())
                })
        })?;
    Some(ResolvedRef::framework(reference, target.id, 0.6))
}

fn resolve_objc_call_to_swift(
    reference: &UnresolvedRef,
    context: &mut dyn ResolutionContext,
) -> Option<ResolvedRef> {
    let raw_selector = reference
        .reference_name
        .rsplit('.')
        .next()
        .unwrap_or(&reference.reference_name);
    if !raw_selector.contains(':') {
        return None;
    }
    for candidate in swift_base_names_for_objc_selector(raw_selector) {
        for node in context.get_nodes_by_name(&candidate) {
            if node.language == Language::Swift
                && matches!(node.kind, NodeKind::Method | NodeKind::Function)
                && is_objc_exposed(&declaration_source_window(&node, context))
            {
                return Some(ResolvedRef::framework(reference, node.id, 0.6));
            }
        }
    }
    None
}

fn swift_base_names_for_objc_selector(selector: &str) -> Vec<String> {
    let selector = selector.trim_matches(':');
    if selector.is_empty() {
        return Vec::new();
    }
    let first = selector.split(':').next().unwrap_or(selector);
    let mut candidates = vec![first.to_string()];
    for suffix in ["With", "By", "For", "From", "To", "In", "On"] {
        if let Some(idx) = first.find(suffix) {
            let base = &first[..idx];
            if !base.is_empty() {
                candidates.push(base.to_string());
            }
        }
    }
    candidates.sort();
    candidates.dedup();
    candidates
}

fn declaration_source_window(node: &Node, context: &mut dyn ResolutionContext) -> String {
    // `@objc` 往往在声明前几行而不是节点体内；只取一个小窗口检查，避免读取
    // 整个文件参与匹配。
    let Some(content) = context.read_file(&node.file_path) else {
        return String::new();
    };
    let lines = content.lines().collect::<Vec<_>>();
    let end = node.start_line as usize;
    let start = end.saturating_sub(SOURCE_PROBE_LINES + 1);
    lines[start..end.min(lines.len())].join("\n")
}

fn is_objc_exposed(source: &str) -> bool {
    source.contains("@objc") && !source.contains("@nonobjc")
}
