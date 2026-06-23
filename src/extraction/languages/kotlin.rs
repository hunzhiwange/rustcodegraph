//! Rust translation of `src/extraction/languages/kotlin.ts`.

use crate::extraction::tree_sitter_helpers::{get_child_by_field, get_node_text};
use crate::extraction::tree_sitter_types::{
    ClassClassification, ExtractorContext, ImportInfo, LanguageExtractor, NodeExtra, Visibility,
};
use crate::types::NodeKind;
use crate::web_tree_sitter::SyntaxNode;

pub struct KotlinExtractor;

pub const KOTLIN_EXTRACTOR: KotlinExtractor = KotlinExtractor;

pub fn kotlin_extractor() -> &'static KotlinExtractor {
    &KOTLIN_EXTRACTOR
}

const KOTLIN_NON_CLASS_RETURN: &[&str] = &["Unit", "Nothing"];

fn extract_kotlin_return_type(node: &SyntaxNode, source: &str) -> Option<String> {
    // Kotlin 返回类型出现在参数列表之后；遇到函数体/约束说明没有显式可用的类返回值。
    let mut seen_params = false;
    for i in 0..node.named_child_count() {
        let Some(child) = node.named_child(i) else {
            continue;
        };
        if child.type_name() == "function_value_parameters" {
            seen_params = true;
            continue;
        }
        if !seen_params {
            continue;
        }
        if child.type_name() == "function_body" || child.type_name() == "type_constraints" {
            return None;
        }
        if child.type_name() == "user_type" || child.type_name() == "nullable_type" {
            let user_type = if child.type_name() == "nullable_type" {
                child
                    .named_children()
                    .into_iter()
                    .find(|c| c.type_name() == "user_type")
                    .unwrap_or_else(|| child.clone())
            } else {
                child.clone()
            };
            let type_id = user_type
                .named_children()
                .into_iter()
                .find(|c| c.type_name() == "type_identifier")
                .unwrap_or(user_type);
            let name = get_node_text(&type_id, source).trim().to_string();
            if !is_identifier(&name) || KOTLIN_NON_CLASS_RETURN.contains(&name.as_str()) {
                return None;
            }
            return Some(name);
        }
    }
    None
}

fn is_fun_interface_node(node: &SyntaxNode) -> bool {
    // tree-sitter-kotlin 对 `fun interface` 有时会恢复成 ERROR 节点，
    // 这里按 token 组合识别，保住接口节点和内部 lambda 访问。
    let mut has_fun = false;
    let mut has_interface_type = false;
    for i in 0..node.child_count() {
        let Some(child) = node.child(i) else {
            continue;
        };
        if child.type_name() == "fun" && !child.is_named() {
            has_fun = true;
        }
        if child.type_name() == "user_type"
            && child
                .named_children()
                .into_iter()
                .any(|c| c.type_name() == "type_identifier" && c.text() == "interface")
        {
            has_interface_type = true;
        }
        if child.type_name() == "ERROR" {
            for j in 0..child.child_count() {
                if child.child(j).is_some_and(|gc| {
                    gc.type_name() == "user_type"
                        && gc.named_children().into_iter().any(|id| {
                            id.type_name() == "type_identifier" && id.text() == "interface"
                        })
                }) {
                    has_interface_type = true;
                }
            }
        }
    }
    has_fun && has_interface_type
}

fn is_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some('_') | Some('A'..='Z') | Some('a'..='z'))
        && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

fn kotlin_import_name(import_text: &str) -> Option<String> {
    // import alias 和 wildcard 都不应进入最终模块名，否则解析器会拿到不可定位的名字。
    let rest = import_text.trim().strip_prefix("import")?.trim();
    let without_alias = rest.split(" as ").next().unwrap_or(rest).trim();
    let without_wildcard = without_alias.trim_end_matches(".*").trim();
    (!without_wildcard.is_empty()).then(|| without_wildcard.to_string())
}

