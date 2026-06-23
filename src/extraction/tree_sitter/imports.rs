use super::*;

impl TreeSitterExtractor {
    /// 创建 import 节点并补局部绑定引用。模块级 imports 边负责文件依赖，
    /// binding refs 负责把后续符号解析收敛到具体导入名。
    pub(super) fn extract_import(&mut self, node: &SyntaxNode) {
        let Some(extractor) = self.extractor else {
            return;
        };
        let Some(info) = extractor.extract_import(node, &self.source) else {
            return;
        };
        if let Some(created) = self.create_node(
            NodeKind::Import,
            &info.module_name,
            node,
            NodeExtra {
                signature: Some(info.signature.clone()),
                ..NodeExtra::default()
            },
        ) {
            let id = node_id(&created);
            self.nodes.push(created);
            let from_node_id = self
                .node_stack
                .last()
                .cloned()
                .unwrap_or_else(|| id.clone());
            if !info.handled_refs {
                self.unresolved_references.push(unresolved_reference(
                    from_node_id.clone(),
                    info.module_name,
                    ReferenceKind::Imports,
                    node.start_position.row + 1,
                    node.start_position.column,
                ));
            }
            match self.language {
                Language::TypeScript | Language::Tsx | Language::JavaScript | Language::Jsx => {
                    self.emit_import_binding_refs(node, &from_node_id);
                }
                Language::Python if node.node_type() == "import_from_statement" => {
                    self.emit_py_from_import_refs(node, &from_node_id);
                }
                Language::Rust if node.node_type() == "use_declaration" => {
                    self.emit_rust_use_binding_refs(node, &from_node_id);
                }
                _ => {}
            }
        }
    }

    pub(super) fn emit_import_binding_refs(&mut self, node: &SyntaxNode, from_node_id: &str) {
        // TS/JS 的 default、named、namespace import 在 AST 中形态不同，但 resolver
        // 只需要“本文件引入了哪个本地名”。
        let Some(clause) = node
            .named_children
            .iter()
            .find(|child| child.node_type() == "import_clause")
        else {
            return;
        };
        for child in &clause.named_children {
            match child.node_type() {
                "identifier" => self.push_import_ref(from_node_id, child),
                "named_imports" => {
                    for spec in &child.named_children {
                        if spec.node_type() != "import_specifier" {
                            continue;
                        }
                        if let Some(name_node) = get_child_by_field(spec, "alias")
                            .or_else(|| get_child_by_field(spec, "name"))
                            .or_else(|| spec.named_child(0))
                        {
                            self.push_import_ref(from_node_id, name_node);
                        }
                    }
                }
                "namespace_import" => {
                    if let Some(name_node) = child
                        .named_children
                        .iter()
                        .find(|grandchild| grandchild.node_type() == "identifier")
                        .or_else(|| child.named_child(0))
                    {
                        self.push_import_ref(from_node_id, name_node);
                    }
                }
                _ => {}
            }
        }
    }

    pub(super) fn emit_re_export_refs(&mut self, node: &SyntaxNode, from_node_id: &str) {
        // `export { Foo } from "./x"` 对当前文件同样是依赖入口；用 Imports 引用
        // 表示可让 import resolver 复用同一套路径解析逻辑。
        let Some(clause) = node
            .named_children
            .iter()
            .find(|child| child.node_type() == "export_clause")
        else {
            return;
        };
        for spec in &clause.named_children {
            if spec.node_type() != "export_specifier" {
                continue;
            }
            let Some(name_node) = get_child_by_field(spec, "name").or_else(|| spec.named_child(0))
            else {
                continue;
            };
            let name = get_node_text(name_node, &self.source);
            if name.is_empty() || name == "default" {
                continue;
            }
            self.unresolved_references.push(unresolved_reference(
                from_node_id.to_owned(),
                name,
                ReferenceKind::Imports,
                name_node.start_position.row + 1,
                name_node.start_position.column,
            ));
        }
    }

    pub(super) fn emit_rust_use_binding_refs(&mut self, node: &SyntaxNode, from_node_id: &str) {
        let mut paths = Vec::new();
        for child in &node.named_children {
            self.collect_rust_use_paths(child, "", &mut paths);
        }
        for (path, path_node) in paths {
            let leaf = path.rsplit("::").next().unwrap_or(path.as_str());
            if matches!(leaf, "" | "self" | "super" | "crate" | "*") {
                continue;
            }
            self.unresolved_references.push(unresolved_reference(
                from_node_id.to_owned(),
                path,
                ReferenceKind::Imports,
                path_node.start_position.row + 1,
                path_node.start_position.column,
            ));
        }
    }

