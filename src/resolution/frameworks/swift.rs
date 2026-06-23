//! SwiftUI, UIKit, and Vapor framework resolvers translated from `swift.ts`.
//!
//! Swift 生态的 UI 与服务端框架都高度依赖命名/目录约定。这里把 SwiftUI/UIKit
//! 类型和 Vapor 路由补进图，让跨框架入口能继续走普通引用解析。

use regex::Regex;

use crate::resolution::strip_comments::{CommentLang, strip_comments_for_regex};
use crate::resolution::types::{
    FrameworkExtractionResult, FrameworkResolver, ResolutionContext, ResolvedRef, UnresolvedRef,
    line_for_byte, make_node, make_reference, resolve_by_name_and_kind,
};
use crate::types::{Language, NodeKind, ReferenceKind};

const VIEW_DIRS: &[&str] = &["/Views/", "/View/", "/Screens/", "/Components/", "/UI/"];
const VIEWMODEL_DIRS: &[&str] = &[
    "/ViewModels/",
    "/ViewModel/",
    "/Stores/",
    "/Managers/",
    "/Services/",
];
const MODEL_DIRS: &[&str] = &["/Models/", "/Model/", "/Entities/", "/Domain/"];
const VC_DIRS: &[&str] = &[
    "/ViewControllers/",
    "/ViewController/",
    "/Controllers/",
    "/Screens/",
];
const UIVIEW_DIRS: &[&str] = &["/Views/", "/View/", "/UI/", "/Components/"];
const CELL_DIRS: &[&str] = &[
    "/Cells/",
    "/Cell/",
    "/Views/",
    "/TableViewCells/",
    "/CollectionViewCells/",
];
const VAPOR_CONTROLLER_DIRS: &[&str] = &["/Controllers/", "/Controller/", "/Routes/"];
const FLUENT_MODEL_DIRS: &[&str] = &["/Models/", "/Model/", "/Entities/", "/Database/"];
const VAPOR_MIDDLEWARE_DIRS: &[&str] = &["/Middleware/", "/Middlewares/"];

const VIEW_KINDS: &[NodeKind] = &[NodeKind::Struct, NodeKind::Component];
const CLASS_KINDS: &[NodeKind] = &[NodeKind::Class];
const MODEL_KINDS: &[NodeKind] = &[NodeKind::Struct, NodeKind::Class];
const PROTOCOL_KINDS: &[NodeKind] = &[NodeKind::Protocol];
const VAPOR_CONTROLLER_KINDS: &[NodeKind] = &[NodeKind::Class, NodeKind::Struct];

pub struct SwiftUiResolver;
pub struct UIKitResolver;
pub struct VaporResolver;

pub const SWIFTUI_RESOLVER: SwiftUiResolver = SwiftUiResolver;
pub const UIKIT_RESOLVER: UIKitResolver = UIKitResolver;
pub const VAPOR_RESOLVER: VaporResolver = VaporResolver;

