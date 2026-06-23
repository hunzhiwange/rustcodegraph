//! 从本机 Claude/Codex 兼容 JSONL 会话记录里抽取最近一次 agent-eval 摘要。
//!
//! 该报告主要用于人工复盘：看主线程和 subagent 是否真的使用了 rustcodegraph，
//! 以及 Read/Grep/Bash 是否仍在泄漏到本应由图工具回答的流程问题里。

use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;

use super::formatting::{count_explore_tools, count_named, format_counts, k_tokens};
use super::parser::{jsonl_files, sum_tokens, tally_tool_counts};
use super::types::TokenTotals;

pub fn parse_session_report(project_dir: &Path) -> Result<String, String> {
    let home = std::env::var_os("HOME").ok_or_else(|| "HOME is not set".to_string())?;
    parse_session_report_with_home(project_dir, Path::new(&home))
}

#[doc(hidden)]
pub fn parse_session_report_with_home(project_dir: &Path, home: &Path) -> Result<String, String> {
    let real = fs::canonicalize(project_dir)
        .map_err(|err| format!("failed to resolve {}: {err}", project_dir.display()))?;
    // Claude 会把绝对项目路径编码进 ~/.claude/projects；这里必须复刻它的斜杠替换规则。
    let escaped = real.to_string_lossy().replace('/', "-");
    let project_log_dir = home.join(".claude").join("projects").join(escaped);
    if !project_log_dir.exists() {
        return Err(format!("no session logs at {}", project_log_dir.display()));
    }

    let mut sessions = Vec::new();
    for entry in fs::read_dir(&project_log_dir)
        .map_err(|err| format!("failed to read {}: {err}", project_log_dir.display()))?
    {
        let entry = entry.map_err(|err| err.to_string())?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("jsonl") {
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_millis())
            .unwrap_or(0);
        sessions.push((path, modified));
    }
    sessions.sort_by_key(|session| Reverse(session.1));
    // 只取最近一次主会话；历史会话混在一起会把一次评测的工具计数放大。
    let Some((main_file, _)) = sessions.first() else {
        return Err(format!(
            "no .jsonl sessions in {}",
            project_log_dir.display()
        ));
    };
    let session_id = main_file
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| "session file name is not UTF-8".to_string())?
        .to_string();

    let main_counts = tally_tool_counts(main_file)?;
    let mut sub_counts: BTreeMap<String, usize> = BTreeMap::new();
    let sub_dir = project_log_dir.join(&session_id).join("subagents");
    let mut sub_agent_files = 0usize;
    if sub_dir.exists() {
        // subagent transcript 独立落盘；合并它们才能得到一次任务真实的 Read/Grep 泄漏。
        for file in jsonl_files(&sub_dir)? {
            sub_agent_files += 1;
            for (name, count) in tally_tool_counts(&file)? {
                *sub_counts.entry(name).or_default() += count;
            }
        }
    }

    let explore = count_explore_tools(&main_counts) + count_explore_tools(&sub_counts);
    let reads = count_named(&main_counts, "Read") + count_named(&sub_counts, "Read");
    // 很多 grep 是通过 Bash 包装执行的，所以这里把 Bash 计入“文本检索回退”信号。
    let greps = count_named(&main_counts, "Grep")
        + count_named(&sub_counts, "Grep")
        + count_named(&main_counts, "Bash")
        + count_named(&sub_counts, "Bash");

    let mut tokens = TokenTotals::default();
    tokens.add(sum_tokens(main_file)?);
    if sub_dir.exists() {
        for file in jsonl_files(&sub_dir)? {
            tokens.add(sum_tokens(&file)?);
        }
    }

    let mut out = String::new();
    writeln!(out, "session: {session_id}").unwrap();
    writeln!(out, "\nMAIN thread tools:\n{}", format_counts(&main_counts)).unwrap();
    writeln!(
        out,
        "\nSUBAGENT tools ({sub_agent_files} subagent transcript{}):\n{}",
        if sub_agent_files == 1 { "" } else { "s" },
        format_counts(&sub_counts)
    )
    .unwrap();
    writeln!(
        out,
        "\nVERDICT: rustcodegraph_explore used {explore}x | Read {reads} | Grep/Bash {greps}"
    )
    .unwrap();
    writeln!(
        out,
        "TOKENS: gen {} | fresh-in {} | cached-in {} | billable≈ {}",
        k_tokens(tokens.generated),
        k_tokens(tokens.fresh),
        k_tokens(tokens.cached),
        // 近似账单口径只算生成和 fresh input；cached input 单独展示，避免误读 token 节省。
        k_tokens(tokens.generated + tokens.fresh)
    )
    .unwrap();
    Ok(out)
}