    #[allow(dead_code)]
    pub(super) fn emit_php_use_refs(&mut self, node: &SyntaxNode, from_node_id: &str) {
        let _ = (node, from_node_id);
    }

    #[allow(dead_code)]
    pub(super) fn emit_ruby_require_refs(&mut self, node: &SyntaxNode, from_node_id: &str) {
        let _ = (node, from_node_id);
    }

    #[allow(dead_code)]
    pub(super) fn push_php_use_ref(&mut self, fqn: &str, from_node_id: &str, node: &SyntaxNode) {
        self.unresolved_references.push(unresolved_reference(
            from_node_id.to_owned(),
            fqn.to_owned(),
            ReferenceKind::Imports,
            node.start_position.row + 1,
            node.start_position.column,
        ));
    }

    pub(super) fn emit_py_from_import_refs(&mut self, node: &SyntaxNode, from_node_id: &str) {
        let module_name = get_child_by_field(node, "module_name");
        for child in &node.named_children {
            if child.node_type() == "wildcard_import" {
                continue;
            }
            if let Some(module_name) = module_name
                && child.start_index == module_name.start_index
                && child.end_index == module_name.end_index
            {
                continue;
            }
            let name_node = match child.node_type() {
                "aliased_import" => get_child_by_field(child, "alias")
                    .or_else(|| get_child_by_field(child, "name"))
                    .or_else(|| child.named_child(0)),
                "dotted_name" | "identifier" => Some(child),
                _ => None,
            };
            let Some(name_node) = name_node else {
                continue;
            };
            let raw = get_node_text(name_node, &self.source);
            let local = raw.rsplit('.').next().unwrap_or(raw.as_str()).to_owned();
            if local.is_empty() || local == "*" {
                continue;
            }
            self.unresolved_references.push(unresolved_reference(
                from_node_id.to_owned(),
                local,
                ReferenceKind::Imports,
                name_node.start_position.row + 1,
                name_node.start_position.column,
            ));
        }
    }

    pub(super) fn push_import_ref(&mut self, from_node_id: &str, name_node: &SyntaxNode) {
        let name = get_node_text(name_node, &self.source);
        if name.is_empty() {
            return;
        }
        self.unresolved_references.push(unresolved_reference(
            from_node_id.to_owned(),
            name,
            ReferenceKind::Imports,
            name_node.start_position.row + 1,
            name_node.start_position.column,
        ));
    }

    pub(super) fn collect_rust_use_paths(
        &self,
        node: &SyntaxNode,
        prefix: &str,
        paths: &mut Vec<(String, SyntaxNode)>,
    ) {
        // Rust use tree 可以嵌套、重命名和成组展开，这里保留完整 `a::b::C`
        // 叶子路径，后续 resolver 再决定它指向模块还是符号。
        let join = |prefix: &str, segment: &str| {
            if prefix.is_empty() {
                segment.to_owned()
            } else {
                format!("{prefix}::{segment}")
            }
        };
        match node.node_type() {
            "identifier" => {
                paths.push((
                    join(prefix, &get_node_text(node, &self.source)),
                    node.clone(),
                ));
            }
            "scoped_identifier" => {
                let full = get_node_text(node, &self.source).trim().to_owned();
                if !full.is_empty() {
                    paths.push((join(prefix, &full), node.clone()));
                }
            }
            "scoped_use_list" => {
                let path_node = get_child_by_field(node, "path");
                let segment = path_node
                    .map(|path| get_node_text(path, &self.source).trim().to_owned())
                    .unwrap_or_default();
                let next_prefix = if segment.is_empty() {
                    prefix.to_owned()
                } else {
                    join(prefix, &segment)
                };
                if let Some(list) = get_child_by_field(node, "list").or_else(|| {
                    node.named_children
                        .iter()
                        .find(|child| child.node_type() == "use_list")
                }) {
                    self.collect_rust_use_paths(list, &next_prefix, paths);
                }
            }
            "use_list" => {
                for child in &node.named_children {
                    self.collect_rust_use_paths(child, prefix, paths);
                }
            }
            "use_as_clause" => {
                if let Some(path) = get_child_by_field(node, "path").or_else(|| node.named_child(0))
                {
                    self.collect_rust_use_paths(path, prefix, paths);
                }
            }
            _ => {}
        }
    }
}
