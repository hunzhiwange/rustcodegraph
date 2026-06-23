use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use rustcodegraph::{CodeGraph, IndexOptions};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

pub(super) struct TempDir {
    path: PathBuf,
}

impl TempDir {
    pub(super) fn new(prefix: &str) -> Self {
        for _ in 0..100 {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time should be after Unix epoch")
                .as_nanos();
            let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("{prefix}{}-{nanos}-{counter}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => return Self { path },
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(err) => panic!("failed to create temp dir {}: {err}", path.display()),
            }
        }

        panic!("failed to create unique temp dir with prefix {prefix}");
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    pub(super) fn write(&self, relative_path: &str, contents: &str) {
        let path = self.path.join(relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .unwrap_or_else(|err| panic!("failed to create {}: {err}", parent.display()));
        }
        fs::write(&path, contents)
            .unwrap_or_else(|err| panic!("failed to write {}: {err}", path.display()));
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        if self.path.exists() {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

pub(super) fn init_indexed_project(temp: &TempDir) {
    let mut cg = CodeGraph::init_sync(temp.path()).expect("CodeGraph should initialize");
    let result = cg.index_all(IndexOptions::default());
    assert!(result.success, "indexing should succeed");
    cg.close();
}

pub(super) fn init_original_eval_fixture() -> TempDir {
    let temp = TempDir::new("cg-eval-original-");

    temp.write(
        "server/rest/RestController.java",
        r#"
package org.elasticsearch.rest;

class RestController {
    private final BaseRestHandler handler = new BaseRestHandler();

    RestStatus dispatchRequest(RestRequest request) {
        return handler.handleRequest(request);
    }
}

interface RestHandler {
    RestStatus handleRequest(RestRequest request);
}

class BaseRestHandler implements RestHandler {
    public RestStatus handleRequest(RestRequest request) {
        return RestStatus.OK;
    }
}

class RestRequest {
    private final String path;

    RestRequest(String path) {
        this.path = path;
    }
}

enum RestStatus {
    OK,
    ERROR
}
"#,
    );

    temp.write(
        "search/SearchExecutionService.java",
        r#"
package org.elasticsearch.search;

class SearchExecutionService {
    SearchShardsGroup executeSearch(ShardSearchRequest shardRequest) {
        SearchShardsRequest request = new SearchShardsRequest();
        return request.shardsFor(shardRequest);
    }
}

class ShardSearchRequest {
    int shardId() {
        return 1;
    }
}

class SearchShardsRequest {
    SearchShardsGroup shardsFor(ShardSearchRequest request) {
        return new SearchShardsGroup(request.shardId());
    }
}

class SearchShardsGroup {
    private final int shardId;

    SearchShardsGroup(int shardId) {
        this.shardId = shardId;
    }
}

class SearchPhaseExecutionException extends RuntimeException {
}
"#,
    );

    temp.write(
        "bulk/TransportBulkAction.java",
        r#"
package org.elasticsearch.action.bulk;

class TransportBulkAction {
    BulkResponse execute(BulkRequest request) {
        return new BulkResponse(request.size());
    }
}

class BulkRequest {
    int size() {
        return 1;
    }
}

class BulkResponse {
    private final int itemCount;

    BulkResponse(int itemCount) {
        this.itemCount = itemCount;
    }
}
"#,
    );

    temp.write(
        "cluster/routing/allocation/AllocationService.java",
        r#"
package org.elasticsearch.cluster.routing.allocation;

class AllocationService {
    private final BalancedShardsAllocator allocator = new BalancedShardsAllocator();

    void reroute() {
        allocator.allocate();
    }
}

class BalancedShardsAllocator {
    void allocate() {
    }
}
"#,
    );

    temp.write(
        "transport/TransportService.java",
        r#"
package org.elasticsearch.transport;

class TransportService {
    private final SearchTransportService searchTransportService = new SearchTransportService();

    void sendRequest(String action, ActionListener listener) {
        searchTransportService.search(action, listener);
    }
}

class SearchTransportService {
    void search(String action, ActionListener listener) {
        listener.onResponse(action);
    }
}

interface ActionListener {
    void onResponse(String response);
}
"#,
    );

    temp.write(
        "index/engine/Engine.java",
        r#"
package org.elasticsearch.index.engine;

class Engine {
    static class Index {
    }

    void index(Index operation) {
    }
}

class InternalEngine extends Engine {
    void indexDocument(Index operation) {
        index(operation);
    }
}

class ReadOnlyEngine extends Engine {
    boolean isReadOnly() {
        return true;
    }
}
"#,
    );

    init_indexed_project(&temp);
    temp
}

pub(super) fn cleanup_report(path: Option<&Path>) {
    if let Some(path) = path {
        let _ = fs::remove_file(path);
        if let Some(parent) = path.parent() {
            let _ = fs::remove_dir(parent);
        }
    }
}
