//! Rust translation of `src/extraction/languages/ruby.ts`.

use crate::extraction::tree_sitter_helpers::{get_child_by_field, get_node_text};
use crate::extraction::tree_sitter_types::{
    ExtractorContext, ImportInfo, LanguageExtractor, NodeExtra, UnresolvedReferenceInput,
    Visibility,
};
use crate::types::{NodeKind, ReferenceKind};
use crate::web_tree_sitter::SyntaxNode;

pub struct RubyExtractor;

pub const RUBY_EXTRACTOR: RubyExtractor = RubyExtractor;

pub fn ruby_extractor() -> &'static RubyExtractor {
    &RUBY_EXTRACTOR
}

impl LanguageExtractor for RubyExtractor {
    fn function_types(&self) -> &'static [&'static str] {
        &["method"]
    }

    fn class_types(&self) -> &'static [&'static str] {
        &["class"]
    }

    fn method_types(&self) -> &'static [&'static str] {
        &["method", "singleton_method"]
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

    fn type_alias_types(&self) -> &'static [&'static str] {
        &[]
    }

    fn import_types(&self) -> &'static [&'static str] {
        &["call"]
    }

    fn call_types(&self) -> &'static [&'static str] {
        &["call", "method_call"]
    }

    fn variable_types(&self) -> &'static [&'static str] {
        &["assignment"]
    }

    fn name_field(&self) -> &'static str {
        "name"
    }

    fn body_field(&self) -> &'static str {
        "body"
    }

    fn params_field(&self) -> &'static str {
        "parameters"
    }

    fn visit_node(&self, node: &SyntaxNode, ctx: &mut dyn ExtractorContext) -> bool {
        if node.type_name() == "call" && node.child_for_field_name("receiver").is_none() {
            let method = node.child_for_field_name("method");
            let mname = method.as_ref().map(|m| m.text());
            if matches!(mname.as_deref(), Some("include" | "extend" | "prepend")) {
                // Ruby mixin 是类/模块体内的裸调用，需要转换成 implements 引用。
                let args = node.child_for_field_name("arguments").cloned().or_else(|| {
                    node.named_children()
                        .into_iter()
                        .find(|c| c.type_name() == "argument_list")
                });
                if let (Some(parent_id), Some(args)) = (ctx.node_stack().last().cloned(), args) {
                    for i in 0..args.named_child_count() {
                        let Some(arg) = args.named_child(i) else {
                            continue;
                        };
                        if arg.type_name() == "constant" || arg.type_name() == "scope_resolution" {
                            ctx.add_unresolved_reference(UnresolvedReferenceInput {
                                from_node_id: parent_id.clone(),
                                reference_name: get_node_text(arg, ctx.source()),
                                reference_kind: ReferenceKind::Implements,
                                file_path: Some(ctx.file_path().to_string()),
                                line: Some(node.start_position().row + 1),
                                column: Some(node.start_position().column),
                            });
                        }
                    }
                    return true;
                }
            }
        }

        if node.type_name() != "module" {
            return false;
        }

        // module 既是命名空间也是 mixin 目标，通用 class_types 不处理它，所以这里手动建 Module。
        let Some(name_node) = node.child_for_field_name("name") else {
            return false;
        };
        let Some(module_node) = ctx.create_node(
            NodeKind::Module,
            &name_node.text(),
            node,
            NodeExtra::default(),
        ) else {
            return false;
        };
        ctx.push_scope(module_node.id.clone());
        if let Some(body) = node.child_for_field_name("body") {
            for child in body.named_children() {
                ctx.visit_node(&child);
            }
        }
        ctx.pop_scope();
        true
    }

    fn extract_bare_call(&self, node: &SyntaxNode, _source: &str) -> Option<String> {
        if node.type_name() != "identifier" {
            return None;
        }
        let parent = node.parent()?;
        const BLOCK_PARENTS: &[&str] = &[
            "body_statement",
            "then",
            "else",
            "do",
            "begin",
            "rescue",
            "ensure",
            "when",
        ];
        if !BLOCK_PARENTS.contains(&parent.type_name().as_str()) {
            return None;
        }
        // Ruby 允许无括号方法调用；只在语句块上下文里把裸 identifier 当 call，降低局部变量误报。
        let name = node.text();
        const SKIP: &[&str] = &[
            "true", "false", "nil", "self", "super", "__FILE__", "__LINE__", "__dir__",
        ];
        if SKIP.contains(&name.as_str()) {
            return None;
        }
        if name
            .as_bytes()
            .first()
            .is_some_and(|first| first.is_ascii_uppercase())
        {
            return None;
        }
        Some(name)
    }

    fn get_visibility(&self, node: &SyntaxNode) -> Option<Visibility> {
        // Ruby 的 visibility 是向后影响后续方法的声明调用，因此从当前方法向前找最近的 public/private/protected。
        let mut sibling = node.previous_named_sibling();
        while let Some(s) = sibling {
            if s.type_name() == "call"
                && let Some(method_name) = get_child_by_field(&s, "method")
            {
                return match method_name.text().as_str() {
                    "private" => Some(Visibility::Private),
                    "protected" => Some(Visibility::Protected),
                    "public" => Some(Visibility::Public),
                    _ => None,
                };
            }
            sibling = s.previous_named_sibling();
        }
        Some(Visibility::Public)
    }

    fn extract_import(&self, node: &SyntaxNode, source: &str) -> Option<ImportInfo> {
        let import_text = get_node_text(node, source).trim().to_string();
        let identifier = node
            .named_children()
            .into_iter()
            .find(|c| c.type_name() == "identifier")?;
        let method_name = get_node_text(&identifier, source);
        if method_name != "require" && method_name != "require_relative" {
            return None;
        }
        let arg_list = node
            .named_children()
            .into_iter()
            .find(|c| c.type_name() == "argument_list")?;
        let string_node = arg_list
            .named_children()
            .into_iter()
            .find(|c| c.type_name() == "string")?;
        let string_content = string_node
            .named_children()
            .into_iter()
            .find(|c| c.type_name() == "string_content")?;
        Some(ImportInfo {
            module_name: get_node_text(&string_content, source),
            signature: import_text,
            handled_refs: false,
        })
    }
}
