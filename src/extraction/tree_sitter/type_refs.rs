use super::*;

#[allow(dead_code)]
pub(super) static TYPE_ANNOTATION_LANGUAGES: &[&str] = &[
    "typescript",
    "tsx",
    "python",
    "go",
    "rust",
    "java",
    "csharp",
    "php",
    "kotlin",
    "swift",
    "dart",
    "scala",
    "pascal",
];

impl TreeSitterExtractor {
    /// 抽取继承/实现关系。多数语言能用通用 extends/implements 子树处理，
    /// 但 Swift、C#、PHP、Dart、Kotlin 等 grammar 命名差异较大，需要在这里收敛。
    pub(super) fn extract_inheritance(&mut self, node: &SyntaxNode, class_id: &str) {
        if self.language == Language::ObjC {
            self.extract_objc_inheritance(node, class_id);
        }
        for child in &node.named_children {
            let child_type = child.node_type();

            if child_type == "trait_bounds" {
                for bound in &child.named_children {
                    if let Some((name, pos)) = self.rust_trait_bound_name(bound) {
                        self.push_unresolved_reference(
                            class_id,
                            name,
                            ReferenceKind::Extends,
                            &pos,
                        );
                    }
                }
                continue;
            }

            if self.language == Language::Swift && child_type == "inheritance_specifier" {
                for target in collect_descendants_of_type(child, "user_type") {
                    if let Some(type_id) = target
                        .named_children
                        .iter()
                        .find(|node| node.node_type() == "type_identifier")
                    {
                        self.push_unresolved_reference(
                            class_id,
                            get_node_text(type_id, &self.source),
                            ReferenceKind::Extends,
                            type_id,
                        );
                    }
                }
                continue;
            }

            if self.language == Language::CSharp && child_type == "base_list" {
                for base_type in &child.named_children {
                    if let Some((name, pos)) = self.csharp_base_type_name(base_type) {
                        self.push_unresolved_reference(
                            class_id,
                            name,
                            ReferenceKind::Extends,
                            &pos,
                        );
                    }
                }
                continue;
            }

            if self.language == Language::Php && child_type == "base_clause" {
                for (name, pos) in self.php_named_types(child) {
                    self.push_unresolved_reference(class_id, name, ReferenceKind::Extends, &pos);
                }
                continue;
            }

            if self.language == Language::Php && child_type == "class_interface_clause" {
                for (name, pos) in self.php_named_types(child) {
                    self.push_unresolved_reference(class_id, name, ReferenceKind::Implements, &pos);
                }
                continue;
            }

            if self.language == Language::Dart && child_type == "superclass" {
                for target in &child.named_children {
                    match target.node_type() {
                        "type_identifier" => self.push_unresolved_reference(
                            class_id,
                            get_node_text(target, &self.source),
                            ReferenceKind::Extends,
                            target,
                        ),
                        "mixins" => {
                            for mixin in &target.named_children {
                                if mixin.node_type() == "type_identifier" {
                                    self.push_unresolved_reference(
                                        class_id,
                                        get_node_text(mixin, &self.source),
                                        ReferenceKind::Implements,
                                        mixin,
                                    );
                                }
                            }
                        }
                        _ => {}
                    }
                }
                continue;
            }

            if self.language == Language::Dart && child_type == "interfaces" {
                for iface in collect_descendants_of_type(child, "type_identifier") {
                    self.push_unresolved_reference(
                        class_id,
                        get_node_text(&iface, &self.source),
                        ReferenceKind::Implements,
                        &iface,
                    );
                }
                continue;
            }

            if self.language == Language::Kotlin && child_type == "delegation_specifier" {
                if let Some((name, pos)) = self.kotlin_delegation_type_name(child) {
                    self.push_unresolved_reference(class_id, name, ReferenceKind::Extends, &pos);
                }
                continue;
            }

            if matches!(
                child_type,
                "extends_clause"
                    | "superclass"
                    | "extends_interfaces"
                    | "implements_clause"
                    | "super_interfaces"
            ) {
                let kind = if matches!(child_type, "implements_clause" | "super_interfaces") {
                    ReferenceKind::Implements
                } else {
                    ReferenceKind::Extends
                };
                for target in self.direct_type_targets(child) {
                    self.push_unresolved_reference(
                        class_id,
                        simple_type_name(&get_node_text(&target, &self.source)),
                        kind,
                        &target,
                    );
                }
                continue;
            }

            if matches!(child_type, "field_declaration_list" | "class_heritage") {
                self.extract_inheritance(child, class_id);
            }
        }
    }

