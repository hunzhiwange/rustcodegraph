//! Read/query helpers over the lightweight SQLite schema.
//!
//! 这些查询服务 CLI 和小型 MCP shim，优先稳定、可解释和低依赖；复杂排序、FTS
//! 与完整图遍历仍由库层 db/graph 模块承担。

use std::collections::{BTreeSet, HashSet};

use rusqlite::{Connection, params};
use rustcodegraph::types::FileRecord;

use super::json_helpers::{parse_enum, parse_json_optional};
use super::source::normalize_slashes;
use super::{EdgeDirection, QueryMatch};

pub(crate) fn read_files(
    conn: &Connection,
    filter: Option<&str>,
) -> Result<Vec<FileRecord>, String> {
    // 过滤在 Rust 端做，语义是“前缀或包含”；这比暴露 SQL LIKE 细节更符合 CLI 使用。
    let mut stmt = conn
        .prepare(
            r#"
            SELECT path, content_hash, language, size, modified_at, indexed_at, node_count, errors
            FROM files
            ORDER BY path
            "#,
        )
        .map_err(|err| format!("failed to prepare files query: {err}"))?;
    let mut rows = stmt
        .query([])
        .map_err(|err| format!("failed to query files: {err}"))?;
    let mut out = Vec::new();
    while let Some(row) = rows
        .next()
        .map_err(|err| format!("failed to read file row: {err}"))?
    {
        let path: String = row.get(0).map_err(|err| err.to_string())?;
        if !filter
            .map(|filter| path.starts_with(filter) || path.contains(filter))
            .unwrap_or(true)
        {
            continue;
        }
        let language_raw: String = row.get(2).map_err(|err| err.to_string())?;
        let errors_raw: Option<String> = row.get(7).map_err(|err| err.to_string())?;
        out.push(FileRecord {
            path,
            content_hash: row.get(1).map_err(|err| err.to_string())?,
            language: parse_enum(&language_raw, "language")?,
            size: row.get::<_, i64>(3).map_err(|err| err.to_string())? as u64,
            modified_at: row.get(4).map_err(|err| err.to_string())?,
            indexed_at: row.get(5).map_err(|err| err.to_string())?,
            node_count: row.get::<_, i64>(6).map_err(|err| err.to_string())? as u64,
            errors: parse_json_optional(errors_raw, "errors")?,
        });
    }
    Ok(out)
}

pub(crate) fn query_nodes(
    conn: &Connection,
    search: &str,
    kind_filter: Option<&str>,
    limit: usize,
) -> Result<Vec<QueryMatch>, String> {
    // 使用 lower LIKE 做宽松搜索，避免 CLI 用户必须记住 FTS tokenization 规则。
    let needle = format!("%{}%", search.to_ascii_lowercase());
    let mut sql = String::from(
        r#"
        SELECT id, kind, name, qualified_name, file_path, start_line, signature
        FROM nodes
        WHERE (lower(name) LIKE ?1 OR lower(qualified_name) LIKE ?1 OR lower(file_path) LIKE ?1)
        "#,
    );
    if kind_filter.is_some() {
        sql.push_str(" AND kind = ?2");
    }
    sql.push_str(" ORDER BY file_path, start_line, name LIMIT ");
    sql.push_str(&limit.to_string());

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|err| format!("failed to prepare query: {err}"))?;
    let mut rows = match kind_filter {
        Some(kind) => stmt
            .query(params![needle, kind])
            .map_err(|err| format!("failed to query nodes: {err}"))?,
        None => stmt
            .query(params![needle])
            .map_err(|err| format!("failed to query nodes: {err}"))?,
    };

    let mut out = Vec::new();
    while let Some(row) = rows
        .next()
        .map_err(|err| format!("failed to read query row: {err}"))?
    {
        out.push(QueryMatch {
            id: row.get(0).map_err(|err| err.to_string())?,
            kind: row.get(1).map_err(|err| err.to_string())?,
            name: row.get(2).map_err(|err| err.to_string())?,
            qualified_name: row.get(3).map_err(|err| err.to_string())?,
            file_path: row.get(4).map_err(|err| err.to_string())?,
            start_line: row.get::<_, i64>(5).map_err(|err| err.to_string())? as u64,
            signature: row.get(6).map_err(|err| err.to_string())?,
        });
    }
    Ok(out)
}

