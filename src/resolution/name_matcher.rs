//! Name-based reference matching.
//!
//! Strategy order and confidence values mirror `name-matcher.ts`: file-path,
//! qualified, dynamic chain, method, exact, then fuzzy. This facade keeps the
//! original module path stable while strategy-specific code lives in smaller
//! modules.
//!
//! 中文维护提示：这里的顺序本身就是行为。越靠前的策略越精确，后面的 fuzzy
//! 只作为低置信度兜底，调整顺序会直接影响图边质量。

mod chains;
mod common;
mod cpp;
mod exact;
mod file_path;
mod function_ref;
mod fuzzy;
mod method_call;
mod typed;

use crate::resolution::types::{ResolutionContext, ResolvedRef, UnresolvedRef};
use crate::types::{Language, ReferenceKind};

pub use chains::{match_dotted_call_chain, match_scoped_call_chain};
pub use common::{crosses_known_family, is_known_language_family, same_language_family};
pub use cpp::match_cpp_call_chain;
pub use exact::{match_by_exact_name, match_by_qualified_name};
pub use file_path::match_by_file_path;
pub use function_ref::match_function_ref;
pub use fuzzy::match_fuzzy;
pub use method_call::match_method_call;

pub fn match_reference(
    reference: &UnresolvedRef,
    context: &mut dyn ResolutionContext,
) -> Option<ResolvedRef> {
    if reference.reference_kind == ReferenceKind::FunctionRef {
        return match_function_ref(reference, context);
    }

    // 优先选择路径、限定名和 typed chain 这类高信号策略；普通 exact/fuzzy 放在
    // 最后，避免一个常见短名抢走更具体的 import 或 receiver-type 结果。
    match_by_file_path(reference, context)
        .or_else(|| match_by_qualified_name(reference, context))
        .or_else(|| {
            matches!(reference.language, Language::Cpp | Language::C)
                .then(|| match_cpp_call_chain(reference, context))
                .flatten()
        })
        .or_else(|| {
            matches!(reference.language, Language::Php | Language::Rust)
                .then(|| match_scoped_call_chain(reference, context))
                .flatten()
        })
        .or_else(|| {
            matches!(
                reference.language,
                Language::Java
                    | Language::Kotlin
                    | Language::CSharp
                    | Language::Swift
                    | Language::Go
                    | Language::Scala
                    | Language::Dart
                    | Language::ObjC
                    | Language::Pascal
            )
            .then(|| match_dotted_call_chain(reference, context))
            .flatten()
        })
        .or_else(|| match_method_call(reference, context))
        .or_else(|| match_by_exact_name(reference, context))
        .or_else(|| match_fuzzy(reference, context))
}
