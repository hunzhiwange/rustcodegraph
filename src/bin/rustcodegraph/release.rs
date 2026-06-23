//! CLI 暴露的 release 和 upgrade 辅助命令。
//!
//! 发布动作本身由 GitHub Actions 执行；这些命令只负责本地 changelog 准备、
//! release notes 提取、版本展示和升级流程编排。

use std::io::{self, Read};
use std::path::Path;

use rustcodegraph::mcp::version::code_graph_package_version;
use rustcodegraph::release::{extract_release_notes_from_stdin_text, extract_release_notes_in_dir};
use rustcodegraph::upgrade::index as upgrade;

use super::args::has_flag;

pub(crate) fn command_version(_args: &[String]) -> Result<(), String> {
    println!("{}", code_graph_package_version());
    Ok(())
}

pub(crate) fn command_prepare_release(args: &[String]) -> Result<(), String> {
    // 版本参数可为空，由库层 helper 负责校验当前 package/changelog 状态。
    let version = args.get(1).map(String::as_str);
    let report = rustcodegraph::release::prepare_release_in_dir(Path::new("."), version)?;
    println!("{}", report.summary);
    Ok(())
}

pub(crate) fn command_extract_release_notes(args: &[String]) -> Result<(), String> {
    let Some(arg) = args.get(1).map(String::as_str) else {
        return Err("usage: rustcodegraph extract-release-notes <version> | --stdin".to_owned());
    };

    let notes = if arg == "--stdin" {
        // --stdin 供 release workflow 直接传入已抽取块，避免再次读取工作区文件。
        let mut input = String::new();
        io::stdin()
            .read_to_string(&mut input)
            .map_err(|err| format!("failed to read stdin: {err}"))?;
        extract_release_notes_from_stdin_text(&input)
    } else {
        extract_release_notes_in_dir(Path::new("."), arg)?
    };
    print!("{notes}");
    Ok(())
}

pub(crate) fn command_add_lang(args: &[String]) -> Result<(), String> {
    // add-lang 返回结构化 exit_code；CLI 入口必须原样退出，方便脚本判断失败类型。
    match rustcodegraph::add_lang::run_cli(args) {
        Ok(output) => {
            print!("{}", output.text);
            if output.exit_code != 0 {
                std::process::exit(output.exit_code);
            }
            Ok(())
        }
        Err(err) => {
            eprintln!("{}", err.message);
            std::process::exit(err.code);
        }
    }
}

pub(crate) fn command_upgrade(args: &[String]) -> Result<(), String> {
    let version = args
        .iter()
        .skip(1)
        .find(|arg| !arg.starts_with('-'))
        .cloned()
        .or_else(|| std::env::var("RUSTCODEGRAPH_VERSION").ok());
    let cwd = std::env::current_dir()
        .map_err(|err| format!("failed to resolve current directory: {err}"))?
        .display()
        .to_string();
    let exe = current_exe_string()?;
    let platform = current_platform();
    // 升级策略依赖安装方式和平台；检测逻辑留在库层，CLI 只注入真实 IO 依赖。
    let method = upgrade::detect_install_method(upgrade::DetectInput {
        filename: &exe,
        platform,
        cwd: &cwd,
        exists: |path| Path::new(path).exists(),
    });
    let code = upgrade::run_upgrade(
        upgrade::UpgradeOptions {
            version,
            check: has_flag(args, "--check", "--check"),
            force: has_flag(args, "-f", "--force"),
        },
        upgrade::UpgradeDeps {
            current_version: code_graph_package_version().to_owned(),
            method,
            resolve_latest: Box::new(upgrade::resolve_latest_version),
            run: Box::new(upgrade::default_run),
            has_command: Box::new(upgrade::has_command),
            log: Box::new(|message| println!("{message}")),
            warn: Box::new(|message| eprintln!("Warning: {message}")),
            error: Box::new(|message| eprintln!("Error: {message}")),
            platform: platform.to_owned(),
        },
    );
    if code == 0 {
        Ok(())
    } else {
        Err(format!("upgrade failed with exit code {code}"))
    }
}

fn current_exe_string() -> Result<String, String> {
    std::env::current_exe()
        .map(|path| path.display().to_string())
        .map_err(|err| format!("failed to resolve current executable: {err}"))
}

fn current_platform() -> &'static str {
    if cfg!(windows) {
        "win32"
    } else if cfg!(target_os = "macos") {
        "darwin"
    } else {
        "linux"
    }
}
