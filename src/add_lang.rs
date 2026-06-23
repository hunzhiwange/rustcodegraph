//! Rust-owned helper commands for the add-language workflow.
//!
//! These replace the old Node/web-tree-sitter development scripts with checks
//! that use the same native tree-sitter facade as indexing.
//!
//! 本模块服务 `/add-lang` 技能的本地验证链路：先确认 native grammar
//! 能稳定解析，再把 AST 形状暴露给提取器作者，最后读取已索引的 SQLite
//! 库做轻量健康检查。这里刻意不走 MCP 或完整 agent-eval，目的是让新增语言
//! 在接入 Rust 侧 tree-sitter 管线时能快速定位“语法注册、节点映射、索引输出”
//! 中哪一层出了问题。

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::Path;

use rusqlite::{Connection, OpenFlags};

use crate::directory::get_code_graph_dir;
use crate::web_tree_sitter::{FieldTarget, Language as RuntimeLanguage, Parser, SyntaxNode};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddLangError {
    pub code: i32,
    pub message: String,
}

impl AddLangError {
    fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddLangOutput {
    pub text: String,
    pub exit_code: i32,
}

impl AddLangOutput {
    fn ok(text: String) -> Self {
        Self { text, exit_code: 0 }
    }

    fn with_code(text: String, exit_code: i32) -> Self {
        Self { text, exit_code }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DumpAstOptions {
    /// 默认只展示有限深度，避免大文件 AST 把终端刷满；`--full` 才展开全部节点。
    pub max_depth: Option<usize>,
    /// tree-sitter 的匿名标点/关键字节点通常对提取器无帮助，只有排查字段映射时才需要。
    pub show_all: bool,
}

impl Default for DumpAstOptions {
    fn default() -> Self {
        Self {
            max_depth: Some(8),
            show_all: false,
        }
    }
}

pub fn help_text() -> &'static str {
    "Usage: rustcodegraph add-lang <command>\n\nCommands:\n  check-grammar <lang> <valid-sample> [iterations]\n  dump-ast <lang> <sample-file> [--depth=N] [--full]\n  verify-extraction <repo-path> <lang>"
}

pub fn run_cli(args: &[String]) -> Result<AddLangOutput, AddLangError> {
    // 子命令保持很薄：这里负责参数和错误码，真正的验证逻辑放到可测试的报告函数里。
    match args.get(1).map(String::as_str) {
        None | Some("help" | "-h" | "--help") => {
            Ok(AddLangOutput::ok(format!("{}\n", help_text())))
        }
        Some("check-grammar") => {
            let token = required_arg(
                args,
                2,
                "usage: rustcodegraph add-lang check-grammar <lang> <valid-sample> [iterations]",
            )?;
            let sample = required_arg(
                args,
                3,
                "usage: rustcodegraph add-lang check-grammar <lang> <valid-sample> [iterations]",
            )?;
            let iterations = args
                .get(4)
                .map(|value| {
                    value.parse::<usize>().map_err(|_| {
                        AddLangError::new(2, format!("invalid iterations value: {value}"))
                    })
                })
                .transpose()?
                .unwrap_or(20);
            check_grammar_report(token, Path::new(sample), iterations)
        }
        Some("dump-ast") => {
            let tail = args.iter().skip(2).cloned().collect::<Vec<_>>();
            let (token, sample, options) = parse_dump_ast_args(&tail)?;
            dump_ast_report(&token, Path::new(&sample), options)
        }
        Some("verify-extraction") => {
            let repo = required_arg(
                args,
                2,
                "usage: rustcodegraph add-lang verify-extraction <repo-path> <lang>",
            )?;
            let lang = required_arg(
                args,
                3,
                "usage: rustcodegraph add-lang verify-extraction <repo-path> <lang>",
            )?;
            verify_extraction_report(Path::new(repo), lang)
        }
        Some(other) => Err(AddLangError::new(
            2,
            format!("unknown add-lang command '{other}'\n\n{}", help_text()),
        )),
    }
}

pub fn check_grammar_report(
    token: &str,
    sample_path: &Path,
    iterations: usize,
) -> Result<AddLangOutput, AddLangError> {
    if iterations == 0 {
        return Err(AddLangError::new(2, "iterations must be greater than zero"));
    }
    let source = fs::read_to_string(sample_path).map_err(|err| {
        AddLangError::new(
            2,
            format!(
                "sample not found or unreadable: {} ({err})",
                sample_path.display()
            ),
        )
    })?;
    let (language_token, language) = load_native_language(token)?;

    // 预热一个常见 grammar，尽早暴露 registry / ABI 初始化类问题；解析结果本身不参与报告。
    let _ = RuntimeLanguage::load("python");
    let mut parser = Parser::default();
    parser.set_language(Some(language.clone()));

    // 同一样本重复解析能捕捉到偶发状态污染或 native grammar 初始化不稳定的问题。
    let mut ok = 0usize;
    let mut err = 0usize;
    for _ in 0..iterations {
        match parser.parse(&source, None) {
            Some(tree) if !tree.root_node.has_error => ok += 1,
            _ => err += 1,
        }
    }

    let mut out = String::new();
    out.push_str(&format!("grammar: {language_token}\n"));
    out.push_str(&format!("  ABI version: {}\n", language.abi_version));
    out.push_str(&format!(
        "  parses: {ok} clean / {err} with errors (of {iterations})\n"
    ));
    if err > 0 {
        out.push_str(
            "RESULT: FAIL - the native grammar produced ERROR trees on a valid sample. Confirm the sample is syntactically valid, then inspect the grammar crate or node mapping.\n",
        );
        return Ok(AddLangOutput::with_code(out, 1));
    }
    out.push_str("RESULT: PASS - grammar parses cleanly through the Rust parser path.\n");
    Ok(AddLangOutput::ok(out))
}

pub fn dump_ast_report(
    token: &str,
    sample_path: &Path,
    options: DumpAstOptions,
) -> Result<AddLangOutput, AddLangError> {
    let source = fs::read_to_string(sample_path).map_err(|err| {
        AddLangError::new(
            2,
            format!(
                "sample file not found or unreadable: {} ({err})",
                sample_path.display()
            ),
        )
    })?;
    let (language_token, language) = load_native_language(token)?;
    let mut parser = Parser::default();
    parser.set_language(Some(language));
    let tree = parser.parse(&source, None).ok_or_else(|| {
        AddLangError::new(
            2,
            format!("failed to parse sample with native grammar '{language_token}'"),
        )
    })?;

    let mut freq = BTreeMap::new();
    let mut out = String::new();
    out.push_str(&format!(
        "\n# AST for {}  (grammar: {language_token})\n\n",
        sample_path.display()
    ));
    walk_ast(&tree.root_node, 0, None, &options, &mut freq, &mut out);
    // 频次表按出现次数排序，方便新增 extractor 时先覆盖高价值的声明/调用节点。
    out.push_str(
        "\n# Node-type frequency (named nodes) - map the relevant ones in your extractor:\n\n",
    );
    let mut counts = freq.into_iter().collect::<Vec<_>>();
    counts.sort_by(|(left_ty, left_count), (right_ty, right_count)| {
        right_count
            .cmp(left_count)
            .then_with(|| left_ty.cmp(right_ty))
    });
    for (ty, count) in counts {
        out.push_str(&format!("  {count:>5}  {ty}\n"));
    }
    out.push('\n');
    Ok(AddLangOutput::ok(out))
}

pub fn verify_extraction_report(
    repo_path: &Path,
    lang: &str,
) -> Result<AddLangOutput, AddLangError> {
    let language = normalize_language_token(lang);
    let db_path = get_code_graph_dir(repo_path).join("rustcodegraph.db");
    if !db_path.exists() {
        return Err(AddLangError::new(
            2,
            format!(
                "could not read RustCodeGraph status for {}: {} does not exist",
                repo_path.display(),
                db_path.display()
            ),
        ));
    }
    let conn =
        Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(|err| {
            AddLangError::new(
                2,
                format!(
                    "could not read RustCodeGraph status for {}: {err}",
                    repo_path.display()
                ),
            )
        })?;

    let files = count(&conn, "SELECT COUNT(*) FROM files")?;
    let nodes = count(&conn, "SELECT COUNT(*) FROM nodes")?;
    let edges = count(&conn, "SELECT COUNT(*) FROM edges")?;
    let nodes_by_kind = grouped_counts(&conn, "SELECT kind, COUNT(*) FROM nodes GROUP BY kind")?;
    let files_by_language = grouped_counts(
        &conn,
        "SELECT language, COUNT(*) FROM files GROUP BY language",
    )?;

    let symbol_kinds = structural_symbol_kinds();
    let symbol_count = nodes_by_kind
        .iter()
        .filter(|(kind, _)| symbol_kinds.contains(kind.as_str()))
        .map(|(_, count)| *count)
        .sum::<u64>();
    let symbol_kind_names = nodes_by_kind
        .keys()
        .filter(|kind| symbol_kinds.contains(kind.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    let language_file_count = files_by_language.get(&language).copied().unwrap_or(0);

    // 这些检查故意粗粒度：新增语言早期只需要判断“有文件、有结构符号、有边”，
    // 不在这里评价语义精度，避免把 framework/resolver 的问题误判成 grammar 接入失败。
    let checks = vec![
        Check::critical(
            true,
            "index initialized",
            format!("database={}", db_path.display()),
        ),
        Check::critical(
            language_file_count > 0,
            format!("language \"{language}\" detected"),
            format!("filesByLanguage={}", json_map(&files_by_language)),
        ),
        Check::critical(
            symbol_count > 0,
            "structural symbols extracted",
            format!(
                "{symbol_count} symbols ({})",
                if symbol_kind_names.is_empty() {
                    "NONE - only file/import/export nodes!".to_owned()
                } else {
                    symbol_kind_names.join(", ")
                }
            ),
        ),
        Check::soft(
            symbol_count >= files,
            "symbol density >= 1/file",
            format!("{symbol_count} symbols across {files} files"),
        ),
        Check::soft(
            edges > files,
            "edges resolved",
            format!("{edges} edges across {files} files"),
        ),
    ];

    let mut out = String::new();
    out.push_str(&format!(
        "\n# Extraction check - {}  (lang={language}, backend=sqlite)\n",
        repo_path.display()
    ));
    out.push_str(&format!("  files={files} nodes={nodes} edges={edges}\n"));
    out.push_str(&format!("  nodesByKind: {}\n\n", json_map(&nodes_by_kind)));
    for check in &checks {
        out.push_str(&format!(
            "  {} {} - {}\n",
            if check.ok { "[ok]" } else { "[fail]" },
            check.label,
            check.detail
        ));
    }

    let critical = checks
        .iter()
        .filter(|check| !check.ok && check.severity == Severity::Critical)
        .count();
    let soft = checks
        .iter()
        .filter(|check| !check.ok && check.severity == Severity::Soft)
        .count();
    out.push('\n');
    if critical > 0 {
        out.push_str(&format!(
            "RESULT: FAIL ({critical} critical) - extractor or grammar wiring is broken. Re-run add-lang dump-ast and fix the node-type mappings.\n"
        ));
        return Ok(AddLangOutput::with_code(out, 1));
    }
    if soft > 0 {
        out.push_str(&format!(
            "RESULT: WARN ({soft} soft) - extraction works but looks thin; inspect the counts above.\n"
        ));
        return Ok(AddLangOutput::ok(out));
    }
    out.push_str("RESULT: PASS - extraction looks healthy.\n");
    Ok(AddLangOutput::ok(out))
}

fn required_arg<'a>(
    args: &'a [String],
    index: usize,
    usage: &'static str,
) -> Result<&'a str, AddLangError> {
    args.get(index)
        .map(String::as_str)
        .ok_or_else(|| AddLangError::new(2, usage))
}

fn parse_dump_ast_args(args: &[String]) -> Result<(String, String, DumpAstOptions), AddLangError> {
    let mut positional = Vec::new();
    let mut max_depth = Some(8usize);
    let mut show_all = false;
    let mut index = 0usize;

    while index < args.len() {
        let arg = &args[index];
        if arg == "--full" {
            show_all = true;
            max_depth = None;
        } else if let Some(value) = arg.strip_prefix("--depth=") {
            max_depth = Some(parse_depth(value)?);
        } else if arg == "--depth" {
            let value = args
                .get(index + 1)
                .ok_or_else(|| AddLangError::new(2, "--depth requires a value"))?;
            max_depth = Some(parse_depth(value)?);
            index += 1;
        } else if arg.starts_with("--") {
            return Err(AddLangError::new(2, format!("unknown option: {arg}")));
        } else {
            positional.push(arg.clone());
        }
        index += 1;
    }

    // `dump-ast` 允许选项穿插在位置参数之间，便于从 shell 历史里微调深度。
    if positional.len() < 2 {
        return Err(AddLangError::new(
            2,
            "usage: rustcodegraph add-lang dump-ast <lang> <sample-file> [--depth=N] [--full]",
        ));
    }
    Ok((
        positional[0].clone(),
        positional[1].clone(),
        DumpAstOptions {
            max_depth,
            show_all,
        },
    ))
}

fn parse_depth(value: &str) -> Result<usize, AddLangError> {
    value
        .parse::<usize>()
        .map_err(|_| AddLangError::new(2, format!("invalid depth value: {value}")))
}

fn load_native_language(token: &str) -> Result<(String, RuntimeLanguage), AddLangError> {
    // 新增语言必须先接入 native grammar crate。接受 .wasm 会掩盖 Rust 索引路径没有注册的问题。
    if Path::new(token)
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("wasm"))
    {
        return Err(AddLangError::new(
            2,
            "Rust add-lang helpers use native tree-sitter grammar crates, not .wasm files. Add the crate and registry wiring first, then pass the language token.",
        ));
    }

