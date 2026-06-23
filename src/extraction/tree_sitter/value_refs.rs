use super::*;

impl TreeSitterExtractor {
    pub(super) fn maybe_capture_fn_refs(&mut self, node: &SyntaxNode, node_type: &str) {
        let Some(spec) = self.fn_ref_spec.as_ref() else {
            return;
        };
        let Some(rule) = spec.dispatch.get(node_type).copied() else {
            return;
        };
        let Some(from_node_id) = self.node_stack.last().cloned() else {
            return;
        };
        for candidate in capture_fn_ref_candidates(node, rule, spec, &self.source) {
            self.fn_ref_candidates.push(FnRefCandidateInScope {
                candidate,
                from_node_id: from_node_id.clone(),
            });
        }
    }

    pub(super) fn scan_fn_ref_subtree(&mut self, node: &SyntaxNode, depth: usize) {
        // 函数引用扫描只穿透表达式层，遇到嵌套函数就停住；否则外层 owner 会
        // 错误“认领”内层闭包里的引用。
        if self.fn_ref_spec.is_none() || depth > 12 {
            return;
        }
        if depth > 0
            && (self
                .extractor
                .as_ref()
                .is_some_and(|extractor| extractor.function_types().contains(&node.node_type()))
                || matches!(
                    node.node_type(),
                    "arrow_function"
                        | "function_expression"
                        | "lambda_literal"
                        | "lambda_expression"
                ))
        {
            return;
        }
        self.maybe_capture_fn_refs(node, node.node_type());
        for child in &node.named_children {
            self.scan_fn_ref_subtree(child, depth + 1);
        }
    }

    pub(super) fn flush_fn_ref_candidates(&mut self) {
        // 这里只把语法候选变成 function_ref；候选是否真正能解析到函数，交给
        // 后续 resolver/name matcher。这样抽取阶段不会因为导入顺序或重名而丢边。
        let mut seen = HashSet::new();
        for scoped in self.fn_ref_candidates.drain(..) {
            let key = format!("{}:{}", scoped.from_node_id, scoped.candidate.name);
            if !seen.insert(key) {
                continue;
            }
            self.unresolved_references.push(unresolved_reference(
                scoped.from_node_id,
                scoped.candidate.name,
                ReferenceKind::FunctionRef,
                scoped.candidate.line,
                scoped.candidate.column,
            ));
        }
    }

    pub(super) fn capture_value_ref_scope(
        &mut self,
        kind: NodeKind,
        name: &str,
        id: &str,
        node: &SyntaxNode,
    ) {
        // value-ref 只追踪“像常量/配置项”的文件级值，避免把所有局部变量都放进图
        // 导致节点和边爆炸。Pascal 只开放常量，是因为变量声明区噪音更高。
        if !self.value_refs_enabled || self.nodes.len() > MAX_VALUE_REF_NODES {
            return;
        }
        if !VALUE_REF_LANGS.contains(&language_key(&self.language).as_str()) {
            return;
        }
        let target_kind_ok = if self.language == Language::Pascal {
            kind == NodeKind::Constant
        } else {
            matches!(kind, NodeKind::Constant | NodeKind::Variable)
        };
        let is_value_binding = matches!(kind, NodeKind::Constant | NodeKind::Variable);
        let is_target_scope = self.node_stack.last().is_some_and(|parent_id| {
            let parent_is_target_scope = parent_id.starts_with("file:")
                || parent_id.starts_with("class:")
                || parent_id.starts_with("module:")
                || parent_id.starts_with("struct:")
                || parent_id.starts_with("enum:");
            parent_is_target_scope
                && value_ref_decl_is_target_scope(
                    &self.source,
                    self.language,
                    name,
                    node,
                    parent_id,
                )
        });

        if target_kind_ok
            && name.len() >= 3
            && name.chars().any(|ch| ch == '_' || ch.is_ascii_uppercase())
            && is_target_scope
        {
            *self
                .file_scope_value_counts
                .entry(name.to_owned())
                .or_insert(0) += 1;
            self.file_scope_values
                .insert(name.to_owned(), id.to_owned());
        }

        if matches!(
            kind,
            NodeKind::Function | NodeKind::Method | NodeKind::Constant | NodeKind::Variable
        ) {
            self.value_ref_scopes.push(ValueRefScope {
                id: id.to_owned(),
                node: node.clone(),
                name: name.to_owned(),
                is_value_binding,
                is_target_scope,
            });
        }
    }

