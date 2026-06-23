//! 增量同步：比较当前源码文件与已持久化 hash，再只重建变更文件。
//!
//! removed 文件也会进入 selected_paths，让索引事务清理旧节点和边，而不是只处理还能读取到的文件。

use super::*;

pub(super) fn sync_facade_database(project_root: &Path, started: Instant) -> SyncResult {
    let files_checked = existing_source_files(project_root).len();
    let changes = changed_facade_files(project_root).unwrap_or_default();
    let mut changed_file_paths = Vec::new();
    changed_file_paths.extend(changes.added.iter().cloned());
    changed_file_paths.extend(changes.modified.iter().cloned());
    changed_file_paths.extend(changes.removed.iter().cloned());

    let nodes_updated = if changed_file_paths.is_empty() {
        0
    } else {
        // 这里重用完整索引管线的“选中文件”模式，保证增量和全量产生同一种边解析结果。
        let result = index_facade_changed_files(project_root, Instant::now(), &changes);
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
    }
}

pub(super) fn changed_facade_files(project_root: &Path) -> Result<ChangedFiles, String> {
    let conn = open_facade_database(project_root)?;
    let tracked = read_facade_file_hashes(&conn)?;
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
        let content = fs::read_to_string(project_root.join(&path))
            .map_err(|err| format!("failed to read {path}: {err}"))?;
        let current_hash = hash_content(&content);
        match tracked.get(&path) {
            None => changes.added.push(path),
            Some(previous_hash) if previous_hash != &current_hash => changes.modified.push(path),
            _ => {}
        }
    }

    changes.added.sort();
    changes.modified.sort();
    changes.removed.sort();
    Ok(changes)
}

pub(super) fn read_facade_file_hashes(
    conn: &Connection,
) -> Result<HashMap<String, String>, String> {
    let mut stmt = conn
        .prepare("SELECT path, content_hash FROM files")
        .map_err(|err| format!("failed to prepare file hash query: {err}"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|err| format!("failed to query file hashes: {err}"))?;
    let mut out = HashMap::new();
    for row in rows {
        let (path, hash) = row.map_err(|err| format!("failed to read file hash row: {err}"))?;
        out.insert(path, hash);
    }
    Ok(out)
}
