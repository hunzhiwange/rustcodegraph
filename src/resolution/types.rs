//! Reference resolution types.
//!
//! This mirrors `src/resolution/types.ts`: extracted references are normalized
//! into a non-optional resolver shape, resolution strategies return a target
//! node plus confidence, and framework resolvers share the same context trait
//! as the core resolver.
//!
//! 中文维护提示：这里是 resolver、framework resolver 与测试替身之间的契约层。
//! 修改字段或默认方法会影响 import/name/framework 三条解析链路。

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::types::{ColumnNumber, Count, Language, LineNumber, Node, NodeKind, ReferenceKind};

use super::go_module::GoModule;
use super::path_aliases::AliasMap;
use super::workspace_packages::WorkspacePackages;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnresolvedRef {
    /// 发起引用的节点 id；最终边的 source。
    pub from_node_id: String,
    /// extractor 归一化后的引用名，可能是裸名、FQN、路径或链式调用形状。
    pub reference_name: String,
    pub reference_kind: ReferenceKind,
    pub line: LineNumber,
    pub column: ColumnNumber,
    pub file_path: String,
    pub language: Language,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidates: Option<Vec<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ResolvedBy {
    #[serde(rename = "exact-match")]
    ExactMatch,
    #[serde(rename = "import")]
    Import,
    #[serde(rename = "qualified-name")]
    QualifiedName,
    #[serde(rename = "framework")]
    Framework,
    #[serde(rename = "fuzzy")]
    Fuzzy,
    #[serde(rename = "instance-method")]
    InstanceMethod,
    #[serde(rename = "file-path")]
    FilePath,
    #[serde(rename = "function-ref")]
    FunctionRef,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedRef {
    pub original: UnresolvedRef,
    pub target_node_id: String,
    pub confidence: f64,
    pub resolved_by: ResolvedBy,
}

impl ResolvedRef {
    /// 框架 resolver 的统一构造器，保证 resolved_by metadata 一致。
    pub fn framework(
        original: &UnresolvedRef,
        target_node_id: impl Into<String>,
        confidence: f64,
    ) -> Self {
        Self {
            original: original.clone(),
            target_node_id: target_node_id.into(),
            confidence,
            resolved_by: ResolvedBy::Framework,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolutionStats {
    pub total: Count,
    pub resolved: Count,
    pub unresolved: Count,
    pub by_method: HashMap<String, Count>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolutionResult {
    pub resolved: Vec<ResolvedRef>,
    pub unresolved: Vec<UnresolvedRef>,
    pub stats: ResolutionStats,
}

/// Re-export from a JS/TS barrel file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ReExport {
    Named {
        #[serde(rename = "exportedName")]
        exported_name: String,
        #[serde(rename = "originalName")]
        original_name: String,
        source: String,
    },
    Wildcard {
        source: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportMapping {
    pub local_name: String,
    pub exported_name: String,
    pub source: String,
    pub is_default: bool,
    pub is_namespace: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_path: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameworkExtractionResult {
    pub nodes: Vec<Node>,
    pub references: Vec<UnresolvedRef>,
}

/// Context facade used by resolver helpers.
///
/// Optional TypeScript methods become default trait methods. Production
/// `ReferenceResolver` overrides them; tests and narrow helpers can implement
/// only the required surface.
pub trait ResolutionContext {
    fn get_nodes_in_file(&mut self, file_path: &str) -> Vec<Node>;
    fn get_nodes_by_name(&mut self, name: &str) -> Vec<Node>;
    fn get_nodes_by_qualified_name(&mut self, qualified_name: &str) -> Vec<Node>;
    fn get_nodes_by_kind(&mut self, kind: NodeKind) -> Vec<Node>;
    fn file_exists(&mut self, file_path: &str) -> bool;
    fn read_file(&mut self, file_path: &str) -> Option<String>;
    fn get_project_root(&self) -> String;
    fn get_all_files(&mut self) -> Vec<String>;
    fn get_nodes_by_lower_name(&mut self, lower_name: &str) -> Vec<Node>;
    fn get_import_mappings(&mut self, file_path: &str, language: Language) -> Vec<ImportMapping>;

    fn get_supertypes(&mut self, _type_name: &str, _language: Language) -> Vec<String> {
        Vec::new()
    }

    fn get_node_by_id(&mut self, _id: &str) -> Option<Node> {
        None
    }

    fn get_project_aliases(&mut self) -> Option<AliasMap> {
        None
    }

    fn get_go_module(&mut self) -> Option<GoModule> {
        None
    }

    fn get_workspace_packages(&mut self) -> Option<WorkspacePackages> {
        None
    }

    fn get_re_exports(&mut self, _file_path: &str, _language: Language) -> Vec<ReExport> {
        Vec::new()
    }

    fn list_directories(&mut self, _relative_path: &str) -> Vec<String> {
        Vec::new()
    }

    fn get_cpp_include_dirs(&mut self) -> Vec<String> {
        Vec::new()
    }
}

pub trait FrameworkResolver: Send + Sync {
    fn name(&self) -> &str;

    fn languages(&self) -> Option<&[Language]> {
        None
    }

    fn detect(&self, context: &mut dyn ResolutionContext) -> bool;

    fn resolve(
        &self,
        reference: &UnresolvedRef,
        context: &mut dyn ResolutionContext,
    ) -> Option<ResolvedRef>;

    fn claims_reference(&self, _name: &str) -> bool {
        // 主 resolver 的快速剪枝会调用它；框架若能识别特殊符号前缀，应在这里声明，
        // 否则该引用可能在进入 framework resolve 前就被跳过。
        false
    }

    fn extract(&self, _file_path: &str, _content: &str) -> FrameworkExtractionResult {
        FrameworkExtractionResult::default()
    }

    fn post_extract(&self, _context: &mut dyn ResolutionContext) -> Vec<Node> {
        Vec::new()
    }
}

pub fn language_name(language: Language) -> &'static str {
    match language {
        Language::TypeScript => "typescript",
        Language::JavaScript => "javascript",
        Language::Tsx => "tsx",
        Language::Jsx => "jsx",
        Language::Python => "python",
        Language::Go => "go",
        Language::Rust => "rust",
        Language::Java => "java",
        Language::C => "c",
        Language::Cpp => "cpp",
        Language::CSharp => "csharp",
        Language::Razor => "razor",
        Language::Php => "php",
        Language::Ruby => "ruby",
        Language::Swift => "swift",
        Language::Kotlin => "kotlin",
        Language::Dart => "dart",
        Language::Svelte => "svelte",
        Language::Vue => "vue",
        Language::Astro => "astro",
        Language::Liquid => "liquid",
        Language::Pascal => "pascal",
        Language::Scala => "scala",
        Language::Lua => "lua",
        Language::Luau => "luau",
        Language::ObjC => "objc",
        Language::R => "r",
        Language::Yaml => "yaml",
        Language::Twig => "twig",
        Language::Xml => "xml",
        Language::Properties => "properties",
        Language::Unknown => "unknown",
    }
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

pub fn line_for_byte(content: &str, byte_index: usize) -> LineNumber {
    (content[..byte_index.min(content.len())]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1) as LineNumber
}

pub fn language_for_path(file_path: &str) -> Language {
    // 这里只覆盖 resolver 辅助创建节点/引用时需要的扩展名；主 extraction 的语言
    // 判定在上游模块中维护。
    let lower = file_path.to_ascii_lowercase();
    if lower.ends_with(".tsx") {
        Language::Tsx
    } else if lower.ends_with(".ts") {
        Language::TypeScript
    } else if lower.ends_with(".jsx") {
        Language::Jsx
    } else if lower.ends_with(".js") || lower.ends_with(".mjs") || lower.ends_with(".cjs") {
        Language::JavaScript
    } else if lower.ends_with(".py") {
        Language::Python
    } else if lower.ends_with(".go") {
        Language::Go
    } else if lower.ends_with(".rs") {
        Language::Rust
    } else if lower.ends_with(".java") {
        Language::Java
    } else if lower.ends_with(".kt") {
        Language::Kotlin
    } else if lower.ends_with(".cshtml") || lower.ends_with(".razor") {
        Language::Razor
    } else if lower.ends_with(".cs") {
        Language::CSharp
    } else if lower.ends_with(".php") {
        Language::Php
    } else if lower.ends_with(".rb") {
        Language::Ruby
    } else if lower.ends_with(".swift") {
        Language::Swift
    } else if lower.ends_with(".dart") {
        Language::Dart
    } else if lower.ends_with(".svelte") {
        Language::Svelte
    } else if lower.ends_with(".vue") {
        Language::Vue
    } else if lower.ends_with(".astro") {
        Language::Astro
    } else if lower.ends_with(".yaml") || lower.ends_with(".yml") {
        Language::Yaml
    } else if lower.ends_with(".properties") {
        Language::Properties
    } else {
        Language::Unknown
    }
}

pub fn resolve_by_name_and_kind(
    name: &str,
    kinds: &[NodeKind],
    preferred_dirs: &[&str],
    context: &mut dyn ResolutionContext,
) -> Option<String> {
    // 框架 helper 的小型解析器：先按 kind 过滤，再允许调用方用目录偏好处理
    // controller/view/model 这类约定式位置。
    let candidates = context
        .get_nodes_by_name(name)
        .into_iter()
        .filter(|node| kinds.contains(&node.kind))
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return None;
    }
    if let Some(preferred) = candidates.iter().find(|node| {
        preferred_dirs
            .iter()
            .any(|dir| node.file_path.contains(dir))
    }) {
        return Some(preferred.id.clone());
    }
    candidates.first().map(|node| node.id.clone())
}

#[allow(clippy::too_many_arguments)]
pub fn make_node(
    id: impl Into<String>,
    kind: NodeKind,
    name: impl Into<String>,
    qualified_name: impl Into<String>,
    file_path: impl Into<String>,
    language: Language,
    line: LineNumber,
    signature: Option<String>,
    docstring: Option<String>,
) -> Node {
    // 框架抽取生成的轻量节点默认视为 exported，方便后续 resolver/import 逻辑发现。
    let name = name.into();
    Node {
        id: id.into(),
        kind,
        name: name.clone(),
        qualified_name: qualified_name.into(),
        file_path: file_path.into(),
        language,
        start_line: line,
        end_line: line,
        start_column: 0,
        end_column: name.len() as ColumnNumber,
        docstring,
        signature,
        visibility: None,
        is_exported: Some(true),
        is_async: None,
        is_static: None,
        is_abstract: None,
        decorators: None,
        type_parameters: None,
        return_type: None,
        updated_at: now_ms(),
    }
}

pub fn make_reference(
    from_node_id: impl Into<String>,
    reference_name: impl Into<String>,
    reference_kind: ReferenceKind,
    line: LineNumber,
    column: ColumnNumber,
    file_path: impl Into<String>,
    language: Language,
) -> UnresolvedRef {
    UnresolvedRef {
        from_node_id: from_node_id.into(),
        reference_name: reference_name.into(),
        reference_kind,
        line,
        column,
        file_path: file_path.into(),
        language,
        candidates: None,
    }
}
