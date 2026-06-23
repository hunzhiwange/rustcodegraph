//! 引用解析模块的小型共享工具。
//!
//! 这里放的是纯函数级策略常量：缓存大小、支持链式调用的语言、reference/edge
//! 名称映射，以及跨 JS 家族扩展名判断。

use std::env;

use crate::types::{EdgeKind, Language, NodeKind, ReferenceKind};

use super::ResolvedBy;

const DEFAULT_CACHE_LIMIT: usize = 5_000;

pub(super) fn resolve_cache_limit() -> usize {
    // 允许压测和超大仓库通过环境变量调大缓存；无效值回退默认值，避免启动失败。
    env::var("RUSTCODEGRAPH_RESOLVER_CACHE_SIZE")
        .ok()
        .and_then(|raw| raw.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_CACHE_LIMIT)
}

pub(super) fn supertype_bearing(kind: NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::Class
            | NodeKind::Struct
            | NodeKind::Interface
            | NodeKind::Trait
            | NodeKind::Protocol
            | NodeKind::Enum
    )
}

pub(super) fn chain_language(language: Language) -> bool {
    // 这些语言的 extractor 会产出 `factory().method` 一类延迟形状；动态语言暂不
    // 进入此路径，宁可少连也不把宽泛调用链误连到错误方法。
    matches!(
        language,
        Language::Java
            | Language::Kotlin
            | Language::CSharp
            | Language::Swift
            | Language::Rust
            | Language::Go
            | Language::Scala
            | Language::Dart
            | Language::ObjC
            | Language::Pascal
    )
}

pub(super) fn scoped_chain_language(language: Language) -> bool {
    language == Language::Rust
}

pub(super) fn chain_shape(name: &str) -> bool {
    // 只接受中间带 `().` 且两端都有内容的形状，避免普通 dotted name 被送入
    // 返回类型推断逻辑。
    name.contains("().")
        && name
            .rsplit_once("().")
            .map(|(a, b)| !a.is_empty() && !b.is_empty())
            .unwrap_or(false)
}

pub(super) fn edge_kind_from_reference(reference_kind: ReferenceKind) -> EdgeKind {
    match reference_kind {
        ReferenceKind::Contains => EdgeKind::Contains,
        ReferenceKind::Calls => EdgeKind::Calls,
        ReferenceKind::Imports => EdgeKind::Imports,
        ReferenceKind::Exports => EdgeKind::Exports,
        ReferenceKind::Extends => EdgeKind::Extends,
        ReferenceKind::Implements => EdgeKind::Implements,
        ReferenceKind::References | ReferenceKind::FunctionRef => EdgeKind::References,
        ReferenceKind::TypeOf => EdgeKind::TypeOf,
        ReferenceKind::Returns => EdgeKind::Returns,
        ReferenceKind::Instantiates => EdgeKind::Instantiates,
        ReferenceKind::Overrides => EdgeKind::Overrides,
        ReferenceKind::Decorates => EdgeKind::Decorates,
    }
}

pub(super) fn reference_kind_name(kind: ReferenceKind) -> &'static str {
    match kind {
        ReferenceKind::Contains => "contains",
        ReferenceKind::Calls => "calls",
        ReferenceKind::Imports => "imports",
        ReferenceKind::Exports => "exports",
        ReferenceKind::Extends => "extends",
        ReferenceKind::Implements => "implements",
        ReferenceKind::References => "references",
        ReferenceKind::TypeOf => "type_of",
        ReferenceKind::Returns => "returns",
        ReferenceKind::Instantiates => "instantiates",
        ReferenceKind::Overrides => "overrides",
        ReferenceKind::Decorates => "decorates",
        ReferenceKind::FunctionRef => "function_ref",
    }
}

pub(super) fn resolved_by_name(resolved_by: ResolvedBy) -> &'static str {
    match resolved_by {
        ResolvedBy::ExactMatch => "exact-match",
        ResolvedBy::Import => "import",
        ResolvedBy::QualifiedName => "qualified-name",
        ResolvedBy::Framework => "framework",
        ResolvedBy::Fuzzy => "fuzzy",
        ResolvedBy::InstanceMethod => "instance-method",
        ResolvedBy::FilePath => "file-path",
        ResolvedBy::FunctionRef => "function-ref",
    }
}

pub(super) fn js_family_extension(file_path: &str) -> bool {
    let lower = file_path.to_ascii_lowercase();
    lower.ends_with(".d.ts")
        || lower.ends_with(".ts")
        || lower.ends_with(".tsx")
        || lower.ends_with(".mts")
        || lower.ends_with(".cts")
        || lower.ends_with(".js")
        || lower.ends_with(".jsx")
        || lower.ends_with(".mjs")
        || lower.ends_with(".cjs")
}

pub(super) fn capitalize(value: &str) -> String {
    let mut chars = value.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
        .unwrap_or_default()
}
