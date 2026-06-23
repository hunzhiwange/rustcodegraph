use super::*;

impl TreeSitterExtractor {
    pub(super) fn ts_js_call_callee_name(&self, node: &SyntaxNode) -> Option<String> {
        let func = get_child_by_field(node, "function").or_else(|| node.named_child(0))?;
        if func.node_type() == "member_expression" {
            return self.ts_js_member_callee_name(func);
        }
        let name = get_node_text(func, &self.source);
        (!name.is_empty()).then_some(name)
    }

    pub(super) fn ts_js_member_callee_name(&self, member: &SyntaxNode) -> Option<String> {
        let property = get_child_by_field(member, "property")
            .or_else(|| get_child_by_field(member, "field"))
            .or_else(|| member.named_child(1))?;
        let method_name = get_node_text(property, &self.source);
        if method_name.is_empty() {
            return None;
        }

        let receiver = get_child_by_field(member, "object")
            .or_else(|| get_child_by_field(member, "operand"))
            .or_else(|| member.named_child(0));
        let Some(receiver) = receiver else {
            return Some(method_name);
        };
        // 对普通 receiver 保留 `receiver.method` 可帮助 resolver 做同文件成员匹配；
        // `this/self/super` 则退回短名，避免把当前对象名当成类型前缀。
        if matches!(
            receiver.node_type(),
            "identifier" | "simple_identifier" | "field_identifier"
        ) {
            let receiver_name = get_node_text(receiver, &self.source);
            if !matches!(receiver_name.as_str(), "self" | "this" | "cls" | "super") {
                return Some(format!("{receiver_name}.{method_name}"));
            }
        }

        Some(method_name)
    }

    pub(super) fn extract_call(&mut self, node: &SyntaxNode) {
        // 调用边先作为 unresolved reference 记录，真正目标由后续 name matcher 和
        // import resolver 决定；这样能跨文件、跨语言统一处理重名和导入别名。
        let Some(caller_id) = self.node_stack.last().cloned() else {
            return;
        };
        let callee = if matches!(
            self.language,
            Language::TypeScript | Language::JavaScript | Language::Tsx | Language::Jsx
        ) {
            self.ts_js_call_callee_name(node)
        } else if self.language == Language::ObjC && node.node_type() == "message_expression" {
            self.objc_message_callee_name(node, &caller_id)
        } else {
            node.named_child(0)
                .map(|child| get_node_text(child, &self.source))
                .filter(|name| !name.is_empty())
        };
        if let Some(callee) = callee {
            self.unresolved_references.push(unresolved_reference(
                caller_id,
                callee,
                ReferenceKind::Calls,
                node.start_position.row + 1,
                node.start_position.column,
            ));
        }
        for child in &node.named_children {
            self.visit_node(child);
        }
    }

    pub(super) fn extract_instantiation(&mut self, node: &SyntaxNode) {
        let Some(caller_id) = self.node_stack.last().cloned() else {
            return;
        };
        let Some(ctor) = get_child_by_field(node, "constructor")
            .or_else(|| get_child_by_field(node, "type"))
            .or_else(|| get_child_by_field(node, "name"))
            .or_else(|| node.named_child(0))
        else {
            return;
        };
        let target = if self.language == Language::Go && node.node_type() == "composite_literal" {
            let mut name = get_node_text(ctor, &self.source).trim().to_owned();
            if let Some(idx) = name.find('[') {
                name.truncate(idx);
                name = name.trim().to_owned();
            }
            name
        } else if self.language == Language::Scala && node.node_type() == "instance_expression" {
            scala_base_type_name(Some(ctor), &self.source).unwrap_or_default()
        } else {
            simple_type_name(&get_node_text(ctor, &self.source))
        };
        if !target.is_empty() {
            self.unresolved_references.push(unresolved_reference(
                caller_id,
                target,
                ReferenceKind::Instantiates,
                node.start_position.row + 1,
                node.start_position.column,
            ));
        }
    }

    pub(super) fn extract_static_member_ref(&mut self, node: &SyntaxNode) {
        if !STATIC_MEMBER_LANGS.contains(&language_key(&self.language).as_str()) {
            return;
        }
        let Some(owner_id) = self.node_stack.last().cloned() else {
            return;
        };
        let Some(first) = node.named_child(0) else {
            return;
        };
        let owner_name = get_node_text(first, &self.source);
        if owner_name
            .chars()
            .next()
            .map(|ch| ch.is_ascii_uppercase())
            .unwrap_or(false)
        {
            self.push_static_member_ref(&owner_name, &owner_id, node);
        }
    }

