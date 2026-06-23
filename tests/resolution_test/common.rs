use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rustcodegraph::resolution::import_resolver::clear_cpp_include_dir_cache;
use rustcodegraph::resolution::types::{ImportMapping, ResolutionContext, UnresolvedRef, now_ms};
use rustcodegraph::types::{Language, Node, NodeKind, ReferenceKind};

const BACKEND_BLOCKER: &str =
    "Rust CodeGraph end-to-end extraction/query/reference backend is not at TypeScript parity yet";

#[derive(Default)]
pub(crate) struct MockResolutionContext {
    nodes: Vec<Node>,
    file_contents: HashMap<String, String>,
    files: HashSet<String>,
    all_files: Vec<String>,
    project_root: String,
    file_exists_default: bool,
    cpp_include_dirs: Vec<String>,
    import_mappings: Vec<ImportMapping>,
}

impl MockResolutionContext {
    pub(crate) fn new() -> Self {
        Self {
            project_root: String::new(),
            ..Self::default()
        }
    }

    pub(crate) fn with_nodes(nodes: Vec<Node>) -> Self {
        Self {
            nodes,
            ..Self::new()
        }
    }

    pub(crate) fn with_files(files: &[&str]) -> Self {
        Self {
            files: files.iter().map(|file| (*file).to_owned()).collect(),
            all_files: files.iter().map(|file| (*file).to_owned()).collect(),
            ..Self::new()
        }
    }

    pub(crate) fn with_project_root(mut self, project_root: impl Into<String>) -> Self {
        self.project_root = project_root.into();
        self
    }

    pub(crate) fn with_file_contents(mut self, entries: &[(&str, &str)]) -> Self {
        for (path, content) in entries {
            self.file_contents
                .insert((*path).to_owned(), (*content).to_owned());
            self.files.insert((*path).to_owned());
        }
        self
    }

    pub(crate) fn with_all_files(mut self, files: &[&str]) -> Self {
        self.all_files = files.iter().map(|file| (*file).to_owned()).collect();
        for file in files {
            self.files.insert((*file).to_owned());
        }
        self
    }

    pub(crate) fn with_cpp_include_dirs(mut self, dirs: &[&str]) -> Self {
        self.cpp_include_dirs = dirs.iter().map(|dir| (*dir).to_owned()).collect();
        self
    }

    pub(crate) fn with_file_exists_default(mut self, value: bool) -> Self {
        self.file_exists_default = value;
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
        self.file_exists_default
            || self.files.contains(file_path)
            || self.file_contents.contains_key(file_path)
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
        self.import_mappings.clone()
    }

    fn get_cpp_include_dirs(&mut self) -> Vec<String> {
        self.cpp_include_dirs.clone()
    }
}

pub(crate) struct TempProject {
    root: PathBuf,
}

impl TempProject {
    pub(crate) fn new(prefix: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after Unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&root).unwrap_or_else(|err| {
            panic!("failed to create temp project {}: {err}", root.display())
        });
        Self { root }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.root
    }

    pub(crate) fn write(&self, relative_path: &str, content: &str) {
        let path = self.root.join(relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .unwrap_or_else(|err| panic!("failed to create {}: {err}", parent.display()));
        }
        fs::write(&path, content)
            .unwrap_or_else(|err| panic!("failed to write {}: {err}", path.display()));
    }

    pub(crate) fn mkdir(&self, relative_path: &str) {
        fs::create_dir_all(self.root.join(relative_path))
            .unwrap_or_else(|err| panic!("failed to create fixture dir {relative_path}: {err}"));
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

pub(crate) struct CppIncludeCacheGuard;

impl CppIncludeCacheGuard {
    pub(crate) fn new() -> Self {
        clear_cpp_include_dir_cache();
        Self
    }
}

impl Drop for CppIncludeCacheGuard {
    fn drop(&mut self) {
        clear_cpp_include_dir_cache();
    }
}

pub(crate) fn test_node(
    id: &str,
    kind: NodeKind,
    name: &str,
    qualified_name: &str,
    file_path: &str,
    language: Language,
    start_line: u64,
) -> Node {
    Node {
        id: id.to_owned(),
        kind,
        name: name.to_owned(),
        qualified_name: qualified_name.to_owned(),
        file_path: file_path.to_owned(),
        language,
        start_line,
        end_line: start_line + 10,
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

pub(crate) fn unresolved_ref(
    reference_name: &str,
    reference_kind: ReferenceKind,
    file_path: &str,
    language: Language,
) -> UnresolvedRef {
    UnresolvedRef {
        from_node_id: format!("caller:{file_path}:caller:5"),
        reference_name: reference_name.to_owned(),
        reference_kind,
        line: 5,
        column: 10,
        file_path: file_path.to_owned(),
        language,
        candidates: None,
    }
}

pub(crate) fn record_backend_blocker(case_name: &str) {
    assert!(
        !case_name.is_empty(),
        "ignored backend-parity case must record the TypeScript case name"
    );
    eprintln!("{BACKEND_BLOCKER}: {case_name}");
}