impl FrameworkResolver for SwiftUiResolver {
    fn name(&self) -> &'static str {
        "swiftui"
    }

    fn languages(&self) -> Option<&'static [Language]> {
        Some(&[Language::Swift])
    }

    fn detect(&self, context: &mut dyn ResolutionContext) -> bool {
        for file in context.get_all_files() {
            if file.ends_with(".swift")
                && context
                    .read_file(&file)
                    .is_some_and(|content| content.contains("import SwiftUI"))
            {
                return true;
            }
            if file.ends_with(".xcodeproj") || file.ends_with(".xcworkspace") {
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
        // SwiftUI 里 View、ViewModel、Store 等后缀很强，但仍按目录过滤，
        // 避免同名模型或工具类型抢占 UI flow。
        if reference.reference_name.ends_with("View")
            && starts_upper(&reference.reference_name)
            && let Some(target) =
                resolve_by_name_and_kind(&reference.reference_name, VIEW_KINDS, VIEW_DIRS, context)
        {
            return Some(ResolvedRef::framework(reference, target, 0.85));
        }
        if (reference.reference_name.ends_with("ViewModel")
            || reference.reference_name.ends_with("Store")
            || reference.reference_name.ends_with("Manager"))
            && let Some(target) = resolve_by_name_and_kind(
                &reference.reference_name,
                CLASS_KINDS,
                VIEWMODEL_DIRS,
                context,
            )
        {
            return Some(ResolvedRef::framework(reference, target, 0.85));
        }
        if is_pascal_word(&reference.reference_name)
            && let Some(target) = resolve_by_name_and_kind(
                &reference.reference_name,
                MODEL_KINDS,
                MODEL_DIRS,
                context,
            )
        {
            return Some(ResolvedRef::framework(reference, target, 0.7));
        }
        None
    }

    fn extract(&self, file_path: &str, content: &str) -> FrameworkExtractionResult {
        if !file_path.ends_with(".swift") {
            return FrameworkExtractionResult::default();
        }
        let safe = strip_comments_for_regex(content, CommentLang::Swift);
        let content = safe.as_str();
        let mut result = FrameworkExtractionResult::default();
        let view_re = Regex::new(r"struct\s+(\w+)\s*:\s*(?:\w+\s*,\s*)*View").unwrap();
        for caps in view_re.captures_iter(content) {
            let whole = caps.get(0).unwrap();
            let name = caps.get(1).unwrap().as_str();
            let line = line_for_byte(content, whole.start());
            result.nodes.push(make_node(
                format!("view:{file_path}:{name}:{line}"),
                NodeKind::Component,
                name,
                format!("{file_path}::{name}"),
                file_path,
                Language::Swift,
                line,
                None,
                None,
            ));
        }
        let app_re = Regex::new(r"@main\s+struct\s+(\w+)\s*:\s*App").unwrap();
        for caps in app_re.captures_iter(content) {
            let whole = caps.get(0).unwrap();
            let name = caps.get(1).unwrap().as_str();
            let line = line_for_byte(content, whole.start());
            result.nodes.push(make_node(
                format!("app:{file_path}:{name}:{line}"),
                NodeKind::Class,
                name,
                format!("{file_path}::{name}"),
                file_path,
                Language::Swift,
                line,
                None,
                None,
            ));
        }
        result
    }
}

impl FrameworkResolver for UIKitResolver {
    fn name(&self) -> &'static str {
        "uikit"
    }

    fn languages(&self) -> Option<&'static [Language]> {
        Some(&[Language::Swift])
    }

    fn detect(&self, context: &mut dyn ResolutionContext) -> bool {
        context.get_all_files().into_iter().any(|file| {
            file.ends_with(".swift")
                && context.read_file(&file).is_some_and(|content| {
                    content.contains("import UIKit")
                        || content.contains("UIViewController")
                        || content.contains("UIView")
                })
        })
    }

    fn resolve(
        &self,
        reference: &UnresolvedRef,
        context: &mut dyn ResolutionContext,
    ) -> Option<ResolvedRef> {
        // UIKit 的 ViewController/View/Cell 后缀对应不同目录和节点 kind，
        // 拆开处理能减少 `FooView` 和 `FooViewController` 的互相误配。
        if reference.reference_name.ends_with("ViewController")
            && let Some(target) =
                resolve_by_name_and_kind(&reference.reference_name, CLASS_KINDS, VC_DIRS, context)
        {
            return Some(ResolvedRef::framework(reference, target, 0.85));
        }
        if reference.reference_name.ends_with("View")
            && !reference.reference_name.ends_with("ViewController")
            && let Some(target) = resolve_by_name_and_kind(
                &reference.reference_name,
                CLASS_KINDS,
                UIVIEW_DIRS,
                context,
            )
        {
            return Some(ResolvedRef::framework(reference, target, 0.8));
        }
        if reference.reference_name.ends_with("Cell")
            && let Some(target) =
                resolve_by_name_and_kind(&reference.reference_name, CLASS_KINDS, CELL_DIRS, context)
        {
            return Some(ResolvedRef::framework(reference, target, 0.85));
        }
        if (reference.reference_name.ends_with("Delegate")
            || reference.reference_name.ends_with("DataSource"))
            && let Some(target) =
                resolve_by_name_and_kind(&reference.reference_name, PROTOCOL_KINDS, &[], context)
        {
            return Some(ResolvedRef::framework(reference, target, 0.8));
        }
        None
    }

    fn extract(&self, file_path: &str, content: &str) -> FrameworkExtractionResult {
        if !file_path.ends_with(".swift") {
            return FrameworkExtractionResult::default();
        }
        // Vapor 分组路由会把前缀保存在局部变量或 closure 参数里；先收集 group
        // 前缀，再提取最终 method/path/handler。
        let safe = strip_comments_for_regex(content, CommentLang::Swift);
        let content = safe.as_str();
        let mut result = FrameworkExtractionResult::default();
        let vc_re = Regex::new(r"class\s+(\w+)\s*:\s*(?:\w+\s*,\s*)*UIViewController").unwrap();
        for caps in vc_re.captures_iter(content) {
            let whole = caps.get(0).unwrap();
            let name = caps.get(1).unwrap().as_str();
            let line = line_for_byte(content, whole.start());
            result.nodes.push(make_node(
                format!("viewcontroller:{file_path}:{name}:{line}"),
                NodeKind::Class,
                name,
                format!("{file_path}::{name}"),
                file_path,
                Language::Swift,
                line,
                None,
                None,
            ));
        }
        let view_re = Regex::new(r"class\s+(\w+)\s*:\s*(?:\w+\s*,\s*)*UIView[^C]").unwrap();
        for caps in view_re.captures_iter(content) {
            let whole = caps.get(0).unwrap();
            let name = caps.get(1).unwrap().as_str();
            let line = line_for_byte(content, whole.start());
            result.nodes.push(make_node(
                format!("uiview:{file_path}:{name}:{line}"),
                NodeKind::Class,
                name,
                format!("{file_path}::{name}"),
                file_path,
                Language::Swift,
                line,
                None,
                None,
            ));
        }
        result
    }
}

