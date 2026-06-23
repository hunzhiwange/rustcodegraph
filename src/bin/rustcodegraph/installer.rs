//! Rust CLI 入口使用的最小 agent 配置安装器。
//!
//! 这个模块只处理当前二进制直接支持的 Codex/Cursor/opencode 配置写入；复杂的
//! 多 agent 安装契约和注释块管理在库层 installer 模块中维护。

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::args::{absolutize, has_flag, option_value};

pub(crate) fn command_install(args: &[String]) -> Result<(), String> {
    let location = install_location(args);
    if let Some(target) = option_value(args, "--print-config") {
        // --print-config 必须是纯输出路径，不触碰用户配置文件，方便文档和排障复制。
        println!("{}", mcp_config_snippet(&target, location)?);
        return Ok(());
    }

    let target_flag = option_value(args, "-t")
        .or_else(|| option_value(args, "--target"))
        .unwrap_or_else(|| "auto".to_owned());
    let targets = resolve_install_targets(&target_flag, location);
    if targets.is_empty() {
        println!("No install targets selected.");
        return Ok(());
    }

    if !has_flag(args, "-y", "--yes") {
        // Rust CLI 没有交互式 prompt；无 -y 时只展示将要写入的位置。
        println!("Rust installer is non-interactive in this build. Re-run with -y to write:");
        for target in &targets {
            println!("  {target}: {}", install_target_path(target, location));
        }
        return Ok(());
    }

    for target in targets {
        let path = install_mcp_target(&target, location)?;
        println!("Configured {target} at {path}");
    }
    Ok(())
}