impl LanguageExtractor for KotlinExtractor {
    fn function_types(&self) -> &'static [&'static str] {
        &["function_declaration"]
    }

    fn class_types(&self) -> &'static [&'static str] {
        &["class_declaration"]
    }

    fn method_types(&self) -> &'static [&'static str] {
        &["function_declaration"]
    }

    fn interface_types(&self) -> &'static [&'static str] {
        &[]
    }

    fn struct_types(&self) -> &'static [&'static str] {
        &[]
    }

    fn enum_types(&self) -> &'static [&'static str] {
        &[]
    }

    fn enum_member_types(&self) -> &'static [&'static str] {
        &["enum_entry"]
    }

    fn type_alias_types(&self) -> &'static [&'static str] {
        &["type_alias"]
    }

    fn import_types(&self) -> &'static [&'static str] {
        &["import_header", "import"]
    }

    fn call_types(&self) -> &'static [&'static str] {
        &["call_expression"]
    }

    fn variable_types(&self) -> &'static [&'static str] {
        &["property_declaration"]
    }

    fn field_types(&self) -> &'static [&'static str] {
        &["property_declaration"]
    }

    fn extra_class_node_types(&self) -> &'static [&'static str] {
        &["object_declaration"]
    }

    fn name_field(&self) -> &'static str {
        "simple_identifier"
    }

    fn body_field(&self) -> &'static str {
        "function_body"
    }

    fn visit_node(&self, node: &SyntaxNode, ctx: &mut dyn ExtractorContext) -> bool {
        if node.type_name() == "property_declaration" {
            return visit_kotlin_property(node, ctx);
        }

        if node.type_name() == "lambda_literal" {
            // ERROR 形式的 fun interface 会把实现体放到相邻 lambda；前一个分支创建 scope 后再访问。
            return node
                .previous_sibling()
                .is_some_and(|prev| prev.type_name() == "ERROR" && is_fun_interface_node(&prev));
        }

        if node.type_name() != "ERROR" && node.type_name() != "function_declaration" {
            return false;
        }
        if node.type_name() == "ERROR"
            && node.child(0).is_some_and(|first| first.type_name() == "{")
        {
            return false;
        }
        if !is_fun_interface_node(node) {
            return false;
        }

        let name_text = kotlin_fun_interface_name(node);
        let Some(name_text) = name_text else {
            return false;
        };
        let Some(iface_node) =
            ctx.create_node(NodeKind::Interface, &name_text, node, NodeExtra::default())
        else {
            return false;
        };
        ctx.push_scope(iface_node.id.clone());
        if node.type_name() == "ERROR"
            && let Some(next) = node
                .next_sibling()
                .filter(|n| n.type_name() == "lambda_literal")
        {
            for child in next.named_children() {
                if child.type_name() == "statements" {
                    for stmt in child.named_children() {
                        ctx.visit_node(&stmt);
                    }
                }
            }
        }
        ctx.pop_scope();
        true
    }

    fn params_field(&self) -> &'static str {
        "function_value_parameters"
    }

    fn return_field(&self) -> Option<&'static str> {
        Some("type")
    }

    fn get_return_type(&self, node: &SyntaxNode, source: &str) -> Option<String> {
        extract_kotlin_return_type(node, source)
    }

    fn resolve_body(&self, node: &SyntaxNode, _body_field: &str) -> Option<SyntaxNode> {
        // Kotlin 的函数体、类体、枚举体节点名不同；错误恢复出的 `{...}` 也要当 body。
        for i in 0..node.named_child_count() {
            let Some(child) = node.named_child(i) else {
                continue;
            };
            if child.type_name() == "ERROR"
                && child.child(0).is_some_and(|first| first.type_name() == "{")
            {
                return Some(child.clone());
            }
            if matches!(
                child.type_name().as_str(),
                "function_body" | "class_body" | "enum_class_body"
            ) {
                return Some(child.clone());
            }
        }
        None
    }

    fn classify_class_node(&self, node: &SyntaxNode) -> ClassClassification {
        for i in 0..node.child_count() {
            let Some(child) = node.child(i) else {
                continue;
            };
            if child.type_name() == "interface" {
                return ClassClassification::Interface;
            }
            if child.type_name() == "enum" {
                return ClassClassification::Enum;
            }
        }
        ClassClassification::Class
    }

    fn get_receiver_type(&self, node: &SyntaxNode, source: &str) -> Option<String> {
        // 扩展函数 `fun Receiver.name(...)` 的 receiver 在函数名前的 user_type + `.`。
        let mut found_user_type = None;
        for i in 0..node.child_count() {
            let Some(child) = node.child(i) else {
                continue;
            };
            if child.type_name() == "user_type" {
                found_user_type = Some(child);
            } else if child.type_name() == "." {
                let found = found_user_type?;
                let type_id = found
                    .named_children()
                    .into_iter()
                    .find(|c| c.type_name() == "type_identifier");
                return Some(
                    type_id
                        .map(|id| get_node_text(&id, source))
                        .unwrap_or_else(|| get_node_text(found, source)),
                );
            } else if child.type_name() == "simple_identifier"
                || child.type_name() == "function_value_parameters"
            {
                break;
            }
        }
        None
    }

    fn get_signature(&self, node: &SyntaxNode, source: &str) -> Option<String> {
        let params = get_child_by_field(node, "function_value_parameters")?;
        let mut sig = get_node_text(params, source);
        if let Some(return_type) = get_child_by_field(node, "type") {
            sig.push_str(": ");
            sig.push_str(&get_node_text(return_type, source));
        }
        Some(sig)
    }

    fn get_visibility(&self, node: &SyntaxNode) -> Option<Visibility> {
        for i in 0..node.child_count() {
            let Some(child) = node.child(i) else {
                continue;
            };
            if child.type_name() == "modifiers" {
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
                if text.contains("internal") {
                    return Some(Visibility::Internal);
                }
            }
        }
        Some(Visibility::Public)
    }

    fn is_static(&self, _node: &SyntaxNode) -> bool {
        false
    }

    fn is_async(&self, node: &SyntaxNode) -> bool {
        (0..node.child_count()).any(|i| {
            node.child(i)
                .is_some_and(|c| c.type_name() == "modifiers" && c.text().contains("suspend"))
        })
    }

    fn extract_modifiers(&self, node: &SyntaxNode) -> Option<Vec<String>> {
        let mut mods = Vec::new();
        for child in node
            .children()
            .into_iter()
            .filter(|child| child.type_name() == "modifiers")
        {
            for platform_modifier in child
                .children()
                .into_iter()
                .filter(|pm| pm.type_name() == "platform_modifier")
            {
                for keyword in platform_modifier.children() {
                    if keyword.type_name() == "expect" || keyword.type_name() == "actual" {
                        mods.push(keyword.type_name());
                    }
                }
            }
        }
        (!mods.is_empty()).then_some(mods)
    }

    fn extract_import(&self, node: &SyntaxNode, source: &str) -> Option<ImportInfo> {
        let import_text = get_node_text(node, source).trim().to_string();
        let module_name = kotlin_import_name(&import_text).or_else(|| {
            node.named_children()
                .into_iter()
                .find(|c| c.type_name() == "identifier")
                .map(|identifier| get_node_text(&identifier, source))
        })?;
        Some(ImportInfo {
            module_name,
            signature: import_text,
            handled_refs: false,
        })
    }

    fn package_types(&self) -> &'static [&'static str] {
        &["package_header"]
    }

    fn extract_package(&self, node: &SyntaxNode, source: &str) -> Option<String> {
        node.named_children()
            .into_iter()
            .find(|c| c.type_name() == "identifier" || c.type_name() == "qualified_identifier")
            .map(|id| get_node_text(&id, source).trim().to_string())
    }
}

