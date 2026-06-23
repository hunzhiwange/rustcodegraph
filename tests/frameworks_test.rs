//! Framework resolver tests.
//!
//! Rust port of `__tests__/frameworks.test.ts`.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use rustcodegraph::extraction::grammars::{is_play_routes_file, is_source_file};
use rustcodegraph::resolution::frameworks::index::{
    ASPNET_RESOLVER, ASTRO_RESOLVER, DJANGO_RESOLVER, EXPRESS_RESOLVER, FASTAPI_RESOLVER,
    FLASK_RESOLVER, GO_RESOLVER, LARAVEL_RESOLVER, NESTJS_RESOLVER, PLAY_RESOLVER, RAILS_RESOLVER,
    REACT_RESOLVER, RUST_RESOLVER, ResolverRef, SPRING_RESOLVER, SVELTE_RESOLVER, VAPOR_RESOLVER,
    get_applicable_frameworks,
};
use rustcodegraph::resolution::types::{
    FrameworkExtractionResult, FrameworkResolver, ImportMapping, ResolutionContext, ResolvedBy,
    ResolvedRef, UnresolvedRef, now_ms,
};
use rustcodegraph::types::{Language, Node, NodeKind, ReferenceKind};

#[derive(Default)]
struct MockResolutionContext {
    nodes: Vec<Node>,
    file_contents: HashMap<String, String>,
    files: HashSet<String>,
    all_files: Vec<String>,
    dirs_by_path: HashMap<String, Vec<String>>,
    project_root: String,
}

impl MockResolutionContext {
    fn new() -> Self {
        Self {
            project_root: "/test".to_string(),
            ..Self::default()
        }
    }

    fn with_nodes(nodes: Vec<Node>) -> Self {
        Self {
            nodes,
            ..Self::new()
        }
    }

    fn with_file_contents(mut self, entries: &[(&str, &str)]) -> Self {
        for (path, content) in entries {
            self.file_contents
                .insert((*path).to_string(), (*content).to_string());
            self.files.insert((*path).to_string());
        }
        self
    }

    fn with_files(mut self, files: &[&str]) -> Self {
        for file in files {
            self.files.insert((*file).to_string());
        }
        self
    }

    fn with_all_files(mut self, files: &[&str]) -> Self {
        self.all_files = files.iter().map(|file| (*file).to_string()).collect();
        for file in files {
            self.files.insert((*file).to_string());
        }
        self
    }

    fn with_dirs(mut self, entries: &[(&str, &[&str])]) -> Self {
        for (path, dirs) in entries {
            self.dirs_by_path.insert(
                (*path).to_string(),
                dirs.iter().map(|dir| (*dir).to_string()).collect(),
            );
        }
        self
    }
}

impl ResolutionContext for MockResolutionContext {
    fn get_nodes_in_file(&mut self, file_path: &str) -> Vec<Node> {
        self.nodes
            .iter()
            .filter(|node| node.file_path == file_path)
            .cloned()
            .collect()
    }

    fn get_nodes_by_name(&mut self, name: &str) -> Vec<Node> {
        self.nodes
            .iter()
            .filter(|node| node.name == name)
            .cloned()
            .collect()
    }

    fn get_nodes_by_qualified_name(&mut self, qualified_name: &str) -> Vec<Node> {
        self.nodes
            .iter()
            .filter(|node| node.qualified_name == qualified_name)
            .cloned()
            .collect()
    }

    fn get_nodes_by_kind(&mut self, kind: NodeKind) -> Vec<Node> {
        self.nodes
            .iter()
            .filter(|node| node.kind == kind)
            .cloned()
            .collect()
    }

    fn file_exists(&mut self, file_path: &str) -> bool {
        self.files.contains(file_path)
            || self.file_contents.contains_key(file_path)
            || self.nodes.iter().any(|node| node.file_path == file_path)
    }

    fn read_file(&mut self, file_path: &str) -> Option<String> {
        self.file_contents.get(file_path).cloned()
    }

    fn get_project_root(&self) -> String {
        self.project_root.clone()
    }

    fn get_all_files(&mut self) -> Vec<String> {
        if !self.all_files.is_empty() {
            return self.all_files.clone();
        }
        let mut files = self.files.iter().cloned().collect::<Vec<_>>();
        for path in self.file_contents.keys() {
            if !files.iter().any(|file| file == path) {
                files.push(path.clone());
            }
        }
        for node in &self.nodes {
            if !files.iter().any(|file| file == &node.file_path) {
                files.push(node.file_path.clone());
            }
        }
        files.sort();
        files
    }

    fn get_nodes_by_lower_name(&mut self, lower_name: &str) -> Vec<Node> {
        self.nodes
            .iter()
            .filter(|node| node.name.to_ascii_lowercase() == lower_name)
            .cloned()
            .collect()
    }

