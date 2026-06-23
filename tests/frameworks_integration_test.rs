//! End-to-end framework extraction and synthesis regressions.
//!
//! This is the Rust port of `__tests__/frameworks-integration.test.ts`.

use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Once;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
use std::time::{SystemTime, UNIX_EPOCH};

use rustcodegraph::extraction::grammars::{init_grammars, load_all_grammars};
use rustcodegraph::types::{Edge, EdgeKind, EdgeProvenance, Language, NodeKind};
use rustcodegraph::{CodeGraph, IndexOptions};

static GRAMMAR_INIT: Once = Once::new();
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn noop_raw_waker() -> RawWaker {
    fn clone(_: *const ()) -> RawWaker {
        noop_raw_waker()
    }
    fn wake(_: *const ()) {}
    fn wake_by_ref(_: *const ()) {}
    fn drop(_: *const ()) {}

    static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, wake, wake_by_ref, drop);
    RawWaker::new(std::ptr::null(), &VTABLE)
}

fn block_on<F: Future>(future: F) -> F::Output {
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

fn before_all_init_grammars() {
    GRAMMAR_INIT.call_once(|| {
        let _ = block_on(init_grammars());
        let _ = block_on(load_all_grammars());
    });
}

struct TempProject {
    root: PathBuf,
}

impl TempProject {
    fn new(prefix: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after the Unix epoch")
            .as_nanos();
        let sequence = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "{prefix}-{}-{sequence}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap_or_else(|err| {
            panic!("failed to create temp project {}: {err}", root.display())
        });
        Self { root }
    }

    fn path(&self) -> &Path {
        &self.root
    }

    fn mkdir(&self, relative_path: &str) {
        let path = self.root.join(relative_path);
        fs::create_dir_all(&path)
            .unwrap_or_else(|err| panic!("failed to create {}: {err}", path.display()));
    }

    fn write(&self, relative_path: &str, content: &str) {
        let path = self.root.join(relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .unwrap_or_else(|err| panic!("failed to create {}: {err}", parent.display()));
        }
        fs::write(&path, content)
            .unwrap_or_else(|err| panic!("failed to write {}: {err}", path.display()));
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn index(project: &TempProject) -> CodeGraph {
    before_all_init_grammars();
    let mut cg = CodeGraph::init_sync(project.path()).expect("CodeGraph should initialize");
    let result = cg.index_all(IndexOptions::default());
    assert!(result.success, "indexing failed: {:?}", result.errors);
    cg
}

fn edge_metadata_str<'a>(edge: &'a Edge, key: &str) -> Option<&'a str> {
    edge.metadata
        .as_ref()
        .and_then(|metadata| metadata.get(key))
        .and_then(|value| value.as_str())
}

#[path = "frameworks_integration_test/cpp_end_to_end_virtual_override_synthesis.rs"]
mod cpp_end_to_end_virtual_override_synthesis;
#[path = "frameworks_integration_test/django_end_to_end_framework_extraction.rs"]
mod django_end_to_end_framework_extraction;
#[path = "frameworks_integration_test/flask_end_to_end_framework_extraction.rs"]
mod flask_end_to_end_framework_extraction;
#[path = "frameworks_integration_test/flutter_end_to_end_setstate_build_synthesis.rs"]
mod flutter_end_to_end_setstate_build_synthesis;
#[path = "frameworks_integration_test/go_grpc_stub_impl_synthesis.rs"]
mod go_grpc_stub_impl_synthesis;
#[path = "frameworks_integration_test/java_anonymous_class_override_synthesis_end_to_end.rs"]
mod java_anonymous_class_override_synthesis_end_to_end;
#[path = "frameworks_integration_test/java_end_to_end_field_injected_bean_trace_issue_389.rs"]
mod java_end_to_end_field_injected_bean_trace_issue_389;
#[path = "frameworks_integration_test/jvm_fqn_imports_end_to_end.rs"]
mod jvm_fqn_imports_end_to_end;
