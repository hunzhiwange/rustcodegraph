use super::*;

impl TreeSitterExtractor {
    /// 通用 AST 分发入口。顺序很重要：先给语言特例和适配器自定义 hook 机会，
    /// 再落到跨语言的声明/调用/变量规则，避免同一个语法节点被重复抽取。
    pub(super) fn visit_node(&mut self, node: &SyntaxNode) {
        let node_type = node.node_type();
        self.maybe_capture_fn_refs(node, node_type);

        if self.language == Language::Pascal && self.visit_pascal_node(node) {
            return;
        }

        if let Some(extractor) = self.extractor {
            let mut ctx = CoreExtractorContext { inner: self };
            if extractor.visit_node(node, &mut ctx) {
                return;
            }
        }

        if let Some(extractor) = self.extractor {
            if extractor.method_types().contains(&node_type) {
                let receiver = extractor.get_receiver_type(node, &self.source);
                if self.is_inside_class_like_node()
                    || extractor.methods_are_top_level()
                    || receiver.is_some()
                {
                    self.extract_method(node);
                    return;
                }
            }
            if extractor.function_types().contains(&node_type) {
                if extractor.get_receiver_type(node, &self.source).is_some() {
                    self.extract_method(node);
                } else {
                    self.extract_function(node, None);
                }
                return;
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
            if extractor.method_types().contains(&node_type) {
                match extractor.classify_method_node(node) {
                    MethodNodeKind::Property => {
                        self.extract_property(node);
                    }
                    MethodNodeKind::Method => self.extract_function(node, None),
                }
                return;
            }
            if extractor.interface_types().contains(&node_type) {
                self.extract_interface(node);
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
            if extractor.type_alias_types().contains(&node_type) && self.extract_type_alias(node) {
                return;
            }
            if extractor.import_types().contains(&node_type) {
                self.extract_import(node);
                return;
            }
            if matches!(
                self.language,
                Language::TypeScript | Language::Tsx | Language::JavaScript | Language::Jsx
            ) && node_type == "export_statement"
                && get_child_by_field(node, "source").is_some()
                && let Some(parent_id) = self.node_stack.last().cloned()
            {
                self.emit_re_export_refs(node, &parent_id);
            }
            if extractor.call_types().contains(&node_type) {
                self.extract_call(node);
                return;
            }
            if self.language == Language::Swift
                && node_type == "property_declaration"
                && self.is_inside_class_like_node()
            {
                self.extract_swift_property_dependencies(node);
            }
            if extractor.variable_types().contains(&node_type) {
                self.extract_variable(node);
                return;
            }
            if extractor.field_types().contains(&node_type) {
                self.extract_field(node);
                return;
            }
            if extractor.property_types().contains(&node_type) {
                self.extract_property(node);
                return;
            }
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
        if node_type == "attribute_item" {
            self.extract_rust_route_macro(node);
        }
        if node_type == "impl_item" {
            self.extract_rust_impl_item(node);
        }
        if self.is_ts_interface_member_signature(node)
            && let Some(parent_id) = self.node_stack.last().cloned()
        {
            self.extract_type_annotations(node, &parent_id);
        }

        for child in &node.named_children {
            self.visit_node(child);
        }
    }

    pub(super) fn create_node(
        &mut self,
        kind: NodeKind,
        name: &str,
        node: &SyntaxNode,
        extra: NodeExtra,
    ) -> Option<Node> {
        // 匿名函数通常只作为调用容器存在；不创建节点，但调用方会继续扫描 body，
        // 否则会制造大量无法稳定命名的符号并降低 resolver 精度。
        if kind == NodeKind::Function && name == "<anonymous>" {
            return None;
        }

        let extra_signature = extra.signature;
        let extra_visibility = extra.visibility;
        let extra_is_exported = extra.is_exported;
        let extra_docstring = extra.docstring;
        let id = generate_node_id(&self.file_path, kind, name, node.start_position.row + 1);
        let qualified_name = self.build_qualified_name(name);
        let docstring = extra_docstring.or_else(|| get_preceding_docstring(node, &self.source));
        let _generated = is_generated_file(&self.file_path);
        let signature = extra_signature.or_else(|| {
            self.extractor
                .as_ref()
                .and_then(|extractor| extractor.get_signature(node, &self.source))
        });
        let visibility = extra_visibility
            .or_else(|| {
                (self.language == Language::Pascal)
                    .then(|| self.pascal_visibility_stack.last().copied())
                    .flatten()
            })
            .or_else(|| {
                self.extractor
                    .as_ref()
                    .and_then(|extractor| extractor.get_visibility(node))
            });
        let is_exported = extra_is_exported.unwrap_or_else(|| {
            self.extractor
                .as_ref()
                .is_some_and(|extractor| extractor.is_exported(node, &self.source))
        });
        let is_async = self
            .extractor
            .as_ref()
            .is_some_and(|extractor| extractor.is_async(node));
        let is_static = self
            .extractor
            .as_ref()
            .is_some_and(|extractor| extractor.is_static(node));
        let decorators = self
            .extractor
            .as_ref()
            .and_then(|extractor| extractor.extract_modifiers(node));
        let return_type = self
            .extractor
            .as_ref()
            .and_then(|extractor| extractor.get_return_type(node, &self.source));

        let mut created = node_from_parts(
            id.clone(),
            kind,
            name.to_owned(),
            qualified_name,
            self.file_path.clone(),
            self.language,
            node,
        )?;
        created.docstring = docstring;
        created.signature = signature;
        created.visibility = visibility;
        created.is_exported = Some(is_exported);
        created.is_async = Some(is_async);
        created.is_static = Some(is_static);
        created.decorators = decorators;
        created.return_type = return_type;

        // 包含关系来自当前 scope 栈；所有声明入口都必须在进入子 body 前 push。
        if let Some(parent_id) = self.node_stack.last().cloned() {
            self.edges.push(edge(
                parent_id,
                id.clone(),
                EdgeKind::Contains,
                Some(self.file_path.clone()),
            ));
        }
        // value-ref 候选在这里登记，真正连边要等全文件唯一性检查完成。
        self.capture_value_ref_scope(kind, name, &id, node);
        Some(created)
    }

    #[allow(dead_code)]
    pub(super) fn find_child_by_types<'a>(
        &self,
        node: &'a SyntaxNode,
        types: &[&str],
    ) -> Option<&'a SyntaxNode> {
        node.named_children
            .iter()
            .find(|child| types.contains(&child.node_type()))
    }

    pub(super) fn extract_file_package(&mut self, root_node: &SyntaxNode) -> Option<String> {
        // package/namespace 节点作为 file 下的额外 scope，可以让 qualified_name 与
        // Java/Kotlin/Scala 等语言的包层级保持一致。
        let extractor = self.extractor?;
        for package_type in extractor.package_types() {
            if let Some(package_node) = root_node
                .named_children
                .iter()
                .find(|child| child.node_type() == *package_type)
                && let Some(package_name) = extractor.extract_package(package_node, &self.source)
            {
                let id = generate_node_id(
                    &self.file_path,
                    NodeKind::Namespace,
                    &package_name,
                    package_node.start_position.row + 1,
                );
                if let Some(namespace_node) = node_from_parts(
                    id.clone(),
                    NodeKind::Namespace,
                    package_name,
                    self.build_qualified_name(""),
                    self.file_path.clone(),
                    self.language,
                    package_node,
                ) {
                    self.nodes.push(namespace_node);
                    return Some(id);
                }
            }
        }
        None
    }

    pub(super) fn build_qualified_name(&self, name: &str) -> String {
        // qualified_name 只从已入栈的语义节点拼出，不直接读 AST 父链；这样自定义
        // 抽取器只要维护 node_stack，就能得到一致命名。
        let mut parts = self
            .node_stack
            .iter()
            .filter_map(|id| {
                self.nodes
                    .iter()
                    .find(|node| node_id(node) == *id)
                    .map(node_name)
            })
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>();
        parts.push(name.to_owned());
        parts.join("::")
    }

    #[allow(dead_code)]
    pub(super) fn make_extractor_context(&mut self) -> CoreExtractorContext<'_> {
        CoreExtractorContext { inner: self }
    }

