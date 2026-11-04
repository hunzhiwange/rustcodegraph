//! Conservative Erlang module and name/arity binding; never fall through to fuzzy names.
use super::{common::resolved, path::resolve_import_path};
use crate::extraction::grammars::get_parser;
use crate::resolution::types::{ResolutionContext, ResolvedBy, ResolvedRef, UnresolvedRef};
use crate::types::{Language, NodeKind, ReferenceKind};

pub fn resolve_erlang_reference(
    reference: &UnresolvedRef,
    context: &mut dyn ResolutionContext,
) -> Option<ResolvedRef> {
    let name = &reference.reference_name;
    let mut candidates = Vec::new();
    if reference.reference_kind == ReferenceKind::Imports && name.ends_with(".hrl") {
        let path = if name.starts_with('.') {
            name.clone()
        } else {
            format!("./{name}")
        };
        let file = resolve_import_path(&path, &reference.file_path, Language::Erlang, context)?;
        candidates.extend(
            context
                .get_nodes_in_file(&file)
                .into_iter()
                .filter(|n| n.kind == NodeKind::File),
        );
    } else if matches!(
        reference.reference_kind,
        ReferenceKind::Imports | ReferenceKind::Implements
    ) {
        candidates.extend(
            context
                .get_nodes_by_name(name)
                .into_iter()
                .filter(|n| n.language == Language::Erlang && n.kind == NodeKind::Namespace),
        );
    } else if name.contains("::") {
        candidates.extend(
            context
                .get_nodes_by_qualified_name(name)
                .into_iter()
                .filter(|n| n.language == Language::Erlang && n.kind == NodeKind::Function),
        );
    } else {
        candidates.extend(
            context
                .get_nodes_in_file(&reference.file_path)
                .into_iter()
                .filter(|n| {
                    n.language == Language::Erlang
                        && if name.contains('/') {
                            n.kind == NodeKind::Function
                                && n.qualified_name
                                    .rsplit_once("::")
                                    .is_some_and(|(_, tail)| tail == name)
                        } else {
                            n.kind == NodeKind::Constant && n.name == *name
                        }
                }),
        );
        if candidates.is_empty() && name.contains('/') {
            let content = context.read_file(&reference.file_path)?;
            let mut parser = get_parser(Language::Erlang)?;
            let tree = parser.parse(&content, None)?;
            for form in &tree.root_node.named_children {
                if form.node_type() != "import_attribute" || form.has_error {
                    continue;
                }
                let Some(module) = form.child_for_field_name("module") else {
                    continue;
                };
                for fa in form.named_children.iter().filter(|n| n.node_type() == "fa") {
                    let Some(fun) = fa.child_for_field_name("fun") else {
                        continue;
                    };
                    let Some(arity) = fa
                        .child_for_field_name("arity")
                        .and_then(|n| n.child_for_field_name("value"))
                    else {
                        continue;
                    };
                    if format!("{}/{}", fun.text.trim_matches('\''), arity.text) == *name {
                        let qualified = format!("{}::{name}", module.text.trim_matches('\''));
                        candidates.extend(
                            context
                                .get_nodes_by_qualified_name(&qualified)
                                .into_iter()
                                .filter(|n| {
                                    n.language == Language::Erlang && n.kind == NodeKind::Function
                                }),
                        );
                    }
                }
            }
        }
    }
    candidates.sort_by(|a, b| a.id.cmp(&b.id));
    candidates.dedup_by(|a, b| a.id == b.id);
    let [target] = candidates.as_slice() else {
        return None;
    };
    Some(resolved(reference, &target.id, 1.0, ResolvedBy::Import))
}
