//! Rust translation of `src/extraction/languages/c-cpp.ts`.
//!
//! Original TS file name contains a hyphen, so the Rust module uses `c_cpp`.

use crate::extraction::tree_sitter_helpers::{get_child_by_field, get_node_text};
use crate::extraction::tree_sitter_types::{ImportInfo, LanguageExtractor, Visibility};
use crate::types::NodeKind;
use crate::web_tree_sitter::SyntaxNode;

pub struct CExtractor;
pub struct CppExtractor;

pub const C_EXTRACTOR: CExtractor = CExtractor;
pub const CPP_EXTRACTOR: CppExtractor = CppExtractor;

pub fn c_extractor() -> &'static CExtractor {
    &C_EXTRACTOR
}

pub fn cpp_extractor() -> &'static CppExtractor {
    &CPP_EXTRACTOR
}

// C++ 成员函数定义常嵌在多层 declarator 里；这里只在声明头里找 qualified_identifier，
// 刻意跳过参数和 trailing return，避免把参数类型里的 `A::B` 当成 receiver。
fn find_declarator_qualified_id(declarator: &SyntaxNode) -> Option<SyntaxNode> {
    let mut queue = vec![declarator.clone()];
    while let Some(current) = queue.first().cloned() {
        queue.remove(0);
        if current.type_name() == "qualified_identifier" {
            return Some(current);
        }
        for i in 0..current.named_child_count() {
            let Some(child) = current.named_child(i) else {
                continue;
            };
            if child.type_name() != "parameter_list" && child.type_name() != "trailing_return_type"
            {
                queue.push(child.clone());
            }
        }
    }
    None
}

fn extract_cpp_qualified_method_name(node: &SyntaxNode, source: &str) -> Option<String> {
    let declarator = get_child_by_field(node, "declarator")?;
    let qid = find_declarator_qualified_id(declarator)?;
    get_node_text(&qid, source)
        .trim()
        .split("::")
        .filter(|part| !part.is_empty())
        .last()
        .map(str::to_string)
}

fn extract_cpp_receiver_type(node: &SyntaxNode, source: &str) -> Option<String> {
    let declarator = get_child_by_field(node, "declarator")?;
    let qid = find_declarator_qualified_id(declarator)?;
    let parts = get_node_text(&qid, source)
        .trim()
        .split("::")
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    (parts.len() > 1).then(|| parts[..parts.len() - 1].join("::"))
}

const CPP_NON_CLASS_RETURN: &[&str] = &[
    "void",
    "bool",
    "char",
    "short",
    "int",
    "long",
    "float",
    "double",
    "unsigned",
    "signed",
    "size_t",
    "ssize_t",
    "auto",
    "wchar_t",
    "char8_t",
    "char16_t",
    "char32_t",
    "int8_t",
    "int16_t",
    "int32_t",
    "int64_t",
    "uint8_t",
    "uint16_t",
    "uint32_t",
    "uint64_t",
    "intptr_t",
    "uintptr_t",
    "nullptr_t",
];

/// Normalize a C++ return type to the bare class name a method could be called on.
pub fn normalize_cpp_return_type(raw: &str) -> Option<String> {
    let mut t = raw.trim().to_string();
    if t.is_empty() {
        return None;
    }

    if let Some(inner) = unwrap_cpp_template_wrapper(&t) {
        t = inner;
    }

    for word in ["const", "volatile", "typename", "struct", "class", "enum"] {
        t = t.replace(word, " ");
    }
    t = strip_angle_generics(&t);
    t = t.replace(['*', '&'], " ");
    t = t.split_whitespace().collect::<Vec<_>>().join(" ");

    let last = t.split("::").filter(|part| !part.is_empty()).last()?.trim();
    if CPP_NON_CLASS_RETURN.contains(&last) || !is_identifier(last) {
        return None;
    }
    Some(last.to_string())
}

