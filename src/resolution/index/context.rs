//! `ResolutionContext` 的数据库/文件系统适配层。
//!
//! 名称匹配、import resolver 和框架 resolver 都通过这个 trait 访问索引。
//! 这里负责把昂贵的 DB 查询、文件读取和项目配置加载包上一层批次级缓存。

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::types::{EdgeKind, Language, Node, NodeKind};

use super::helpers::{js_family_extension, supertype_bearing};
use super::{ImportMapping, ReExport, ReferenceResolver, ResolutionContext};
use crate::resolution::go_module::{GoModule, load_go_module};
use crate::resolution::import_resolver::{
    extract_import_mappings, extract_re_exports, load_cpp_include_dirs,
};
use crate::resolution::path_aliases::{AliasMap, load_project_aliases};
use crate::resolution::workspace_packages::{WorkspacePackages, load_workspace_packages};

impl<'db> ResolutionContext for ReferenceResolver<'db> {
    fn get_nodes_in_file(&mut self, file_path: &str) -> Vec<Node> {
        let key = file_path.to_string();
        if let Some(cached) = self.node_cache.get(&key) {
            return cached;
        }
        let result = self
            .queries
            .get_nodes_by_file(file_path)
            .unwrap_or_default();
        self.node_cache.set(key, result.clone());
        result
    }

    fn get_nodes_by_name(&mut self, name: &str) -> Vec<Node> {
        let key = name.to_string();
        if let Some(cached) = self.name_cache.get(&key) {
            return cached;
        }
        let result = self.queries.get_nodes_by_name(name).unwrap_or_default();
        self.name_cache.set(key, result.clone());
        result
    }

    fn get_nodes_by_qualified_name(&mut self, qualified_name: &str) -> Vec<Node> {
        let key = qualified_name.to_string();
        if let Some(cached) = self.qualified_name_cache.get(&key) {
            return cached;
        }
        let result = self
            .queries
            .get_nodes_by_qualified_name_exact(qualified_name)
            .unwrap_or_default();
        self.qualified_name_cache.set(key, result.clone());
        result
    }

    fn get_nodes_by_kind(&mut self, kind: NodeKind) -> Vec<Node> {
        self.queries.get_nodes_by_kind(kind).unwrap_or_default()
    }

    fn file_exists(&mut self, file_path: &str) -> bool {
        if let Some(known_files) = &self.known_files {
            let normalized = file_path.replace('\\', "/");
            // 索引中的路径统一偏向 `/`，但某些 import resolver 会传入平台路径；
            // 同时查原始值和归一化值，避免 Windows 路径导致的假阴性。
            if known_files.contains(file_path) || known_files.contains(&normalized) {
                return true;
            }
        }
        fs::metadata(Path::new(&self.project_root).join(file_path)).is_ok()
    }

    fn read_file(&mut self, file_path: &str) -> Option<String> {
        let key = file_path.to_string();
        if let Some(cached) = self.file_cache.get(&key) {
            return cached;
        }
        let content = fs::read_to_string(Path::new(&self.project_root).join(file_path)).ok();
        self.file_cache.set(key, content.clone());
        content
    }

    fn get_project_root(&self) -> String {
        self.project_root.clone()
    }

    fn get_all_files(&mut self) -> Vec<String> {
        self.queries.get_all_file_paths().unwrap_or_default()
    }

    fn get_nodes_by_lower_name(&mut self, lower_name: &str) -> Vec<Node> {
        let key = lower_name.to_string();
        if let Some(cached) = self.lower_name_cache.get(&key) {
            return cached;
        }
        let result = self
            .queries
            .get_nodes_by_lower_name(lower_name)
            .unwrap_or_default();
        self.lower_name_cache.set(key, result.clone());
        result
    }