    fn get_import_mappings(&mut self, _file_path: &str, _language: Language) -> Vec<ImportMapping> {
        Vec::new()
    }

    fn list_directories(&mut self, relative_path: &str) -> Vec<String> {
        self.dirs_by_path
            .get(relative_path)
            .cloned()
            .unwrap_or_default()
    }
}

struct NamedResolver {
    name: &'static str,
    languages: Option<&'static [Language]>,
}

impl FrameworkResolver for NamedResolver {
    fn name(&self) -> &str {
        self.name
    }

    fn languages(&self) -> Option<&[Language]> {
        self.languages
    }

    fn detect(&self, _context: &mut dyn ResolutionContext) -> bool {
        true
    }

    fn resolve(
        &self,
        _reference: &UnresolvedRef,
        _context: &mut dyn ResolutionContext,
    ) -> Option<ResolvedRef> {
        None
    }
}

fn names(nodes: &[Node]) -> Vec<String> {
    nodes.iter().map(|node| node.name.clone()).collect()
}

fn route_names(nodes: &[Node]) -> Vec<String> {
    nodes
        .iter()
        .filter(|node| node.kind == NodeKind::Route)
        .map(|node| node.name.clone())
        .collect()
}

fn reference_names(references: &[UnresolvedRef]) -> Vec<String> {
    references
        .iter()
        .map(|reference| reference.reference_name.clone())
        .collect()
}

fn sorted(mut values: Vec<String>) -> Vec<String> {
    values.sort();
    values
}

#[allow(clippy::too_many_arguments)]
fn node(
    id: &str,
    kind: NodeKind,
    name: &str,
    qualified_name: &str,
    file_path: &str,
    language: Language,
    start_line: u64,
    end_line: u64,
) -> Node {
    Node {
        id: id.to_string(),
        kind,
        name: name.to_string(),
        qualified_name: qualified_name.to_string(),
        file_path: file_path.to_string(),
        language,
        start_line,
        end_line,
        start_column: 0,
        end_column: 0,
        docstring: None,
        signature: None,
        visibility: None,
        is_exported: None,
        is_async: None,
        is_static: None,
        is_abstract: None,
        decorators: None,
        type_parameters: None,
        return_type: None,
        updated_at: now_ms(),
    }
}

fn mk_class(name: &str, file_path: &str, start_line: u64, end_line: u64) -> Node {
    node(
        &format!("class:{file_path}:{start_line}:{name}"),
        NodeKind::Class,
        name,
        &format!("{file_path}::{name}"),
        file_path,
        Language::TypeScript,
        start_line,
        end_line,
    )
}

fn mk_route(
    file_path: &str,
    line: u64,
    method: &str,
    path: &str,
    name_override: Option<&str>,
) -> Node {
    node(
        &format!("route:{file_path}:{line}:{method}:{path}"),
        NodeKind::Route,
        name_override
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| format!("{method} {path}"))
            .as_str(),
        &format!("{file_path}::{method}:{path}"),
        file_path,
        Language::TypeScript,
        line,
        line,
    )
}

fn unresolved_ref(
    from_node_id: &str,
    reference_name: &str,
    reference_kind: ReferenceKind,
    file_path: &str,
    language: Language,
) -> UnresolvedRef {
    UnresolvedRef {
        from_node_id: from_node_id.to_string(),
        reference_name: reference_name.to_string(),
        reference_kind,
        line: 1,
        column: 1,
        file_path: file_path.to_string(),
        language,
        candidates: None,
    }
}

fn cargo_workspace_context(
    files_by_path: &[(&str, &str)],
    nodes_by_file: &[(&str, Vec<Node>)],
    dirs_by_path: &[(&str, &[&str])],
) -> MockResolutionContext {
    let mut nodes = Vec::new();
    for (_, file_nodes) in nodes_by_file {
        nodes.extend(file_nodes.clone());
    }
    let all_files = files_by_path
        .iter()
        .map(|(path, _)| *path)
        .chain(nodes_by_file.iter().map(|(path, _)| *path))
        .collect::<Vec<_>>();
    MockResolutionContext::with_nodes(nodes)
        .with_file_contents(files_by_path)
        .with_all_files(&all_files)
        .with_dirs(dirs_by_path)
}
#[path = "frameworks_test/basic.rs"]
mod basic;
#[path = "frameworks_test/comments.rs"]
mod comments;
#[path = "frameworks_test/dotnet_swift.rs"]
mod dotnet_swift;
#[path = "frameworks_test/go_rust.rs"]
mod go_rust;
#[path = "frameworks_test/javascript.rs"]
mod javascript;
#[path = "frameworks_test/nestjs_extract.rs"]
mod nestjs_extract;
#[path = "frameworks_test/nestjs_router.rs"]
mod nestjs_router;
#[path = "frameworks_test/php_ruby_jvm.rs"]
mod php_ruby_jvm;
#[path = "frameworks_test/python.rs"]
mod python;
