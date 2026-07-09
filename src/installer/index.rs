//! Installer orchestrator translation.
//!
//! This module keeps the TypeScript install/uninstall control flow traceable:
//! target resolution, location/default handling, print-config reporting,
//! lifecycle records, and local-project initialization decisions. It does not
//! run interactive prompts or write agent config files yet.
//!
//! 安装器入口只负责编排：解析目标、选择 local/global、收集各 target
//! 的写入结果，并保持 CLI/测试可序列化的报告形状。真正的配置文件细节
//! 由 `targets/*` 拥有，避免一个入口文件同时理解所有 agent 的格式。

use serde::{Deserialize, Serialize};

use crate::directory::unsafe_index_root_reason;

use super::targets::registry::{all_targets, get_target, resolve_target_flag};
use super::targets::types::{
    AgentTarget, FileWrite, InstallOptions, Location, TargetId, WriteResult,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetInstallReport {
    pub target_id: TargetId,
    pub display_name: String,
    pub location: Location,
    pub files: Vec<FileWrite>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallerReport {
    pub targets: Vec<TargetInstallReport>,
    pub printed_config: Option<String>,
    pub skipped_local_init_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UninstallStatus {
    Removed,
    NotConfigured,
    Unsupported,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UninstallReport {
    pub id: TargetId,
    pub display_name: String,
    pub status: UninstallStatus,
    pub removed_paths: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallCommandOptions {
    pub target: Option<String>,
    pub location: Option<Location>,
    pub yes: bool,
    pub permissions: bool,
    pub print_config: Option<String>,
}

impl Default for InstallCommandOptions {
    fn default() -> Self {
        Self {
            target: None,
            location: None,
            yes: false,
            permissions: true,
            print_config: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UninstallCommandOptions {
    pub target: Option<String>,
    pub location: Option<Location>,
    pub yes: bool,
}

impl Default for UninstallCommandOptions {
    fn default() -> Self {
        Self {
            target: Some("all".to_owned()),
            location: None,
            yes: false,
        }
    }
}

pub fn install_codegraph(opts: InstallCommandOptions) -> Result<InstallerReport, String> {
    // `--yes` 沿用非交互安装的全局默认；交互/未确认路径默认本地，
    // 这样不会意外写入用户级配置。
    let loc = opts.location.unwrap_or(if opts.yes {
        Location::Global
    } else {
        Location::Local
    });

    // `--print-config` 是纯展示路径：不做 detect、不写文件，便于文档和
    // 用户手动复制配置时复用同一 target 渲染逻辑。
    if let Some(id) = opts.print_config {
        let target = get_target(&id).ok_or_else(|| format!("Unknown target id: {id}"))?;
        return Ok(InstallerReport {
            printed_config: Some(target.print_config(loc)),
            ..InstallerReport::default()
        });
    }

    let flag = opts.target.unwrap_or_else(|| "auto".to_owned());
    let targets = resolve_target_flag(&flag, loc)?;
    let mut report = InstallerReport::default();
    for target in targets {
        if !target.supports_location(loc) {
            continue;
        }
        report.targets.push(run_target_install(
            target.as_ref(),
            loc,
            target.install(
                loc,
                InstallOptions {
                    auto_allow: opts.permissions,
                },
            ),
        ));
    }
    report.skipped_local_init_reason = initialize_local_project_plan(loc, opts.yes);
    Ok(report)
}

pub fn uninstall_codegraph(opts: UninstallCommandOptions) -> Result<InstallerReport, String> {
    let loc = opts.location.unwrap_or(if opts.yes {
        Location::Global
    } else {
        Location::Local
    });
    let flag = opts.target.unwrap_or_else(|| "all".to_owned());
    let targets = resolve_target_flag(&flag, loc)?;
    let mut report = InstallerReport::default();
    for target in targets {
        if !target.supports_location(loc) {
            continue;
        }
        report.targets.push(run_target_install(
            target.as_ref(),
            loc,
            target.uninstall(loc),
        ));
    }
    Ok(report)
}

fn run_target_install(
    target: &dyn AgentTarget,
    location: Location,
    result: WriteResult,
) -> TargetInstallReport {
    TargetInstallReport {
        target_id: target.id(),
        display_name: target.display_name().to_owned(),
        location,
        files: result.files,
        notes: result.notes,
    }
}

pub fn initialize_local_project_plan(loc: Location, use_defaults: bool) -> Option<String> {
    // 自动索引只在本地安装路径上考虑；全局默认安装不应该顺手初始化
    // 当前目录，否则在 HOME、/tmp 等敏感位置会产生意外的 `.rustcodegraph/`。
    if loc != Location::Local && use_defaults {
        return None;
    }
    let cwd = std::env::current_dir().ok()?;
    unsafe_index_root_reason(&cwd).map(|reason| {
        format!(
            "Skipping automatic indexing - {} looks like {}.",
            cwd.display(),
            reason
        )
    })
}

pub fn known_targets() -> Vec<TargetId> {
    all_targets()
        .into_iter()
        .map(|target| target.id())
        .collect()
}

pub fn uninstall_targets(
    targets: Vec<Box<dyn AgentTarget>>,
    location: Location,
) -> Vec<UninstallReport> {
    // 卸载报告面向 CLI 文案：unsupported / not-configured / removed
    // 都是成功形状，只有 target 解析失败才在上层返回 Err。
    targets
        .into_iter()
        .map(|target| {
            if !target.supports_location(location) {
                let only = if location == Location::Local {
                    "global"
                } else {
                    "local"
                };
                return UninstallReport {
                    id: target.id(),
                    display_name: target.display_name().to_owned(),
                    status: UninstallStatus::Unsupported,
                    removed_paths: Vec::new(),
                    notes: vec![format!(
                        "no {} config - this agent is {only}-only",
                        location.as_str()
                    )],
                };
            }

            let result = target.uninstall(location);
            let removed_paths = result
                .files
                .into_iter()
                .filter(|file| file.action == super::targets::types::WriteAction::Removed)
                .map(|file| file.path)
                .collect::<Vec<_>>();
            UninstallReport {
                id: target.id(),
                display_name: target.display_name().to_owned(),
                status: if removed_paths.is_empty() {
                    UninstallStatus::NotConfigured
                } else {
                    UninstallStatus::Removed
                },
                removed_paths,
                notes: result.notes.unwrap_or_default(),
            }
        })
        .collect()
}
