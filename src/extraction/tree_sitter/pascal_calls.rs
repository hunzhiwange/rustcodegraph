use super::*;

impl TreeSitterExtractor {
    /// 实现区的 `TClass.Method` 需要回挂到 interface/type 区已经创建的 Method 节点；
    /// 找不到声明时才创建顶层 Function，避免同一方法出现两份节点。
    pub(super) fn extract_pascal_def_proc(&mut self, node: &SyntaxNode) {
        let Some(decl_proc) = node
            .named_children
            .iter()
            .find(|child| child.node_type() == "declProc")
        else {
            return;
        };
        let Some(name_node) = get_child_by_field(decl_proc, "name") else {
            return;
        };
        let full_name = get_node_text(name_node, &self.source).trim().to_owned();
        if full_name.is_empty() {
            return;
        }
        let short_name = full_name
            .rsplit('.')
            .next()
            .unwrap_or(&full_name)
            .to_owned();
        let full_key = full_name.to_ascii_lowercase();
        let short_key = short_name.to_ascii_lowercase();

        self.ensure_pascal_method_index();
        let mut parent_id = self.method_index.as_ref().and_then(|index| {
            index
                .get(&full_key)
                .or_else(|| index.get(&short_key))
                .cloned()
        });

        if parent_id.is_none() && !full_name.contains('.') {
            let signature = self
                .extractor
                .as_ref()
                .and_then(|extractor| extractor.get_signature(decl_proc, &self.source));
            let visibility = self
                .extractor
                .as_ref()
                .and_then(|extractor| extractor.get_visibility(decl_proc));
            if let Some(created) = self.create_node(
                NodeKind::Function,
                &full_name,
                decl_proc,
                NodeExtra {
                    signature,
                    visibility,
                    ..NodeExtra::default()
                },
            ) {
                let id = node_id(&created);
                self.nodes.push(created);
                if let Some(index) = self.method_index.as_mut() {
                    index.insert(full_key.clone(), id.clone());
                    index.entry(short_key.clone()).or_insert_with(|| id.clone());
                }
                parent_id = Some(id);
            }
        }

        let parent_id = parent_id.or_else(|| self.node_stack.last().cloned());
        let Some(parent_id) = parent_id else {
            return;
        };
        let Some(block) = node
            .named_children
            .iter()
            .find(|child| child.node_type() == "block")
        else {
            return;
        };
        self.node_stack.push(parent_id);
        self.visit_pascal_block(block);
        self.node_stack.pop();
    }

    pub(super) fn extract_pascal_call(&mut self, node: &SyntaxNode) {
        // Pascal 支持链式调用和无括号属性/方法访问；这里尽量保留
        // `Type().method` 形态，方便后续按类型前缀解析。
        let Some(caller_id) = self.node_stack.last().cloned() else {
            return;
        };
        let Some(first_child) = node.named_child(0) else {
            return;
        };
        let mut callee = String::new();
        if first_child.node_type() == "exprDot" {
            let inner_call = first_child
                .named_children
                .iter()
                .find(|child| child.node_type() == "exprCall");
            let method = first_child
                .named_children
                .iter()
                .rfind(|child| child.node_type() == "identifier")
                .map(|child| get_node_text(child, &self.source))
                .unwrap_or_default();
            if let Some(inner_call) = inner_call.filter(|_| is_word_identifier(&method)) {
                let inner_callee = inner_call
                    .named_child(0)
                    .map(|inner_first| {
                        if inner_first.node_type() == "exprDot" {
                            pascal_identifier_texts(inner_first, &self.source).join(".")
                        } else if inner_first.node_type() == "identifier" {
                            get_node_text(inner_first, &self.source)
                        } else {
                            String::new()
                        }
                    })
                    .unwrap_or_default();
                callee = if is_pascal_type_prefix(&inner_callee) {
                    format!("{inner_callee}().{method}")
                } else {
                    method
                };
            } else {
                callee = pascal_identifier_texts(first_child, &self.source).join(".");
            }
        } else if first_child.node_type() == "identifier" {
            callee = get_node_text(first_child, &self.source);
        }

        if !callee.is_empty() {
            self.unresolved_references.push(unresolved_reference(
                caller_id,
                callee,
                ReferenceKind::Calls,
                node.start_position.row + 1,
                node.start_position.column,
            ));
        }
        if let Some(args) = node
            .named_children
            .iter()
            .find(|child| child.node_type() == "exprArgs")
        {
            self.visit_pascal_block(args);
        }
    }

