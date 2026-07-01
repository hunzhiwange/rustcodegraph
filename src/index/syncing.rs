//! 增量同步：比较当前源码文件与已持久化 hash，再只重建变更文件。
//!
//! removed 文件也会进入 selected_paths，让索引事务清理旧节点和边，而不是只处理还能读取到的文件。
//!
//! 预筛：先用 DB 中已存的 size/modified_at 与磁盘 metadata 比对，size+mtime 都未变的文件
//! 直接判定未变更，跳过 read_to_string + hash，避免每次单文件 watch 同步把整个项目读进内存。
//! mtime/size 只用于“快速排除”，不用于“直接判定已变更”——stat 变了仍要读内容做 hash 确认，
//! 以正确处理 mtime 变但内容相同的情况（不归入 modified）。

use std::sync::atomic::{AtomicU64, Ordering};

use super::*;

/// 累计调用 `changed_facade_files` 时实际读内容做 hash 的文件数。
/// 用于观测预筛效果（OOM 修复）：单文件改动后本轮读盘数应远小于项目文件总数。
static FACADE_FILE_CONTENT_READS: AtomicU64 = AtomicU64::new(0);

/// 读取自进程启动以来 `changed_facade_files` 读内容做 hash 的累计文件数。
pub fn facade_file_content_reads() -> u64 {
    FACADE_FILE_CONTENT_READS.load(Ordering::Relaxed)
}

/// Compatibility helper retained for callers from the old watch memory guard.
///
/// Built-in watch sync no longer skips work based on process memory, so this
/// counter no longer increments.
#[deprecated(note = "built-in watch sync no longer skips based on process memory")]
pub fn facade_watch_memory_skips() -> u64 {
    0
}

pub(super) fn sync_facade_database(project_root: &Path, started: Instant) -> SyncResult {
    crate::utils::debug_rss("sync:start");
    let files_checked = existing_source_files(project_root).len();
    crate::utils::debug_rss("sync:after existing_source_files");
    let changes = changed_facade_files(project_root).unwrap_or_default();
    crate::utils::debug_rss("sync:after changed_facade_files");
    let mut changed_file_paths = Vec::new();
    changed_file_paths.extend(changes.added.iter().cloned());
    changed_file_paths.extend(changes.modified.iter().cloned());
    changed_file_paths.extend(changes.removed.iter().cloned());

    let nodes_updated = if changed_file_paths.is_empty() {
        0
    } else {
        // 这里重用完整索引管线的“选中文件”模式，保证增量和全量产生同一种边解析结果。
        let result = index_facade_changed_files(project_root, Instant::now(), &changes);
        crate::utils::debug_rss("sync:after index_facade_changed_files");
        result.nodes_created
    };

    SyncResult {
        files_checked,
        files_added: changes.added.len(),
        files_modified: changes.modified.len(),
        files_removed: changes.removed.len(),
        nodes_updated,
        duration_ms: started.elapsed().as_millis() as u64,
        changed_file_paths: (!changed_file_paths.is_empty()).then_some(changed_file_paths),
        memory_skipped: false,
    }
}

pub(super) fn changed_facade_files(project_root: &Path) -> Result<ChangedFiles, String> {
    let conn = open_facade_database(project_root)?;
    let tracked = read_facade_file_stats(&conn)?;
    let current_files = existing_source_files(project_root)
        .into_iter()
        .collect::<HashSet<_>>();
    let mut changes = ChangedFiles::default();

    for path in tracked.keys() {
        if !current_files.contains(path) {
            changes.removed.push(path.clone());
        }
    }

    for path in current_files {
        let stored = tracked.get(&path);

        // 预筛：已知文件且磁盘 size+mtime 与 DB 记录一致 → 直接判未变更，不读内容。
        // metadata 取不到（权限/竞态）时退回读内容确认，保证不漏判。
        if let Some(stored) = stored {
            if let Ok(metadata) = fs::metadata(project_root.join(&path)) {
                let disk_size = metadata.len() as ByteSize;
                let disk_mtime = metadata.modified().ok().map(system_time_ms);
                if disk_size == stored.size && disk_mtime == Some(stored.modified_at) {
                    continue;
                }
            }
        }

        // 新文件，或 stat 变了（含 stat 不可用）：读内容做 hash 确认。
        // mtime 变但内容相同时会在这里被正确归类为未变更。
        FACADE_FILE_CONTENT_READS.fetch_add(1, Ordering::Relaxed);
        let content = fs::read_to_string(project_root.join(&path))
            .map_err(|err| format!("failed to read {path}: {err}"))?;
        let current_hash = hash_content(&content);
        match stored {
            None => changes.added.push(path),
            Some(stored) if stored.content_hash != current_hash => changes.modified.push(path),
            _ => {}
        }
    }

    changes.added.sort();
    changes.modified.sort();
    changes.removed.sort();
    Ok(changes)
}

/// DB 中持久化的文件 stat，用于增量同步预筛。
pub(super) struct FacadeFileStat {
    pub size: ByteSize,
    pub modified_at: TimestampMs,
    pub content_hash: String,
}

pub(super) fn read_facade_file_stats(
    conn: &Connection,
) -> Result<HashMap<String, FacadeFileStat>, String> {
    let mut stmt = conn
        .prepare("SELECT path, size, modified_at, content_hash FROM files")
        .map_err(|err| format!("failed to prepare file stat query: {err}"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                FacadeFileStat {
                    size: row.get::<_, i64>(1)? as ByteSize,
                    modified_at: row.get::<_, i64>(2)? as TimestampMs,
                    content_hash: row.get::<_, String>(3)?,
                },
            ))
        })
        .map_err(|err| format!("failed to query file stats: {err}"))?;
    let mut out = HashMap::new();
    for row in rows {
        let (path, stat) = row.map_err(|err| format!("failed to read file stat row: {err}"))?;
        out.insert(path, stat);
    }
    Ok(out)
}
