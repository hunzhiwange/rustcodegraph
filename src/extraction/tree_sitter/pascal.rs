use super::*;

impl TreeSitterExtractor {
    /// Pascal grammar 的节点命名与其它语言差异较大，直接在这里接管分发。
    /// 返回 true 表示该节点已完整处理，通用 visitor 不再递归。
    pub(super) fn visit_pascal_node(&mut self, node: &SyntaxNode) -> bool {
        match node.node_type() {
            "unit" | "program" | "library" => {
                let module_name = node
                    .named_children
                    .iter()
                    .find(|child| child.node_type() == "moduleName")
                    .map(|child| get_node_text(child, &self.source))
                    .filter(|name| !name.trim().is_empty())
                    .unwrap_or_else(|| {
                        Path::new(&self.file_path)
                            .file_stem()
                            .and_then(|name| name.to_str())
                            .unwrap_or(&self.file_path)
                            .to_owned()
                    });
                if let Some(created) =
                    self.create_node(NodeKind::Module, &module_name, node, NodeExtra::default())
                {
                    self.nodes.push(created);
                }
                for child in &node.named_children {
                    self.visit_node(child);
                }
                true
            }
            "declType" => {
                self.extract_pascal_decl_type(node);
                true
            }
            "declUses" => {
                self.extract_pascal_uses(node);
                true
            }
            "declConsts" => {
                for child in &node.named_children {
                    if child.node_type() == "declConst" {
                        self.extract_pascal_const(child);
                    }
                }
                true
            }
            "declConst" => {
                self.extract_pascal_const(node);
                true
            }
            "declSection" => {
                let visibility = pascal_decl_section_visibility(node);
                if let Some(visibility) = visibility {
                    self.pascal_visibility_stack.push(visibility);
                }
                for child in &node.named_children {
                    self.visit_node(child);
                }
                if visibility.is_some() {
                    self.pascal_visibility_stack.pop();
                }
                true
            }
            "declTypes" | "interface" | "implementation" => {
                for child in &node.named_children {
                    self.visit_node(child);
                }
                true
            }
            "declVars" => {
                for child in &node.named_children {
                    if child.node_type() != "declVar" {
                        continue;
                    }
                    let Some(name_node) = get_child_by_field(child, "name") else {
                        continue;
                    };
                    let name = get_node_text(name_node, &self.source);
                    if let Some(created) =
                        self.create_node(NodeKind::Variable, &name, child, NodeExtra::default())
                    {
                        self.nodes.push(created);
                    }
                }
                true
            }
            "defProc" => {
                self.extract_pascal_def_proc(node);
                true
            }
            "declProp" => {
                if let Some(name_node) = get_child_by_field(node, "name") {
                    let name = get_node_text(name_node, &self.source);
                    let visibility = self
                        .extractor
                        .as_ref()
                        .and_then(|extractor| extractor.get_visibility(node));
                    if let Some(created) = self.create_node(
                        NodeKind::Property,
                        &name,
                        node,
                        NodeExtra {
                            visibility,
                            ..NodeExtra::default()
                        },
                    ) {
                        self.nodes.push(created);
                    }
                }
                true
            }
            "declField" => {
                if let Some(name_node) = get_child_by_field(node, "name") {
                    let name = get_node_text(name_node, &self.source);
                    let visibility = self
                        .extractor
                        .as_ref()
                        .and_then(|extractor| extractor.get_visibility(node));
                    if let Some(created) = self.create_node(
                        NodeKind::Field,
                        &name,
                        node,
                        NodeExtra {
                            visibility,
                            ..NodeExtra::default()
                        },
                    ) {
                        self.nodes.push(created);
                    }
                }
                true
            }
            "exprCall" => {
                self.extract_pascal_call(node);
                true
            }
            "block" => {
                self.visit_pascal_block(node);
                true
            }
            _ => false,
        }
    }