    pub(super) fn extract_objc_inheritance(&mut self, node: &SyntaxNode, class_id: &str) {
        // Objective-C 的 superclass/protocol 常藏在声明头文本里，tree-sitter
        // 不一定拆成稳定字段，因此从首行文本兜底解析。
        let text = get_node_text(node, &self.source);
        let header = text.lines().next().unwrap_or_default();
        if node.node_type() == "class_interface" {
            if let Some(after_colon) = header.split_once(':').map(|(_, rest)| rest) {
                let superclass = after_colon
                    .split('<')
                    .next()
                    .unwrap_or(after_colon)
                    .split(|ch: char| !(ch == '_' || ch.is_ascii_alphanumeric()))
                    .find(|part| !part.is_empty())
                    .unwrap_or_default();
                if !superclass.is_empty() {
                    self.push_unresolved_reference(
                        class_id,
                        superclass.to_owned(),
                        ReferenceKind::Extends,
                        node,
                    );
                }
            }
            for protocol in objc_protocol_names_from_header(header) {
                self.push_unresolved_reference(class_id, protocol, ReferenceKind::Implements, node);
            }
        } else if node.node_type() == "protocol_declaration" {
            for protocol in objc_protocol_names_from_header(header) {
                self.push_unresolved_reference(class_id, protocol, ReferenceKind::Extends, node);
            }
        }
    }

    pub(super) fn extract_rust_impl_item(&mut self, node: &SyntaxNode) {
        // `impl Trait for Type` 不会自然出现在 Type 的子树里；索引已建节点后，
        // 这里把实现关系回挂到 Type，保证 type hierarchy 可查询。
        let has_for = node
            .children
            .iter()
            .any(|child| child.node_type() == "for" && !child.is_named);
        if !has_for {
            return;
        }

        let type_nodes = node
            .named_children
            .iter()
            .filter(|child| {
                matches!(
                    child.node_type(),
                    "type_identifier" | "generic_type" | "scoped_type_identifier"
                )
            })
            .collect::<Vec<_>>();
        if type_nodes.len() < 2 {
            return;
        }

        let trait_node = type_nodes[0];
        let Some(type_node) = type_nodes.last().copied() else {
            return;
        };
        let trait_name = simple_type_name(&get_node_text(trait_node, &self.source));
        let type_name = self.rust_impl_type_name(type_node);
        if trait_name.is_empty() || type_name.is_empty() {
            return;
        }

        if let Some(type_node_id) = self.find_node_by_name(&type_name) {
            self.push_unresolved_reference(
                &type_node_id,
                trait_name,
                ReferenceKind::Implements,
                trait_node,
            );
        }
    }

    pub(super) fn push_unresolved_reference(
        &mut self,
        from_node_id: &str,
        reference_name: String,
        reference_kind: ReferenceKind,
        node: &SyntaxNode,
    ) {
        if reference_name.is_empty() {
            return;
        }
        self.unresolved_references.push(unresolved_reference(
            from_node_id.to_owned(),
            reference_name,
            reference_kind,
            node.start_position.row + 1,
            node.start_position.column,
        ));
    }

    pub(super) fn rust_trait_bound_name(&self, node: &SyntaxNode) -> Option<(String, SyntaxNode)> {
        match node.node_type() {
            "type_identifier" => Some((get_node_text(node, &self.source), node.clone())),
            "generic_type" => node
                .named_children
                .iter()
                .find(|child| child.node_type() == "type_identifier")
                .map(|child| (get_node_text(child, &self.source), child.clone())),
            "higher_ranked_trait_bound" => collect_descendants_of_type(node, "type_identifier")
                .into_iter()
                .next()
                .map(|child| (get_node_text(&child, &self.source), child)),
            _ => None,
        }
    }

    pub(super) fn rust_impl_type_name(&self, node: &SyntaxNode) -> String {
        if node.node_type() == "generic_type"
            && let Some(inner) = node
                .named_children
                .iter()
                .find(|child| child.node_type() == "type_identifier")
        {
            return get_node_text(inner, &self.source);
        }
        simple_type_name(&get_node_text(node, &self.source))
    }

    pub(super) fn csharp_base_type_name(&self, node: &SyntaxNode) -> Option<(String, SyntaxNode)> {
        match node.node_type() {
            "identifier" => Some((get_node_text(node, &self.source), node.clone())),
            "qualified_name" => Some((
                simple_type_name(&get_node_text(node, &self.source)),
                node.clone(),
            )),
            "generic_name" => node
                .named_children
                .iter()
                .find(|child| child.node_type() == "identifier")
                .map(|child| (get_node_text(child, &self.source), child.clone())),
            _ => collect_descendants_matching(
                node,
                &["identifier", "qualified_name", "generic_name"],
            )
            .into_iter()
            .next()
            .and_then(|child| self.csharp_base_type_name(&child)),
        }
    }

    pub(super) fn php_named_types(&self, node: &SyntaxNode) -> Vec<(String, SyntaxNode)> {
        collect_descendants_matching(node, &["name", "qualified_name"])
            .into_iter()
            .map(|child| {
                (
                    simple_php_type_name(&get_node_text(&child, &self.source)),
                    child,
                )
            })
            .filter(|(name, _)| !name.is_empty())
            .collect()
    }

    pub(super) fn kotlin_delegation_type_name(
        &self,
        node: &SyntaxNode,
    ) -> Option<(String, SyntaxNode)> {
        collect_descendants_of_type(node, "type_identifier")
            .into_iter()
            .next()
            .map(|child| (get_node_text(&child, &self.source), child))
    }

