//! 链式调用引用的匹配策略。
//!
//! extractor 会把部分 `factory().method` 形状保留下来；这里通过 factory 的返回
//! 类型或语言特定构造规则，把最终调用连到实例方法。

use crate::resolution::types::{ResolutionContext, ResolvedBy, ResolvedRef, UnresolvedRef};
use crate::types::Language;

use super::common::split_chain_shape;
use super::exact::match_by_exact_name;
use super::fuzzy::match_fuzzy;
use super::typed::{imported_fqn_of, lookup_callee_return_type, resolve_method_on_type};

pub fn match_scoped_call_chain(
    reference: &UnresolvedRef,
    context: &mut dyn ResolutionContext,
) -> Option<ResolvedRef> {
    // Rust/PHP 等 scoped 形状用 `Type::factory().method` 表达；先解析 factory
    // 的返回类型，再落到最终 method。
    let (inner, method) = split_chain_shape(&reference.reference_name)?;
    if !inner.contains("::") {
        return None;
    }
    let factory_class = inner[..inner.rfind("::")?].to_string();
    let ret = lookup_callee_return_type(inner, reference, context)?;
    let resolved_class = if ret == "self" { factory_class } else { ret };
    resolve_method_on_type(
        &resolved_class,
        method,
        reference,
        context,
        0.85,
        ResolvedBy::InstanceMethod,
        None,
        0,
    )
}

const CONSTRUCTS_VIA_BARE_CALL: &[Language] = &[
    Language::Kotlin,
    Language::Swift,
    Language::Scala,
    Language::Dart,
    Language::Pascal,
];

pub fn match_dotted_call_chain(
    reference: &UnresolvedRef,
    context: &mut dyn ResolutionContext,
) -> Option<ResolvedRef> {
    // dotted 形状覆盖 Java/Kotlin/C#/Swift/Go 等语言；不同语言的构造函数和
    // factory 命名习惯不同，因此下面保留了几条保守的启发式路径。
    let (inner, method) = split_chain_shape(&reference.reference_name)?;
    let Some(last_dot) = inner.rfind('.') else {
        if reference.language == Language::Go {
            if let Some(ret) = lookup_callee_return_type(inner, reference, context) {
                let fqn = imported_fqn_of(&ret, reference, context);
                return resolve_method_on_type(
                    &ret,
                    method,
                    reference,
                    context,
                    0.85,
                    ResolvedBy::InstanceMethod,
                    fqn.as_deref(),
                    0,
                );
            }
            let mut bare_ref = reference.clone();
            bare_ref.reference_name = method.to_string();
            let bare =
                match_by_exact_name(&bare_ref, context).or_else(|| match_fuzzy(&bare_ref, context));
            return bare.map(|mut resolved_ref| {
                resolved_ref.original = reference.clone();
                resolved_ref
            });
        }
        if !CONSTRUCTS_VIA_BARE_CALL.contains(&reference.language)
            || !inner
                .chars()
                .next()
                .map(|ch| ch.is_ascii_uppercase())
                .unwrap_or(false)
        {
            return None;
        }
        let fqn = imported_fqn_of(inner, reference, context);
        return resolve_method_on_type(
            inner,
            method,
            reference,
            context,
            0.85,
            ResolvedBy::InstanceMethod,
            fqn.as_deref(),
            0,
        );
    };

    if last_dot == 0 {
        return None;
    }
    let factory_class = inner[..last_dot].rsplit('.').next()?;
    let factory_method = &inner[last_dot + 1..];
    let ret = lookup_callee_return_type(
        &format!("{factory_class}::{factory_method}"),
        reference,
        context,
    );
    let receiver_type = if let Some(ret) = ret {
        ret
    } else if (reference.language == Language::ObjC
        && factory_class
            .chars()
            .next()
            .map(|ch| ch.is_ascii_uppercase())
            .unwrap_or(false))
        || (reference.language == Language::Pascal
            && factory_class
                .chars()
                .next()
                .map(|ch| ch == 'T' || ch == 'I')
                .unwrap_or(false))
    {
        factory_class.to_string()
    } else {
        return None;
    };

    let fqn = imported_fqn_of(&receiver_type, reference, context);
    resolve_method_on_type(
        &receiver_type,
        method,
        reference,
        context,
        0.85,
        ResolvedBy::InstanceMethod,
        fqn.as_deref(),
        0,
    )
}
