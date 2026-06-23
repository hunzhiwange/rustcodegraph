//! 方法调用形状的名称匹配。
//!
//! 这里处理 `obj.method`、`Type::method` 和少量语言特定 receiver 推断；无法确定
//! receiver 类型时，才退回到同名方法的保守打分。

use crate::resolution::types::{ResolutionContext, ResolvedBy, ResolvedRef, UnresolvedRef};
use crate::types::{Language, NodeKind};

use super::common::{capitalize, resolved, split_camel_case};
use super::cpp::infer_cpp_receiver_type;
use super::typed::{imported_fqn_of, infer_java_field_receiver_type, resolve_method_on_type};

pub fn match_method_call(
    reference: &UnresolvedRef,
    context: &mut dyn ResolutionContext,
) -> Option<ResolvedRef> {
    let parsed = parse_method_call(&reference.reference_name)?;
    let (object_or_class, method_name, was_dot) = parsed;

    if reference.language == Language::Cpp
        && was_dot
        && let Some(inferred_type) = infer_cpp_receiver_type(object_or_class, reference, context, 0)
        && let Some(typed) = resolve_method_on_type(
            &inferred_type,
            method_name,
            reference,
            context,
            0.9,
            ResolvedBy::InstanceMethod,
            None,
            0,
        )
    {
        // C++ 的 `obj.method` 需要先从局部声明/auto initializer 推断 obj 类型。
        return Some(typed);
    }

    if matches!(reference.language, Language::Java | Language::Kotlin)
        && was_dot
        && let Some(inferred_type) =
            infer_java_field_receiver_type(object_or_class, reference, context)
    {
        // Java/Kotlin 字段调用常见于服务类成员：`repo.find()`；字段声明提供的
        // 类型比同名方法全局搜索更可靠。
        let imported_fqn = imported_fqn_of(&inferred_type, reference, context);
        if let Some(typed) = resolve_method_on_type(
            &inferred_type,
            method_name,
            reference,
            context,
            0.9,
            ResolvedBy::InstanceMethod,
            imported_fqn.as_deref(),
            0,
        ) {
            return Some(typed);
        }
    }

    for class_node in context.get_nodes_by_name(object_or_class) {
        if !matches!(
            class_node.kind,
            NodeKind::Class | NodeKind::Struct | NodeKind::Interface
        ) || class_node.language != reference.language
        {
            continue;
        }
        if let Some(method_node) = context
            .get_nodes_in_file(&class_node.file_path)
            .into_iter()
            .find(|node| {
                node.kind == NodeKind::Method
                    && node.name == method_name
                    && node.qualified_name.contains(&class_node.name)
            })
        {
            return Some(resolved(
                reference,
                &method_node.id,
                0.85,
                ResolvedBy::QualifiedName,
            ));
        }
    }

    let capitalized_receiver = capitalize(object_or_class);
    if capitalized_receiver != object_or_class {
        // 动态语言或 Pascal 风格里 receiver 变量名可能只是类型名的首字母小写版；
        // 这条低一点的置信度路径只在找到对应类型内方法时生效。
        for class_node in context.get_nodes_by_name(&capitalized_receiver) {
            if !matches!(
                class_node.kind,
                NodeKind::Class | NodeKind::Struct | NodeKind::Interface
            ) || class_node.language != reference.language
            {
                continue;
            }
            if let Some(method_node) = context
                .get_nodes_in_file(&class_node.file_path)
                .into_iter()
                .find(|node| {
                    node.kind == NodeKind::Method
                        && node.name == method_name
                        && node.qualified_name.contains(&class_node.name)
                })
            {
                return Some(resolved(
                    reference,
                    &method_node.id,
                    0.8,
                    ResolvedBy::InstanceMethod,
                ));
            }
        }
    }

    let methods = context
        .get_nodes_by_name(method_name)
        .into_iter()
        .filter(|node| node.kind == NodeKind::Method && node.name == method_name)
        .collect::<Vec<_>>();
    let same_language = methods
        .iter()
        .filter(|node| node.language == reference.language)
        .cloned()
        .collect::<Vec<_>>();
    let target_methods = if same_language.is_empty() {
        methods
    } else {
        same_language
    };

    if target_methods.len() == 1 && target_methods[0].language == reference.language {
        return Some(resolved(
            reference,
            &target_methods[0].id,
            0.7,
            ResolvedBy::InstanceMethod,
        ));
    }

    if target_methods.len() > 1 {
        // 最后的歧义处理只比较 receiver 与 qualified_name 的驼峰词重合度；
        // 分数门槛保守，避免在 god-class 或 common method 名称上误连。
        let receiver_words = split_camel_case(object_or_class);
        let mut best = None;
        let mut best_score = 0;
        for method in target_methods {
            let class_words = split_camel_case(&method.qualified_name);
            let mut score = receiver_words
                .iter()
                .filter(|word| {
                    class_words
                        .iter()
                        .any(|class_word| class_word.eq_ignore_ascii_case(word))
                })
                .count() as i32;
            if method.language == reference.language {
                score += 1;
            }
            if score > best_score {
                best_score = score;
                best = Some(method);
            }
        }
        if best_score >= 2 {
            let best = best?;
            return Some(resolved(
                reference,
                &best.id,
                0.65,
                ResolvedBy::InstanceMethod,
            ));
        }
    }

    None
}

fn parse_method_call(value: &str) -> Option<(&str, &str, bool)> {
    // 解析器只接受简单 ASCII 标识符链；包含泛型、调用参数或复杂表达式时返回 None，
    // 让更精确的 extractor/resolver 处理。
    if let Some(idx) = value.rfind('.') {
        let receiver = &value[..idx];
        let method = &value[idx + 1..];
        if !receiver.is_empty()
            && !method.is_empty()
            && receiver
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '.')
            && method
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == ':')
        {
            return Some((receiver, method, true));
        }
    }
    if let Some(idx) = value.rfind("::") {
        let receiver = &value[..idx];
        let method = &value[idx + 2..];
        if !receiver.is_empty()
            && !method.is_empty()
            && !receiver.contains("::")
            && receiver
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
            && method
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        {
            return Some((receiver, method, false));
        }
    }
    None
}
