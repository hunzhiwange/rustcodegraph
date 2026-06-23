//! Razor 文件的 `@using` 解析辅助。
//!
//! Razor 视图会继承当前目录及父目录中的 `_Imports.razor`，因此单看当前文件
//! 无法还原命名空间；这里把这条目录链缓存起来，供普通引用解析补齐 FQN。

use std::collections::{HashMap, HashSet};

use crate::types::Node;

use super::{ReferenceResolver, ResolutionContext, ResolvedBy, ResolvedRef, UnresolvedRef};

impl<'db> ReferenceResolver<'db> {
    fn get_razor_usings(&mut self, file_path: &str) -> Vec<String> {
        if let Some(cached) = self.razor_usings_cache.get(file_path) {
            return cached.clone();
        }
        let mut usings = HashSet::new();
        let mut add_from = |src: Option<String>| {
            let Some(src) = src else {
                return;
            };
            for line in src.lines() {
                let trimmed = line.trim_start();
                let Some(rest) = trimmed.strip_prefix("@using") else {
                    continue;
                };
                let rest = rest
                    .trim_start()
                    .strip_prefix("static")
                    .unwrap_or(rest)
                    .trim_start();
                let ns = rest
                    .chars()
                    .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '.')
                    .collect::<String>();
                if !ns.is_empty() {
                    usings.insert(ns);
                }
            }
        };

        add_from(self.read_file(file_path));
        // `_Imports.razor` 从当前目录向项目根级联生效，越靠上的文件越通用；
        // 用 set 去重即可，后续只有在唯一候选时才会建边。
        let mut dir = file_path
            .rsplit_once('/')
            .map(|(dir, _)| dir.to_string())
            .unwrap_or_default();
        loop {
            let imports = if dir.is_empty() {
                "_Imports.razor".to_string()
            } else {
                format!("{dir}/_Imports.razor")
            };
            add_from(self.read_file(&imports));
            if dir.is_empty() {
                break;
            }
            dir = dir
                .rsplit_once('/')
                .map(|(parent, _)| parent.to_string())
                .unwrap_or_default();
        }
        let arr = usings.into_iter().collect::<Vec<_>>();
        self.razor_usings_cache
            .insert(file_path.to_string(), arr.clone());
        arr
    }

    pub(super) fn resolve_razor_using(&mut self, reference: &UnresolvedRef) -> Option<ResolvedRef> {
        // 已带限定符的名字交给常规 qualified/import 逻辑；这里专门处理视图中
        // 未限定的组件/类型名。
        if reference.reference_name.contains('.') || reference.reference_name.contains("::") {
            return None;
        }
        let usings = self.get_razor_usings(&reference.file_path);
        if usings.is_empty() {
            return None;
        }
        let mut found: HashMap<String, Node> = HashMap::new();
        for ns in usings {
            for candidate in
                self.get_nodes_by_qualified_name(&format!("{}::{}", ns, reference.reference_name))
            {
                found.insert(candidate.id.clone(), candidate);
            }
        }
        if found.len() != 1 {
            return None;
        }
        let target = found.into_values().next()?;
        Some(ResolvedRef {
            original: reference.clone(),
            target_node_id: target.id,
            confidence: 0.9,
            resolved_by: ResolvedBy::Import,
        })
    }
}