pub(crate) fn find_symbol_nodes(
    conn: &Connection,
    symbol: &str,
) -> Result<Vec<QueryMatch>, String> {
    let exact = symbol.to_ascii_lowercase();
    // 先精确匹配 name/qualified_name；只有找不到时才退到模糊搜索，降低重名噪音。
    let exact_matches = lookup_symbol_nodes(
        conn,
        r#"
            SELECT id, kind, name, qualified_name, file_path, start_line, signature
            FROM nodes
            WHERE lower(name) = ?1
               OR lower(qualified_name) = ?1
            ORDER BY file_path, start_line
            LIMIT 50
            "#,
        &exact,
    )?;
    if !exact_matches.is_empty() {
        return Ok(exact_matches);
    }

    let fuzzy = format!("%{exact}%");
    let mut stmt = conn
        .prepare(
            r#"
            SELECT id, kind, name, qualified_name, file_path, start_line, signature
            FROM nodes
            WHERE lower(name) LIKE ?1
               OR lower(qualified_name) LIKE ?1
            ORDER BY file_path, start_line
            LIMIT 50
            "#,
        )
        .map_err(|err| format!("failed to prepare fuzzy symbol lookup: {err}"))?;
    let mut rows = stmt
        .query(params![fuzzy])
        .map_err(|err| format!("failed to query fuzzy symbol lookup: {err}"))?;
    let mut out = Vec::new();
    while let Some(row) = rows
        .next()
        .map_err(|err| format!("failed to read fuzzy symbol lookup row: {err}"))?
    {
        out.push(QueryMatch {
            id: row.get(0).map_err(|err| err.to_string())?,
            kind: row.get(1).map_err(|err| err.to_string())?,
            name: row.get(2).map_err(|err| err.to_string())?,
            qualified_name: row.get(3).map_err(|err| err.to_string())?,
            file_path: row.get(4).map_err(|err| err.to_string())?,
            start_line: row.get::<_, i64>(5).map_err(|err| err.to_string())? as u64,
            signature: row.get(6).map_err(|err| err.to_string())?,
        });
    }
    if out.is_empty() {
        query_nodes(conn, symbol, None, 50)
    } else {
        Ok(out)
    }
}

fn lookup_symbol_nodes(
    conn: &Connection,
    sql: &str,
    needle: &str,
) -> Result<Vec<QueryMatch>, String> {
    let mut stmt = conn
        .prepare(sql)
        .map_err(|err| format!("failed to prepare symbol lookup: {err}"))?;
    let mut rows = stmt
        .query(params![needle])
        .map_err(|err| format!("failed to query symbol lookup: {err}"))?;
    let mut out = Vec::new();
    while let Some(row) = rows
        .next()
        .map_err(|err| format!("failed to read symbol lookup row: {err}"))?
    {
        out.push(QueryMatch {
            id: row.get(0).map_err(|err| err.to_string())?,
            kind: row.get(1).map_err(|err| err.to_string())?,
            name: row.get(2).map_err(|err| err.to_string())?,
            qualified_name: row.get(3).map_err(|err| err.to_string())?,
            file_path: row.get(4).map_err(|err| err.to_string())?,
            start_line: row.get::<_, i64>(5).map_err(|err| err.to_string())? as u64,
            signature: row.get(6).map_err(|err| err.to_string())?,
        });
    }
    Ok(out)
}

pub(crate) fn edge_matches_for_symbol(
    conn: &Connection,
    symbol: &str,
    direction: EdgeDirection,
    depth: usize,
    limit: usize,
) -> Result<Vec<QueryMatch>, String> {
    // callers/callees/impact 只沿 calls 边 BFS；depth 控制扩散层数，limit 控制返回体量。
    let seeds = find_symbol_nodes(conn, symbol)?;
    if seeds.is_empty() || depth == 0 || limit == 0 {
        return Ok(Vec::new());
    }

    let mut seen = seeds
        .iter()
        .map(|seed| seed.id.clone())
        .collect::<HashSet<_>>();
    let mut frontier = seeds.iter().map(|seed| seed.id.clone()).collect::<Vec<_>>();
    let mut out = Vec::new();

    for _ in 0..depth {
        let mut next_frontier = Vec::new();
        for node_id in &frontier {
            for node in edge_neighbors(conn, node_id, direction)? {
                if !seen.insert(node.id.clone()) {
                    continue;
                }
                next_frontier.push(node.id.clone());
                out.push(node);
                if out.len() >= limit {
                    return Ok(out);
                }
            }
        }
        if next_frontier.is_empty() {
            break;
        }
        frontier = next_frontier;
    }

    Ok(out)
}

