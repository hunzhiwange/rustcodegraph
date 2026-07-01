//! Local Rust facade for the `web-tree-sitter` declaration file.
//!
//! This keeps the web-tree-sitter-shaped API the translated extraction code
//! expects while backing parsing with native Rust tree-sitter grammars.
//!
//! 迁移过程中大量抽取代码仍按 web-tree-sitter 的对象模型编写；本文件提供一层
//! Rust 本地 facade，让上层不用关心 WASM 与 native tree-sitter 的差异。

use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use crate::types::Language as CodeLanguage;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Point {
    pub row: usize,
    pub column: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Range {
    pub start_position: Point,
    pub end_position: Point,
    pub start_index: usize,
    pub end_index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Edit {
    pub start_position: Point,
    pub old_end_position: Point,
    pub new_end_position: Point,
    pub start_index: usize,
    pub old_end_index: usize,
    pub new_end_index: usize,
}

#[derive(Debug, Clone, Default)]
pub struct ParseOptions {
    pub included_ranges: Vec<Range>,
}

#[derive(Debug, Clone, Default)]
pub struct EmscriptenModule {
    pub options: HashMap<String, String>,
}

pub type ParseCallback = dyn Fn(usize, Point) -> Option<String> + Send + Sync;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldTarget {
    Child(usize),
    NamedChild(usize),
}

#[derive(Debug, Clone, Default)]
pub struct Language {
    native: Option<tree_sitter::Language>,
    pub types: Vec<String>,
    pub fields: Vec<Option<String>>,
    pub name: Option<String>,
    pub version: u32,
    pub abi_version: u32,
    pub field_count: u32,
    pub state_count: u32,
    pub node_type_count: u32,
    pub supertypes: Vec<u32>,
}

impl Language {
    pub fn load(input: &str) -> Result<Self, String> {
        let lower = input.to_ascii_lowercase();
        // 输入可能是语言名，也可能是旧 WASM 文件路径；按包含关系兼容两种调用方式。
        let language = if lower.contains("tsx") {
            CodeLanguage::Tsx
        } else if lower.contains("typescript") {
            CodeLanguage::TypeScript
        } else if lower.contains("javascript") {
            CodeLanguage::JavaScript
        } else if lower.contains("python") {
            CodeLanguage::Python
        } else if lower.contains("go") {
            CodeLanguage::Go
        } else if lower.contains("rust") {
            CodeLanguage::Rust
        } else if lower.contains("csharp") || lower.contains("c-sharp") {
            CodeLanguage::CSharp
        } else if lower.contains("cpp") || lower.contains("c++") {
            CodeLanguage::Cpp
        } else if lower == "c" || lower.ends_with("/c.wasm") {
            CodeLanguage::C
        } else if lower.contains("ruby") {
            CodeLanguage::Ruby
        } else if lower.contains("php") {
            CodeLanguage::Php
        } else if lower.contains("swift") {
            CodeLanguage::Swift
        } else if lower.contains("kotlin") {
            CodeLanguage::Kotlin
        } else if lower.contains("dart") {
            CodeLanguage::Dart
        } else if lower.contains("pascal") {
            CodeLanguage::Pascal
        } else if lower.contains("scala") {
            CodeLanguage::Scala
        } else if lower.contains("luau") {
            CodeLanguage::Luau
        } else if lower.contains("lua") {
            CodeLanguage::Lua
        } else if lower.contains("java") {
            CodeLanguage::Java
        } else {
            return Err(format!(
                "No native tree-sitter grammar registered for {input}"
            ));
        };
        Self::for_code_language(language)
    }

    pub fn for_code_language(language: CodeLanguage) -> Result<Self, String> {
        let native = native_language(language)
            .ok_or_else(|| format!("No native tree-sitter grammar registered for {language:?}"))?;
        Ok(Self::from_native(language, native))
    }

    fn from_native(language: CodeLanguage, native: tree_sitter::Language) -> Self {
        let node_type_count = native.node_kind_count() as u32;
        let field_count = native.field_count() as u32;
        // web-tree-sitter 暴露 node type/field 表；这里启动时从 native grammar 快照一份。
        let types = (0..node_type_count)
            .filter_map(|id| native.node_kind_for_id(id as u16).map(ToOwned::to_owned))
            .collect::<Vec<_>>();
        let fields = (0..=field_count)
            .map(|id| native.field_name_for_id(id as u16).map(ToOwned::to_owned))
            .collect::<Vec<_>>();
        Self {
            native: Some(native.clone()),
            types,
            fields,
            name: Some(format!("{language:?}")),
            version: native.abi_version() as u32,
            abi_version: native.abi_version() as u32,
            field_count,
            state_count: native.parse_state_count() as u32,
            node_type_count,
            supertypes: Vec::new(),
        }
    }

    pub fn field_id_for_name(&self, field_name: &str) -> Option<u32> {
        self.fields
            .iter()
            .position(|field| field.as_deref() == Some(field_name))
            .map(|idx| idx as u32)
    }

    pub fn field_name_for_id(&self, field_id: u32) -> Option<&str> {
        self.fields
            .get(field_id as usize)
            .and_then(|field| field.as_deref())
    }

    pub fn id_for_node_type(&self, node_type: &str, _named: bool) -> Option<u32> {
        // native API 的 id_for_node_kind 需要 named 参数；抽取代码历史上只依赖命名节点。
        self.native
            .as_ref()
            .map(|language| language.id_for_node_kind(node_type, true) as u32)
            .filter(|id| *id != 0)
            .or_else(|| {
                self.types
                    .iter()
                    .position(|ty| ty == node_type)
                    .map(|idx| idx as u32)
            })
    }

    pub fn node_type_for_id(&self, type_id: u32) -> Option<&str> {
        self.native
            .as_ref()
            .and_then(|language| language.node_kind_for_id(type_id as u16))
            .or_else(|| self.types.get(type_id as usize).map(String::as_str))
    }

    pub fn node_type_is_named(&self, type_id: u32) -> bool {
        self.native
            .as_ref()
            .map(|language| language.node_kind_is_named(type_id as u16))
            .unwrap_or(true)
    }

    pub fn node_type_is_visible(&self, type_id: u32) -> bool {
        self.native
            .as_ref()
            .map(|language| language.node_kind_is_visible(type_id as u16))
            .unwrap_or(true)
    }

    pub fn subtypes(&self, _supertype: u32) -> Vec<u32> {
        Vec::new()
    }

    pub fn next_state(&self, state_id: u32, type_id: u32) -> u32 {
        self.native
            .as_ref()
            .map(|language| language.next_state(state_id as u16, type_id as u16) as u32)
            .unwrap_or(0)
    }
}

#[derive(Debug, Clone, Default)]
pub struct Parser {
    pub language: Option<Language>,
    timeout_micros: u64,
}

impl Parser {
    pub fn init(_module_options: Option<EmscriptenModule>) -> Result<(), String> {
        // native grammar 已静态链接，无需像 WASM 版那样异步初始化运行时模块。
        Ok(())
    }

    pub fn set_language(&mut self, language: Option<Language>) -> &mut Self {
        self.language = language;
        self
    }

    pub fn parse(&mut self, source: &str, old_tree: Option<&Tree>) -> Option<Tree> {
        let language = self.language.clone()?;
        let native_language = language.native.clone()?;
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&native_language).ok()?;
        // old_tree 透传给 native parser，保留增量解析能力；随后转换成兼容节点树。
        let native_old_tree = old_tree.and_then(|tree| tree.native.as_ref());
        let native_tree = parser.parse(source, native_old_tree)?;
        let root_node = syntax_node_from_native(&native_tree.root_node(), source, None);
        Some(Tree {
            native: Some(native_tree),
            language,
            root_node,
        })
    }

    pub fn reset(&mut self) {}

    pub fn get_included_ranges(&self) -> Vec<Range> {
        // 当前 Rust facade 尚未实现 included_ranges，调用方看到空列表等价于全文件解析。
        Vec::new()
    }

    pub fn get_timeout_micros(&self) -> u64 {
        self.timeout_micros
    }

    pub fn set_timeout_micros(&mut self, timeout: u64) {
        self.timeout_micros = timeout;
    }
}

#[derive(Debug, Clone)]
pub struct Tree {
    native: Option<tree_sitter::Tree>,
    pub language: Language,
    pub root_node: SyntaxNode,
}

impl Tree {
    pub fn copy(&self) -> Self {
        self.clone()
    }

    pub fn delete(self) {}

    pub fn root_node_with_offset(&self, _offset_bytes: usize, _offset_extent: Point) -> SyntaxNode {
        // web-tree-sitter 支持 offset view；现有抽取路径只需要根节点本身。
        self.root_node.clone()
    }

    pub fn edit(&mut self, _edit: Edit) {}

    pub fn walk(&self) -> TreeCursor {
        TreeCursor::new(self.root_node.clone())
    }

    pub fn get_changed_ranges(&self, other: &Tree) -> Vec<Range> {
        let Some(this) = &self.native else {
            return Vec::new();
        };
        let Some(other) = &other.native else {
            return Vec::new();
        };
        this.changed_ranges(other)
            .map(range_from_native)
            .collect::<Vec<_>>()
    }

    pub fn get_included_ranges(&self) -> Vec<Range> {
        Vec::new()
    }
}

fn native_language(language: CodeLanguage) -> Option<tree_sitter::Language> {
    // 只有真正有 native grammar 的语言才能通过这里；模板/配置格式由专用抽取器处理。
    match language {
        CodeLanguage::TypeScript => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
        CodeLanguage::Tsx => Some(tree_sitter_typescript::LANGUAGE_TSX.into()),
        CodeLanguage::JavaScript | CodeLanguage::Jsx => {
            Some(tree_sitter_javascript::LANGUAGE.into())
        }
        CodeLanguage::Python => Some(tree_sitter_python::LANGUAGE.into()),
        CodeLanguage::Go => Some(tree_sitter_go::LANGUAGE.into()),
        CodeLanguage::Rust => Some(tree_sitter_rust::LANGUAGE.into()),
        CodeLanguage::Ruby => Some(tree_sitter_ruby::LANGUAGE.into()),
        CodeLanguage::C => Some(tree_sitter_c::LANGUAGE.into()),
        CodeLanguage::Cpp => Some(tree_sitter_cpp::LANGUAGE.into()),
        CodeLanguage::Lua => Some(tree_sitter_lua::LANGUAGE.into()),
        CodeLanguage::Luau => Some(tree_sitter_luau::LANGUAGE.into()),
        CodeLanguage::Java => Some(tree_sitter_java::LANGUAGE.into()),
        CodeLanguage::CSharp => Some(tree_sitter_c_sharp::LANGUAGE.into()),
        CodeLanguage::Php => Some(tree_sitter_php::LANGUAGE_PHP.into()),
        CodeLanguage::Swift => Some(tree_sitter_swift::LANGUAGE.into()),
        CodeLanguage::Kotlin => Some(tree_sitter_kotlin_ng::LANGUAGE.into()),
        CodeLanguage::Dart => Some(tree_sitter_dart::LANGUAGE.into()),
        CodeLanguage::Pascal => Some(tree_sitter_pascal::LANGUAGE.into()),
        CodeLanguage::Scala => Some(tree_sitter_scala::LANGUAGE.into()),
        CodeLanguage::ObjC => Some(tree_sitter_objc::LANGUAGE.into()),
        CodeLanguage::R => Some(tree_sitter_r::LANGUAGE.into()),
        _ => None,
    }
}

fn point_from_native(point: tree_sitter::Point) -> Point {
    Point {
        row: point.row,
        column: point.column,
    }
}

fn range_from_native(range: tree_sitter::Range) -> Range {
    Range {
        start_position: point_from_native(range.start_point),
        end_position: point_from_native(range.end_point),
        start_index: range.start_byte,
        end_index: range.end_byte,
    }
}

fn syntax_node_stub_from_native(
    node: &tree_sitter::Node,
    source: &str,
    parent: Option<Box<SyntaxNode>>,
) -> SyntaxNode {
    // stub 只复制当前节点的轻量信息，用作 parent/sibling 链接，避免递归展开整棵树。
    let text = node.utf8_text(source.as_bytes()).unwrap_or("").to_owned();
    SyntaxNode {
        id: node.id() as u64,
        start_index: node.start_byte(),
        end_index: node.end_byte(),
        start_position: point_from_native(node.start_position()),
        end_position: point_from_native(node.end_position()),
        node_type: node.kind().to_owned(),
        grammar_type: node.grammar_name().to_owned(),
        text,
        is_named: node.is_named(),
        is_extra: node.is_extra(),
        is_error: node.is_error(),
        is_missing: node.is_missing(),
        has_changes: false,
        has_error: node.has_error(),
        parse_state: node.parse_state() as u32,
        next_parse_state: node.next_parse_state() as u32,
        children: NodeList::default(),
        named_children: NodeList::default(),
        field_names: HashMap::new(),
        parent,
        previous_named_sibling: None,
        next_named_sibling: None,
    }
}

fn syntax_node_from_native(
    node: &tree_sitter::Node,
    source: &str,
    parent: Option<Box<SyntaxNode>>,
) -> SyntaxNode {
    let mut converted = syntax_node_stub_from_native(node, source, parent);
    let parent_stub = Box::new(syntax_node_stub_from_native(
        node,
        source,
        converted.parent.clone(),
    ));

    let child_count = node.child_count();
    let mut children = Vec::with_capacity(child_count);
    let mut named_children = Vec::with_capacity(node.named_child_count());
    let mut field_names = HashMap::new();
    let mut named_positions_by_id = HashMap::new();

    // 深度转换 children/named_children，同时把 field name 映射到对应 child 索引。
    for index in 0..child_count {
        let Some(native_child) = node.child(index) else {
            continue;
        };
        let mut child = syntax_node_from_native(&native_child, source, Some(parent_stub.clone()));
        if let Some(prev) = native_child.prev_named_sibling() {
            child.previous_named_sibling = Some(Box::new(syntax_node_stub_from_native(
                &prev,
                source,
                Some(parent_stub.clone()),
            )));
        }
        if let Some(next) = native_child.next_named_sibling() {
            child.next_named_sibling = Some(Box::new(syntax_node_stub_from_native(
                &next,
                source,
                Some(parent_stub.clone()),
            )));
        }
        if native_child.is_named() {
            named_positions_by_id.insert(native_child.id(), named_children.len());
            named_children.push(child.clone());
        }
        if let Some(field_name) = node.field_name_for_child(index as u32) {
            // 字段查找优先保持 web-tree-sitter 行为：命名 child 能命中 namedChildren 位置。
            let target = named_positions_by_id
                .get(&native_child.id())
                .copied()
                .map(FieldTarget::NamedChild)
                .unwrap_or(FieldTarget::Child(children.len()));
            field_names.insert(field_name.to_owned(), target);
        }
        children.push(child);
    }

    converted.children = NodeList::from(children);
    converted.named_children = NodeList::from(named_children);
    converted.field_names = field_names;
    converted
}

#[derive(Debug, Default)]
pub struct NodeList(Arc<Vec<SyntaxNode>>);

impl Clone for NodeList {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl From<Vec<SyntaxNode>> for NodeList {
    fn from(value: Vec<SyntaxNode>) -> Self {
        Self(Arc::new(value))
    }
}

impl Deref for NodeList {
    type Target = Vec<SyntaxNode>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for NodeList {
    fn deref_mut(&mut self) -> &mut Self::Target {
        Arc::make_mut(&mut self.0)
    }
}

impl Extend<SyntaxNode> for NodeList {
    fn extend<T: IntoIterator<Item = SyntaxNode>>(&mut self, iter: T) {
        Arc::make_mut(&mut self.0).extend(iter);
    }
}

impl IntoIterator for NodeList {
    type Item = SyntaxNode;
    type IntoIter = std::vec::IntoIter<SyntaxNode>;

    fn into_iter(self) -> Self::IntoIter {
        match Arc::try_unwrap(self.0) {
            Ok(values) => values.into_iter(),
            Err(values) => values.iter().cloned().collect::<Vec<_>>().into_iter(),
        }
    }
}

impl<'a> IntoIterator for &'a NodeList {
    type Item = &'a SyntaxNode;
    type IntoIter = std::slice::Iter<'a, SyntaxNode>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

#[derive(Debug, Default)]
pub struct SyntaxNode {
    pub id: u64,
    pub start_index: usize,
    pub end_index: usize,
    pub start_position: Point,
    pub end_position: Point,
    pub node_type: String,
    pub grammar_type: String,
    pub text: String,
    pub is_named: bool,
    pub is_extra: bool,
    pub is_error: bool,
    pub is_missing: bool,
    pub has_changes: bool,
    pub has_error: bool,
    pub parse_state: u32,
    pub next_parse_state: u32,
    pub children: NodeList,
    pub named_children: NodeList,
    pub field_names: HashMap<String, FieldTarget>,
    pub parent: Option<Box<SyntaxNode>>,
    pub previous_named_sibling: Option<Box<SyntaxNode>>,
    pub next_named_sibling: Option<Box<SyntaxNode>>,
}

impl Clone for SyntaxNode {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            start_index: self.start_index,
            end_index: self.end_index,
            start_position: self.start_position,
            end_position: self.end_position,
            node_type: self.node_type.clone(),
            grammar_type: self.grammar_type.clone(),
            text: self.text.clone(),
            is_named: self.is_named,
            is_extra: self.is_extra,
            is_error: self.is_error,
            is_missing: self.is_missing,
            has_changes: self.has_changes,
            has_error: self.has_error,
            parse_state: self.parse_state,
            next_parse_state: self.next_parse_state,
            children: self.children.clone(),
            named_children: self.named_children.clone(),
            field_names: self.field_names.clone(),
            parent: self
                .parent
                .as_ref()
                // parent/sibling 只浅拷贝链接信息，否则任意 clone 都会复制整棵祖先树。
                .map(|parent| Box::new(parent.shallow_link_clone())),
            previous_named_sibling: self
                .previous_named_sibling
                .as_ref()
                .map(|sibling| Box::new(sibling.shallow_link_clone())),
            next_named_sibling: self
                .next_named_sibling
                .as_ref()
                .map(|sibling| Box::new(sibling.shallow_link_clone())),
        }
    }
}

impl SyntaxNode {
    fn shallow_link_clone(&self) -> Self {
        Self {
            id: self.id,
            start_index: self.start_index,
            end_index: self.end_index,
            start_position: self.start_position,
            end_position: self.end_position,
            node_type: self.node_type.clone(),
            grammar_type: self.grammar_type.clone(),
            text: self.text.clone(),
            is_named: self.is_named,
            is_extra: self.is_extra,
            is_error: self.is_error,
            is_missing: self.is_missing,
            has_changes: self.has_changes,
            has_error: self.has_error,
            parse_state: self.parse_state,
            next_parse_state: self.next_parse_state,
            children: NodeList::default(),
            named_children: NodeList::default(),
            field_names: self.field_names.clone(),
            parent: None,
            previous_named_sibling: None,
            next_named_sibling: None,
        }
    }
}

impl SyntaxNode {
    pub fn type_name(&self) -> String {
        self.node_type.clone()
    }

    pub fn node_type(&self) -> &str {
        &self.node_type
    }

    pub fn text(&self) -> String {
        self.text.clone()
    }

    pub fn is_named(&self) -> bool {
        self.is_named
    }

    pub fn equals(&self, other: &SyntaxNode) -> bool {
        self.id == other.id
    }

    pub fn child(&self, index: usize) -> Option<&SyntaxNode> {
        self.children.get(index)
    }

    pub fn named_child(&self, index: usize) -> Option<&SyntaxNode> {
        self.named_children.get(index)
    }

    pub fn children(&self) -> Vec<SyntaxNode> {
        self.children.iter().cloned().collect()
    }

    pub fn named_children(&self) -> Vec<SyntaxNode> {
        self.named_children.iter().cloned().collect()
    }

    pub fn child_for_field_name(&self, field_name: &str) -> Option<&SyntaxNode> {
        self.field_names
            .get(field_name)
            .and_then(|target| match target {
                FieldTarget::Child(idx) => self.children.get(*idx),
                FieldTarget::NamedChild(idx) => self.named_children.get(*idx),
            })
    }

    pub fn children_for_field_name(&self, field_name: &str) -> Vec<&SyntaxNode> {
        self.child_for_field_name(field_name).into_iter().collect()
    }

    pub fn child_count(&self) -> usize {
        self.children.len()
    }

    pub fn named_child_count(&self) -> usize {
        self.named_children.len()
    }

    pub fn first_child(&self) -> Option<&SyntaxNode> {
        self.children.first()
    }

    pub fn first_named_child(&self) -> Option<&SyntaxNode> {
        self.named_children.first()
    }

    pub fn last_child(&self) -> Option<&SyntaxNode> {
        self.children.last()
    }

    pub fn last_named_child(&self) -> Option<&SyntaxNode> {
        self.named_children.last()
    }

    pub fn parent(&self) -> Option<SyntaxNode> {
        self.parent.as_deref().cloned()
    }

    pub fn previous_named_sibling(&self) -> Option<SyntaxNode> {
        self.previous_named_sibling.as_deref().cloned()
    }

    pub fn next_named_sibling(&self) -> Option<SyntaxNode> {
        self.next_named_sibling.as_deref().cloned()
    }

    pub fn previous_sibling(&self) -> Option<SyntaxNode> {
        self.previous_named_sibling()
    }

    pub fn next_sibling(&self) -> Option<SyntaxNode> {
        self.next_named_sibling()
    }

    pub fn start_index(&self) -> usize {
        self.start_index
    }

    pub fn end_index(&self) -> usize {
        self.end_index
    }

    pub fn start_position(&self) -> Point {
        self.start_position
    }

    pub fn end_position(&self) -> Point {
        self.end_position
    }

    pub fn descendants_of_type(&self, types: &[&str]) -> Vec<&SyntaxNode> {
        let mut out = Vec::new();
        self.collect_descendants_of_type(types, &mut out);
        out
    }

    fn collect_descendants_of_type<'a>(&'a self, types: &[&str], out: &mut Vec<&'a SyntaxNode>) {
        if types.contains(&self.node_type.as_str()) {
            out.push(self);
        }
        // web-tree-sitter 的常见遍历关注 named children，跳过标点和 trivia 能减少噪音。
        for child in &self.named_children {
            child.collect_descendants_of_type(types, out);
        }
    }

    pub fn walk(&self) -> TreeCursor {
        TreeCursor::new(self.clone())
    }
}

impl AsRef<SyntaxNode> for SyntaxNode {
    fn as_ref(&self) -> &SyntaxNode {
        self
    }
}

#[derive(Debug, Clone)]
pub struct TreeCursor {
    pub current_node: SyntaxNode,
    pub current_depth: usize,
    pub current_descendant_index: usize,
}

impl TreeCursor {
    pub fn new(current_node: SyntaxNode) -> Self {
        Self {
            current_node,
            current_depth: 0,
            current_descendant_index: 0,
        }
    }

    pub fn copy(&self) -> Self {
        self.clone()
    }

    pub fn delete(self) {}

    pub fn goto_first_child(&mut self) -> bool {
        if let Some(child) = self.current_node.first_child().cloned() {
            self.current_node = child;
            self.current_depth += 1;
            true
        } else {
            false
        }
    }

    pub fn goto_parent(&mut self) -> bool {
        if let Some(parent) = self.current_node.parent.clone() {
            self.current_node = *parent;
            self.current_depth = self.current_depth.saturating_sub(1);
            true
        } else {
            false
        }
    }
}
