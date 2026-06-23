//! Rust translation of `src/extraction/languages/dart.ts`.

use crate::extraction::tree_sitter_helpers::get_node_text;
use crate::extraction::tree_sitter_types::{
    ExtractorContext, ImportInfo, LanguageExtractor, NodeExtra, Visibility,
};
use crate::types::NodeKind;
use crate::web_tree_sitter::SyntaxNode;

pub struct DartExtractor;

pub const DART_EXTRACTOR: DartExtractor = DartExtractor;

pub fn dart_extractor() -> &'static DartExtractor {
    &DART_EXTRACTOR
}

struct DartCtorInfo {
    class_name: String,
    ctor_name: String,
}

// Dart 的函数/方法签名在 declaration、signature、getter/setter 多层节点之间变化；
// 统一剥到最内层后，返回类型、名称和签名抽取才不会各写一套分支。
fn dart_inner_signature(node: &SyntaxNode) -> SyntaxNode {
    if matches!(
        node.type_name().as_str(),
        "function_declaration" | "method_declaration"
    ) && let Some(signature) = node.child_for_field_name("signature")
    {
        return dart_inner_signature(signature);
    }
    if node.type_name() == "method_signature"
        && let Some(inner) = node.named_children().into_iter().find(|c| {
            c.type_name() == "function_signature"
                || c.type_name() == "getter_signature"
                || c.type_name() == "setter_signature"
        })
    {
        return inner;
    }
    node.clone()
}

fn dart_constructor_signature(node: &SyntaxNode) -> Option<SyntaxNode> {
    if node.type_name() == "factory_constructor_signature"
        || node.type_name() == "constructor_signature"
    {
        return Some(node.clone());
    }
    if matches!(node.type_name().as_str(), "method_declaration") {
        return node
            .child_for_field_name("signature")
            .and_then(dart_constructor_signature);
    }
    if node.type_name() == "method_signature" {
        return node.named_children().into_iter().find(|c| {
            c.type_name() == "factory_constructor_signature"
                || c.type_name() == "constructor_signature"
        });
    }
    None
}

fn dart_enclosing_type_name(node: &SyntaxNode) -> Option<String> {
    let mut parent = node.parent();
    while let Some(p) = parent {
        if matches!(
            p.type_name().as_str(),
            "class_declaration"
                | "class_definition"
                | "mixin_declaration"
                | "extension_declaration"
                | "enum_declaration"
        ) {
            return p.child_for_field_name("name").map(|name| name.text());
        }
        parent = p.parent();
    }
    None
}

fn dart_ctor_info(node: &SyntaxNode) -> Option<DartCtorInfo> {
    let ctor = dart_constructor_signature(node)?;
    let ids = ctor
        .named_children()
        .into_iter()
        .filter(|c| c.type_name() == "identifier")
        .collect::<Vec<_>>();
    let class_name = dart_enclosing_type_name(node)?;
    let first = ids.first()?;
    // 只有第一个 identifier 等于外层类型名时才按构造器处理，避免普通方法误判。
    if first.text() != class_name {
        return None;
    }
    Some(DartCtorInfo {
        class_name: class_name.clone(),
        ctor_name: ids.get(1).map(|id| id.text()).unwrap_or(class_name),
    })
}

fn extract_dart_return_type(node: &SyntaxNode, source: &str) -> Option<String> {
    if let Some(ctor) = dart_ctor_info(node) {
        return Some(ctor.class_name);
    }
    let sig = dart_inner_signature(node);
    let ret_type = sig
        .named_children()
        .into_iter()
        .find(|c| c.type_name() == "type_identifier")?;
    let text = strip_angle_generics(get_node_text(&ret_type, source).trim());
    let last = text.split('.').next_back()?.trim();
    is_identifier(last).then(|| last.to_string())
}