    pub(super) fn push_static_member_ref(&mut self, name: &str, owner_id: &str, node: &SyntaxNode) {
        self.unresolved_references.push(unresolved_reference(
            owner_id.to_owned(),
            name.to_owned(),
            ReferenceKind::References,
            node.start_position.row + 1,
            node.start_position.column,
        ));
    }

    #[allow(dead_code)]
    pub(super) fn find_anonymous_class_body(&self, node: &SyntaxNode) -> Option<SyntaxNode> {
        node.named_children
            .iter()
            .find(|child| matches!(child.node_type(), "class_body" | "declaration_list"))
            .cloned()
    }

    pub(super) fn extract_anonymous_class(&mut self, node: &SyntaxNode, body: &SyntaxNode) {
        let (type_name, type_node) = self
            .anonymous_class_type(node)
            .unwrap_or_else(|| ("Object".to_owned(), node.clone()));
        let name = format!("<{}$anon@{}>", type_name, node.start_position.row + 1);
        let Some(created) = self.create_node(NodeKind::Class, &name, node, NodeExtra::default())
        else {
            return;
        };
        let id = node_id(&created);
        self.nodes.push(created);
        self.unresolved_references.push(unresolved_reference(
            id.clone(),
            type_name,
            ReferenceKind::Extends,
            type_node.start_position.row + 1,
            type_node.start_position.column,
        ));
        self.node_stack.push(id);
        for child in &body.named_children {
            self.visit_node(child);
        }
        self.node_stack.pop();
    }

    pub(super) fn anonymous_class_type(&self, node: &SyntaxNode) -> Option<(String, SyntaxNode)> {
        let type_node = get_child_by_field(node, "constructor")
            .or_else(|| get_child_by_field(node, "type"))
            .or_else(|| get_child_by_field(node, "name"))
            .or_else(|| node.named_child(0))?;
        let type_name = simple_type_name(&get_node_text(type_node, &self.source));
        (!type_name.is_empty()).then(|| (type_name, type_node.clone()))
    }

    pub(super) fn extract_decorators_for(&mut self, decl_node: &SyntaxNode, decorated_id: &str) {
        // 各语言 decorator/annotation 的 AST 位置差异很大：可能是子节点、
        // previous sibling，也可能被拼进声明文本前缀，因此这里多路收集并去重。
        let mut seen = HashSet::new();
        for child in &decl_node.named_children {
            self.consider_decorator_node(child, decorated_id, &mut seen);
            if child.node_type() == "modifiers" {
                for modifier_child in &child.named_children {
                    self.consider_decorator_node(modifier_child, decorated_id, &mut seen);
                }
            }
        }

        let mut previous = decl_node.previous_named_sibling();
        while let Some(sibling) = previous {
            if !is_decorator_like_node(&sibling) && !is_at_prefixed_node(&sibling, &self.source) {
                break;
            }
            self.consider_decorator_node(&sibling, decorated_id, &mut seen);
            previous = sibling.previous_named_sibling();
        }

        for (name, line_offset) in
            decorator_names_from_decl_prefix(&get_node_text(decl_node, &self.source))
        {
            let line = decl_node.start_position.row + 1 + line_offset;
            let column = decl_node.start_position.column;
            let key = format!("{decorated_id}:{name}:{line}:{column}");
            if seen.insert(key) {
                self.unresolved_references.push(unresolved_reference(
                    decorated_id.to_owned(),
                    name,
                    ReferenceKind::Decorates,
                    line,
                    column,
                ));
            }
        }

        for (name, column) in decorator_names_before_node(decl_node, &self.source) {
            let line = decl_node.start_position.row + 1;
            let key = format!("{decorated_id}:{name}:{line}:{column}");
            if seen.insert(key) {
                self.unresolved_references.push(unresolved_reference(
                    decorated_id.to_owned(),
                    name,
                    ReferenceKind::Decorates,
                    line,
                    column,
                ));
            }
        }
    }

    pub(super) fn consider_decorator_node(
        &mut self,
        node: &SyntaxNode,
        decorated_id: &str,
        seen: &mut HashSet<String>,
    ) {
        if !is_decorator_like_node(node) && !is_at_prefixed_node(node, &self.source) {
            return;
        }
        let Some((name, line, column)) = decorator_reference_name(node, &self.source) else {
            return;
        };
        if name.is_empty() {
            return;
        }
        let key = format!("{decorated_id}:{name}:{line}:{column}");
        if !seen.insert(key) {
            return;
        }
        self.unresolved_references.push(unresolved_reference(
            decorated_id.to_owned(),
            name,
            ReferenceKind::Decorates,
            line,
            column,
        ));
    }