    let normalized = normalize_language_token(token);
    RuntimeLanguage::load(&normalized)
        .map(|language| (normalized, language))
        .map_err(|err| {
            AddLangError::new(
                2,
                format!(
                    "{err}. Add the native tree-sitter crate and registry wiring before running add-lang helpers."
                ),
            )
        })
}

fn normalize_language_token(token: &str) -> String {
    match token.trim().to_ascii_lowercase().as_str() {
        "ts" => "typescript".to_owned(),
        "js" => "javascript".to_owned(),
        "c#" | "c-sharp" | "c_sharp" => "csharp".to_owned(),
        "objective-c" | "objectivec" => "objc".to_owned(),
        other => other.to_owned(),
    }
}

fn walk_ast(
    node: &SyntaxNode,
    depth: usize,
    field_name: Option<&str>,
    options: &DumpAstOptions,
    freq: &mut BTreeMap<String, u64>,
    out: &mut String,
) {
    if node.is_named() {
        *freq.entry(node.node_type().to_owned()).or_insert(0) += 1;
    }

    // 频次统计始终遍历全树；深度限制只影响打印，避免浅层输出时丢失节点类型统计。
    let within_depth = options
        .max_depth
        .map(|max_depth| depth <= max_depth)
        .unwrap_or(true);
    if (node.is_named() || options.show_all) && within_depth {
        let field = field_name
            .map(|name| format!("{name}: "))
            .unwrap_or_default();
        let leaf = if node.child_count() == 0 {
            format!("  \"{}\"", snippet(&node.text()))
        } else {
            String::new()
        };
        out.push_str(&format!(
            "{}{}{}  @{}:{}{}\n",
            "  ".repeat(depth),
            field,
            node.node_type(),
            node.start_position().row + 1,
            node.start_position().column,
            leaf
        ));
    }

    for (child_index, child) in node.children.iter().enumerate() {
        let field = child_field_name(node, child_index, child);
        walk_ast(child, depth + 1, field.as_deref(), options, freq, out);
    }
}

fn child_field_name(parent: &SyntaxNode, child_index: usize, child: &SyntaxNode) -> Option<String> {
    // facade 同时保留 child 与 named_child 字段索引；这里用节点 id 校验 named_child，
    // 避免匿名节点插入后字段名错贴到相邻的命名节点上。
    for (name, target) in &parent.field_names {
        match target {
            FieldTarget::Child(index) if *index == child_index => return Some(name.clone()),
            FieldTarget::NamedChild(index)
                if parent
                    .named_children
                    .get(*index)
                    .is_some_and(|named| named.id == child.id) =>
            {
                return Some(name.clone());
            }
            _ => {}
        }
    }
    None
}

fn snippet(text: &str) -> String {
    let flattened = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flattened.chars().count() > 48 {
        format!("{}...", flattened.chars().take(48).collect::<String>())
    } else {
        flattened
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Severity {
    Critical,
    Soft,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Check {
    severity: Severity,
    ok: bool,
    label: String,
    detail: String,
}

impl Check {
    fn critical(ok: bool, label: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            severity: Severity::Critical,
            ok,
            label: label.into(),
            detail: detail.into(),
        }
    }

    fn soft(ok: bool, label: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            severity: Severity::Soft,
            ok,
            label: label.into(),
            detail: detail.into(),
        }
    }
}

fn structural_symbol_kinds() -> HashSet<&'static str> {
    // 与 NodeKind 保持同一语义集合，但排除 file/import/export/parameter 这类“存在即正常”的辅助节点。
    [
        "module",
        "class",
        "struct",
        "interface",
        "trait",
        "protocol",
        "function",
        "method",
        "property",
        "field",
        "variable",
        "constant",
        "enum",
        "enum_member",
        "type_alias",
        "namespace",
        "route",
        "component",
    ]
    .into_iter()
    .collect()
}

fn count(conn: &Connection, sql: &str) -> Result<u64, AddLangError> {
    conn.query_row(sql, [], |row| row.get::<_, i64>(0))
        .map(|value| value as u64)
        .map_err(|err| {
            AddLangError::new(2, format!("failed to query RustCodeGraph database: {err}"))
        })
}

fn grouped_counts(conn: &Connection, sql: &str) -> Result<BTreeMap<String, u64>, AddLangError> {
    // BTreeMap 让报告稳定排序，方便 CI 日志和人工比较。
    let mut stmt = conn.prepare(sql).map_err(|err| {
        AddLangError::new(2, format!("failed to query RustCodeGraph database: {err}"))
    })?;
    let rows = stmt
        .query_map([], |row| {
            let key: String = row.get(0)?;
            let value: i64 = row.get(1)?;
            Ok((key, value as u64))
        })
        .map_err(|err| {
            AddLangError::new(2, format!("failed to query RustCodeGraph database: {err}"))
        })?;
    let mut out = BTreeMap::new();
    for row in rows {
        let (key, value) = row.map_err(|err| {
            AddLangError::new(2, format!("failed to query RustCodeGraph database: {err}"))
        })?;
        out.insert(key, value);
    }
    Ok(out)
}

fn json_map(map: &BTreeMap<String, u64>) -> String {
    serde_json::to_string(map).unwrap_or_else(|_| "{}".to_owned())
}