fn edge_neighbors(
    conn: &Connection,
    node_id: &str,
    direction: EdgeDirection,
) -> Result<Vec<QueryMatch>, String> {
    // Incoming/Outgoing 只换 JOIN 的方向，返回统一 QueryMatch，方便上层格式化复用。
    let sql = match direction {
        EdgeDirection::Incoming => {
            r#"
            SELECT DISTINCT n.id, n.kind, n.name, n.qualified_name, n.file_path, n.start_line, n.signature
            FROM edges e
            JOIN nodes n ON n.id = e.source
            WHERE e.target = ?1 AND e.kind = 'calls'
            ORDER BY n.file_path, n.start_line, n.name
            "#
        }
        EdgeDirection::Outgoing => {
            r#"
            SELECT DISTINCT n.id, n.kind, n.name, n.qualified_name, n.file_path, n.start_line, n.signature
            FROM edges e
            JOIN nodes n ON n.id = e.target
            WHERE e.source = ?1 AND e.kind = 'calls'
            ORDER BY n.file_path, n.start_line, n.name
            "#
        }
    };
    let mut stmt = conn
        .prepare(sql)
        .map_err(|err| format!("failed to prepare edge traversal: {err}"))?;
    let mut rows = stmt
        .query(params![node_id])
        .map_err(|err| format!("failed to query edge traversal: {err}"))?;
    let mut out = Vec::new();
    while let Some(row) = rows
        .next()
        .map_err(|err| format!("failed to read edge traversal row: {err}"))?
    {
        out.push(QueryMatch {
            id: row.get(0).map_err(|err| err.to_string())?,
            kind: row.get(1).map_err(|err| err.to_string())?,
            name: row.get(2).map_err(|err| err.to_string())?,
            qualified_name: row.get(3).map_err(|err| err.to_string())?,
            file_path: row.get(4).map_err(|err| err.to_string())?,
            start_line: row.get::<_, i64>(5).map_err(|err| err.to_string())? as u64,
            signature: row.get(6).map_err(|err| err.to_string())?,
        });
    }
    Ok(out)
}

pub(crate) fn affected_files_for_changes(
    conn: &Connection,
    changed_files: &[String],
    depth: usize,
) -> Result<Vec<String>, String> {
    // 结果先包含输入文件自身，再沿 incoming calls 找依赖者；测试过滤在命令层完成。
    let mut frontier = Vec::new();
    let mut affected_paths = BTreeSet::new();
    for file in changed_files {
        let normalized = normalize_slashes(file.trim_start_matches("./"));
        affected_paths.insert(normalized.clone());
        for node in nodes_for_file(conn, &normalized)? {
            frontier.push(node.id);
        }
    }
    let mut seen = frontier.iter().cloned().collect::<HashSet<_>>();
    for _ in 0..depth {
        let mut next = Vec::new();
        for node_id in &frontier {
            for node in edge_neighbors(conn, node_id, EdgeDirection::Incoming)? {
                affected_paths.insert(node.file_path.clone());
                if seen.insert(node.id.clone()) {
                    next.push(node.id);
                }
            }
        }
        if next.is_empty() {
            break;
        }
        frontier = next;
    }
    Ok(affected_paths.into_iter().collect())
}

pub(crate) fn nodes_for_file(conn: &Connection, file: &str) -> Result<Vec<QueryMatch>, String> {
    // 允许传绝对/相对尾缀，支持 git diff 输出和用户手写路径两种来源。
    let suffix = format!("%{}", normalize_slashes(file));
    let mut stmt = conn
        .prepare(
            r#"
            SELECT id, kind, name, qualified_name, file_path, start_line, signature
            FROM nodes
            WHERE file_path = ?1 OR file_path LIKE ?2
            ORDER BY file_path, start_line
            "#,
        )
        .map_err(|err| format!("failed to prepare file node lookup: {err}"))?;
    let mut rows = stmt
        .query(params![file, suffix])
        .map_err(|err| format!("failed to query file nodes: {err}"))?;
    let mut out = Vec::new();
    while let Some(row) = rows
        .next()
        .map_err(|err| format!("failed to read file node row: {err}"))?
    {
        out.push(QueryMatch {
            id: row.get(0).map_err(|err| err.to_string())?,
            kind: row.get(1).map_err(|err| err.to_string())?,
            name: row.get(2).map_err(|err| err.to_string())?,
            qualified_name: row.get(3).map_err(|err| err.to_string())?,
            file_path: row.get(4).map_err(|err| err.to_string())?,
            start_line: row.get::<_, i64>(5).map_err(|err| err.to_string())? as u64,
            signature: row.get(6).map_err(|err| err.to_string())?,
        });
    }
    Ok(out)
}