    pub(super) fn extract_pascal_decl_type(&mut self, node: &SyntaxNode) {
        // Pascal 的 type 区同时承载 class/interface/enum/type alias；先看具体子节点
        // 再决定 NodeKind，避免把类声明降级成普通 type_alias。
        let Some(name_node) = get_child_by_field(node, "name") else {
            return;
        };
        let name = get_node_text(name_node, &self.source);
        let decl_class = node
            .named_children
            .iter()
            .find(|child| child.node_type() == "declClass");
        let decl_intf = node
            .named_children
            .iter()
            .find(|child| child.node_type() == "declIntf");
        let type_child = node
            .named_children
            .iter()
            .find(|child| child.node_type() == "type");

        if let Some(decl_class) = decl_class {
            let Some(created) =
                self.create_node(NodeKind::Class, &name, node, NodeExtra::default())
            else {
                return;
            };
            let id = node_id(&created);
            self.nodes.push(created);
            self.extract_pascal_inheritance(decl_class, &id);
            self.node_stack.push(id);
            for child in &decl_class.named_children {
                self.visit_node(child);
            }
            self.node_stack.pop();
            return;
        }

        if let Some(decl_intf) = decl_intf {
            let Some(created) =
                self.create_node(NodeKind::Interface, &name, node, NodeExtra::default())
            else {
                return;
            };
            let id = node_id(&created);
            self.nodes.push(created);
            self.node_stack.push(id);
            for child in &decl_intf.named_children {
                self.visit_node(child);
            }
            self.node_stack.pop();
            return;
        }

        if let Some(type_child) = type_child {
            if let Some(decl_enum) = type_child
                .named_children
                .iter()
                .find(|child| child.node_type() == "declEnum")
            {
                let Some(created) =
                    self.create_node(NodeKind::Enum, &name, node, NodeExtra::default())
                else {
                    return;
                };
                let id = node_id(&created);
                self.nodes.push(created);
                self.node_stack.push(id);
                for child in &decl_enum.named_children {
                    if child.node_type() != "declEnumValue" {
                        continue;
                    }
                    let Some(member_name) = get_child_by_field(child, "name") else {
                        continue;
                    };
                    let name = get_node_text(member_name, &self.source);
                    if let Some(created) =
                        self.create_node(NodeKind::EnumMember, &name, child, NodeExtra::default())
                    {
                        self.nodes.push(created);
                    }
                }
                self.node_stack.pop();
            } else if let Some(created) =
                self.create_node(NodeKind::TypeAlias, &name, node, NodeExtra::default())
            {
                self.nodes.push(created);
            }
            return;
        }

        if let Some(created) =
            self.create_node(NodeKind::TypeAlias, &name, node, NodeExtra::default())
        {
            self.nodes.push(created);
        }
    }

    pub(super) fn extract_pascal_uses(&mut self, node: &SyntaxNode) {
        let signature = get_node_text(node, &self.source).trim().to_owned();
        for child in &node.named_children {
            if child.node_type() != "moduleName" {
                continue;
            }
            let unit_name = get_node_text(child, &self.source);
            if let Some(created) = self.create_node(
                NodeKind::Import,
                &unit_name,
                child,
                NodeExtra {
                    signature: Some(signature.clone()),
                    ..NodeExtra::default()
                },
            ) {
                self.nodes.push(created);
            }
            if let Some(parent_id) = self.node_stack.last().cloned() {
                self.unresolved_references.push(unresolved_reference(
                    parent_id,
                    unit_name,
                    ReferenceKind::Imports,
                    child.start_position.row + 1,
                    child.start_position.column,
                ));
            }
        }
    }

    pub(super) fn extract_pascal_const(&mut self, node: &SyntaxNode) {
        let Some(name_node) = get_child_by_field(node, "name") else {
            return;
        };
        let name = get_node_text(name_node, &self.source);
        let signature = node
            .named_children
            .iter()
            .find(|child| child.node_type() == "defaultValue")
            .map(|child| get_node_text(child, &self.source));
        if let Some(created) = self.create_node(
            NodeKind::Constant,
            &name,
            node,
            NodeExtra {
                signature,
                ..NodeExtra::default()
            },
        ) {
            self.nodes.push(created);
        }
    }

    pub(super) fn extract_pascal_inheritance(&mut self, decl_class: &SyntaxNode, class_id: &str) {
        // Delphi 约定第一个 typeref 是父类，后续 typeref 是接口实现。
        let typerefs = decl_class
            .named_children
            .iter()
            .filter(|child| child.node_type() == "typeref")
            .collect::<Vec<_>>();
        for (idx, typeref) in typerefs.iter().enumerate() {
            let name = get_node_text(typeref, &self.source);
            self.unresolved_references.push(unresolved_reference(
                class_id.to_owned(),
                name,
                if idx == 0 {
                    ReferenceKind::Extends
                } else {
                    ReferenceKind::Implements
                },
                typeref.start_position.row + 1,
                typeref.start_position.column,
            ));
        }
    }
}
