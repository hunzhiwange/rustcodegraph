//! CLI 和 MCP status 输出使用的状态/统计查询。
//!
//! 统计只读 SQLite 聚合结果，不触发 sync；是否过期由 MCP handler 额外加 stale notice。

use rusqlite::{Connection, OptionalExtension};
use rustcodegraph::types::TimestampMs;

use super::{
    SqliteStats,
    json_helpers::{grouped_counts, scalar_count},
};

pub(crate) fn read_sqlite_stats(conn: &Connection) -> Result<SqliteStats, String> {
    // 所有 count 聚合保持独立 SQL，失败时错误能指向具体表或分组。
    let node_count = scalar_count(conn, "SELECT COUNT(*) FROM nodes")?;
    let edge_count = scalar_count(conn, "SELECT COUNT(*) FROM edges")?;
    let file_count = scalar_count(conn, "SELECT COUNT(*) FROM files")?;
    let last_updated = read_last_indexed_at(conn)?.unwrap_or(0);
    Ok(SqliteStats {
        node_count,
        edge_count,
        file_count,
        nodes_by_kind: grouped_counts(conn, "SELECT kind, COUNT(*) FROM nodes GROUP BY kind")?,
        files_by_language: grouped_counts(
            conn,
            "SELECT language, COUNT(*) FROM files GROUP BY language",
        )?,
        last_updated,
    })
}

pub(crate) fn format_mcp_status(stats: &SqliteStats) -> String {
    // Markdown 输出和主 MCP server 的 status 风格接近，方便 agent 直接扫读。
    let mut lines = vec![
        "## RustCodeGraph Status".to_owned(),
        String::new(),
        format!("**Files indexed:** {}", stats.file_count),
        format!("**Total nodes:** {}", stats.node_count),
        format!("**Total edges:** {}", stats.edge_count),
        "**Backend:** sqlite".to_owned(),
    ];

    lines.push(String::new());
    lines.push("### Nodes by Kind:".to_owned());
    for (kind, count) in &stats.nodes_by_kind {
        if *count > 0 {
            lines.push(format!("- {kind}: {count}"));
        }
    }

    lines.push(String::new());
    lines.push("### Languages:".to_owned());
    for (language, count) in &stats.files_by_language {
        if *count > 0 {
            lines.push(format!("- {language}: {count}"));
        }
    }

    lines.join("\n")
}

pub(crate) fn read_last_indexed_at(conn: &Connection) -> Result<Option<TimestampMs>, String> {
    conn.query_row("SELECT MAX(indexed_at) FROM files", [], |row| {
        row.get::<_, Option<TimestampMs>>(0)
    })
    .optional()
    .map(|value| value.flatten())
    .map_err(|err| format!("failed to read last indexed time: {err}"))
}

pub(crate) fn unix_ms_to_iso(ms: TimestampMs) -> String {
    // 避免为一个 UTC 格式化函数引入 chrono；算法来自常见 civil_from_days 转换。
    let seconds = ms.div_euclid(1000);
    let millis = ms.rem_euclid(1000);
    let days = seconds.div_euclid(86_400);
    let seconds_of_day = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;

    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z")
}

fn civil_from_days(days_since_unix_epoch: i64) -> (i64, i64, i64) {
    // 公历日期换算，覆盖文件 mtime 会用到的 Unix timestamp 范围。
    let z = days_since_unix_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 }.div_euclid(146_097);
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    let year = year + if month <= 2 { 1 } else { 0 };
    (year, month, day)
}