    pub(super) fn extract_pascal_parenless_call(&mut self, node: &SyntaxNode) {
        let Some(caller_id) = self.node_stack.last().cloned() else {
            return;
        };
        let receiver = node.named_child(0);
        let method = node
            .named_children
            .iter()
            .rfind(|child| child.node_type() == "identifier")
            .map(|child| get_node_text(child, &self.source))
            .unwrap_or_default();
        if method.is_empty() {
            return;
        }

        let callee = if receiver.is_some_and(|receiver| {
            matches!(receiver.node_type(), "exprDot" | "exprCall") && is_word_identifier(&method)
        }) {
            let receiver = receiver.expect("receiver was checked above");
            let inner_callee_node = if receiver.node_type() == "exprCall" {
                receiver.named_child(0)
            } else {
                Some(receiver)
            };
            let inner_callee = inner_callee_node
                .map(|inner| {
                    if inner.node_type() == "identifier" {
                        get_node_text(inner, &self.source)
                    } else {
                        pascal_identifier_texts(inner, &self.source).join(".")
                    }
                })
                .unwrap_or_default();
            if is_pascal_type_prefix(&inner_callee) {
                if receiver.node_type() == "exprCall" {
                    self.extract_pascal_call(receiver);
                } else {
                    self.extract_pascal_parenless_call(receiver);
                }
                format!("{inner_callee}().{method}")
            } else {
                method
            }
        } else {
            pascal_identifier_texts(node, &self.source).join(".")
        };

        if !callee.is_empty() {
            self.unresolved_references.push(unresolved_reference(
                caller_id,
                callee,
                ReferenceKind::Calls,
                node.start_position.row + 1,
                node.start_position.column,
            ));
        }
    }

    pub(super) fn visit_pascal_block(&mut self, node: &SyntaxNode) {
        for child in &node.named_children {
            self.maybe_capture_fn_refs(child, child.node_type());
            match child.node_type() {
                "exprCall" => {
                    self.extract_pascal_call(child);
                    if let Some(args) = child
                        .named_children
                        .iter()
                        .find(|candidate| candidate.node_type() == "exprArgs")
                    {
                        self.maybe_capture_fn_refs(args, "exprArgs");
                    }
                }
                "exprDot" if node.node_type() == "statement" => {
                    self.extract_pascal_parenless_call(child)
                }
                "exprDot" => {
                    for grandchild in &child.named_children {
                        if grandchild.node_type() == "exprCall" {
                            self.extract_pascal_call(grandchild);
                        }
                    }
                }
                _ => self.visit_pascal_block(child),
            }
        }
    }

    pub(super) fn ensure_pascal_method_index(&mut self) {
        // 延迟构建索引是因为声明区节点要先建完；key 同时包含短名和 qualified
        // 后缀，兼容实现区写 `Method` 或 `TClass.Method`。
        if self.method_index.is_some() {
            return;
        }
        let mut index = HashMap::new();
        for node in &self.nodes {
            if !matches!(node.kind, NodeKind::Method | NodeKind::Function) {
                continue;
            }
            let name_key = node.name.to_ascii_lowercase();
            index.entry(name_key).or_insert_with(|| node.id.clone());
            if node.kind == NodeKind::Method {
                let parts = node.qualified_name.split("::").collect::<Vec<_>>();
                if parts.len() >= 2 {
                    for idx in 0..parts.len() - 1 {
                        index.insert(parts[idx..].join(".").to_ascii_lowercase(), node.id.clone());
                    }
                }
            }
        }
        self.method_index = Some(index);
    }
}
