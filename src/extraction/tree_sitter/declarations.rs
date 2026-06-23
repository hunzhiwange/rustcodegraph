use super::*;

impl TreeSitterExtractor {
    /// 函数声明的统一入口。匿名函数不直接建节点，但仍会扫描 body 中的调用，
    /// 这样能保留 flow 边而不污染符号表。
    pub(super) fn extract_function(&mut self, node: &SyntaxNode, name_override: Option<String>) {
        self.extract_function_with_extra(node, name_override, NodeExtra::default());
    }

    pub(super) fn extract_function_with_extra(
        &mut self,
        node: &SyntaxNode,
        name_override: Option<String>,
        extra: NodeExtra,
    ) {
        let Some(extractor) = self.extractor else {
            return;
        };
        let has_name_override = name_override.is_some();
        let name = name_override.unwrap_or_else(|| extract_name(node, &self.source, extractor));
        if name == "<anonymous>" && !has_name_override {
            if let Some(body) = extractor.resolve_body(node, extractor.body_field()) {
                self.visit_function_body(&body, "");
            }
            return;
        }
        if extractor.is_misparsed_function(&name, node) {
            if let Some(body) = extractor.resolve_body(node, extractor.body_field()) {
                self.visit_function_body(&body, "");
            }
            return;
        }
        let Some(created) = self.create_node(NodeKind::Function, &name, node, extra) else {
            return;
        };
        let id = node_id(&created);
        self.nodes.push(created);
        self.extract_type_annotations(node, &id);
        self.extract_decorators_for(node, &id);
        if let Some(body) = extractor.resolve_body(node, extractor.body_field()) {
            self.node_stack.push(id.clone());
            self.visit_function_body(&body, &id);
            self.node_stack.pop();
        }
    }

    pub(super) fn extract_class(&mut self, node: &SyntaxNode, kind: NodeKind) {
        let Some(extractor) = self.extractor else {
            return;
        };
        let name = extract_name(node, &self.source, extractor);
        let Some(created) = self.create_node(kind, &name, node, NodeExtra::default()) else {
            return;
        };
        let id = node_id(&created);
        self.nodes.push(created);
        self.extract_inheritance(node, &id);
        self.extract_decorators_for(node, &id);
        // Objective-C 的 property 往往出现在 interface 头部而不是 body 字段中，
        // 需要显式扫一遍，否则 class 的 API 面会缺失。
        if self.language == Language::ObjC && node.node_type() == "class_interface" {
            self.node_stack.push(id.clone());
            for property in collect_descendants_of_type(node, "property_declaration") {
                self.extract_property(&property);
            }
            self.node_stack.pop();
        }
        if let Some(body) = extractor.resolve_body(node, extractor.body_field()) {
            self.node_stack.push(id);
            for child in &body.named_children {
                self.visit_node(child);
            }
            self.node_stack.pop();
        }
    }

    pub(super) fn extract_method(&mut self, node: &SyntaxNode) {
        let Some(extractor) = self.extractor else {
            return;
        };
        let name = extract_name(node, &self.source, extractor);
        let receiver = extractor.get_receiver_type(node, &self.source);
        let Some(mut created) =
            self.create_node(NodeKind::Method, &name, node, NodeExtra::default())
        else {
            return;
        };
        // Go/Lua/Luau 的 receiver 方法不一定嵌在 class 节点下，qualified_name
        // 需要由语言适配器提供的 receiver 类型补齐。
        if let Some(receiver) = receiver {
            let separator =
                if matches!(self.language, Language::Go | Language::Lua | Language::Luau) {
                    "::"
                } else {
                    "."
                };
            created.qualified_name = format!("{receiver}{separator}{name}");
        }
        let id = node_id(&created);
        self.nodes.push(created);
        self.extract_type_annotations(node, &id);
        self.extract_decorators_for(node, &id);
        if let Some(body) = extractor.resolve_body(node, extractor.body_field()) {
            self.node_stack.push(id.clone());
            self.visit_function_body(&body, &id);
            self.node_stack.pop();
        }
    }

    pub(super) fn extract_interface(&mut self, node: &SyntaxNode) {
        let Some(extractor) = self.extractor else {
            return;
        };
        let name = extract_name(node, &self.source, extractor);
        let kind = extractor.interface_kind().unwrap_or(NodeKind::Interface);
        if let Some(created) = self.create_node(kind, &name, node, NodeExtra::default()) {
            let id = node_id(&created);
            self.nodes.push(created);
            self.extract_inheritance(node, &id);
            if let Some(body) = extractor.resolve_body(node, extractor.body_field()) {
                self.node_stack.push(id);
                for child in &body.named_children {
                    self.visit_node(child);
                }
                self.node_stack.pop();
            }
        }
    }