    pub(super) fn extract_swift_property_dependencies(&mut self, node: &SyntaxNode) {
        let Some(owner_id) = self.node_stack.last().cloned() else {
            return;
        };
        self.extract_decorators_for(node, &owner_id);
        self.extract_variable_type_annotation(node, &owner_id);
        if let Some(modifiers) = node
            .named_children
            .iter()
            .find(|child| child.node_type() == "modifiers")
            .cloned()
        {
            self.walk_swift_attribute_args(&modifiers, &owner_id);
        }
    }

    pub(super) fn walk_swift_attribute_args(&mut self, node: &SyntaxNode, owner_id: &str) {
        let pushed = self.node_stack.last().is_none_or(|id| id != owner_id);
        if pushed {
            self.node_stack.push(owner_id.to_owned());
        }
        self.extract_static_member_ref(node);
        if pushed {
            self.node_stack.pop();
        }
        for child in &node.named_children {
            self.walk_swift_attribute_args(child, owner_id);
        }
    }

    pub(super) fn objc_message_callee_name(
        &mut self,
        node: &SyntaxNode,
        caller_id: &str,
    ) -> Option<String> {
        // Objective-C selector 既要保留 `receiver.selector:` 的可解析形态，又要
        // 为大写 receiver 补一条类型引用，帮助类方法调用和实例方法区分。
        let (receiver, selector) =
            objc_message_receiver_and_selector(&get_node_text(node, &self.source))?;
        if receiver.is_empty() || selector.is_empty() {
            return None;
        }
        if matches!(receiver.as_str(), "self" | "super") {
            return Some(selector);
        }
        if receiver.starts_with('[') && is_word_identifier(&selector) {
            if let Some((inner_receiver, inner_selector)) =
                objc_message_receiver_and_selector(&receiver)
                && inner_receiver
                    .chars()
                    .next()
                    .is_some_and(|ch| ch.is_ascii_uppercase())
            {
                return Some(format!("{inner_receiver}.{inner_selector}().{selector}"));
            }
            return Some(selector);
        }
        if receiver
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_uppercase())
        {
            self.unresolved_references.push(unresolved_reference(
                caller_id.to_owned(),
                receiver.clone(),
                ReferenceKind::References,
                node.start_position.row + 1,
                node.start_position.column,
            ));
        }
        Some(format!("{receiver}.{selector}"))
    }

    pub(super) fn extract_rust_route_macro(&mut self, node: &SyntaxNode) {
        // Rust web framework route macros are preserved as a distinct hook so
        // Task 05/07 can keep Rocket/Axum-style routing traceable.
        let _ = node;
    }

    pub(super) fn visit_function_body(&mut self, body: &SyntaxNode, _function_id: &str) {
        for child in &body.named_children {
            self.visit_function_body_node(child);
        }
    }

    pub(super) fn visit_function_body_node(&mut self, node: &SyntaxNode) {
        // body 遍历只关心会影响调用图/类型图的节点；嵌套具名声明交回声明入口，
        // 匿名闭包则继续向下扫，避免重复创建不稳定节点。
        let Some(extractor) = self.extractor else {
            return;
        };
        let node_type = node.node_type();

        if extractor.call_types().contains(&node_type) {
            self.extract_call(node);
            return;
        }
        if INSTANTIATION_KINDS.contains(&node_type) {
            self.extract_instantiation(node);
            if let Some(body) = self.find_anonymous_class_body(node) {
                self.extract_anonymous_class(node, &body);
                return;
            }
        }
        if MEMBER_ACCESS_TYPES.contains(&node_type) {
            self.extract_static_member_ref(node);
        }
        if node_type == "variable_declarator"
            && let Some(owner_id) = self.node_stack.last().cloned()
        {
            self.extract_variable_type_annotation(node, &owner_id);
        }
        if extractor.function_types().contains(&node_type) {
            let nested_name = extract_name(node, &self.source, extractor);
            if nested_name != "<anonymous>" {
                self.extract_function(node, None);
                return;
            }
        }
        if extractor.class_types().contains(&node_type)
            || extractor.extra_class_node_types().contains(&node_type)
        {
            let kind = match extractor.classify_class_node(node) {
                ClassNodeKind::Struct => NodeKind::Struct,
                ClassNodeKind::Enum => NodeKind::Enum,
                ClassNodeKind::Interface => NodeKind::Interface,
                ClassNodeKind::Trait => NodeKind::Trait,
                ClassNodeKind::Class => NodeKind::Class,
            };
            self.extract_class(node, kind);
            return;
        }
        if extractor.struct_types().contains(&node_type) {
            self.extract_struct(node);
            return;
        }
        if extractor.enum_types().contains(&node_type) {
            self.extract_enum(node);
            return;
        }
        if extractor.interface_types().contains(&node_type) {
            self.extract_interface(node);
            return;
        }

        for child in &node.named_children {
            self.visit_function_body_node(child);
        }
    }
}