fn visit_kotlin_property(node: &SyntaxNode, ctx: &mut dyn ExtractorContext) -> bool {
    let var_decl = node
        .named_children()
        .into_iter()
        .find(|c| c.type_name() == "variable_declaration");
    let name_node = var_decl.and_then(|decl| {
        decl.named_children()
            .into_iter()
            .find(|c| c.type_name() == "simple_identifier")
    });
    let Some(name_node) = name_node else {
        return false;
    };
    let name = get_node_text(&name_node, ctx.source());
    if name.is_empty() {
        return false;
    }

    let mut scope = "const";
    // Kotlin property 的图 kind 取决于出现位置：类体是字段，对象/companion 常量化，函数内跳过。
    let mut parent = node.parent();
    while let Some(p) = parent {
        let pt = p.type_name();
        if matches!(
            pt.as_str(),
            "function_body"
                | "function_declaration"
                | "lambda_literal"
                | "anonymous_initializer"
                | "control_structure_body"
                | "getter"
                | "setter"
        ) {
            scope = "local";
            break;
        }
        if pt == "companion_object" || pt == "object_declaration" {
            scope = "const";
            break;
        }
        if pt == "class_declaration" {
            scope = "instance";
            break;
        }
        parent = p.parent();
    }
    if scope == "local" {
        return true;
    }

    let binding = node
        .named_children()
        .into_iter()
        .find(|c| c.type_name() == "binding_pattern_kind");
    let is_val = binding
        .as_ref()
        .is_some_and(|binding| get_node_text(binding, ctx.source()) == "val");
    let kind = if scope == "instance" {
        NodeKind::Field
    } else if is_val {
        NodeKind::Constant
    } else {
        NodeKind::Variable
    };

    let type_node = node.child_for_field_name("type");
    let extra = NodeExtra {
        signature: type_node.map(|ty| {
            format!(
                "{} {}: {}",
                if is_val { "val" } else { "var" },
                name,
                get_node_text(ty, ctx.source())
            )
        }),
        ..Default::default()
    };
    ctx.create_node(kind, &name, node, extra);
    true
}

fn kotlin_fun_interface_name(node: &SyntaxNode) -> Option<String> {
    if node.type_name() == "function_declaration" {
        for child in node.children() {
            if child.type_name() == "ERROR"
                && let Some(id) = child
                    .children()
                    .into_iter()
                    .find(|gc| gc.type_name() == "simple_identifier")
            {
                return Some(id.text());
            }
        }
    }
    node.children()
        .into_iter()
        .find(|child| child.type_name() == "simple_identifier")
        .map(|child| child.text())
}
