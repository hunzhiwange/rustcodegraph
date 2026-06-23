//! Tree-sitter extraction types.
//!
//! This mirrors `tree-sitter-types.ts`: language adapters expose node-type
//! tables plus optional hooks, and the core extractor gives hooks a narrow
//! callback context instead of exposing all internals.
//!
//! 新语言接入时优先实现这个 trait 的表驱动钩子；只有语法确实无法用通用
//! 流程表达时，才在核心抽取器里添加语言特例。

pub use crate::types::Visibility;
use crate::types::{Node, NodeKind, ReferenceKind};
use crate::web_tree_sitter::SyntaxNode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportInfo {
    /// 模块路径或包名，后续 resolver 会把它解析成文件/符号边。
    pub module_name: String,
    /// 原始 import 片段，用于 context 展示和调试。
    pub signature: String,
    /// 语言适配器已经自行发出 binding 引用时设为 true，避免核心层重复补边。
    pub handled_refs: bool,
}

#[derive(Debug, Clone)]
pub struct VariableInfo {
    pub name: String,
    pub kind: NodeKind,
    pub signature: Option<String>,
    pub is_exported: Option<bool>,
    /// `const foo = () => {}` 这类变量声明应提升为函数节点时使用。
    pub delegate_to_function: Option<SyntaxNode>,
    pub position_node: Option<SyntaxNode>,
    /// 变量初始化表达式中可能还有调用或类型引用，需要回到核心遍历继续扫描。
    pub visit_value: Option<SyntaxNode>,
    /// Zustand/Pinia 等 object-literal action 形态会在这里暴露给通用补洞逻辑。
    pub object_literal_functions: Option<SyntaxNode>,
}

/// Rust stand-in for TS `Partial<Node>` extras passed to `createNode`.
#[derive(Debug, Clone, Default)]
pub struct NodeExtra {
    pub docstring: Option<String>,
    pub signature: Option<String>,
    pub visibility: Option<Visibility>,
    pub is_exported: Option<bool>,
}

pub trait ExtractorContext {
    /// 语言适配器自定义 visit 时只能通过这个窄接口回写节点和引用，避免直接
    /// 操作 TreeSitterExtractor 的内部状态导致 scope 栈不一致。
    fn create_node(
        &mut self,
        kind: NodeKind,
        name: &str,
        node: &SyntaxNode,
        extra: NodeExtra,
    ) -> Option<Node>;

    fn visit_node(&mut self, node: &SyntaxNode);
    fn visit_function_body(&mut self, body: &SyntaxNode, function_id: &str);
    fn add_unresolved_reference(&mut self, reference: UnresolvedReferenceInput);
    fn push_scope(&mut self, node_id: String);
    fn pop_scope(&mut self);

    fn file_path(&self) -> &str;
    fn source(&self) -> &str;
    fn node_stack(&self) -> &[String];
    fn nodes(&self) -> &[Node];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassNodeKind {
    Class,
    Struct,
    Enum,
    Interface,
    Trait,
}

pub type ClassClassification = ClassNodeKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodNodeKind {
    Method,
    Property,
}

pub type MethodClassification = MethodNodeKind;

#[derive(Debug, Clone)]
pub struct UnresolvedReferenceInput {
    pub from_node_id: String,
    pub reference_name: String,
    pub reference_kind: ReferenceKind,
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub file_path: Option<String>,
}

pub trait LanguageExtractor: Send + Sync {
    /// 可选预处理只用于 grammar 兼容性修正；返回内容会替换源码参与坐标计算，
    /// 所以实现时必须保持行结构尽量稳定。
    fn pre_parse(&self, source: &str) -> Option<String> {
        let _ = source;
        None
    }

    fn function_types(&self) -> &[&'static str];
    fn class_types(&self) -> &[&'static str];
    fn method_types(&self) -> &[&'static str];
    fn interface_types(&self) -> &[&'static str];
    fn struct_types(&self) -> &[&'static str];
    fn enum_types(&self) -> &[&'static str];
    fn type_alias_types(&self) -> &[&'static str];
    fn import_types(&self) -> &[&'static str];
    fn call_types(&self) -> &[&'static str];
    fn variable_types(&self) -> &[&'static str];

    fn enum_member_types(&self) -> &[&'static str] {
        &[]
    }

    fn field_types(&self) -> &[&'static str] {
        &[]
    }

    fn property_types(&self) -> &[&'static str] {
        &[]
    }

    fn name_field(&self) -> &'static str;
    fn body_field(&self) -> &'static str;
    fn params_field(&self) -> &'static str;

    fn return_field(&self) -> Option<&'static str> {
        None
    }

    fn resolve_name(&self, _node: &SyntaxNode, _source: &str) -> Option<String> {
        None
    }

    fn extract_property_name(&self, _node: &SyntaxNode, _source: &str) -> Option<String> {
        None
    }

    fn get_signature(&self, _node: &SyntaxNode, _source: &str) -> Option<String> {
        None
    }

    fn get_visibility(&self, _node: &SyntaxNode) -> Option<Visibility> {
        None
    }

    fn is_exported(&self, _node: &SyntaxNode, _source: &str) -> bool {
        false
    }

    fn is_async(&self, _node: &SyntaxNode) -> bool {
        false
    }

    fn is_static(&self, _node: &SyntaxNode) -> bool {
        false
    }

    fn is_const(&self, _node: &SyntaxNode) -> bool {
        false
    }

    fn extract_modifiers(&self, _node: &SyntaxNode) -> Option<Vec<String>> {
        None
    }

    fn extra_class_node_types(&self) -> &[&'static str] {
        &[]
    }

    fn methods_are_top_level(&self) -> bool {
        false
    }

    fn interface_kind(&self) -> Option<NodeKind> {
        None
    }

    fn visit_node(&self, _node: &SyntaxNode, _ctx: &mut dyn ExtractorContext) -> bool {
        false
    }

    fn classify_class_node(&self, _node: &SyntaxNode) -> ClassNodeKind {
        ClassNodeKind::Class
    }

    fn classify_method_node(&self, _node: &SyntaxNode) -> MethodNodeKind {
        MethodNodeKind::Method
    }

    fn resolve_body(&self, node: &SyntaxNode, body_field: &str) -> Option<SyntaxNode> {
        node.child_for_field_name(body_field).cloned()
    }

    fn extract_import(&self, _node: &SyntaxNode, _source: &str) -> Option<ImportInfo> {
        None
    }

    fn extract_variables(&self, _node: &SyntaxNode, _source: &str) -> Vec<VariableInfo> {
        Vec::new()
    }

    fn get_receiver_type(&self, _node: &SyntaxNode, _source: &str) -> Option<String> {
        None
    }

    fn get_return_type(&self, _node: &SyntaxNode, _source: &str) -> Option<String> {
        None
    }

    fn resolve_type_alias_kind(&self, _node: &SyntaxNode, _source: &str) -> Option<NodeKind> {
        None
    }

    fn is_misparsed_function(&self, _name: &str, _node: &SyntaxNode) -> bool {
        false
    }

    fn extract_bare_call(&self, _node: &SyntaxNode, _source: &str) -> Option<String> {
        None
    }

    fn package_types(&self) -> &[&'static str] {
        &[]
    }

    fn extract_package(&self, _node: &SyntaxNode, _source: &str) -> Option<String> {
        None
    }
}