    fn get_supertypes(&mut self, type_name: &str, language: Language) -> Vec<String> {
        // 只通过已抽取的 extends/implements 边向外看一层；更深层递归由调用方
        // 控制深度，防止继承环或宽继承图在 resolver 内爆开。
        let type_nodes = self
            .get_nodes_by_name(type_name)
            .into_iter()
            .filter(|node| supertype_bearing(node.kind) && node.language == language)
            .collect::<Vec<_>>();
        let mut supertypes = HashSet::new();
        for type_node in type_nodes {
            for edge in self
                .queries
                .get_outgoing_edges(
                    &type_node.id,
                    Some(vec![EdgeKind::Implements, EdgeKind::Extends]),
                    None,
                )
                .unwrap_or_default()
            {
                if let Some(target) = self.queries.get_node_by_id(&edge.target).unwrap_or(None)
                    && !target.name.is_empty()
                    && target.name != type_name
                {
                    supertypes.insert(target.name);
                }
            }
        }
        supertypes.into_iter().collect()
    }

    fn get_node_by_id(&mut self, id: &str) -> Option<Node> {
        self.queries.get_node_by_id(id).unwrap_or(None)
    }

    fn get_import_mappings(&mut self, file_path: &str, language: Language) -> Vec<ImportMapping> {
        let key = file_path.to_string();
        if let Some(cached) = self.import_mapping_cache.get(&key) {
            return cached;
        }
        let Some(content) = self.read_file(file_path) else {
            self.import_mapping_cache.set(key, Vec::new());
            return Vec::new();
        };
        let mappings = extract_import_mappings(file_path, &content, language);
        self.import_mapping_cache.set(key, mappings.clone());
        mappings
    }

    fn get_project_aliases(&mut self) -> Option<AliasMap> {
        // tsconfig/jsconfig 路径别名是项目级配置，懒加载一次即可；不存在也要
        // 记录 loaded，避免每个 import 都重新碰文件系统。
        if !self.project_aliases_loaded {
            self.project_aliases = load_project_aliases(&self.project_root);
            self.project_aliases_loaded = true;
        }
        self.project_aliases.clone()
    }

    fn get_go_module(&mut self) -> Option<GoModule> {
        if !self.go_module_loaded {
            self.go_module = load_go_module(&self.project_root);
            self.go_module_loaded = true;
        }
        self.go_module.clone()
    }

    fn get_workspace_packages(&mut self) -> Option<WorkspacePackages> {
        if !self.workspace_packages_loaded {
            self.workspace_packages = load_workspace_packages(&self.project_root);
            self.workspace_packages_loaded = true;
        }
        self.workspace_packages.clone()
    }

    fn get_re_exports(&mut self, file_path: &str, language: Language) -> Vec<ReExport> {
        let key = file_path.to_string();
        if let Some(cached) = self.re_export_cache.get(&key) {
            return cached;
        }
        let Some(content) = self.read_file(file_path) else {
            self.re_export_cache.set(key, Vec::new());
            return Vec::new();
        };
        let is_js_family = js_family_extension(file_path);
        // `.d.ts`/`.mts`/`.cjs` 等 JS 家族文件共用 TS re-export 语法解析；
        // 这里按扩展名覆盖语言，避免 Unknown 或 Jsx 让 barrel 解析漏掉。
        let re_exports = extract_re_exports(
            &content,
            if is_js_family {
                Language::TypeScript
            } else {
                language
            },
        );
        self.re_export_cache.set(key, re_exports.clone());
        re_exports
    }

    fn list_directories(&mut self, relative_path: &str) -> Vec<String> {
        // workspace/package resolver 只需要目录名列表；错误按空列表处理，让
        // import 解析保持“尽力而为”而不是中断整个索引流程。
        let target = if relative_path == "." || relative_path.is_empty() {
            PathBuf::from(&self.project_root)
        } else {
            Path::new(&self.project_root).join(relative_path)
        };
        fs::read_dir(target)
            .ok()
            .into_iter()
            .flat_map(|entries| entries.filter_map(Result::ok))
            .filter(|entry| entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false))
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect()
    }

    fn get_cpp_include_dirs(&mut self) -> Vec<String> {
        load_cpp_include_dirs(&self.project_root)
    }
}
