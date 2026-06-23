//! Cargo workspace helper translated from `cargo-workspace.ts`.
//!
//! Rust import resolver 用这里把 workspace member 的 package name 映射到路径。
//! 解析保持轻量手写，避免为了少量字段引入完整 TOML/glob 依赖。

use std::collections::{HashMap, HashSet};

use crate::resolution::types::ResolutionContext;

const SKIP_DIRS: &[&str] = &["target", "node_modules", ".git", "dist", "build"];
const MAX_GLOB_WALK_DEPTH: usize = 5;

pub fn get_cargo_workspace_crate_map(
    context: &mut dyn ResolutionContext,
) -> HashMap<String, String> {
    // 根 Cargo.toml 没有 workspace 时直接返回空表；普通单 crate 由常规路径解析处理。
    let mut result = HashMap::new();
    let Some(root_cargo_toml) = context.read_file("Cargo.toml") else {
        return result;
    };

    let members = parse_workspace_members(&root_cargo_toml);
    for member_path in expand_members(&members, context) {
        let member_cargo_path = format!("{member_path}/Cargo.toml");
        let Some(member_cargo_toml) = context.read_file(&member_cargo_path) else {
            continue;
        };
        if let Some(package_name) = parse_package_name(&member_cargo_toml) {
            add_crate_alias(&mut result, &package_name, &member_path);
        }
    }

    result
}

fn get_section(content: &str, section_name: &str) -> Option<String> {
    // 只需要顶层 `[workspace]`/`[package]`；遇到下一个表头就停止。
    let mut in_section = false;
    let mut section_lines = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if !in_section {
            if trimmed == format!("[{section_name}]") {
                in_section = true;
            }
            continue;
        }
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            break;
        }
        section_lines.push(line);
    }
    in_section.then(|| section_lines.join("\n"))
}

fn extract_quoted_values(value_list: &str) -> Vec<String> {
    // members 数组可能跨行并混用单双引号；只抽引号内值，保留转义字符后的字面字符。
    let mut values = Vec::new();
    let mut quote = None;
    let mut escaped = false;
    let mut current = String::new();

    for ch in value_list.chars() {
        if quote.is_none() {
            if ch == '"' || ch == '\'' {
                quote = Some(ch);
                current.clear();
            }
            continue;
        }
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if Some(ch) == quote {
            let value = current.trim();
            if !value.is_empty() {
                values.push(value.to_string());
            }
            quote = None;
            current.clear();
            continue;
        }
        current.push(ch);
    }

    values
}

fn get_array_value(section: &str, key: &str) -> Option<String> {
    // 手写数组扫描支持跨行和嵌套括号/引号，避免简单 `]` 查找被字符串干扰。
    let assignment = section.find(&key.to_string())?;
    let after_key = &section[assignment + key.len()..];
    let eq = after_key.find('=')?;
    let mut i = assignment + key.len() + eq + 1;
    while section
        .as_bytes()
        .get(i)
        .is_some_and(|b| b.is_ascii_whitespace())
    {
        i += 1;
    }
    if section.as_bytes().get(i) != Some(&b'[') {
        return None;
    }
    i += 1;
    let start = i;
    let mut quote = None;
    let mut escaped = false;
    let mut depth = 1usize;
    let chars = section.char_indices().collect::<Vec<_>>();
    let mut pos = chars
        .iter()
        .position(|(idx, _)| *idx >= i)
        .unwrap_or(chars.len());
    while pos < chars.len() {
        let (idx, ch) = chars[pos];
        if quote.is_some() {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if Some(ch) == quote {
                quote = None;
            }
            pos += 1;
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
        } else if ch == '[' {
            depth += 1;
        } else if ch == ']' {
            depth -= 1;
            if depth == 0 {
                return Some(section[start..idx].to_string());
            }
        }
        pos += 1;
    }
    None
}

fn parse_workspace_members(cargo_toml: &str) -> Vec<String> {
    get_section(cargo_toml, "workspace")
        .and_then(|section| get_array_value(&section, "members"))
        .map(|value| extract_quoted_values(&value))
        .unwrap_or_default()
}

fn parse_package_name(cargo_toml: &str) -> Option<String> {
    let section = get_section(cargo_toml, "package")?;
    for line in section.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("name") {
            continue;
        }
        let rhs = trimmed.split_once('=')?.1.trim();
        let value = rhs.trim_matches(|ch| ch == '"' || ch == '\'');
        if !value.is_empty() {
            return Some(value.to_string());
        }
    }
    None
}

fn add_crate_alias(map: &mut HashMap<String, String>, crate_name: &str, member_path: &str) {
    // Rust import 可用 hyphen 包名，也常以 underscore crate ident 出现。
    let normalized = crate_name.replace('-', "_");
    map.insert(crate_name.to_string(), member_path.to_string());
    if normalized != crate_name {
        map.insert(normalized, member_path.to_string());
    }
}

fn clean_path(member_path: &str) -> String {
    member_path
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_string()
}

fn has_glob(value: &str) -> bool {
    value
        .chars()
        .any(|ch| matches!(ch, '*' | '?' | '[' | ']' | '{' | '}' | '!'))
}

fn expand_members(members: &[String], context: &mut dyn ResolutionContext) -> Vec<String> {
    // workspace members 允许 glob；最终结果要去重并统一 `/` 路径。
    let mut expanded = Vec::new();
    let mut seen = HashSet::new();
    for member in members {
        let candidates = if has_glob(member) {
            expand_glob_member(member, context)
        } else {
            vec![member.clone()]
        };
        for candidate in candidates {
            let cleaned = clean_path(&candidate);
            if seen.insert(cleaned.clone()) {
                expanded.push(cleaned);
            }
        }
    }
    expanded
}

fn expand_glob_member(member: &str, context: &mut dyn ResolutionContext) -> Vec<String> {
    // 从 glob 前的静态目录开始走，避免从仓库根无界扫描。
    let first_glob = member
        .find(['*', '?', '[', ']', '{', '}', '!'])
        .unwrap_or(member.len());
    let static_prefix = member[..first_glob]
        .rsplit_once('/')
        .map(|(prefix, _)| prefix)
        .unwrap_or(".")
        .trim_end_matches('/');
    let mut matches = Vec::new();
    let mut seen = HashSet::new();
    walk_glob(
        if static_prefix.is_empty() {
            "."
        } else {
            static_prefix
        },
        0,
        member,
        context,
        &mut seen,
        &mut matches,
    );
    matches
}

fn walk_glob(
    dir: &str,
    depth: usize,
    glob: &str,
    context: &mut dyn ResolutionContext,
    seen: &mut HashSet<String>,
    matches: &mut Vec<String>,
) {
    if depth > MAX_GLOB_WALK_DEPTH {
        return;
    }
    for child in context.list_directories(dir) {
        if SKIP_DIRS.contains(&child.as_str()) || child.starts_with('.') {
            // target/node_modules/.git 等目录既大又不会是 workspace member。
            continue;
        }
        let rel = if dir == "." {
            child
        } else {
            format!("{dir}/{child}")
        };
        if glob_match(glob, &rel) && seen.insert(rel.clone()) {
            matches.push(rel.clone());
        }
        walk_glob(&rel, depth + 1, glob, context, seen, matches);
    }
}

fn glob_match(glob: &str, value: &str) -> bool {
    // 当前只覆盖 Cargo workspace 常见的 `crates/*` / `packages/*` 形式。
    if glob == value {
        return true;
    }
    if let Some((prefix, suffix)) = glob.split_once('*') {
        return value.starts_with(prefix) && value.ends_with(suffix);
    }
    false
}
