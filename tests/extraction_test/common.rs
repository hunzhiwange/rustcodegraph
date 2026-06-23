pub(crate) use rustcodegraph::extraction::grammars::{
    detect_language, get_supported_languages, init_grammars, is_language_supported, is_source_file,
    language_key, load_all_grammars,
};
pub(crate) use rustcodegraph::extraction::index::{
    build_default_ignore, extract_from_source, scan_directory,
};
pub(crate) use rustcodegraph::types::{
    EdgeKind, ExtractionResult, Language, Node, NodeKind, ReferenceKind, Visibility,
};
pub(crate) use rustcodegraph::utils::normalize_path;
pub(crate) use rustcodegraph::{CodeGraph, IndexOptions};
pub(crate) use std::ffi::OsStr;
pub(crate) use std::fs;
use std::future::Future;
pub(crate) use std::path::{Path, PathBuf};
use std::pin::Pin;
pub(crate) use std::process::{Command, Stdio};
use std::sync::Once;
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
use std::time::{SystemTime, UNIX_EPOCH};
pub(crate) static GRAMMAR_INIT: Once = Once::new();
pub(crate) const TS_DESCRIBE_COUNT: usize = 117;
pub(crate) const TS_TEST_CASE_COUNT: usize = 372;
pub(crate) const TS_EXTRACTION_TEST: &str = "__tests__/extraction.test.ts";
pub(crate) const PARITY_IGNORE_REASON: &str = "Rust native extraction parity for this TypeScript case is still in progress; \
     this wrapper runs the original Vitest assertion path when explicitly included";
pub(crate) fn noop_raw_waker() -> RawWaker {
    fn clone(_: *const ()) -> RawWaker {
        noop_raw_waker()
    }
    fn wake(_: *const ()) {}
    fn wake_by_ref(_: *const ()) {}
    fn drop(_: *const ()) {}
    static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, wake, wake_by_ref, drop);
    RawWaker::new(std::ptr::null(), &VTABLE)
}
pub(crate) fn block_on<F: Future>(future: F) -> F::Output {
    let waker = unsafe { Waker::from_raw(noop_raw_waker()) };
    let mut cx = Context::from_waker(&waker);
    let mut future = Pin::from(Box::new(future));
    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(value) => return value,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}