    pub(super) fn direct_type_targets(&self, node: &SyntaxNode) -> Vec<SyntaxNode> {
        if let Some(type_list) = node
            .named_children
            .iter()
            .find(|child| child.node_type() == "type_list")
        {
            return type_list.named_children.to_vec();
        }
        node.named_children
            .iter()
            .filter(|child| {
                matches!(
                    child.node_type(),
                    "type_identifier"
                        | "identifier"
                        | "scoped_type_identifier"
                        | "qualified_name"
                        | "generic_name"
                )
            })
            .cloned()
            .collect()
    }

    #[allow(dead_code)]
    pub(super) fn find_node_by_name(&self, name: &str) -> Option<String> {
        self.nodes
            .iter()
            .find(|node| node_name(node) == name)
            .map(node_id)
    }

    pub(super) fn extract_type_annotations(&mut self, node: &SyntaxNode, node_id: &str) {
        // TS/JS 类型语法里很多 identifier 只是值名，单独走更保守的引用抽取；
        // 其它静态语言则按 type position 统一发 TypeOf 引用。
        if !TYPE_ANNOTATION_LANGUAGES.contains(&language_key(&self.language).as_str()) {
            return;
        }
        if matches!(
            self.language,
            Language::TypeScript | Language::Tsx | Language::JavaScript | Language::Jsx
        ) {
            self.extract_ts_js_type_annotations(node, node_id);
            return;
        }
        self.extract_type_refs_from_subtree(node, node_id);
    }

    #[allow(dead_code)]
    pub(super) fn extract_csharp_type_refs(&mut self, node: &SyntaxNode, node_id: &str) {
        let _ = (node, node_id);
    }

    #[allow(dead_code)]
    pub(super) fn extract_csharp_primary_ctor_param_refs(
        &mut self,
        node: &SyntaxNode,
        owner_id: &str,
    ) {
        let _ = (node, owner_id);
    }

    #[allow(dead_code)]
    pub(super) fn walk_csharp_type_position(&mut self, node: &SyntaxNode, from_node_id: &str) {
        let _ = (node, from_node_id);
    }

    #[allow(dead_code)]
    pub(super) fn extract_php_type_refs(&mut self, node: &SyntaxNode, node_id: &str) {
        let _ = (PHP_TYPE_NODES, node, node_id);
    }

    #[allow(dead_code)]
    pub(super) fn walk_php_type_position(&mut self, node: &SyntaxNode, from_node_id: &str) {
        let _ = (node, from_node_id);
    }

    pub(super) fn extract_variable_type_annotation(&mut self, node: &SyntaxNode, node_id: &str) {
        if !TYPE_ANNOTATION_LANGUAGES.contains(&language_key(&self.language).as_str()) {
            return;
        }
        let type_annotation = get_child_by_field(node, "type")
            .or_else(|| get_child_by_field(node, "type_annotation"))
            .or_else(|| {
                node.named_children
                    .iter()
                    .find(|child| child.node_type() == "type_annotation")
            });
        if let Some(type_annotation) = type_annotation {
            if matches!(
                self.language,
                Language::TypeScript | Language::Tsx | Language::JavaScript | Language::Jsx
            ) {
                self.extract_reference_refs_from_type_subtree(type_annotation, node_id);
            } else {
                self.extract_type_refs_from_subtree(type_annotation, node_id);
            }
        }
    }

    pub(super) fn extract_ts_js_type_annotations(&mut self, node: &SyntaxNode, node_id: &str) {
        // TS interface member 的 type 子节点有时缺失，最后的整节点兜底是为了
        // 保住 `foo: Bar` 这类契约边。
        let mut extracted = false;
        if let Some(params) = get_child_by_field(node, "parameters") {
            self.extract_reference_refs_from_type_subtree(params, node_id);
            extracted = true;
        }
        if let Some(return_type) = get_child_by_field(node, "return_type") {
            self.extract_reference_refs_from_type_subtree(return_type, node_id);
            extracted = true;
        }
        if let Some(type_node) =
            get_child_by_field(node, "type").or_else(|| get_child_by_field(node, "type_annotation"))
        {
            self.extract_reference_refs_from_type_subtree(type_node, node_id);
            extracted = true;
        }
        for child in &node.named_children {
            if child.node_type() == "type_annotation" {
                self.extract_reference_refs_from_type_subtree(child, node_id);
                extracted = true;
            }
        }
        if !extracted && self.is_ts_interface_member_signature(node) {
            self.extract_reference_refs_from_type_subtree(node, node_id);
        }
    }

    pub(super) fn extract_type_refs_from_subtree(&mut self, node: &SyntaxNode, from_node_id: &str) {
        for child in &node.named_children {
            if matches!(
                child.node_type(),
                "type_identifier" | "identifier" | "simple_identifier" | "named_type"
            ) {
                self.unresolved_references.push(unresolved_reference(
                    from_node_id.to_owned(),
                    get_node_text(child, &self.source),
                    ReferenceKind::TypeOf,
                    child.start_position.row + 1,
                    child.start_position.column,
                ));
            }
            self.extract_type_refs_from_subtree(child, from_node_id);
        }
    }
}