    #[allow(dead_code)]
    pub(super) fn is_inside_class_like_node(&self) -> bool {
        self.node_stack.iter().any(|id| {
            self.nodes.iter().any(|node| {
                node_id(node) == *id
                    && matches!(
                        node_kind(node),
                        NodeKind::Class
                            | NodeKind::Struct
                            | NodeKind::Interface
                            | NodeKind::Trait
                            | NodeKind::Module
                    )
            })
        })
    }

    pub(super) fn is_ts_interface_member_signature(&self, node: &SyntaxNode) -> bool {
        matches!(self.language, Language::TypeScript | Language::Tsx)
            && matches!(node.node_type(), "property_signature" | "method_signature")
            && self.is_inside_class_like_node()
    }

    #[allow(dead_code)]
    pub(super) fn is_class_scope_constant_assignment(&self, node: &SyntaxNode) -> bool {
        self.is_inside_class_like_node()
            && matches!(
                node.node_type(),
                "assignment_expression" | "lexical_declaration"
            )
    }

    pub(super) fn create_file_node(&self) -> Option<Node> {
        node_from_parts(
            format!("file:{}", self.file_path),
            NodeKind::File,
            Path::new(&self.file_path)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(&self.file_path)
                .to_owned(),
            self.file_path.clone(),
            self.file_path.clone(),
            self.language,
            &SyntaxNode {
                start_position: crate::web_tree_sitter::Point { row: 0, column: 0 },
                end_position: crate::web_tree_sitter::Point {
                    row: self.source.lines().count().saturating_sub(1),
                    column: 0,
                },
                ..SyntaxNode::default()
            },
        )
    }
}