    pub(super) fn extract_struct(&mut self, node: &SyntaxNode) {
        let Some(extractor) = self.extractor else {
            return;
        };
        let name = extract_name(node, &self.source, extractor);
        if let Some(created) = self.create_node(NodeKind::Struct, &name, node, NodeExtra::default())
        {
            let id = node_id(&created);
            self.nodes.push(created);
            self.extract_inheritance(node, &id);
        }
    }

    pub(super) fn extract_enum(&mut self, node: &SyntaxNode) {
        let Some(extractor) = self.extractor else {
            return;
        };
        let name = extract_name(node, &self.source, extractor);
        if let Some(created) = self.create_node(NodeKind::Enum, &name, node, NodeExtra::default()) {
            let id = node_id(&created);
            self.nodes.push(created);
            self.extract_inheritance(node, &id);
            self.node_stack.push(id);
            self.extract_enum_members(node);
            self.node_stack.pop();
        }
    }

    pub(super) fn extract_enum_members(&mut self, node: &SyntaxNode) {
        let Some(extractor) = self.extractor else {
            return;
        };
        let member_types = extractor.enum_member_types();
        for child in &node.named_children {
            if member_types.contains(&child.node_type()) {
                let name = extract_name(child, &self.source, extractor);
                if let Some(created) =
                    self.create_node(NodeKind::EnumMember, &name, child, NodeExtra::default())
                {
                    self.nodes.push(created);
                }
            } else {
                self.extract_enum_members(child);
            }
        }
    }

    pub(super) fn extract_property(&mut self, node: &SyntaxNode) -> Option<Node> {
        let extractor = self.extractor?;
        let name = extractor
            .extract_property_name(node, &self.source)
            .unwrap_or_else(|| extract_name(node, &self.source, extractor));
        let signature = self
            .ts_js_property_type_annotation(node)
            .map(|type_name| format!("{type_name} {name}"));
        let created = self.create_node(
            NodeKind::Property,
            &name,
            node,
            NodeExtra {
                signature,
                ..NodeExtra::default()
            },
        )?;
        let id = node_id(&created);
        self.nodes.push(created.clone());
        self.extract_type_annotations(node, &id);
        self.extract_ts_js_property_type_refs(node, &id);
        self.extract_decorators_for(node, &id);
        // 属性初始化器里可能藏着函数引用或闭包，临时把属性作为 owner 扫描子树。
        self.node_stack.push(id);
        self.scan_fn_ref_subtree(node, 0);
        self.node_stack.pop();
        Some(created)
    }

    pub(super) fn ts_js_property_type_annotation(&self, node: &SyntaxNode) -> Option<String> {
        if !matches!(
            self.language,
            Language::TypeScript | Language::Tsx | Language::JavaScript | Language::Jsx
        ) {
            return None;
        }
        if let Some(annotation) =
            get_child_by_field(node, "type").or_else(|| get_child_by_field(node, "type_annotation"))
        {
            let text = get_node_text(annotation, &self.source)
                .trim()
                .trim_start_matches(':')
                .trim()
                .to_owned();
            if !text.is_empty() {
                return Some(text);
            }
        }
        self.ts_js_property_type_annotation_from_text(node)
    }

    pub(super) fn ts_js_property_type_annotation_from_text(
        &self,
        node: &SyntaxNode,
    ) -> Option<String> {
        let text = get_node_text(node, &self.source);
        let before_eq = text.split('=').next().unwrap_or(text.as_str());
        let colon = before_eq.find(':')?;
        let type_text = before_eq[colon + 1..]
            .trim()
            .trim_end_matches(';')
            .trim()
            .to_owned();
        (!type_text.is_empty()).then_some(type_text)
    }

    pub(super) fn extract_ts_js_property_type_refs(
        &mut self,
        node: &SyntaxNode,
        from_node_id: &str,
    ) {
        if !matches!(
            self.language,
            Language::TypeScript | Language::Tsx | Language::JavaScript | Language::Jsx
        ) {
            return;
        }
        if let Some(annotation) =
            get_child_by_field(node, "type").or_else(|| get_child_by_field(node, "type_annotation"))
        {
            self.extract_reference_refs_from_type_subtree(annotation, from_node_id);
        } else if let Some(type_text) = self.ts_js_property_type_annotation_from_text(node) {
            for name in ts_js_type_identifier_names(&type_text) {
                self.unresolved_references.push(unresolved_reference(
                    from_node_id.to_owned(),
                    name,
                    ReferenceKind::References,
                    node.start_position.row + 1,
                    node.start_position.column,
                ));
            }
        }
    }