    pub(super) fn flush_value_refs(&mut self) {
        // flush 时先确认目标名在文件级唯一，再在各 scope 子树中找引用；如果发现
        // 同名局部声明遮蔽，就移除目标，宁可漏边也不连错边。
        let scopes = std::mem::take(&mut self.value_ref_scopes);
        let mut targets = std::mem::take(&mut self.file_scope_values);
        let file_scope_counts = std::mem::take(&mut self.file_scope_value_counts);
        if !self.value_refs_enabled
            || !VALUE_REF_LANGS.contains(&language_key(&self.language).as_str())
        {
            return;
        }
        if targets.is_empty() || scopes.is_empty() || is_generated_file(&self.file_path) {
            return;
        }

        for scope in &scopes {
            if scope.is_value_binding && !scope.is_target_scope {
                targets.remove(&scope.name);
            }
        }
        if targets.is_empty() {
            return;
        }

        if let Some(tree) = &self.tree {
            let mut decl_counts: HashMap<String, usize> = HashMap::new();
            let mut stack = vec![tree.root_node.clone()];
            let mut visited = 0usize;
            while let Some(node) = stack.pop() {
                visited += 1;
                if visited > MAX_VALUE_REF_NODES {
                    break;
                }
                for name in self.value_decl_names(&node) {
                    if targets.contains_key(&name) {
                        *decl_counts.entry(name).or_default() += 1;
                    }
                }
                stack.extend(node.named_children.iter().cloned());
            }
            for name in targets.keys() {
                let text_count = count_value_declarations_in_source(&self.source, name);
                if text_count > 0 {
                    let entry = decl_counts.entry(name.clone()).or_default();
                    *entry = (*entry).max(text_count);
                }
            }
            for (name, count) in decl_counts {
                if self.language == Language::Python
                    && count > 1
                    && self.source.contains("try:")
                    && self.source.contains("except")
                {
                    continue;
                }
                if count > file_scope_counts.get(&name).copied().unwrap_or(1) {
                    targets.remove(&name);
                }
            }
            if targets.is_empty() {
                return;
            }
        }

        for scope in &scopes {
            let mut seen = HashSet::new();
            let mut stack = vec![scope.node.clone()];
            if let Some(sibling) = scope.node.next_named_sibling()
                && matches!(sibling.node_type(), "function_body" | "block")
            {
                stack.push(sibling);
            }
            let mut visited = 0usize;
            while let Some(node) = stack.pop() {
                visited += 1;
                if visited > MAX_VALUE_REF_NODES {
                    break;
                }
                if matches!(
                    node.node_type(),
                    "identifier" | "constant" | "name" | "simple_identifier"
                ) {
                    let ref_name = get_node_text(&node, &self.source);
                    if let Some(target_id) = targets.get(&ref_name)
                        && target_id != &scope.id
                        && ref_name != scope.name
                        && seen.insert(target_id.clone())
                    {
                        self.edges.push(Edge {
                            source: scope.id.clone(),
                            target: target_id.clone(),
                            kind: EdgeKind::References,
                            metadata: Some(HashMap::from([(
                                "valueRef".to_owned(),
                                serde_json::json!(true),
                            )])),
                            line: Some(node.start_position.row as u64 + 1),
                            column: Some(node.start_position.column as u64),
                            provenance: Some(EdgeProvenance::TreeSitter),
                        });
                    }
                }
                stack.extend(node.named_children.iter().cloned());
            }
        }
    }

    pub(super) fn value_decl_names(&self, node: &SyntaxNode) -> Vec<String> {
        // 不同语言对声明左侧的 AST 拆分差异很大，这里只提取最保守的标识符名，
        // Java 再用文本兜底处理 grammar 未拆开的局部声明。
        let mut names = Vec::new();
        let mut bump = |name_node: Option<&SyntaxNode>| {
            if let Some(name_node) = name_node
                && matches!(name_node.node_type(), "identifier" | "simple_identifier")
            {
                names.push(get_node_text(name_node, &self.source));
            }
        };
        match node.node_type() {
            "variable_declarator" | "const_spec" | "var_spec" => bump(node.named_child(0)),
            "const_item" | "static_item" => bump(get_child_by_field(node, "name")),
            "let_declaration" | "short_var_declaration" | "assignment" => {
                let left = get_child_by_field(node, "left")
                    .or_else(|| get_child_by_field(node, "pattern"))
                    .or_else(|| node.named_child(0));
                if let Some(left) = left {
                    if left.node_type() == "identifier" {
                        bump(Some(left));
                    } else {
                        for child in &left.named_children {
                            bump(Some(child));
                        }
                    }
                }
            }
            "local_variable_declaration" => {
                for child in &node.named_children {
                    if child.node_type() == "variable_declarator" {
                        bump(get_child_by_field(child, "name").or_else(|| child.named_child(0)));
                    }
                }
            }
            _ => {}
        }
        if names.is_empty() && self.language == Language::Java {
            names.extend(java_decl_names_from_text(&get_node_text(
                node,
                &self.source,
            )));
        }
        names
    }
}