fn unwrap_cpp_template_wrapper(raw: &str) -> Option<String> {
    // 智能指针/optional 的返回值通常仍表示业务类型，resolver 需要看到内部类名来连上链式调用。
    for wrapper in ["unique_ptr", "shared_ptr", "weak_ptr", "optional"] {
        let plain = format!("{wrapper}<");
        let std = format!("std::{wrapper}<");
        let idx = raw.find(&plain).or_else(|| raw.find(&std))?;
        let open = raw[idx..].find('<')? + idx;
        let inner = raw[open + 1..].split([',', '>']).next()?.trim();
        if !inner.is_empty() {
            return Some(inner.to_string());
        }
    }
    None
}

fn strip_angle_generics(input: &str) -> String {
    let mut out = String::new();
    let mut depth = 0usize;
    for ch in input.chars() {
        match ch {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(ch),
            _ => {}
        }
    }
    out
}

fn is_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some('_') | Some('A'..='Z') | Some('a'..='z'))
        && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

fn extract_cpp_return_type(node: &SyntaxNode, source: &str) -> Option<String> {
    let type_node = get_child_by_field(node, "type")?;
    normalize_cpp_return_type(&get_node_text(type_node, source))
}

fn resolve_c_typedef_kind(node: &SyntaxNode) -> Option<NodeKind> {
    // `typedef struct {...} Foo` 和 `typedef enum {...} Foo` 在图里应呈现为结构/枚举，
    // 不是普通 type_alias，否则后续成员和枚举值关系会弱很多。
    for i in 0..node.named_child_count() {
        let Some(child) = node.named_child(i) else {
            continue;
        };
        if child.type_name() == "enum_specifier" && get_child_by_field(child, "body").is_some() {
            return Some(NodeKind::Enum);
        }
        if child.type_name() == "struct_specifier" && get_child_by_field(child, "body").is_some() {
            return Some(NodeKind::Struct);
        }
    }
    None
}

fn extract_c_include(node: &SyntaxNode, source: &str) -> Option<ImportInfo> {
    let import_text = get_node_text(node, source).trim().to_string();
    // 同时保留系统 include 和本地 include 的原始签名；路径解析阶段再决定候选文件。
    if let Some(system_lib) = node
        .named_children()
        .into_iter()
        .find(|c| c.type_name() == "system_lib_string")
    {
        return Some(ImportInfo {
            module_name: get_node_text(&system_lib, source)
                .trim_matches(['<', '>'])
                .to_string(),
            signature: import_text,
            handled_refs: false,
        });
    }
    let string_literal = node
        .named_children()
        .into_iter()
        .find(|c| c.type_name() == "string_literal")?;
    let string_content = string_literal
        .named_children()
        .into_iter()
        .find(|c| c.type_name() == "string_content")?;
    Some(ImportInfo {
        module_name: get_node_text(&string_content, source),
        signature: import_text,
        handled_refs: false,
    })
}

impl LanguageExtractor for CExtractor {
    fn function_types(&self) -> &'static [&'static str] {
        &["function_definition"]
    }

    fn class_types(&self) -> &'static [&'static str] {
        &[]
    }

    fn method_types(&self) -> &'static [&'static str] {
        &[]
    }

    fn interface_types(&self) -> &'static [&'static str] {
        &[]
    }

    fn struct_types(&self) -> &'static [&'static str] {
        &["struct_specifier"]
    }

    fn enum_types(&self) -> &'static [&'static str] {
        &["enum_specifier"]
    }

    fn enum_member_types(&self) -> &'static [&'static str] {
        &["enumerator"]
    }

    fn type_alias_types(&self) -> &'static [&'static str] {
        &["type_definition"]
    }

    fn import_types(&self) -> &'static [&'static str] {
        &["preproc_include"]
    }

    fn call_types(&self) -> &'static [&'static str] {
        &["call_expression"]
    }

    fn variable_types(&self) -> &'static [&'static str] {
        &["declaration"]
    }

    fn name_field(&self) -> &'static str {
        "declarator"
    }

    fn body_field(&self) -> &'static str {
        "body"
    }

    fn params_field(&self) -> &'static str {
        "parameters"
    }

    fn is_const(&self, node: &SyntaxNode) -> bool {
        node.named_children()
            .into_iter()
            .any(|c| c.type_name() == "type_qualifier" && c.text() == "const")
    }

    fn get_return_type(&self, node: &SyntaxNode, source: &str) -> Option<String> {
        extract_cpp_return_type(node, source)
    }

    fn resolve_type_alias_kind(&self, node: &SyntaxNode, _source: &str) -> Option<NodeKind> {
        resolve_c_typedef_kind(node)
    }

    fn extract_import(&self, node: &SyntaxNode, source: &str) -> Option<ImportInfo> {
        extract_c_include(node, source)
    }
}