pub(crate) fn command_uninstall(args: &[String]) -> Result<(), String> {
    let location = install_location(args);
    let target_flag = option_value(args, "-t")
        .or_else(|| option_value(args, "--target"))
        .unwrap_or_else(|| "all".to_owned());
    let targets = resolve_install_targets(&target_flag, location);
    if targets.is_empty() {
        println!("No uninstall targets selected.");
        return Ok(());
    }
    if !has_flag(args, "-y", "--yes") {
        // 与 install 对称：默认 dry-run，避免误删用户的 agent 配置。
        println!("Rust uninstaller is non-interactive in this build. Re-run with -y to remove:");
        for target in &targets {
            println!("  {target}: {}", install_target_path(target, location));
        }
        return Ok(());
    }
    for target in targets {
        let path = uninstall_mcp_target(&target, location)?;
        println!("Removed {target} config from {path}");
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum InstallLocation {
    /// 用户级 agent 配置，通常位于 HOME 下。
    Global,
    /// 当前 workspace 内的项目级配置。
    Local,
}

pub(super) fn install_location(args: &[String]) -> InstallLocation {
    match option_value(args, "-l")
        .or_else(|| option_value(args, "--location"))
        .as_deref()
    {
        Some("local") => InstallLocation::Local,
        _ => InstallLocation::Global,
    }
}

pub(super) fn resolve_install_targets(flag: &str, location: InstallLocation) -> Vec<String> {
    let expanded = match flag {
        "all" => vec!["codex", "cursor", "opencode"],
        "auto" => {
            // local 安装不依赖用户目录是否存在：项目内配置可以直接创建。
            let mut detected = Vec::new();
            if home_dir().join(".codex").exists() {
                detected.push("codex");
            }
            if home_dir().join(".cursor").exists() || location == InstallLocation::Local {
                detected.push("cursor");
            }
            if home_dir().join(".config").join("opencode").exists()
                || location == InstallLocation::Local
            {
                detected.push("opencode");
            }
            if detected.is_empty() {
                detected.push("codex");
            }
            detected
        }
        "none" => Vec::new(),
        other => other
            .split(',')
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .collect(),
    };
    expanded
        .into_iter()
        .map(str::to_owned)
        .filter(|id| matches!(id.as_str(), "codex" | "cursor" | "opencode"))
        .collect()
}

pub(super) fn install_target_path(target: &str, location: InstallLocation) -> String {
    match (target, location) {
        ("codex", _) => home_dir()
            .join(".codex")
            .join("config.toml")
            .display()
            .to_string(),
        ("cursor", InstallLocation::Global) => home_dir()
            .join(".cursor")
            .join("mcp.json")
            .display()
            .to_string(),
        ("cursor", InstallLocation::Local) => absolutize(".cursor/mcp.json").display().to_string(),
        ("opencode", InstallLocation::Global) => home_dir()
            .join(".config")
            .join("opencode")
            .join("opencode.jsonc")
            .display()
            .to_string(),
        ("opencode", InstallLocation::Local) => absolutize("opencode.jsonc").display().to_string(),
        _ => "<unsupported>".to_owned(),
    }
}

pub(super) fn install_mcp_target(
    target: &str,
    location: InstallLocation,
) -> Result<String, String> {
    match target {
        "codex" => {
            let path = home_dir().join(".codex").join("config.toml");
            let block = codex_mcp_toml_block()?;
            upsert_toml_block(&path, "mcp_servers.rustcodegraph", &block)?;
            Ok(path.display().to_string())
        }
        "cursor" => {
            let path = match location {
                InstallLocation::Global => home_dir().join(".cursor").join("mcp.json"),
                InstallLocation::Local => absolutize(".cursor/mcp.json"),
            };
            upsert_json_object(
                &path,
                &["mcpServers", "rustcodegraph"],
                cursor_mcp_json(location)?,
            )?;
            Ok(path.display().to_string())
        }
        "opencode" => {
            let path = match location {
                InstallLocation::Global => home_dir()
                    .join(".config")
                    .join("opencode")
                    .join("opencode.jsonc"),
                InstallLocation::Local => absolutize("opencode.jsonc"),
            };
            upsert_json_object(&path, &["mcp", "rustcodegraph"], opencode_mcp_json()?)?;
            Ok(path.display().to_string())
        }
        _ => Err(format!("unsupported install target: {target}")),
    }
}

pub(super) fn uninstall_mcp_target(
    target: &str,
    location: InstallLocation,
) -> Result<String, String> {
    match target {
        "codex" => {
            let path = home_dir().join(".codex").join("config.toml");
            remove_toml_block(&path, "mcp_servers.rustcodegraph")?;
            Ok(path.display().to_string())
        }
        "cursor" => {
            let path = match location {
                InstallLocation::Global => home_dir().join(".cursor").join("mcp.json"),
                InstallLocation::Local => absolutize(".cursor/mcp.json"),
            };
            remove_json_object(&path, &["mcpServers", "rustcodegraph"])?;
            Ok(path.display().to_string())
        }
        "opencode" => {
            let path = match location {
                InstallLocation::Global => home_dir()
                    .join(".config")
                    .join("opencode")
                    .join("opencode.jsonc"),
                InstallLocation::Local => absolutize("opencode.jsonc"),
            };
            remove_json_object(&path, &["mcp", "rustcodegraph"])?;
            Ok(path.display().to_string())
        }
        _ => Err(format!("unsupported uninstall target: {target}")),
    }
}

pub(super) fn mcp_config_snippet(
    target: &str,
    location: InstallLocation,
) -> Result<String, String> {
    match target {
        "codex" => Ok(codex_mcp_toml_block()?),
        "cursor" => serde_json::to_string_pretty(&json!({
            "mcpServers": { "rustcodegraph": cursor_mcp_json(location)? }
        }))
        .map_err(|err| err.to_string()),
        "opencode" => serde_json::to_string_pretty(&json!({
            "mcp": { "rustcodegraph": opencode_mcp_json()? }
        }))
        .map_err(|err| err.to_string()),
        _ => Err(format!("unsupported target for --print-config: {target}")),
    }
}

fn codex_mcp_toml_block() -> Result<String, String> {
    Ok(
        "[mcp_servers.rustcodegraph]\ncommand = \"rustcodegraph\"\nargs = [\"serve\", \"--mcp\"]\n"
            .to_owned(),
    )
}

fn cursor_mcp_json(location: InstallLocation) -> Result<Value, String> {
    let mut args = vec![json!("serve"), json!("--mcp")];
    if location == InstallLocation::Global {
        // Cursor 全局 MCP 子进程 cwd 不可靠；传 workspaceFolder 才能让 server 找到项目索引。
        args.push(json!("--path"));
        args.push(json!("${workspaceFolder}"));
    }
    Ok(json!({
        "type": "stdio",
        "command": "rustcodegraph",
        "args": args,
    }))
}

fn opencode_mcp_json() -> Result<Value, String> {
    Ok(json!({
        "type": "local",
        "command": ["rustcodegraph", "serve", "--mcp"],
        "enabled": true
    }))
}

fn upsert_toml_block(path: &Path, header: &str, block: &str) -> Result<(), String> {
    // 先移除旧块再追加，保证重复 install 字节稳定且不会生成多个 rustcodegraph server。
    let existing = fs::read_to_string(path).unwrap_or_default();
    let next = remove_toml_block_from_text(&existing, header);
    let mut combined = next.trim_end().to_owned();
    if !combined.is_empty() {
        combined.push_str("\n\n");
    }
    combined.push_str(block.trim_end());
    combined.push('\n');
    write_text_file(path, &combined)
}

fn remove_toml_block(path: &Path, header: &str) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let existing = fs::read_to_string(path)
        .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    write_text_file(path, &remove_toml_block_from_text(&existing, header))
}

fn remove_toml_block_from_text(content: &str, header: &str) -> String {
    let wanted = format!("[{header}]");
    let mut out = Vec::new();
    let mut skipping = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == wanted {
            skipping = true;
            continue;
        }
        if skipping && trimmed.starts_with('[') && trimmed.ends_with(']') {
            // 遇到下一个表头即停止跳过；当前简化 serializer 不解析 TOML AST，
            // 因此要求 managed block 是普通单表而不是 array-of-tables。
            skipping = false;
        }
        if !skipping {
            out.push(line);
        }
    }
    out.join("\n").trim_end().to_owned() + "\n"
}

