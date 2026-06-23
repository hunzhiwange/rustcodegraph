//! Function-as-value capture.
//!
//! Mirrors `function-ref.ts`: a language-specific table describes value
//! positions and wrapper forms, and the core extractor collects candidate
//! function references during traversal. The end-of-file gate still lives in
//! `TreeSitterExtractor`.
//!
//! 这里捕获“函数作为值传递”的候选项，例如回调参数、赋值右侧、数组/字典值。
//! 它只产出候选名，是否真的连边由后续 gate/resolver 决定，以降低误报。

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use crate::extraction::tree_sitter_helpers::{get_child_by_field, get_node_text};
use crate::web_tree_sitter::SyntaxNode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CaptureMode {
    // Args/List 直接扫描容器子节点；Rhs/Value/Varinit 会优先按字段取值。
    // mode 会写进候选，供后续 gate 判断这个引用是否足够可信。
    Args,
    Rhs,
    Value,
    List,
    Varinit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptureRule {
    pub mode: CaptureMode,
    pub field: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FnRefCandidate {
    pub name: String,
    pub line: usize,
    pub column: usize,
    pub mode: CaptureMode,
    pub explicit_ref: bool,
    pub skip_gate: bool,
}

#[derive(Debug, Clone)]
pub struct FnRefSpec {
    pub id_types: HashSet<&'static str>,
    pub dispatch: HashMap<&'static str, CaptureRule>,
    /// 只起包装作用的语法节点，例如 argument/value_argument。
    pub layers: HashMap<&'static str, Option<&'static str>>,
    /// 显式引用包装，例如 C/C++ 的 `&foo` 或 Pascal unary 节点。
    pub unwrap: HashMap<&'static str, Option<&'static str>>,
    /// 需要语言专门逻辑的 callable 表达式。
    pub special: HashSet<&'static str>,
    pub ungated_modes: HashSet<CaptureMode>,
    pub address_of_only: bool,
}

impl FnRefSpec {
    fn new(id_types: &[&'static str], dispatch: &[(&'static str, CaptureRule)]) -> Self {
        Self {
            id_types: id_types.iter().copied().collect(),
            dispatch: dispatch.iter().copied().collect(),
            layers: HashMap::new(),
            unwrap: HashMap::new(),
            special: HashSet::new(),
            ungated_modes: HashSet::new(),
            address_of_only: false,
        }
    }

    fn with_layers(mut self, layers: &[(&'static str, Option<&'static str>)]) -> Self {
        self.layers = layers.iter().copied().collect();
        self
    }

    fn with_unwrap(mut self, unwrap: &[(&'static str, Option<&'static str>)]) -> Self {
        self.unwrap = unwrap.iter().copied().collect();
        self
    }

    fn with_special(mut self, special: &[&'static str]) -> Self {
        self.special = special.iter().copied().collect();
        self
    }

    fn with_ungated_modes(mut self, modes: &[CaptureMode]) -> Self {
        self.ungated_modes = modes.iter().copied().collect();
        self
    }

    fn address_of_only(mut self) -> Self {
        self.address_of_only = true;
        self
    }
}

const NAME_STOPLIST: &[&str] = &[
    "this",
    "self",
    "super",
    "null",
    "nil",
    "true",
    "false",
    "undefined",
    "new",
    "NULL",
    "nullptr",
    "None",
];

pub static FN_REF_SPECS: LazyLock<HashMap<&'static str, FnRefSpec>> = LazyLock::new(|| {
    // 规则表尽量描述“在哪些语法位置可能出现函数值”，不要直接把普通调用
    // 加进来；普通 calls 边由 tree_sitter/calls.rs 负责。
    let ts_js_spec = FnRefSpec::new(
        &["identifier"],
        &[
            ("arguments", rule(CaptureMode::Args, None)),
            (
                "assignment_expression",
                rule(CaptureMode::Rhs, Some("right")),
            ),
            (
                "variable_declarator",
                rule(CaptureMode::Varinit, Some("value")),
            ),
            ("pair", rule(CaptureMode::Value, Some("value"))),
            ("array", rule(CaptureMode::List, None)),
        ],
    )
    .with_special(&["member_expression"]);

    let python_spec = FnRefSpec::new(
        &["identifier"],
        &[
            ("argument_list", rule(CaptureMode::Args, None)),
            ("assignment", rule(CaptureMode::Rhs, Some("right"))),
            ("keyword_argument", rule(CaptureMode::Value, Some("value"))),
            ("pair", rule(CaptureMode::Value, Some("value"))),
            ("list", rule(CaptureMode::List, None)),
        ],
    )
    .with_special(&["attribute"]);

    let go_spec = FnRefSpec::new(
        &["identifier"],
        &[
            ("argument_list", rule(CaptureMode::Args, None)),
            (
                "assignment_statement",
                rule(CaptureMode::Rhs, Some("right")),
            ),
            (
                "short_var_declaration",
                rule(CaptureMode::Rhs, Some("right")),
            ),
            ("var_spec", rule(CaptureMode::Varinit, Some("value"))),
            ("keyed_element", rule(CaptureMode::Value, None)),
            ("literal_value", rule(CaptureMode::List, None)),
        ],
    )
    .with_layers(&[("literal_element", None), ("expression_list", None)]);

    let rust_spec = FnRefSpec::new(
        &["identifier"],
        &[
            ("arguments", rule(CaptureMode::Args, None)),
            (
                "assignment_expression",
                rule(CaptureMode::Rhs, Some("right")),
            ),
            ("field_initializer", rule(CaptureMode::Value, Some("value"))),
            ("array_expression", rule(CaptureMode::List, None)),
            ("static_item", rule(CaptureMode::Varinit, Some("value"))),
            ("let_declaration", rule(CaptureMode::Varinit, Some("value"))),
        ],
    );

    let java_spec = FnRefSpec::new(
        &[],
        &[
            ("argument_list", rule(CaptureMode::Args, None)),
            (
                "assignment_expression",
                rule(CaptureMode::Rhs, Some("right")),
            ),
            (
                "variable_declarator",
                rule(CaptureMode::Varinit, Some("value")),
            ),
        ],
    )
    .with_special(&["method_reference"]);

    let kotlin_spec = FnRefSpec::new(
        &[],
        &[
            ("value_arguments", rule(CaptureMode::Args, None)),
            ("assignment", rule(CaptureMode::Rhs, None)),
        ],
    )
    .with_layers(&[("value_argument", None)])
    .with_special(&["callable_reference", "navigation_expression"]);

    let csharp_spec = FnRefSpec::new(
        &["identifier"],
        &[
            ("argument_list", rule(CaptureMode::Args, None)),
            (
                "assignment_expression",
                rule(CaptureMode::Rhs, Some("right")),
            ),
            ("initializer_expression", rule(CaptureMode::List, None)),
            ("variable_declarator", rule(CaptureMode::Varinit, None)),
        ],
    )
    .with_layers(&[("argument", None)])
    .with_special(&["member_access_expression"]);

    let ruby_spec = FnRefSpec::new(
        &[],
        &[
            ("argument_list", rule(CaptureMode::Args, None)),
            ("pair", rule(CaptureMode::Value, Some("value"))),
        ],
    )
    .with_layers(&[("block_argument", None)])
    .with_special(&["call", "simple_symbol"]);

    let swift_spec = FnRefSpec::new(
        &["simple_identifier"],
        &[
            ("value_arguments", rule(CaptureMode::Args, None)),
            ("assignment", rule(CaptureMode::Rhs, Some("result"))),
            ("array_literal", rule(CaptureMode::List, None)),
            (
                "property_declaration",
                rule(CaptureMode::Varinit, Some("value")),
            ),
        ],
    )
    .with_layers(&[("value_argument", Some("value"))])
    .with_special(&["selector_expression"]);

    let scala_spec = FnRefSpec::new(
        &["identifier"],
        &[
            ("arguments", rule(CaptureMode::Args, None)),
            (
                "assignment_expression",
                rule(CaptureMode::Rhs, Some("right")),
            ),
            ("val_definition", rule(CaptureMode::Varinit, Some("value"))),
        ],
    )
    .with_unwrap(&[("postfix_expression", None)]);

    let dart_spec = FnRefSpec::new(
        &["identifier"],
        &[
            ("arguments", rule(CaptureMode::Args, None)),
            (
                "assignment_expression",
                rule(CaptureMode::Rhs, Some("right")),
            ),
            ("pair", rule(CaptureMode::Value, Some("value"))),
            ("list_literal", rule(CaptureMode::List, None)),
            ("static_final_declaration", rule(CaptureMode::Varinit, None)),
        ],
    )
    .with_layers(&[("argument", None)]);

    let lua_spec = FnRefSpec::new(
        &["identifier"],
        &[
            ("arguments", rule(CaptureMode::Args, None)),
            ("assignment_statement", rule(CaptureMode::Rhs, None)),
            ("field", rule(CaptureMode::Value, Some("value"))),
        ],
    )
    .with_layers(&[("expression_list", None)]);

    let pascal_spec = FnRefSpec::new(
        &["identifier"],
        &[
            ("exprArgs", rule(CaptureMode::Args, None)),
            ("assignment", rule(CaptureMode::Rhs, Some("rhs"))),
        ],
    )
    .with_unwrap(&[("exprUnary", Some("operand"))]);

    let php_spec = FnRefSpec::new(&[], &[("arguments", rule(CaptureMode::Args, None))])
        .with_layers(&[("argument", None)])
        .with_special(&["encapsed_string", "string", "array_creation_expression"]);

    let c_spec = c_family_spec(&[], false);
    let cpp_spec = c_family_spec(&[], true);
    let objc_spec = c_family_spec(&["selector_expression"], false);

    let mut out = HashMap::new();
    // key 必须和 grammars::language_key / tree-sitter 抽取器使用的语言 key 对齐。
    out.insert("c", c_spec);
    out.insert("cpp", cpp_spec);
    out.insert("objc", objc_spec);
    out.insert("typescript", ts_js_spec.clone());
    out.insert("tsx", ts_js_spec.clone());
    out.insert("javascript", ts_js_spec.clone());
    out.insert("jsx", ts_js_spec);
    out.insert("python", python_spec);
    out.insert("go", go_spec);
    out.insert("rust", rust_spec);
    out.insert("java", java_spec);
    out.insert("kotlin", kotlin_spec);
    out.insert("csharp", csharp_spec);
    out.insert("php", php_spec);
    out.insert("ruby", ruby_spec);
    out.insert("swift", swift_spec);
    out.insert("scala", scala_spec);
    out.insert("dart", dart_spec);
    out.insert("lua", lua_spec.clone());
    out.insert("luau", lua_spec);
    out.insert("pascal", pascal_spec);
    out
});