impl LanguageExtractor for CppExtractor {
    fn function_types(&self) -> &'static [&'static str] {
        &["function_definition"]
    }

    fn class_types(&self) -> &'static [&'static str] {
        &["class_specifier"]
    }

    fn method_types(&self) -> &'static [&'static str] {
        &["function_definition"]
    }

    fn interface_types(&self) -> &'static [&'static str] {
        &[]
    }

    fn struct_types(&self) -> &'static [&'static str] {
        &["struct_specifier"]
    }

    fn enum_types(&self) -> &'static [&'static str] {
        &["enum_specifier"]
    }

    fn enum_member_types(&self) -> &'static [&'static str] {
        &["enumerator"]
    }

    fn type_alias_types(&self) -> &'static [&'static str] {
        &["type_definition", "alias_declaration"]
    }

    fn import_types(&self) -> &'static [&'static str] {
        &["preproc_include"]
    }

    fn call_types(&self) -> &'static [&'static str] {
        &["call_expression"]
    }

    fn variable_types(&self) -> &'static [&'static str] {
        &["declaration"]
    }

    fn name_field(&self) -> &'static str {
        "declarator"
    }

    fn body_field(&self) -> &'static str {
        "body"
    }

    fn params_field(&self) -> &'static str {
        "parameters"
    }

    fn resolve_name(&self, node: &SyntaxNode, source: &str) -> Option<String> {
        extract_cpp_qualified_method_name(node, source)
    }

    fn get_receiver_type(&self, node: &SyntaxNode, source: &str) -> Option<String> {
        extract_cpp_receiver_type(node, source)
    }

    fn get_return_type(&self, node: &SyntaxNode, source: &str) -> Option<String> {
        extract_cpp_return_type(node, source)
    }

    fn get_visibility(&self, node: &SyntaxNode) -> Option<Visibility> {
        let parent = node.parent()?;
        // tree-sitter 不把 access specifier 挂到每个成员上，只能向同级前序声明扫描。
        // 这里沿用“最后一个访问标签生效”的 C++ 语义。
        for i in 0..parent.child_count() {
            let Some(child) = parent.child(i) else {
                continue;
            };
            if child.type_name() == "access_specifier" {
                let text = child.text();
                if text.contains("public") {
                    return Some(Visibility::Public);
                }
                if text.contains("private") {
                    return Some(Visibility::Private);
                }
                if text.contains("protected") {
                    return Some(Visibility::Protected);
                }
            }
        }
        None
    }

    fn resolve_type_alias_kind(&self, node: &SyntaxNode, _source: &str) -> Option<NodeKind> {
        resolve_c_typedef_kind(node)
    }

    fn is_misparsed_function(&self, name: &str, _node: &SyntaxNode) -> bool {
        // C/C++ 语法在错误恢复时会把控制流关键字包成 function_definition；
        // 过滤这些假阳性比在后续 resolver 里清理更便宜。
        name.starts_with("namespace")
            || matches!(
                name,
                "switch" | "if" | "for" | "while" | "do" | "case" | "return"
            )
    }

    fn extract_import(&self, node: &SyntaxNode, source: &str) -> Option<ImportInfo> {
        extract_c_include(node, source)
    }
}
