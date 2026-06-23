//! 解析阶段的内置符号/外部库过滤。
//!
//! 这里的判断发生在真正做名称匹配之前，用来避免把 `console.log`、
//! Python 内置方法、Go 标准库或 C++ 标准库符号误连到项目内同名节点。

use crate::types::Language;

use super::builtins_core::{
    GO_BUILT_INS, GO_STDLIB_PACKAGES, JS_BUILT_INS, PYTHON_BUILT_IN_METHODS, PYTHON_BUILT_IN_TYPES,
    PYTHON_BUILT_INS, REACT_HOOKS,
};
use super::builtins_native::{C_BUILT_INS, CPP_BUILT_INS, PASCAL_BUILT_INS, PASCAL_UNIT_PREFIXES};
use super::helpers::capitalize;
use super::{ReferenceResolver, UnresolvedRef};

impl<'db> ReferenceResolver<'db> {
    /// 判断一个引用是否应当视为语言内置或外部符号。
    ///
    /// 返回 `true` 表示 resolver 不再尝试建边；少数语言会先检查索引中
    /// 是否存在可信的同名项目符号，避免把用户自定义类型误判为内置。
    pub(super) fn is_built_in_or_external(&mut self, reference: &UnresolvedRef) -> bool {
        let name = reference.reference_name.as_str();
        let is_js_ts = matches!(
            reference.language,
            Language::TypeScript | Language::JavaScript | Language::Tsx | Language::Jsx
        );
        if is_js_ts && JS_BUILT_INS.contains(&name) {
            return true;
        }
        if is_js_ts
            && (name.starts_with("console.")
                || name.starts_with("Math.")
                || name.starts_with("JSON."))
        {
            return true;
        }
        if is_js_ts && REACT_HOOKS.contains(&name) {
            return true;
        }

        if reference.language == Language::Python && PYTHON_BUILT_INS.contains(&name) {
            return true;
        }
        if reference.language == Language::Python {
            if let Some(dot_idx) = name.find('.') {
                let receiver = &name[..dot_idx];
                let method = &name[dot_idx + 1..];
                if PYTHON_BUILT_IN_TYPES.contains(&receiver) {
                    return true;
                }
                // `list.append` 这类形状很像实例方法；若接收者首字母大写后
                // 已是项目内类型名，则保留给正常 resolver 处理。
                if PYTHON_BUILT_IN_METHODS.contains(&method) {
                    let capitalized = capitalize(receiver);
                    if !self
                        .known_names
                        .as_ref()
                        .map(|names| names.contains(&capitalized))
                        .unwrap_or(false)
                    {
                        return true;
                    }
                }
            }
            if PYTHON_BUILT_IN_METHODS.contains(&name)
                && !self
                    .known_names
                    .as_ref()
                    .map(|names| names.contains(name))
                    .unwrap_or(false)
            {
                return true;
            }
        }

        if reference.language == Language::Go {
            if let Some(dot_idx) = name.find('.') {
                let pkg = &name[..dot_idx];
                if GO_STDLIB_PACKAGES.contains(&pkg) {
                    return true;
                }
            }
            if GO_BUILT_INS.contains(&name) {
                return true;
            }
        }

        if reference.language == Language::Pascal {
            if PASCAL_UNIT_PREFIXES
                .iter()
                .any(|prefix| name.starts_with(prefix))
            {
                return true;
            }
            if PASCAL_BUILT_INS.contains(&name) {
                return true;
            }
        }

        if matches!(reference.language, Language::C | Language::Cpp) {
            if name.starts_with("std::") {
                return true;
            }
            // C/C++ 内置名经常与本地包装函数同名；只有在索引里完全找不到
            // 可能命中时才把它当成外部符号跳过。
            if C_BUILT_INS.contains(&name) || CPP_BUILT_INS.contains(&name) {
                return !self.has_any_possible_match(name);
            }
        }

        false
    }
}
