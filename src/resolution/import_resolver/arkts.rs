//! ArkTS uses TS import bindings, but unresolved imports must never fall back
//! to unrelated same-name declarations in another module.
use crate::resolution::types::{ResolutionContext, ResolvedRef, UnresolvedRef};
use crate::types::ReferenceKind;

pub fn resolve_arkts_reference(
    reference: &UnresolvedRef,
    context: &mut dyn ResolutionContext,
) -> Option<ResolvedRef> {
    let bindings = context.get_import_mappings(&reference.file_path, reference.language);
    if reference.reference_kind == ReferenceKind::Imports
        || bindings.iter().any(|binding| {
            reference.reference_name == binding.local_name
                || reference
                    .reference_name
                    .starts_with(&format!("{}.", binding.local_name))
        })
    {
        return super::via::resolve_via_import(reference, context);
    }
    // Unimported names may resolve within the current file (including this
    // methods), but global exact/fuzzy matching is not module visibility.
    let result = crate::resolution::name_matcher::match_method_call(reference, context)
        .or_else(|| crate::resolution::name_matcher::match_by_exact_name(reference, context))?;
    let target = context.get_node_by_id(&result.target_node_id)?;
    (target.file_path == reference.file_path).then_some(result)
}