fn upsert_json_object(path: &Path, keys: &[&str], value: Value) -> Result<(), String> {
    let mut root = read_json_config(path)?;
    set_nested_json(&mut root, keys, value)?;
    write_json_file(path, &root)
}

fn remove_json_object(path: &Path, keys: &[&str]) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let mut root = read_json_config(path)?;
    remove_nested_json(&mut root, keys);
    write_json_file(path, &root)
}

fn read_json_config(path: &Path) -> Result<Value, String> {
    if !path.exists() {
        return Ok(json!({}));
    }
    let text = fs::read_to_string(path)
        .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    if text.trim().is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_str(&text).map_err(|err| {
        // 这里明确拒绝 JSONC，避免用 serde_json 重写时吞掉用户注释。
        format!(
            "failed to parse {} as JSON; existing JSONC/comments are not edited by the quick Rust installer yet: {err}",
            path.display()
        )
    })
}

fn set_nested_json(root: &mut Value, keys: &[&str], value: Value) -> Result<(), String> {
    if keys.is_empty() {
        *root = value;
        return Ok(());
    }
    if !root.is_object() {
        *root = json!({});
    }
    let mut current = root;
    for key in &keys[..keys.len() - 1] {
        // 中间路径不是 object 时直接替换，保证部分损坏配置可被 install 修复。
        let obj = current
            .as_object_mut()
            .ok_or_else(|| "invalid JSON object shape".to_owned())?;
        current = obj.entry((*key).to_owned()).or_insert_with(|| json!({}));
        if !current.is_object() {
            *current = json!({});
        }
    }
    let obj = current
        .as_object_mut()
        .ok_or_else(|| "invalid JSON object shape".to_owned())?;
    obj.insert(keys[keys.len() - 1].to_owned(), value);
    Ok(())
}

fn remove_nested_json(root: &mut Value, keys: &[&str]) {
    if keys.is_empty() {
        return;
    }
    let mut current = root;
    for key in &keys[..keys.len() - 1] {
        let Some(next) = current.get_mut(*key) else {
            return;
        };
        current = next;
    }
    if let Some(obj) = current.as_object_mut() {
        obj.remove(keys[keys.len() - 1]);
    }
}

fn write_json_file(path: &Path, value: &Value) -> Result<(), String> {
    let text = serde_json::to_string_pretty(value).map_err(|err| err.to_string())? + "\n";
    write_text_file(path, &text)
}

fn write_text_file(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
    }
    fs::write(path, text).map_err(|err| format!("failed to write {}: {err}", path.display()))
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}