fn dart_callee_of_arg_part(arg_part: &SyntaxNode) -> Option<String> {
    let prev = arg_part.previous_named_sibling()?;
    if prev.type_name() == "identifier" {
        return Some(prev.text());
    }
    if prev.type_name() == "selector" {
        // 还原 `foo.bar()` / `Foo().bar()` 这类 selector 链，给 call edge resolver 更具体的名字。
        let accessor = prev.named_children().into_iter().find(|c| {
            c.type_name() == "unconditional_assignable_selector"
                || c.type_name() == "conditional_assignable_selector"
        });
        let method_id = accessor?
            .named_children()
            .into_iter()
            .find(|c| c.type_name() == "identifier")?;
        if let Some(accessor_prev) = prev.previous_named_sibling()
            && accessor_prev.type_name() == "identifier"
        {
            return Some(format!("{}.{}", accessor_prev.text(), method_id.text()));
        }
        return Some(method_id.text());
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

impl LanguageExtractor for DartExtractor {
    fn function_types(&self) -> &'static [&'static str] {
        &["function_declaration", "function_signature"]
    }

    fn class_types(&self) -> &'static [&'static str] {
        &["class_declaration", "class_definition"]
    }

    fn method_types(&self) -> &'static [&'static str] {
        &[
            "method_declaration",
            "method_signature",
            "constructor_signature",
        ]
    }

    fn interface_types(&self) -> &'static [&'static str] {
        &[]
    }

    fn struct_types(&self) -> &'static [&'static str] {
        &[]
    }

    fn enum_types(&self) -> &'static [&'static str] {
        &["enum_declaration"]
    }

    fn enum_member_types(&self) -> &'static [&'static str] {
        &["enum_constant"]
    }

    fn type_alias_types(&self) -> &'static [&'static str] {
        &["type_alias"]
    }

    fn import_types(&self) -> &'static [&'static str] {
        &["import_or_export"]
    }

    fn call_types(&self) -> &'static [&'static str] {
        &[]
    }

    fn variable_types(&self) -> &'static [&'static str] {
        &[]
    }

    fn extra_class_node_types(&self) -> &'static [&'static str] {
        &["mixin_declaration", "extension_declaration"]
    }

    fn visit_node(&self, node: &SyntaxNode, ctx: &mut dyn ExtractorContext) -> bool {
        if node.type_name() != "static_final_declaration" {
            return false;
        }
        // `static final` 常作为 Flutter/Dart 的命名常量出现，通用变量抽取拿不到初始化预览。
        let Some(name_node) = node
            .named_children()
            .into_iter()
            .find(|c| c.type_name() == "identifier")
        else {
            return true;
        };
        let value_node = name_node.next_named_sibling();
        let init_value = value_node.map(|value| {
            let mut text = get_node_text(&value, ctx.source());
            if text.len() > 100 {
                text.truncate(100);
                text.push_str("...");
            }
            text
        });
        let extra = NodeExtra {
            signature: init_value.map(|value| format!("= {value}")),
            ..Default::default()
        };
        ctx.create_node(
            NodeKind::Constant,
            &get_node_text(&name_node, ctx.source()),
            node,
            extra,
        );
        true
    }

    fn resolve_body(&self, node: &SyntaxNode, body_field: &str) -> Option<SyntaxNode> {
        // 抽象签名的 body 可能是下一个兄弟节点；类/extension 的 body 字段名也不完全一致。
        if node.type_name() == "function_signature" || node.type_name() == "method_signature" {
            return node
                .next_named_sibling()
                .filter(|next| next.type_name() == "function_body");
        }
        node.child_for_field_name(body_field).cloned().or_else(|| {
            node.named_children()
                .into_iter()
                .find(|c| c.type_name() == "class_body" || c.type_name() == "extension_body")
        })
    }

    fn name_field(&self) -> &'static str {
        "name"
    }

    fn body_field(&self) -> &'static str {
        "body"
    }

    fn params_field(&self) -> &'static str {
        "formal_parameter_list"
    }

    fn return_field(&self) -> Option<&'static str> {
        Some("type")
    }

    fn get_return_type(&self, node: &SyntaxNode, source: &str) -> Option<String> {
        extract_dart_return_type(node, source)
    }

    fn is_misparsed_function(&self, _name: &str, node: &SyntaxNode) -> bool {
        dart_ctor_info(node).is_some_and(|ctor| ctor.ctor_name == ctor.class_name)
    }

    fn get_signature(&self, node: &SyntaxNode, source: &str) -> Option<String> {
        let sig = dart_inner_signature(node);
        let params = sig
            .named_children()
            .into_iter()
            .find(|c| c.type_name() == "formal_parameter_list");
        let ret_type = sig
            .named_children()
            .into_iter()
            .find(|c| c.type_name() == "type_identifier" || c.type_name() == "void_type");
        if params.is_none() && ret_type.is_none() {
            return None;
        }
        let mut result = String::new();
        if let Some(ret) = ret_type {
            result.push_str(&get_node_text(&ret, source));
            result.push(' ');
        }
        if let Some(params) = params {
            result.push_str(&get_node_text(&params, source));
        }
        let result = result.trim().to_string();
        (!result.is_empty()).then_some(result)
    }

    fn get_visibility(&self, node: &SyntaxNode) -> Option<Visibility> {
        let sig = dart_inner_signature(node);
        let name_node = if matches!(
            sig.type_name().as_str(),
            "function_signature" | "getter_signature" | "setter_signature"
        ) {
            sig.child_for_field_name("name").cloned().or_else(|| {
                sig.named_children()
                    .into_iter()
                    .find(|c| c.type_name() == "identifier")
            })
        } else {
            node.child_for_field_name("name").cloned()
        };
        if name_node.is_some_and(|name| name.text().starts_with('_')) {
            Some(Visibility::Private)
        } else {
            Some(Visibility::Public)
        }
    }

    fn is_async(&self, node: &SyntaxNode) -> bool {
        let body = if matches!(
            node.type_name().as_str(),
            "function_declaration" | "method_declaration"
        ) {
            node.child_for_field_name("body").cloned()
        } else {
            node.next_named_sibling()
        };
        body.is_some_and(|body| {
            body.type_name() == "function_body"
                && (0..body.child_count())
                    .any(|i| body.child(i).is_some_and(|c| c.type_name() == "async"))
        })
    }

    fn is_static(&self, node: &SyntaxNode) -> bool {
        let sig = if node.type_name() == "method_declaration" {
            node.child_for_field_name("signature").cloned()
        } else {
            Some(node.clone())
        };
        sig.is_some_and(|sig| {
            sig.type_name() == "method_signature"
                && (0..sig.child_count())
                    .any(|i| sig.child(i).is_some_and(|c| c.type_name() == "static"))
        })
    }

    fn resolve_name(&self, node: &SyntaxNode, source: &str) -> Option<String> {
        if let Some(ctor) = dart_ctor_info(node) {
            return (ctor.ctor_name != ctor.class_name).then_some(ctor.ctor_name);
        }
        let sig = dart_inner_signature(node);
        if !matches!(
            sig.type_name().as_str(),
            "function_signature" | "getter_signature" | "setter_signature"
        ) {
            return None;
        }
        sig.child_for_field_name("name")
            .cloned()
            .or_else(|| {
                sig.named_children()
                    .into_iter()
                    .find(|c| c.type_name() == "identifier")
            })
            .map(|name| get_node_text(&name, source))
    }

    fn extract_import(&self, node: &SyntaxNode, source: &str) -> Option<ImportInfo> {
        let import_text = get_node_text(node, source).trim().to_string();
        let module_name = find_dart_uri(node, source)?;
        Some(ImportInfo {
            module_name,
            signature: import_text,
            handled_refs: false,
        })
    }

    fn extract_bare_call(&self, node: &SyntaxNode, _source: &str) -> Option<String> {
        if node.type_name() == "selector" {
            // Dart 的调用表达式常表现为 selector + argument_part，而不是统一 call_expression。
            // 这里直接从 selector 复原被调用名。
            let has_arg_part = node
                .named_children()
                .into_iter()
                .any(|c| c.type_name() == "argument_part");
            if !has_arg_part {
                return None;
            }
            let prev = node.previous_named_sibling()?;
            if prev.type_name() == "identifier" {
                return Some(prev.text());
            }
            if prev.type_name() == "selector" {
                let accessor = prev.named_children().into_iter().find(|c| {
                    c.type_name() == "unconditional_assignable_selector"
                        || c.type_name() == "conditional_assignable_selector"
                })?;
                let method_id = accessor
                    .named_children()
                    .into_iter()
                    .find(|c| c.type_name() == "identifier")?;
                if let Some(accessor_prev) = prev.previous_named_sibling() {
                    if accessor_prev.type_name() == "identifier" {
                        return Some(format!("{}.{}", accessor_prev.text(), method_id.text()));
                    }
                    if accessor_prev.type_name() == "selector"
                        && accessor_prev
                            .named_children()
                            .into_iter()
                            .any(|c| c.type_name() == "argument_part")
                    {
                        let inner = dart_callee_of_arg_part(&accessor_prev)?;
                        if inner.starts_with(|c: char| c.is_ascii_uppercase()) {
                            return Some(format!("{inner}().{}", method_id.text()));
                        }
                    }
                }
                return Some(method_id.text());
            }
            if prev.type_name() == "unconditional_assignable_selector"
                || prev.type_name() == "conditional_assignable_selector"
            {
                return prev
                    .named_children()
                    .into_iter()
                    .find(|c| c.type_name() == "identifier")
                    .map(|method_id| method_id.text());
            }
            return None;
        }

        if node.type_name() == "new_expression" {
            return node
                .named_children()
                .into_iter()
                .find(|c| c.type_name() == "type_identifier")
                .map(|type_id| type_id.text());
        }

        if node.type_name() == "const_object_expression" {
            let type_id = node
                .named_children()
                .into_iter()
                .find(|c| c.type_name() == "type_identifier");
            let name_id = node
                .named_children()
                .into_iter()
                .find(|c| c.type_name() == "identifier");
            return match (type_id, name_id) {
                (Some(t), Some(n)) => Some(format!("{}.{}", t.text(), n.text())),
                (Some(t), None) => Some(t.text()),
                _ => None,
            };
        }

        None
    }
}

fn find_dart_uri(node: &SyntaxNode, source: &str) -> Option<String> {
    // import/export 共用 import_or_export 节点，真实 URI 需要在内部 root 节点下找字符串。
    for root_type in ["library_import", "library_export"] {
        let Some(root) = node
            .named_children()
            .into_iter()
            .find(|c| c.type_name() == root_type)
        else {
            continue;
        };
        let found = find_descendant(&root, "string_literal")
            .map(|literal| get_node_text(&literal, source).replace(['\'', '"'], ""));
        if found.as_deref().is_some_and(|s| !s.is_empty()) {
            return found;
        }
    }
    None
}

fn find_descendant(node: &SyntaxNode, wanted: &str) -> Option<SyntaxNode> {
    let mut queue = node.named_children();
    while let Some(current) = queue.first().cloned() {
        queue.remove(0);
        if current.type_name() == wanted {
            return Some(current);
        }
        queue.extend(current.named_children());
    }
    None
}
