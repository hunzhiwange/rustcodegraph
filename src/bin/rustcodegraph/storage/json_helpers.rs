//! CLI storage 层的小型 SQLite/JSON 转换辅助。
//!
//! SQLite 表里保存的是字符串、整数和 JSON 文本；这些 helper 把库层强类型枚举/
//! 可选结构转换成稳定的列值，并在读取时给出带字段名的错误。

use std::collections::BTreeMap;

use rusqlite::Connection;
use serde::Serialize;

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

pub(crate) fn enum_string<T: Serialize>(value: &T, field: &str) -> Result<String, String> {
    // types.rs 里的枚举用 serde rename 作为 DB 字符串；这里防止未来枚举改成对象形态。
    match serde_json::to_value(value).map_err(|err| err.to_string())? {
        serde_json::Value::String(value) => Ok(value),
        other => Err(format!("serialized {field} enum was not a string: {other}")),
    }
}

pub(crate) fn parse_enum<T>(raw: &str, field: &str) -> Result<T, String>
where
    T: serde::de::DeserializeOwned,
{
    // 与 enum_string 对称：从 DB 字符串走 serde，保持 Rust/JSON/SQLite 三端命名一致。
    serde_json::from_value(serde_json::Value::String(raw.to_owned()))
        .map_err(|err| format!("invalid {field} value `{raw}`: {err}"))
}

pub(crate) fn json_option<T: Serialize>(value: Option<&T>) -> Result<Option<String>, String> {
    // None 写成 SQL NULL，不写成 "null"，这样旧查询可以继续用 IS NULL 判断。
    value
        .map(serde_json::to_string)
        .transpose()
        .map_err(|err| err.to_string())
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

pub(crate) fn bool_int(value: Option<bool>) -> i64 {
    // schema 使用 INTEGER 存布尔；缺失值按 false 处理以兼容轻量索引器的保守默认。
    if value.unwrap_or(false) { 1 } else { 0 }
}
