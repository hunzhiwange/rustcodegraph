//! JS/TS workspace package resolution.
//!
//! 中文维护提示：这里只解析 package.json/pnpm-workspace 的 workspace globs，并把
//! 包名映射到项目内目录，供 import resolver 处理 monorepo 包名导入。

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde_json::Value;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkspacePackages {
    pub by_name: HashMap<String, String>,
}

pub fn load_workspace_packages(project_root: impl AsRef<Path>) -> Option<WorkspacePackages> {
    let project_root = project_root.as_ref();
    let patterns = read_workspace_globs(project_root);
    if patterns.is_empty() {
        return None;
    }

    let mut by_name = HashMap::new();
    for pattern in patterns {
        for dir in expand_workspace_glob(project_root, &pattern) {
            if let Some(name) = read_package_name(&project_root.join(&dir)) {
                // 第一次发现的包名 wins，避免重复 workspace glob 让后面的目录覆盖前面。
                by_name.entry(name).or_insert(dir);
            }
        }
    }

    if by_name.is_empty() {
        None
    } else {
        Some(WorkspacePackages { by_name })
    }
}

pub fn resolve_workspace_import(import_path: &str, ws: &WorkspacePackages) -> Option<String> {
    let mut best_name: Option<&String> = None;
    for name in ws.by_name.keys() {
        // 选择最长包名前缀，保证 `@scope/pkg-extra` 不会被 `@scope/pkg` 抢先匹配。
        if (import_path == name || import_path.starts_with(&format!("{name}/")))
            && best_name
                .map(|best| name.len() > best.len())
                .unwrap_or(true)
        {
            best_name = Some(name);
        }
    }
    let best_name = best_name?;
    let dir = ws.by_name.get(best_name)?;
    let subpath = &import_path[best_name.len()..];
    Some(format!("{dir}{subpath}").replace("//", "/"))
}

fn read_workspace_globs(project_root: &Path) -> Vec<String> {
    let mut out = Vec::new();

    if let Ok(raw) = fs::read_to_string(project_root.join("package.json"))
        && let Ok(pkg) = serde_json::from_str::<Value>(&raw)
    {
        match pkg.get("workspaces") {
            Some(Value::Array(items)) => {
                out.extend(
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .map(ToOwned::to_owned),
                );
            }
            Some(Value::Object(map)) => {
                if let Some(Value::Array(items)) = map.get("packages") {
                    out.extend(
                        items
                            .iter()
                            .filter_map(Value::as_str)
                            .map(ToOwned::to_owned),
                    );
                }
            }
            _ => {}
        }
    }

    if let Ok(yaml) = fs::read_to_string(project_root.join("pnpm-workspace.yaml")) {
        out.extend(parse_pnpm_packages(&yaml));
    }

    out
}

fn parse_pnpm_packages(yaml: &str) -> Vec<String> {
    // 这里不是通用 YAML parser，只读取 pnpm-workspace.yaml 中最常见的 packages 列表。
    let mut out = Vec::new();
    let mut in_packages = false;
    for line in yaml.lines() {
        if line.trim_start().starts_with("packages:") {
            in_packages = true;
            continue;
        }
        if !in_packages {
            continue;
        }
        let trimmed = line.trim();
        if let Some(item) = trimmed.strip_prefix("- ") {
            out.push(item.trim().trim_matches('"').trim_matches('\'').to_string());
        } else if !trimmed.is_empty() && !line.starts_with(' ') && !line.starts_with('\t') {
            in_packages = false;
        }
    }
    out
}

fn expand_workspace_glob(project_root: &Path, pattern: &str) -> Vec<String> {
    // 支持单星号目录展开即可覆盖 packages/*、apps/* 等主流 workspace 布局；
    // 更复杂的 glob 留给后续显式需求。
    let norm = pattern.replace('\\', "/").trim_end_matches('/').to_string();
    let Some(star) = norm.find('*') else {
        return vec![norm];
    };
    let base = norm[..star].trim_end_matches('/').to_string();
    let Ok(entries) = fs::read_dir(project_root.join(&base)) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false))
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || name == "node_modules" {
                return None;
            }
            Some(if base.is_empty() {
                name
            } else {
                format!("{base}/{name}")
            })
        })
        .collect()
}

fn read_package_name(dir_abs: &Path) -> Option<String> {
    let raw = fs::read_to_string(dir_abs.join("package.json")).ok()?;
    let pkg = serde_json::from_str::<Value>(&raw).ok()?;
    pkg.get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
        .map(ToOwned::to_owned)
}