pub(crate) fn before_all_init_grammars() {
    GRAMMAR_INIT.call_once(|| {
        let _ = block_on(init_grammars());
        let _ = block_on(load_all_grammars());
    });
}
pub(crate) fn assert_detected_language(path: &str, content: Option<&str>, expected: Language) {
    before_all_init_grammars();
    assert_eq!(detect_language(path, content), expected);
}
pub(crate) fn assert_language_support(language: Language, expected: bool) {
    before_all_init_grammars();
    assert_eq!(is_language_supported(language), expected);
}
pub(crate) fn assert_supported_languages_include(expected_languages: &[Language]) {
    before_all_init_grammars();
    let languages = get_supported_languages();
    for expected in expected_languages {
        assert!(languages.contains(expected), "missing {expected:?}");
    }
}
pub(crate) fn extract(file_path: &str, source: &str) -> ExtractionResult {
    before_all_init_grammars();
    extract_from_source(file_path, source, None, None)
}
pub(crate) fn find_node<'a>(
    result: &'a ExtractionResult,
    kind: NodeKind,
    name: &str,
) -> Option<&'a Node> {
    result
        .nodes
        .iter()
        .find(|node| node.kind == kind && node.name == name)
}
pub(crate) fn nodes_by_kind(result: &ExtractionResult, kind: NodeKind) -> Vec<&Node> {
    result
        .nodes
        .iter()
        .filter(|node| node.kind == kind)
        .collect()
}
pub(crate) fn names_by_kind(result: &ExtractionResult, kind: NodeKind) -> Vec<String> {
    nodes_by_kind(result, kind)
        .into_iter()
        .map(|node| node.name.clone())
        .collect()
}
pub(crate) fn is_exported(node: &Node) -> bool {
    node.is_exported.unwrap_or(false)
}
pub(crate) fn references_by_kind(result: &ExtractionResult, kind: ReferenceKind) -> Vec<String> {
    result
        .unresolved_references
        .iter()
        .filter(|reference| reference.reference_kind == kind)
        .map(|reference| reference.reference_name.clone())
        .collect()
}
pub(crate) fn import_nodes(result: &ExtractionResult) -> Vec<&Node> {
    nodes_by_kind(result, NodeKind::Import)
}
pub(crate) fn single_import<'a>(result: &'a ExtractionResult, expected_name: &str) -> &'a Node {
    let imports = import_nodes(result);
    assert_eq!(imports.len(), 1, "imports: {imports:?}");
    let import = imports[0];
    assert_eq!(import.name, expected_name);
    import
}
pub(crate) fn assert_import_names(result: &ExtractionResult, expected: &[&str]) {
    let names = names_by_kind(result, NodeKind::Import);
    assert_eq!(names.len(), expected.len(), "imports: {names:?}");
    for name in expected {
        assert_contains(&names, name);
    }
}
pub(crate) fn assert_no_imports(result: &ExtractionResult) {
    let imports = import_nodes(result);
    assert!(imports.is_empty(), "imports: {imports:?}");
}
pub(crate) fn assert_signature_eq(node: &Node, expected: &str) {
    assert_eq!(node.signature.as_deref(), Some(expected));
}
pub(crate) fn assert_signature_contains(node: &Node, expected: &str) {
    let signature = node.signature.as_deref().unwrap_or_default();
    assert!(
        signature.contains(expected),
        "expected signature {signature:?} to contain {expected:?}"
    );
}
pub(crate) fn assert_return_type(
    result: &ExtractionResult,
    kind: NodeKind,
    name: &str,
    expected: Option<&str>,
) {
    let node = find_node(result, kind, name).unwrap_or_else(|| {
        panic!(
            "missing {kind:?} {name:?}; nodes: {:?}",
            result
                .nodes
                .iter()
                .map(|node| (&node.kind, &node.name))
                .collect::<Vec<_>>()
        )
    });
    assert_eq!(node.return_type.as_deref(), expected);
}
pub(crate) fn expect_node<'a>(
    result: &'a ExtractionResult,
    kind: NodeKind,
    name: &str,
) -> &'a Node {
    find_node(result, kind, name).unwrap_or_else(|| {
        panic!(
            "missing {kind:?} {name:?}; nodes: {:?}",
            result
                .nodes
                .iter()
                .map(|node| (&node.kind, &node.name, &node.file_path))
                .collect::<Vec<_>>()
        )
    })
}
pub(crate) fn assert_names_include(result: &ExtractionResult, kind: NodeKind, expected: &[&str]) {
    let names = names_by_kind(result, kind);
    for name in expected {
        assert_contains(&names, name);
    }
}
pub(crate) fn reference_names(result: &ExtractionResult, kind: ReferenceKind) -> Vec<String> {
    references_by_kind(result, kind)
}
pub(crate) fn assert_reference_names_include(
    result: &ExtractionResult,
    kind: ReferenceKind,
    expected: &[&str],
) {
    let names = reference_names(result, kind);
    for name in expected {
        assert_contains(&names, name);
    }
}
pub(crate) fn index_project(temp: &TempDir) -> CodeGraph {
    let mut cg = CodeGraph::init_sync(temp.path()).expect("CodeGraph should initialize");
    let result = cg.index_all(IndexOptions::default());
    assert!(result.success, "index_all failed: {:?}", result.errors);
    let _ = cg.resolve_references();
    cg
}
pub(crate) struct TempDir {
    path: PathBuf,
}
impl TempDir {
    pub(crate) fn new(prefix: &str) -> Self {
        for attempt in 0..100 {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time should be after Unix epoch")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "{prefix}-{}-{unique}-{attempt}",
                std::process::id()
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Self { path },
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(err) => panic!("failed to create temp dir {}: {err}", path.display()),
            }
        }
        panic!("failed to create unique temp dir for {prefix}");
    }
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
    pub(crate) fn write(&self, relative_path: &str, content: &str) {
        let path = self.path.join(relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .unwrap_or_else(|err| panic!("failed to create {}: {err}", parent.display()));
        }
        fs::write(&path, content)
            .unwrap_or_else(|err| panic!("failed to write {}: {err}", path.display()));
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
pub(crate) fn assert_contains(actual: &[String], expected: &str) {
    assert!(
        actual.iter().any(|value| value == expected),
        "expected {actual:?} to contain {expected:?}"
    );
}
pub(crate) fn assert_not_contains(actual: &[String], unexpected: &str) {
    assert!(
        actual.iter().all(|value| value != unexpected),
        "expected {actual:?} not to contain {unexpected:?}"
    );
}
pub(crate) fn impact_names(cg: &mut CodeGraph, node_id: &str, depth: u64) -> Vec<String> {
    cg.get_impact_radius(node_id, depth)
        .nodes
        .into_values()
        .map(|node| node.name)
        .collect()
}
pub(crate) fn impact_file_paths(cg: &mut CodeGraph, node_id: &str, depth: u64) -> Vec<String> {
    cg.get_impact_radius(node_id, depth)
        .nodes
        .into_values()
        .map(|node| node.file_path)
        .collect()
}
pub(crate) fn assert_not_contains_fragment(actual: &[String], unexpected: &str) {
    assert!(
        actual.iter().all(|value| !value.contains(unexpected)),
        "expected {actual:?} not to contain fragment {unexpected:?}"
    );
}
pub(crate) fn git<I, S>(cwd: &Path, args: I)
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let args = args
        .into_iter()
        .map(|arg| arg.as_ref().to_os_string())
        .collect::<Vec<_>>();
    let output = Command::new("git")
        .args(&args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .unwrap_or_else(|err| panic!("failed to run git {args:?} in {}: {err}", cwd.display()));
    assert!(
        output.status.success(),
        "git {args:?} failed in {}\nstatus: {}\nstderr:\n{}",
        cwd.display(),
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
}
pub(crate) fn git_commit_all(cwd: &Path, message: &str) {
    git(cwd, ["add", "-A"]);
    git(
        cwd,
        [
            "-c",
            "user.email=test@test.com",
            "-c",
            "user.name=Test",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "-m",
            message,
        ],
    );
}
#[allow(dead_code)]
pub(crate) fn regex_escape(input: &str) -> String {
    let mut escaped = String::new();
    for ch in input.chars() {
        if matches!(
            ch,
            '.' | '+' | '*' | '?' | '^' | '$' | '(' | ')' | '[' | ']' | '{' | '}' | '|' | '\\'
        ) {
            escaped.push('\\');
        }
        escaped.push(ch);
    }
    escaped
}
#[allow(dead_code)]
pub(crate) fn run_ts_case(suite: &[&str], case_name: &str) {
    let mut full_name = suite.join(" ");
    if !full_name.is_empty() {
        full_name.push(' ');
    }
    full_name.push_str(case_name);
    let pattern = regex_escape(&full_name);
    let vitest = Path::new(env!("CARGO_MANIFEST_DIR")).join("node_modules/.bin/vitest");
    assert!(
        vitest.is_file(),
        "cannot run Vitest extraction case {full_name:?}: local node_modules/.bin/vitest is missing"
    );
    let output = Command::new(&vitest)
        .args(["run", TS_EXTRACTION_TEST, "-t", &pattern])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap_or_else(|err| panic!("failed to spawn Vitest for {full_name:?}: {err}"));
    assert!(
        output.status.success(),
        "Vitest extraction case failed: {full_name}\nstatus: {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
