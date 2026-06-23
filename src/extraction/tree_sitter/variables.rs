use super::*;

impl TreeSitterExtractor {
    /// 变量抽取优先使用语言适配器返回的结构化结果；适配器没处理时才走少量
    /// 语言 fallback，保证新增语言不会被通用规则误判。
    pub(super) fn extract_variable(&mut self, node: &SyntaxNode) {
        if let Some(extractor) = self.extractor {
            let mut handled = false;
            for variable in extractor.extract_variables(node, &self.source) {
                handled = true;
                if let Some(delegate) = variable.delegate_to_function {
                    let docstring = variable
                        .position_node
                        .as_ref()
                        .and_then(|pos| get_preceding_docstring(pos, &self.source))
                        .or_else(|| get_preceding_docstring(node, &self.source));
                    self.extract_function_with_extra(
                        &delegate,
                        Some(variable.name),
                        NodeExtra {
                            docstring,
                            is_exported: variable.is_exported,
                            ..NodeExtra::default()
                        },
                    );
                    continue;
                }
                let pos = variable.position_node.as_ref().unwrap_or(node);
                let extra = NodeExtra {
                    signature: variable.signature,
                    is_exported: variable.is_exported,
                    ..NodeExtra::default()
                };
                if let Some(created) = self.create_node(variable.kind, &variable.name, pos, extra) {
                    self.nodes.push(created);
                }
                if let Some(value) = variable.visit_value {
                    self.visit_node(&value);
                }
                if let Some(object) = variable.object_literal_functions {
                    self.extract_object_literal_functions(&object);
                }
            }
            if !handled {
                self.extract_variable_fallback(node);
            }
        }
        self.scan_fn_ref_subtree(node, 0);
    }

    pub(super) fn extract_variable_fallback(&mut self, node: &SyntaxNode) {
        match self.language {
            Language::Rust => self.extract_rust_variable(node),
            Language::Go => self.extract_go_variable(node),
            Language::Python => self.extract_python_variable(node),
            Language::Java => self.extract_java_local_variable(node),
            _ => {}
        }
    }

    pub(super) fn extract_rust_variable(&mut self, node: &SyntaxNode) {
        let kind = match node.node_type() {
            "const_item" | "static_item" => NodeKind::Constant,
            "let_declaration" => NodeKind::Variable,
            _ => return,
        };
        let Some(name_node) = get_child_by_field(node, "name")
            .or_else(|| get_child_by_field(node, "pattern"))
            .or_else(|| node.named_child(0))
        else {
            return;
        };
        if name_node.node_type() != "identifier" {
            return;
        }
        let name = get_node_text(name_node, &self.source);
        if let Some(created) = self.create_node(kind, &name, node, NodeExtra::default()) {
            self.nodes.push(created);
        }
    }

    pub(super) fn extract_go_variable(&mut self, node: &SyntaxNode) {
        if node.node_type() == "short_var_declaration" {
            let Some(left) = get_child_by_field(node, "left").or_else(|| node.named_child(0))
            else {
                return;
            };
            let identifiers = if left.node_type() == "expression_list" {
                left.named_children
                    .iter()
                    .filter(|child| child.node_type() == "identifier")
                    .collect::<Vec<_>>()
            } else {
                vec![left]
            };
            for identifier in identifiers {
                let name = get_node_text(identifier, &self.source);
                if let Some(created) =
                    self.create_node(NodeKind::Variable, &name, identifier, NodeExtra::default())
                {
                    self.nodes.push(created);
                }
            }
            return;
        }

        let kind = if node.node_type() == "const_declaration" {
            NodeKind::Constant
        } else {
            NodeKind::Variable
        };
        for spec in node
            .named_children
            .iter()
            .filter(|child| matches!(child.node_type(), "const_spec" | "var_spec"))
        {
            let Some(name_node) = spec.named_child(0) else {
                continue;
            };
            if name_node.node_type() != "identifier" {
                continue;
            }
            let name = get_node_text(name_node, &self.source);
            if let Some(created) = self.create_node(kind, &name, spec, NodeExtra::default()) {
                let id = node_id(&created);
                self.nodes.push(created);
                if let Some(value) = get_child_by_field(spec, "value") {
                    self.node_stack.push(id);
                    let function_id = self.node_stack.last().cloned().unwrap_or_default();
                    self.visit_function_body(value, &function_id);
                    self.node_stack.pop();
                }
            }
        }
    }

    pub(super) fn extract_python_variable(&mut self, node: &SyntaxNode) {
        if !get_node_text(node, &self.source).contains('=') {
            return;
        }
        let Some(left) = get_child_by_field(node, "left").or_else(|| node.named_child(0)) else {
            return;
        };
        if left.node_type() != "identifier" {
            return;
        }
        let name = get_node_text(left, &self.source);
        let kind = if name.chars().any(|ch| ch == '_' || ch.is_ascii_uppercase()) {
            NodeKind::Constant
        } else {
            NodeKind::Variable
        };
        if let Some(created) = self.create_node(kind, &name, node, NodeExtra::default()) {
            self.nodes.push(created);
        }
    }