fn rule(mode: CaptureMode, field: Option<&'static str>) -> CaptureRule {
    CaptureRule { mode, field }
}

fn c_family_spec(special: &[&'static str], address_of_only: bool) -> FnRefSpec {
    let spec = FnRefSpec::new(
        &["identifier"],
        &[
            ("argument_list", rule(CaptureMode::Args, None)),
            (
                "assignment_expression",
                rule(CaptureMode::Rhs, Some("right")),
            ),
            ("init_declarator", rule(CaptureMode::Varinit, Some("value"))),
            ("initializer_list", rule(CaptureMode::List, None)),
            ("initializer_pair", rule(CaptureMode::Value, Some("value"))),
        ],
    )
    .with_unwrap(&[("pointer_expression", Some("argument"))])
    .with_special(special)
    .with_ungated_modes(&[CaptureMode::Value, CaptureMode::List]);

    if address_of_only {
        // C++ 的规则表保留“应更保守”的标记；本层只记录 spec，
        // 真正是否据此过滤由后续 gate/resolver 决定。
        spec.address_of_only()
    } else {
        spec
    }
}

/// Extract candidate names from a dispatched container node.
pub fn capture_fn_ref_candidates(
    container: &SyntaxNode,
    rule: CaptureRule,
    spec: &FnRefSpec,
    source: &str,
) -> Vec<FnRefCandidate> {
    let mut value_nodes: Vec<SyntaxNode> = Vec::new();

    match rule.mode {
        CaptureMode::Args | CaptureMode::List => {
            value_nodes.extend(container.named_children.iter().cloned());
        }
        CaptureMode::Rhs => {
            let rhs = rule
                .field
                .and_then(|field| get_child_by_field(container, field).cloned())
                .or_else(|| container.last_named_child().cloned());
            if let Some(rhs) = rhs {
                let lhs = get_child_by_field(container, "left")
                    .or_else(|| get_child_by_field(container, "lhs"))
                    .or_else(|| get_child_by_field(container, "target"))
                    .or_else(|| {
                        (container.named_child_count() >= 2)
                            .then(|| container.named_child(0))
                            .flatten()
                    });
                let lhs_last_name = lhs
                    .map(|node| get_node_text(node, source))
                    .and_then(|text| trailing_identifier(&text));
                let rhs_text = get_node_text(&rhs, source).trim().to_owned();
                if lhs_last_name.as_deref() != Some(rhs_text.as_str()) {
                    // 跳过 `foo = foo` 这类自赋值/同名绑定，避免把变量名误记成
                    // 函数引用。
                    value_nodes.push(rhs);
                }
            }
        }
        CaptureMode::Value => {
            let value = rule
                .field
                .and_then(|field| get_child_by_field(container, field).cloned())
                .or_else(|| container.last_named_child().cloned());
            if let Some(value) = value {
                value_nodes.push(value);
            }
        }
        CaptureMode::Varinit => {
            let name_node = get_child_by_field(container, "name")
                .or_else(|| get_child_by_field(container, "pattern"));
            if let Some(name_node) = name_node
                && matches!(
                    name_node.node_type(),
                    "object_pattern" | "array_pattern" | "tuple_pattern" | "struct_pattern"
                )
            {
                // 解构声明本身不是一个函数值初始化；继续扫描会把字段名误当成
                // 函数名。
                return Vec::new();
            }
            if let Some(field) = rule.field {
                if let Some(value) = get_child_by_field(container, field).cloned() {
                    value_nodes.push(value);
                }
            } else if let Some(value) = container.last_named_child().cloned() {
                let name_child = get_child_by_field(container, "name")
                    .or_else(|| get_child_by_field(container, "pattern"));
                if container.named_child_count() >= 2
                    && name_child.map(|child| child.id) != Some(value.id)
                {
                    value_nodes.push(value);
                }
            }
        }
    }

    let mut out = Vec::new();
    for value in value_nodes {
        let explicit_ref = !spec.id_types.contains(value.node_type());
        // normalize_value 会剥掉语言特有包装，只留下 resolver 能匹配的候选名。
        for normalized in normalize_value(&value, spec, source, 0) {
            if normalized.name.is_empty() || NAME_STOPLIST.contains(&normalized.name.as_str()) {
                continue;
            }
            out.push(FnRefCandidate {
                name: normalized.name,
                line: normalized.node.start_position.row + 1,
                column: normalized.node.start_position.column,
                mode: rule.mode,
                explicit_ref,
                skip_gate: normalized.skip_gate,
            });
        }
    }

    out
}

#[derive(Debug, Clone)]
struct NormalizedRef {
    name: String,
    node: SyntaxNode,
    skip_gate: bool,
}

fn normalize_value(
    node: &SyntaxNode,
    spec: &FnRefSpec,
    source: &str,
    depth: usize,
) -> Vec<NormalizedRef> {
    if depth > 4 {
        // 规则表只处理少量包装层；超过这个深度通常是复杂表达式，继续递归
        // 更容易误报，也可能放大病态 AST。
        return Vec::new();
    }
    let ty = node.node_type();

    if spec.id_types.contains(ty) {
        return vec![NormalizedRef {
            name: get_node_text(node, source),
            node: node.clone(),
            skip_gate: false,
        }];
    }

    if let Some(layer_field) = spec.layers.get(ty) {
        if ty == "value_argument" {
            let label = get_child_by_field(node, "name");
            let value = get_child_by_field(node, "value").or_else(|| node.last_named_child());
            if let (Some(label), Some(value)) = (label, value)
                && get_node_text(label, source).trim() == get_node_text(value, source).trim()
            {
                // Kotlin/Swift 命名参数 `handler = handler` 这类写法不提供新的
                // 函数引用线索，跳过可减少自引用误报。
                return Vec::new();
            }
        }
        if let Some(field) = layer_field {
            return get_child_by_field(node, field)
                .map(|inner| normalize_value(inner, spec, source, depth + 1))
                .unwrap_or_default();
        }
        return node
            .named_children
            .iter()
            .flat_map(|child| normalize_value(child, spec, source, depth + 1))
            .collect();
    }

    if let Some(unwrap_field) = spec.unwrap.get(ty) {
        if ty == "pointer_expression" && node.child(0).map(|child| child.node_type()) != Some("&") {
            // C 家族只把显式 `&fn` 当作函数值；普通指针/解引用表达式不在这里猜。
            return Vec::new();
        }
        let inner = unwrap_field
            .and_then(|field| get_child_by_field(node, field))
            .or_else(|| node.named_child(0));
        let Some(inner) = inner else {
            return Vec::new();
        };
        if inner.node_type() == "qualified_identifier" {
            let text = get_node_text(inner, source).trim().to_owned();
            return is_qualified_ident(&text)
                .then(|| NormalizedRef {
                    name: text,
                    node: inner.clone(),
                    skip_gate: false,
                })
                .into_iter()
                .collect();
        }
        return normalize_value(inner, spec, source, depth + 1);
    }

    if spec.special.contains(ty) {
        return normalize_special(node, ty, source);
    }

    Vec::new()
}

fn normalize_special(node: &SyntaxNode, ty: &str, source: &str) -> Vec<NormalizedRef> {
    // special 分支只覆盖各语言最有信号的 callable 语法，输出尽量贴近
    // name_matcher/resolver 能识别的格式，如 `this.foo` 或 `Type::method`。
    match ty {
        "method_reference" => {
            let last = node
                .named_children
                .iter()
                .rfind(|child| child.node_type() == "identifier")
                .cloned();
            let Some(last) = last else {
                return Vec::new();
            };
            let method = get_node_text(&last, source);
            let text = get_node_text(node, source);
            if text.starts_with("this::") || text.starts_with("super::") {
                return normalized(format!("this.{method}"), last, false);
            }
            if let Some(receiver) = leading_type_receiver(&text, "::")
                && method != "new"
            {
                return normalized(format!("{receiver}::{method}"), last, false);
            }
            Vec::new()
        }
        "callable_reference" => {
            let receiver = node
                .named_children
                .iter()
                .find(|child| child.node_type() == "type_identifier")
                .cloned();
            let member = node
                .named_children
                .iter()
                .find(|child| child.node_type() == "simple_identifier")
                .cloned();
            let Some(member) = member else {
                return Vec::new();
            };
            let method = get_node_text(&member, source);
            if let Some(receiver) = receiver {
                let receiver_text = get_node_text(&receiver, source);
                if receiver_text
                    .chars()
                    .next()
                    .map(|ch| ch.is_ascii_uppercase())
                    .unwrap_or(false)
                {
                    return normalized(format!("{receiver_text}::{method}"), member, false);
                }
                Vec::new()
            } else {
                normalized(method, member, false)
            }
        }
        "navigation_expression" => {
            if !get_node_text(node, source).starts_with("this::") {
                return Vec::new();
            }
            for child in &node.named_children {
                if child.node_type() == "navigation_suffix"
                    && get_node_text(child, source).starts_with("::")
                    && let Some(id) = child.last_named_child().cloned()
                {
                    return normalized(format!("this.{}", get_node_text(&id, source)), id, false);
                }
            }
            Vec::new()
        }
        "selector_expression" => {
            let Some(inner) = node.named_child(0).cloned() else {
                return Vec::new();
            };
            if matches!(inner.node_type(), "identifier" | "simple_identifier") {
                return normalized(get_node_text(&inner, source), inner, false);
            }
            if let Some(last) = last_named_of_type(node, &["simple_identifier"]) {
                return normalized(get_node_text(&last, source), last, false);
            }
            normalized(
                get_node_text(&inner, source).trim().to_owned(),
                inner,
                false,
            )
        }
        "call" => {
            let Some(method) = get_child_by_field(node, "method") else {
                return Vec::new();
            };
            if get_node_text(method, source) != "method" {
                return Vec::new();
            }
            let Some(args) = get_child_by_field(node, "arguments") else {
                return Vec::new();
            };
            if args.named_child_count() != 1 {
                return Vec::new();
            }
            let Some(sym) = args.named_child(0).cloned() else {
                return Vec::new();
            };
            if sym.node_type() != "simple_symbol" {
                return Vec::new();
            }
            let name = get_node_text(&sym, source)
                .trim_start_matches(':')
                .to_owned();
            if !name.is_empty() {
                normalized(name, sym, false)
            } else {
                Default::default()
            }
        }
        "member_expression" => {
            let obj = get_child_by_field(node, "object");
            let prop = get_child_by_field(node, "property");
            if let (Some(obj), Some(prop)) = (obj, prop)
                && obj.node_type() == "this"
                && prop.node_type() == "property_identifier"
            {
                return normalized(
                    format!("this.{}", get_node_text(prop, source)),
                    prop.clone(),
                    false,
                );
            }
            Vec::new()
        }
        "attribute" => {
            let obj = get_child_by_field(node, "object");
            let attr = get_child_by_field(node, "attribute");
            if let (Some(obj), Some(attr)) = (obj, attr)
                && obj.node_type() == "identifier"
                && get_node_text(obj, source) == "self"
            {
                return normalized(get_node_text(attr, source), attr.clone(), false);
            }
            Vec::new()
        }
        "member_access_expression" => {
            let Some(name) = get_child_by_field(node, "name") else {
                return Vec::new();
            };
            let expression = get_child_by_field(node, "expression");
            let is_this_receiver = expression
                .map(|expr| matches!(expr.node_type(), "this_expression" | "this"))
                .unwrap_or_else(|| get_node_text(node, source).starts_with("this."));
            if is_this_receiver {
                normalized(get_node_text(name, source), name.clone(), false)
            } else {
                Vec::new()
            }
        }
        "encapsed_string" | "string" => {
            // PHP 字符串 callable 只有在高阶函数/注册函数参数里才有意义；
            // 普通字符串不能当函数名。
            let Some(callee) = php_enclosing_call_name(node) else {
                return Vec::new();
            };
            if !PHP_CALLABLE_HOFS.contains(&callee.as_str()) {
                return Vec::new();
            }
            let Some(content) = php_string_content(node, source) else {
                return Vec::new();
            };
            if is_simple_ident(&content) || is_qualified_php_callable(&content) {
                return normalized(content, node.clone(), true);
            }
            Vec::new()
        }
        "array_creation_expression" => {
            if node.named_child_count() != 2 {
                return Vec::new();
            }
            let recv = node.named_child(0).and_then(|child| child.named_child(0));
            let str_el = node.named_child(1).and_then(|child| child.named_child(0));
            let (Some(recv), Some(str_el)) = (recv, str_el) else {
                return Vec::new();
            };
            if !matches!(str_el.node_type(), "encapsed_string" | "string") {
                return Vec::new();
            }
            let Some(member) = php_string_content(str_el, source) else {
                return Vec::new();
            };
            if !is_simple_ident(&member) {
                return Vec::new();
            }
            if recv.node_type() == "variable_name" && get_node_text(recv, source) == "$this" {
                return normalized(format!("this.{member}"), str_el.clone(), false);
            }
            if recv.node_type() == "class_constant_access_expression" {
                let cls = recv.named_child(0);
                let kw = recv.named_child(1);
                if let (Some(cls), Some(kw)) = (cls, kw)
                    && get_node_text(kw, source) == "class"
                {
                    return normalized(
                        format!("{}::{member}", get_node_text(cls, source)),
                        str_el.clone(),
                        false,
                    );
                }
            }
            Vec::new()
        }
        "simple_symbol" => {
            // Ruby symbol 很常见，只在 Rails/Ruby 回调类 API 里把 `:method`
            // 当作当前对象方法引用。
            let Some(call) = ruby_enclosing_call(node) else {
                return Vec::new();
            };
            let Some(method) = get_child_by_field(&call, "method") else {
                return Vec::new();
            };
            if !is_ruby_hook_call(&get_node_text(method, source)) {
                return Vec::new();
            }
            let sym = get_node_text(node, source)
                .trim_start_matches(':')
                .to_owned();
            if is_ruby_method_symbol(&sym) {
                normalized(format!("this.{sym}"), node.clone(), false)
            } else {
                Vec::new()
            }
        }
        _ => Vec::new(),
    }
}

fn normalized(name: String, node: SyntaxNode, skip_gate: bool) -> Vec<NormalizedRef> {
    vec![NormalizedRef {
        name,
        node,
        skip_gate,
    }]
}

fn last_named_of_type(node: &SyntaxNode, types: &[&str]) -> Option<SyntaxNode> {
    let mut found = None;
    for child in &node.named_children {
        if types.contains(&child.node_type()) {
            found = Some(child.clone());
        }
        if let Some(deeper) = last_named_of_type(child, types) {
            found = Some(deeper);
        }
    }
    found
}

fn trailing_identifier(text: &str) -> Option<String> {
    let trimmed = text.trim_end();
    let chars = trimmed.chars().rev();
    let mut out = String::new();
    for ch in chars {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '$' {
            out.push(ch);
        } else {
            break;
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out.chars().rev().collect())
    }
}

fn is_simple_ident(text: &str) -> bool {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn is_qualified_ident(text: &str) -> bool {
    let mut parts = text.split("::");
    let Some(first) = parts.next() else {
        return false;
    };
    is_simple_ident(first) && parts.all(is_simple_ident)
}

fn is_qualified_php_callable(text: &str) -> bool {
    let Some((left, right)) = text.split_once("::") else {
        return false;
    };
    is_simple_ident(left) && is_simple_ident(right)
}

fn leading_type_receiver(text: &str, separator: &str) -> Option<String> {
    let (receiver, _) = text.split_once(separator)?;
    let first = receiver.chars().next()?;
    (first.is_ascii_uppercase() && is_simple_ident(receiver)).then(|| receiver.to_owned())
}

fn php_string_content(node: &SyntaxNode, source: &str) -> Option<String> {
    node.named_children
        .iter()
        .find(|child| child.node_type() == "string_content")
        .map(|child| get_node_text(child, source).trim().to_owned())
}

fn php_enclosing_call_name(node: &SyntaxNode) -> Option<String> {
    let mut cur = node.parent.as_deref();
    for _ in 0..4 {
        let current = cur?;
        match current.node_type() {
            "function_call_expression" => {
                return get_child_by_field(current, "function").map(|node| node.text.clone());
            }
            "member_call_expression" | "scoped_call_expression" => return None,
            _ => cur = current.parent.as_deref(),
        }
    }
    None
}

fn ruby_enclosing_call(node: &SyntaxNode) -> Option<SyntaxNode> {
    let mut cur = node.parent.as_deref();
    for _ in 0..4 {
        let current = cur?;
        if current.node_type() == "call" {
            return Some(current.clone());
        }
        cur = current.parent.as_deref();
    }
    None
}

fn is_ruby_hook_call(name: &str) -> bool {
    name == "validate"
        || name == "set_callback"
        || name == "helper_method"
        || name == "rescue_from"
        || ((name.starts_with("before_")
            || name.starts_with("after_")
            || name.starts_with("around_")
            || name.starts_with("skip_before_")
            || name.starts_with("skip_after_")
            || name.starts_with("skip_around_"))
            && name.chars().all(|ch| ch.is_ascii_lowercase() || ch == '_'))
}

fn is_ruby_method_symbol(sym: &str) -> bool {
    let mut chars = sym.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '?' || ch == '!')
}

static PHP_CALLABLE_HOFS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        "array_map",
        "array_filter",
        "array_walk",
        "array_walk_recursive",
        "array_reduce",
        "usort",
        "uasort",
        "uksort",
        "array_udiff",
        "array_udiff_assoc",
        "array_uintersect",
        "array_uintersect_assoc",
        "call_user_func",
        "call_user_func_array",
        "forward_static_call",
        "forward_static_call_array",
        "preg_replace_callback",
        "preg_replace_callback_array",
        "register_shutdown_function",
        "register_tick_function",
        "set_error_handler",
        "set_exception_handler",
        "spl_autoload_register",
        "ob_start",
        "iterator_apply",
        "header_register_callback",
        "is_callable",
    ]
    .into_iter()
    .collect()
});