    pub(super) fn extract_reference_refs_from_type_subtree(
        &mut self,
        node: &SyntaxNode,
        from_node_id: &str,
    ) {
        if matches!(
            node.node_type(),
            "type_identifier" | "identifier" | "qualified_type_identifier"
        ) {
            let name = get_node_text(node, &self.source);
            if name
                .chars()
                .next()
                .is_some_and(|first| first.is_ascii_uppercase())
            {
                self.unresolved_references.push(unresolved_reference(
                    from_node_id.to_owned(),
                    name,
                    ReferenceKind::References,
                    node.start_position.row + 1,
                    node.start_position.column,
                ));
            }
        }
        for child in &node.named_children {
            if matches!(
                child.node_type(),
                "type_identifier" | "identifier" | "qualified_type_identifier"
            ) {
                let name = get_node_text(child, &self.source);
                if name
                    .chars()
                    .next()
                    .is_some_and(|first| first.is_ascii_uppercase())
                {
                    self.unresolved_references.push(unresolved_reference(
                        from_node_id.to_owned(),
                        name,
                        ReferenceKind::References,
                        child.start_position.row + 1,
                        child.start_position.column,
                    ));
                }
            }
            self.extract_reference_refs_from_type_subtree(child, from_node_id);
        }
    }

    pub(super) fn extract_field(&mut self, node: &SyntaxNode) {
        let Some(extractor) = self.extractor else {
            return;
        };
        if self.language == Language::Java {
            let kind = if extractor.is_const(node) {
                NodeKind::Constant
            } else {
                NodeKind::Field
            };
            let declarators = node
                .named_children
                .iter()
                .filter(|child| child.node_type() == "variable_declarator")
                .collect::<Vec<_>>();
            if !declarators.is_empty() {
                for declarator in declarators {
                    let Some(name_node) = get_child_by_field(declarator, "name")
                        .or_else(|| declarator.named_child(0))
                    else {
                        continue;
                    };
                    let name = get_node_text(name_node, &self.source);
                    if let Some(created) =
                        self.create_node(kind, &name, declarator, NodeExtra::default())
                    {
                        let id = node_id(&created);
                        self.nodes.push(created);
                        self.extract_decorators_for(node, &id);
                        self.extract_type_annotations(node, &id);
                    }
                }
                return;
            }
        }
        let name = extract_name(node, &self.source, extractor);
        if let Some(created) = self.create_node(NodeKind::Field, &name, node, NodeExtra::default())
        {
            let id = node_id(&created);
            self.nodes.push(created);
            self.extract_decorators_for(node, &id);
            self.extract_type_annotations(node, &id);
        }
    }

    pub(super) fn extract_object_literal_functions(&mut self, obj: &SyntaxNode) {
        // object literal action（如 store/create 配置）经常承担业务入口角色；
        // 把函数字段提升为函数节点，explore-flow 才能沿调用边继续走。
        for child in &obj.named_children {
            if child.node_type() == "pair" {
                if let Some(value) = get_child_by_field(child, "value")
                    && matches!(value.node_type(), "arrow_function" | "function_expression")
                    && let Some(key) = get_child_by_field(child, "key")
                    && let Some(name) = self.object_key_name(key)
                {
                    self.extract_function(value, Some(name));
                }
            } else if child.node_type() == "method_definition"
                && let Some(key) = get_child_by_field(child, "name")
                && let Some(name) = self.object_key_name(key)
            {
                self.extract_function(child, Some(name));
            }
        }
    }

    pub(super) fn object_key_name(&self, key: &SyntaxNode) -> Option<String> {
        match key.node_type() {
            "property_identifier"
            | "identifier"
            | "string"
            | "string_fragment"
            | "field_identifier" => Some(
                get_node_text(key, &self.source)
                    .trim_matches(['"', '\''])
                    .to_owned(),
            ),
            _ => None,
        }
    }

    #[allow(dead_code)]
    pub(super) fn find_initializer_returned_object(
        &self,
        call_node: &SyntaxNode,
        depth: usize,
    ) -> Option<SyntaxNode> {
        if depth > 4 {
            return None;
        }
        call_node
            .named_children
            .iter()
            .find(|child| child.node_type() == "object")
            .cloned()
    }

    #[allow(dead_code)]
    pub(super) fn function_returned_object(&self, fn_node: &SyntaxNode) -> Option<SyntaxNode> {
        fn_node
            .named_children
            .iter()
            .find(|child| child.node_type() == "object")
            .cloned()
    }
}