impl FrameworkResolver for VaporResolver {
    fn name(&self) -> &'static str {
        "vapor"
    }

    fn languages(&self) -> Option<&'static [Language]> {
        Some(&[Language::Swift])
    }

    fn detect(&self, context: &mut dyn ResolutionContext) -> bool {
        context
            .read_file("Package.swift")
            .is_some_and(|content| content.contains("vapor"))
            || context.get_all_files().into_iter().any(|file| {
                file.ends_with(".swift")
                    && context
                        .read_file(&file)
                        .is_some_and(|content| content.contains("import Vapor"))
            })
    }

    fn resolve(
        &self,
        reference: &UnresolvedRef,
        context: &mut dyn ResolutionContext,
    ) -> Option<ResolvedRef> {
        if reference.reference_name.ends_with("Controller")
            && let Some(target) = resolve_by_name_and_kind(
                &reference.reference_name,
                VAPOR_CONTROLLER_KINDS,
                VAPOR_CONTROLLER_DIRS,
                context,
            )
        {
            return Some(ResolvedRef::framework(reference, target, 0.85));
        }
        if is_pascal_word(&reference.reference_name)
            && let Some(target) = resolve_by_name_and_kind(
                &reference.reference_name,
                CLASS_KINDS,
                FLUENT_MODEL_DIRS,
                context,
            )
        {
            return Some(ResolvedRef::framework(reference, target, 0.75));
        }
        if reference.reference_name.ends_with("Middleware")
            && let Some(target) = resolve_by_name_and_kind(
                &reference.reference_name,
                VAPOR_CONTROLLER_KINDS,
                VAPOR_MIDDLEWARE_DIRS,
                context,
            )
        {
            return Some(ResolvedRef::framework(reference, target, 0.8));
        }
        None
    }

    fn extract(&self, file_path: &str, content: &str) -> FrameworkExtractionResult {
        if !file_path.ends_with(".swift") {
            return FrameworkExtractionResult::default();
        }
        let safe = strip_comments_for_regex(content, CommentLang::Swift);
        let content = safe.as_str();
        let mut result = FrameworkExtractionResult::default();
        let mut group_prefix = std::collections::HashMap::<String, String>::new();
        let grouped_re = Regex::new(r#"\blet\s+(\w+)\s*=\s*(\w+)\.grouped\s*\(([^)]*)\)"#).unwrap();
        for caps in grouped_re.captures_iter(content) {
            let var = caps.get(1).unwrap().as_str();
            let parent = caps.get(2).unwrap().as_str();
            let prefix = group_prefix.get(parent).cloned().unwrap_or_default();
            group_prefix.insert(
                var.to_string(),
                seg_join(&prefix, caps.get(3).unwrap().as_str()),
            );
        }
        let group_closure_re =
            Regex::new(r#"\b(\w+)\.group\s*\(([^)]*)\)\s*\{\s*(\w+)\s+in"#).unwrap();
        for caps in group_closure_re.captures_iter(content) {
            let parent = caps.get(1).unwrap().as_str();
            let var = caps.get(3).unwrap().as_str();
            let prefix = group_prefix.get(parent).cloned().unwrap_or_default();
            group_prefix.insert(
                var.to_string(),
                seg_join(&prefix, caps.get(2).unwrap().as_str()),
            );
        }

        let route_re = Regex::new(r#"\b(\w+)\.(get|post|put|patch|delete|head|options)\s*\(\s*((?:[^,()]+,\s*)*)use:\s*([A-Za-z_][\w.]*)"#).unwrap();
        for caps in route_re.captures_iter(content) {
            let whole = caps.get(0).unwrap();
            let receiver = caps.get(1).unwrap().as_str();
            let method = caps.get(2).unwrap().as_str().to_ascii_uppercase();
            let prefix = group_prefix.get(receiver).cloned().unwrap_or_default();
            let route_path = {
                let route = format!("{}{}", prefix, seg_join("", caps.get(3).unwrap().as_str()));
                if route.is_empty() {
                    "/".to_string()
                } else {
                    route
                }
            };
            let handler = caps
                .get(4)
                .unwrap()
                .as_str()
                .rsplit('.')
                .next()
                .unwrap_or("");
            let line = line_for_byte(content, whole.start());
            let route_id = format!("route:{file_path}:{line}:{method}:{route_path}");
            result.nodes.push(make_node(
                route_id.clone(),
                NodeKind::Route,
                format!("{method} {route_path}"),
                format!("{file_path}::route:{route_path}"),
                file_path,
                Language::Swift,
                line,
                None,
                None,
            ));
            if !handler.is_empty() {
                result.references.push(make_reference(
                    route_id,
                    handler,
                    ReferenceKind::References,
                    line,
                    0,
                    file_path,
                    Language::Swift,
                ));
            }
        }
        result
    }
}

fn seg_join(existing: &str, segs: &str) -> String {
    // Vapor 路由段通常是多个字符串参数：`grouped("api", "v1")`。
    // 这里只拼接字面量段，动态表达式留给后续更精细的分析。
    let segment_re = Regex::new(r#""([^"]*)""#).unwrap();
    let suffix = segment_re
        .captures_iter(segs)
        .map(|caps| format!("/{}", caps.get(1).unwrap().as_str()))
        .collect::<String>();
    format!("{existing}{suffix}")
}

fn starts_upper(value: &str) -> bool {
    value
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_uppercase())
}

fn is_pascal_word(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some('A'..='Z')) && chars.all(|ch| ch.is_ascii_alphabetic())
}
