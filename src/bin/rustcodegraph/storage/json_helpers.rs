//! CLI storage 层的小型 SQLite/JSON 转换辅助。
//!
//! SQLite 表里保存的是字符串、整数和 JSON 文本；这些 helper 将库层强类型枚举/
//! 可选结构在读取时还原，并给出带字段名的错误。

use std::collections::BTreeMap;

use rusqlite::Connection;

pub(crate) fn scalar_count(conn: &Connection, sql: &str) -> Result<u64, String> {
    // 调用方只传内部固定 SQL；把 SQL 放进错误里可以快速定位是哪一个计数失败。
    conn.query_row(sql, [], |row| row.get::<_, i64>(0))
        .map(|count| count as u64)
        .map_err(|err| format!("failed to run count query `{sql}`: {err}"))
}

pub(crate) fn grouped_counts(
    conn: &Connection,
    sql: &str,
) -> Result<BTreeMap<String, u64>, String> {
    // BTreeMap 保证 status/json 输出稳定排序，便于测试和人工 diff。
    let mut stmt = conn
        .prepare(sql)
        .map_err(|err| format!("failed to prepare grouped count query: {err}"))?;
    let mut rows = stmt
        .query([])
        .map_err(|err| format!("failed to query grouped counts: {err}"))?;
    let mut out = BTreeMap::new();
    while let Some(row) = rows
        .next()
        .map_err(|err| format!("failed to read grouped count row: {err}"))?
    {
        let key: String = row.get(0).map_err(|err| err.to_string())?;
        let count: i64 = row.get(1).map_err(|err| err.to_string())?;
        out.insert(key, count as u64);
    }
    Ok(out)
}

pub(crate) fn parse_enum<T>(raw: &str, field: &str) -> Result<T, String>
where
    T: serde::de::DeserializeOwned,
{
    // 从 DB 字符串走 serde，保持 Rust/JSON/SQLite 三端命名一致。
    serde_json::from_value(serde_json::Value::String(raw.to_owned()))
        .map_err(|err| format!("invalid {field} value `{raw}`: {err}"))
}

pub(crate) fn parse_json_optional<T>(raw: Option<String>, field: &str) -> Result<Option<T>, String>
where
    T: serde::de::DeserializeOwned,
{
    raw.map(|value| {
        serde_json::from_str(&value).map_err(|err| format!("failed to parse {field}: {err}"))
    })
    .transpose()
}
