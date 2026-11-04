//! Redux thunk registration contract, upstream 15483713.
use rustcodegraph::CodeGraph;

fn run(source: &str, expected: &[(&str, &str)]) {
    run_files(&[("thunks.ts", source)], expected);
}
fn run_files(files: &[(&str, &str)], expected: &[(&str, &str)]) {
    let root = Project::new();
    for (name, source) in files {
        std::fs::write(root.path().join(name), source).unwrap();
    }
    let mut graph = CodeGraph::init_sync(root.path()).unwrap();
    let snapshot = |graph: &mut CodeGraph| {
        let nodes: Vec<_> = files
            .iter()
            .flat_map(|(name, _)| graph.get_nodes_in_file(name))
            .collect();
        let mut pairs = Vec::new();
        for node in &nodes {
            for edge in graph.get_outgoing_edges(&node.id) {
                if edge
                    .metadata
                    .as_ref()
                    .and_then(|m| m.get("synthesizedBy"))
                    .and_then(|v| v.as_str())
                    != Some("redux-thunk")
                {
                    continue;
                }
                assert_eq!(
                    edge.provenance,
                    Some(rustcodegraph::types::EdgeProvenance::Heuristic)
                );
                assert!(
                    edge.metadata.as_ref().unwrap()["registeredAt"]
                        .as_str()
                        .unwrap()
                        .starts_with("thunks.ts:")
                );
                let target = nodes.iter().find(|n| n.id == edge.target).unwrap();
                assert_eq!(
                    edge.metadata.as_ref().unwrap()["registeredAt"],
                    serde_json::json!(format!("{}:{}", node.file_path, edge.line.unwrap()))
                );
                if files.len() > 1 {
                    assert_eq!(target.file_path, "actions.ts");
                    assert_eq!(edge.metadata.as_ref().unwrap()["via"], "child");
                }
                pairs.push((node.name.clone(), target.name.clone()));
            }
        }
        pairs.sort();
        (nodes.len(), pairs)
    };
    assert!(graph.index_all(Default::default()).success);
    let first = snapshot(&mut graph);
    let mut want: Vec<_> = expected
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
    want.sort();
    assert_eq!(first.1, want);
    assert!(graph.index_all(Default::default()).success);
    assert_eq!(first, snapshot(&mut graph));
    std::fs::remove_file(root.path().join("thunks.ts")).unwrap();
    graph.sync(Default::default());
    let removed = snapshot(&mut graph);
    assert!(removed.1.is_empty());
    assert!(graph.index_all(Default::default()).success);
    assert_eq!(removed, snapshot(&mut graph));
    graph.close();
}
#[test]
fn case_232_registered_chain() {
    run(
        r#"import { createAsyncThunk } from '@reduxjs/toolkit';
export const deep = createAsyncThunk('deep', async () => 1);
export const inner = createAsyncThunk('inner', async (_, {dispatch}) => { dispatch(deep()); });
export const outer = createAsyncThunk('outer', async (_, {dispatch}) => { dispatch(inner()); });
"#,
        &[("outer", "inner"), ("inner", "deep")],
    );
}
#[test]
fn case_232_unregistered_target() {
    run(
        r#"import { createAsyncThunk } from '@reduxjs/toolkit';
function unrelated() { return 1; }
export const outer = createAsyncThunk('outer', async (_, {dispatch}) => { dispatch(unrelated()); });
"#,
        &[],
    );
}
#[test]
fn case_232_unregistered_factory_and_text() {
    run(
        r#"function createAsyncThunk(...args) { return args; }
export const deep = createAsyncThunk('deep', async () => 1);
export const outer = createAsyncThunk('outer', async (_, {dispatch}) => { dispatch(deep()); });
"#,
        &[],
    );
    run(
        r#"import { createAsyncThunk } from '@reduxjs/toolkit';
export const deep = createAsyncThunk('deep', async () => 1);
export const outer = createAsyncThunk('outer', async (_, {dispatch}) => {
// dispatch(deep());
const text = 'dispatch(deep())';
});
"#,
        &[],
    );
}

struct Project(std::path::PathBuf);
impl Project {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "rcg-232-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn path(&self) -> &std::path::Path {
        &self.0
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn case_232_import_alias_and_unimported_collision() {
    run_files(
        &[
            (
                "actions.ts",
                "import { createAsyncThunk as make } from '@reduxjs/toolkit';\nexport const deep = make('deep', async () => 1);",
            ),
            (
                "other.ts",
                "import { createAsyncThunk } from '@reduxjs/toolkit';\nexport const deep = createAsyncThunk('deep', async () => 2);",
            ),
            (
                "thunks.ts",
                "import { createAsyncThunk } from '@reduxjs/toolkit';\nimport { deep as child } from './actions';\nexport const outer = createAsyncThunk('outer', async (_, {dispatch}) => { dispatch(child()); dispatch(deep()); });",
            ),
        ],
        &[("outer", "deep")],
    );
}
#[test]
fn case_232_shadowed_target_and_deferred_callback() {
    run(
        r#"import { createAsyncThunk } from '@reduxjs/toolkit';
export const deep = createAsyncThunk('deep', async () => 1);
export const outer = createAsyncThunk('outer', async (_, {dispatch}) => {
 const deep = () => 0;
 dispatch(deep());
});
export const deferred = createAsyncThunk('later', async (_, {dispatch}) => {
 const callback = () => dispatch(deep());
});
export const fakeDispatch = createAsyncThunk('fake', async () => { dispatch(deep()); });
export const parameterShadow = createAsyncThunk('param', async (deep, {dispatch}) => { dispatch(deep()); });
"#,
        &[],
    );
}
