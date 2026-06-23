//! Release-note helpers owned by the Rust binary.
//!
//! These functions are the Rust port of the former `prepare-release.mjs` and
//! `extract-release-notes.mjs` scripts. They intentionally preserve the old
//! changelog line-oriented behavior so release reruns stay idempotent.
//!
//! 这里刻意不用 Markdown AST：发布流程依赖现有 CHANGELOG 的空行、链接引用和
//! 小节顺序尽量原样保留，行级处理更容易做到幂等。

use std::fs;
use std::path::Path;
use std::sync::LazyLock;
use std::time::{SystemTime, UNIX_EPOCH};

use regex::Regex;
use serde_json::Value;

static VERSION_HEADER_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^## \[([^\]]+)\](?:\s+-\s+(.+))?\s*$").unwrap());
static SUBSECTION_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^### (\w+)\s*$").unwrap());
static BULLET_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*([-*]|\d+\.)\s+").unwrap());
static FENCE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*```").unwrap());
static LIST_ITEM_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\s*)([-*]|\d+\.)\s+").unwrap());

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrepareReleaseReport {
    pub version: String,
    pub changed: bool,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrepareChangelogResult {
    pub text: String,
    pub report: PrepareReleaseReport,
}

#[derive(Debug, Clone)]
struct ParsedChangelog {
    preface: Vec<String>,
    blocks: Vec<ChangelogBlock>,
}

#[derive(Debug, Clone)]
struct ChangelogBlock {
    header: String,
    name: String,
    body: Vec<String>,
}

#[derive(Debug, Clone)]
struct SplitSubsections {
    leading: Vec<String>,
    subs: Vec<Subsection>,
}

#[derive(Debug, Clone)]
struct Subsection {
    heading: String,
    header_line: String,
    body: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
struct StackFrame {
    indent: usize,
}

pub fn prepare_release_in_dir(
    cwd: &Path,
    version: Option<&str>,
) -> Result<PrepareReleaseReport, String> {
    // CLI 入口只负责读写文件；真正的可测试逻辑放在 `prepare_changelog`。
    let version = match version {
        Some(version) => version.to_owned(),
        None => read_package_version(cwd)?,
    };
    let changelog_path = cwd.join("CHANGELOG.md");
    let text = fs::read_to_string(&changelog_path)
        .map_err(|err| format!("failed to read {}: {err}", changelog_path.display()))?;
    let result = prepare_changelog(&text, &version, &today_utc_iso_date());
    if result.report.changed {
        fs::write(&changelog_path, &result.text)
            .map_err(|err| format!("failed to write {}: {err}", changelog_path.display()))?;
    }
    Ok(result.report)
}

pub fn prepare_changelog(text: &str, version: &str, today: &str) -> PrepareChangelogResult {
    let mut parsed = parse_changelog(text);
    let Some(unrel_idx) = parsed
        .blocks
        .iter()
        .position(|block| block.name == "Unreleased")
    else {
        return PrepareChangelogResult {
            text: text.to_owned(),
            report: PrepareReleaseReport {
                version: version.to_owned(),
                changed: false,
                summary: "prepare-release: no [Unreleased] block - nothing to do".to_owned(),
            },
        };
    };

    let ver_idx = parsed.blocks.iter().position(|block| block.name == version);
    let unrel = parsed.blocks[unrel_idx].clone();

    if !block_has_content(&unrel.body) {
        return PrepareChangelogResult {
            text: text.to_owned(),
            report: PrepareReleaseReport {
                version: version.to_owned(),
                changed: false,
                summary: "prepare-release: [Unreleased] is empty - nothing to do".to_owned(),
            },
        };
    }

    if ver_idx.is_none() {
        // 常规路径：把 Unreleased 原样提升为带日期的版本块，再留下空的
        // Unreleased，供下一轮开发继续追加。
        let promoted = ChangelogBlock {
            header: format!("## [{version}] - {today}"),
            name: version.to_owned(),
            body: append_one_blank(trim_trailing_blank(&unrel.body)),
        };
        let emptied = ChangelogBlock {
            header: "## [Unreleased]".to_owned(),
            name: "Unreleased".to_owned(),
            body: vec!["".to_owned(), "".to_owned()],
        };
        parsed
            .blocks
            .splice(unrel_idx..=unrel_idx, [emptied, promoted]);
        let next = append_link_ref(&join_changelog(&parsed), version);
        return PrepareChangelogResult {
            text: next,
            report: PrepareReleaseReport {
                version: version.to_owned(),
                changed: true,
                summary: format!(
                    "prepare-release: {version} - renamed [Unreleased] to [{version}] - {today}"
                ),
            },
        };
    }

    let ver_idx = ver_idx.expect("checked above");
    let unrel_subs = split_subsections(&parsed.blocks[unrel_idx].body);
    let mut ver_subs = split_subsections(&parsed.blocks[ver_idx].body);

    // 兼容少见的重跑/手工预建版本块场景：把 Unreleased 的小节并入已有版本块。
    // 注意 SUBSECTION_RE 只匹配单词 heading，这是历史脚本行为，不能顺手扩大。
    let mut merged = 0usize;
    for us in unrel_subs.subs {
        let us_body = trim_trailing_blank(&us.body);
        if us_body.is_empty() {
            continue;
        }

        if let Some(target) = ver_subs
            .subs
            .iter_mut()
            .find(|subsection| subsection.heading == us.heading)
        {
            let existing = trim_trailing_blank(&target.body);
            let mut next = existing.clone();
            if !existing.is_empty() && existing.last().is_some_and(|line| !line.trim().is_empty()) {
                next.push(String::new());
            }
            next.extend(us_body.iter().cloned());
            next.push(String::new());
            target.body = next;
        } else {
            ver_subs.subs.push(Subsection {
                heading: us.heading,
                header_line: us.header_line,
                body: append_one_blank(us_body.clone()),
            });
        }

        merged += us_body
            .iter()
            .filter(|line| BULLET_RE.is_match(line))
            .count();
    }

    parsed.blocks[ver_idx].body = rebuild_body(ver_subs);
    parsed.blocks[unrel_idx].body = vec!["".to_owned(), "".to_owned()];

    let next = append_link_ref(&join_changelog(&parsed), version);
    PrepareChangelogResult {
        text: next,
        report: PrepareReleaseReport {
            version: version.to_owned(),
            changed: true,
            summary: format!(
                "prepare-release: {version} - merged {merged} Unreleased entries into existing [{version}] block"
            ),
        },
    }
}

pub fn extract_release_notes_in_dir(cwd: &Path, version: &str) -> Result<String, String> {
    let changelog_path = cwd.join("CHANGELOG.md");
    let text = fs::read_to_string(&changelog_path)
        .map_err(|err| format!("failed to read {}: {err}", changelog_path.display()))?;
    extract_release_notes_from_changelog(&text, version)
}

pub fn extract_release_notes_from_changelog(text: &str, version: &str) -> Result<String, String> {
    // GitHub Release 只需要目标版本块，不包含后面的链接引用或其它版本内容。
    let escaped = regex::escape(version);
    let header_re = Regex::new(&format!(r"^## \[{escaped}\]"))
        .map_err(|err| format!("invalid version regex for {version}: {err}"))?;
    let any_header_re = Regex::new(r"^## \[").expect("release header regex should compile");
    let lines: Vec<String> = text.split('\n').map(ToOwned::to_owned).collect();
    let Some(start) = lines.iter().position(|line| header_re.is_match(line)) else {
        return Err(format!("no '## [{version}]' entry found in CHANGELOG.md"));
    };
    let after = lines
        .iter()
        .enumerate()
        .find(|(idx, line)| *idx > start && any_header_re.is_match(line))
        .map(|(idx, _)| idx)
        .unwrap_or(lines.len());
    Ok(unwrap_release_note_lines(&lines[start..after]))
}

pub fn extract_release_notes_from_stdin_text(text: &str) -> String {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<String> = normalized.split('\n').map(ToOwned::to_owned).collect();
    unwrap_release_note_lines(&lines)
}

pub fn unwrap_release_note_lines(block: &[String]) -> String {
    // 发布说明需要把 Markdown 列表项中的软换行合并成一行，但必须保留代码块、
    // heading 和列表层级。这里用缩进栈模拟旧脚本的行为。
    let mut out = Vec::<String>::new();
    let mut buf = String::new();
    let mut stack = Vec::<StackFrame>::new();
    let mut in_fence = false;

    for line in block {
        if FENCE_RE.is_match(line) {
            flush_buf(&mut out, &mut buf);
            stack.clear();
            out.push(line.clone());
            in_fence = !in_fence;
            continue;
        }

        if in_fence {
            out.push(line.clone());
            continue;
        }

        if line.trim().is_empty() {
            flush_buf(&mut out, &mut buf);
            out.push(String::new());
            continue;
        }

        if line.starts_with('#') {
            flush_buf(&mut out, &mut buf);
            stack.clear();
            out.push(line.clone());
            continue;
        }

        if let Some(caps) = LIST_ITEM_RE.captures(line) {
            flush_buf(&mut out, &mut buf);
            let indent = caps.get(1).map_or(0, |m| m.as_str().len());
            // 新 list item 会结束当前层级及更深层级的缓冲，避免两个 sibling 被拼接。
            while stack.last().is_some_and(|frame| frame.indent >= indent) {
                stack.pop();
            }
            stack.push(StackFrame { indent });
            buf = line.clone();
            continue;
        }

        if line.chars().next().is_some_and(|ch| ch.is_whitespace()) {
            let indent = leading_spaces(line);
            // 缩进文本延续当前 list item；如果缩进退回到父层，先 flush 子项。
            while stack.len() > 1 && stack.last().is_some_and(|frame| frame.indent >= indent) {
                flush_buf(&mut out, &mut buf);
                stack.pop();
            }
            let trimmed = line.trim_start();
            if buf.is_empty() {
                buf = trimmed.to_owned();
            } else {
                buf.push(' ');
                buf.push_str(trimmed);
            }
            continue;
        }

        flush_buf(&mut out, &mut buf);
        stack.clear();
        out.push(line.clone());
    }
    flush_buf(&mut out, &mut buf);

    let mut result = out.join("\n");
    if !result.ends_with('\n') {
        result.push('\n');
    }
    result
}

fn read_package_version(cwd: &Path) -> Result<String, String> {
    let package_path = cwd.join("package.json");
    let text = fs::read_to_string(&package_path)
        .map_err(|err| format!("failed to read {}: {err}", package_path.display()))?;
    let value: Value = serde_json::from_str(&text)
        .map_err(|err| format!("failed to parse {}: {err}", package_path.display()))?;
    value
        .get("version")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| "package.json has no \"version\" field".to_owned())
}

fn parse_changelog(text: &str) -> ParsedChangelog {
    // preface 是第一个版本 heading 前的内容，通常包含标题和格式说明。
    let mut preface = Vec::<String>::new();
    let mut blocks = Vec::<ChangelogBlock>::new();
    let mut cur: Option<ChangelogBlock> = None;

    for line in text.split('\n') {
        if let Some(caps) = VERSION_HEADER_RE.captures(line) {
            if let Some(block) = cur.take() {
                blocks.push(block);
            }
            cur = Some(ChangelogBlock {
                header: line.to_owned(),
                name: caps
                    .get(1)
                    .map(|m| m.as_str())
                    .unwrap_or_default()
                    .to_owned(),
                body: Vec::new(),
            });
        } else if let Some(block) = cur.as_mut() {
            block.body.push(line.to_owned());
        } else {
            preface.push(line.to_owned());
        }
    }

    if let Some(block) = cur {
        blocks.push(block);
    }

    ParsedChangelog { preface, blocks }
}

fn join_changelog(parsed: &ParsedChangelog) -> String {
    let mut parts = vec![parsed.preface.join("\n")];
    for block in &parsed.blocks {
        let mut lines = vec![block.header.clone()];
        lines.extend(block.body.iter().cloned());
        parts.push(lines.join("\n"));
    }
    parts.join("\n")
}

fn split_subsections(body: &[String]) -> SplitSubsections {
    // 只服务 prepare-release 的“合并已有版本块”分支；保持历史单词 heading 规则。
    let mut leading = Vec::<String>::new();
    let mut subs = Vec::<Subsection>::new();
    let mut cur: Option<Subsection> = None;

    for line in body {
        if let Some(caps) = SUBSECTION_RE.captures(line) {
            if let Some(subsection) = cur.take() {
                subs.push(subsection);
            }
            cur = Some(Subsection {
                heading: caps
                    .get(1)
                    .map(|m| m.as_str())
                    .unwrap_or_default()
                    .to_owned(),
                header_line: line.clone(),
                body: Vec::new(),
            });
        } else if let Some(subsection) = cur.as_mut() {
            subsection.body.push(line.clone());
        } else {
            leading.push(line.clone());
        }
    }

    if let Some(subsection) = cur {
        subs.push(subsection);
    }

    SplitSubsections { leading, subs }
}

fn rebuild_body(sections: SplitSubsections) -> Vec<String> {
    let mut parts = Vec::<String>::new();
    if !sections.leading.is_empty() {
        parts.push(sections.leading.join("\n"));
    }
    for subsection in sections.subs {
        let mut lines = vec![subsection.header_line];
        lines.extend(subsection.body);
        parts.push(lines.join("\n"));
    }
    parts
        .join("\n")
        .split('\n')
        .map(ToOwned::to_owned)
        .collect()
}

fn block_has_content(body: &[String]) -> bool {
    body.iter().any(|line| BULLET_RE.is_match(line))
}

fn trim_trailing_blank(lines: &[String]) -> Vec<String> {
    let mut end = lines.len();
    while end > 0 && lines[end - 1].trim().is_empty() {
        end -= 1;
    }
    lines[..end].to_vec()
}

fn append_one_blank(mut lines: Vec<String>) -> Vec<String> {
    lines.push(String::new());
    lines
}

fn append_link_ref(text: &str, version: &str) -> String {
    // release helper 负责追加版本链接引用；CHANGELOG 条目本身不要手写这一行。
    let ref_line = format!(
        "[{version}]: https://github.com/hunzhiwange/rustcodegraph/releases/tag/v{version}"
    );
    if text.split('\n').any(|line| line.trim() == ref_line) {
        return text.to_owned();
    }
    let mut out = text.to_owned();
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(&ref_line);
    out.push('\n');
    out
}

fn flush_buf(out: &mut Vec<String>, buf: &mut String) {
    if !buf.is_empty() {
        out.push(std::mem::take(buf));
    }
}

fn leading_spaces(s: &str) -> usize {
    s.chars().take_while(|ch| ch.is_whitespace()).count()
}

pub fn today_utc_iso_date() -> String {
    // 不引入 chrono 依赖：发布 helper 只需要 UTC 日期，下面用纯整数算法转换。
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);
    let days = seconds / 86_400;
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

fn civil_from_days(days_since_unix_epoch: i64) -> (i64, i64, i64) {
    // Howard Hinnant 的 civil-from-days 算法，适合从 Unix epoch 天数得到公历日期。
    let z = days_since_unix_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    if month <= 2 {
        year += 1;
    }
    (year, month, day)
}