    pub(super) fn extract_java_local_variable(&mut self, node: &SyntaxNode) {
        for declarator in node
            .named_children
            .iter()
            .filter(|child| child.node_type() == "variable_declarator")
        {
            let Some(name_node) =
                get_child_by_field(declarator, "name").or_else(|| declarator.named_child(0))
            else {
                continue;
            };
            let name = get_node_text(name_node, &self.source);
            if let Some(created) =
                self.create_node(NodeKind::Variable, &name, declarator, NodeExtra::default())
            {
                self.nodes.push(created);
            }
        }
    }

    pub(super) fn extract_type_alias(&mut self, node: &SyntaxNode) -> bool {
        // type alias 既可能是普通别名，也可能被语言适配器提升为 interface/struct。
        // 创建节点后立即扫右侧类型引用，保证契约类型能被 explore 展示出来。
        let Some(extractor) = self.extractor else {
            return false;
        };
        let name = extract_name(node, &self.source, extractor);
        let kind = extractor
            .resolve_type_alias_kind(node, &self.source)
            .unwrap_or(NodeKind::TypeAlias);
        if let Some(created) = self.create_node(kind, &name, node, NodeExtra::default()) {
            let id = node_id(&created);
            self.nodes.push(created.clone());
            if let Some(value) = get_child_by_field(node, "value").or_else(|| {
                node.named_children.iter().rev().find(|child| {
                    !matches!(child.node_type(), "type_identifier" | "type_parameters")
                })
            }) {
                if self.language == Language::Go && kind == NodeKind::Interface {
                    self.extract_inheritance(value, &id);
                    self.extract_go_interface_methods(value, &id);
                    return true;
                }
                self.extract_type_refs_from_subtree(value, &id);
                if matches!(self.language, Language::TypeScript | Language::Tsx) {
                    self.extract_ts_tuple_contract_names(value, &created);
                }
            } else {
                self.extract_type_refs_from_subtree(node, &id);
            }
            true
        } else {
            false
        }
    }

    pub(super) fn extract_go_interface_methods(
        &mut self,
        interface_type: &SyntaxNode,
        iface_id: &str,
    ) {
        self.node_stack.push(iface_id.to_owned());
        for method in &interface_type.named_children {
            if !matches!(method.node_type(), "method_elem" | "method_spec") {
                continue;
            }
            let Some(name_node) =
                get_child_by_field(method, "name").or_else(|| method.named_child(0))
            else {
                continue;
            };
            let name = get_node_text(name_node, &self.source);
            if name.is_empty() {
                continue;
            }
            if let Some(created) = self.create_node(
                NodeKind::Method,
                &name,
                method,
                NodeExtra {
                    signature: self
                        .extractor
                        .and_then(|extractor| extractor.get_signature(method, &self.source)),
                    ..NodeExtra::default()
                },
            ) {
                self.nodes.push(created);
            }
        }
        self.node_stack.pop();
    }

    #[allow(dead_code)]
    pub(super) fn extract_ts_type_alias_members(
        &mut self,
        value: &SyntaxNode,
        type_alias_node: &Node,
    ) {
        let _ = (value, type_alias_node);
    }

    pub(super) fn extract_ts_tuple_contract_names(
        &mut self,
        value: &SyntaxNode,
        type_alias_node: &Node,
    ) {
        // 一些 TS RPC/contract 类型把方法名编码在 tuple literal 的字符串参数里；
        // 抽成 Method 节点后，agent 才能按方法名直接检索和追踪。
        let mut tuples = Vec::new();
        collect_ts_tuple_types(value, 0, &mut tuples);
        if tuples.is_empty() {
            return;
        }

        self.node_stack.push(node_id(type_alias_node));
        for tuple in tuples {
            for entry in &tuple.named_children {
                let Some(generic_entry) = ts_tuple_direct_generic_entry(entry) else {
                    continue;
                };
                let Some(type_args) = get_child_by_field(generic_entry, "type_arguments") else {
                    continue;
                };
                for arg in &type_args.named_children {
                    if arg.node_type() != "literal_type" {
                        continue;
                    }
                    let literal_text = arg
                        .named_child(0)
                        .filter(|child| child.node_type() == "string")
                        .map(|child| get_node_text(child, &self.source))
                        .unwrap_or_else(|| get_node_text(arg, &self.source));
                    let name = literal_text
                        .trim()
                        .trim_matches(['\'', '"', '`'])
                        .to_owned();
                    if !is_valid_ts_contract_name(&name) {
                        continue;
                    }
                    let signature =
                        collapse_whitespace(&get_node_text(generic_entry, &self.source))
                            .chars()
                            .take(120)
                            .collect::<String>();
                    if let Some(mut created) = self.create_node(
                        NodeKind::Method,
                        &name,
                        generic_entry,
                        NodeExtra {
                            signature: Some(signature),
                            ..NodeExtra::default()
                        },
                    ) {
                        created.qualified_name = format!("{}::{}", type_alias_node.name, name);
                        self.nodes.push(created);
                    }
                }
            }
        }
        self.node_stack.pop();
    }

    #[allow(dead_code)]
    pub(super) fn is_ts_function_typed_property(&self, property_signature: &SyntaxNode) -> bool {
        property_signature
            .named_children
            .iter()
            .any(|child| child.node_type().contains("function"))
    }
}
