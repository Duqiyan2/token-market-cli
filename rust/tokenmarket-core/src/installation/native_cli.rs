//! Native installers keep vendor-managed runtime layouts intact.
use super::{InstallPlan, InstallStage, InstallStep};
use crate::{
    inventory::{HostPlatform, InstallationEnvironment},
    ToolId,
};
use anyhow::{bail, Result};

const CLAUDE_VERSION: &str = "2.1.309";
const CODEX_VERSION: &str = "0.166.0";

pub(super) fn plan(
    tool: ToolId,
    environment: &InstallationEnvironment,
    architecture: &str,
) -> Result<InstallPlan> {
    let version = match tool {
        ToolId::ClaudeCode => CLAUDE_VERSION,
        ToolId::Codex => CODEX_VERSION,
        _ => bail!("该工具没有原生 CLI 安装方案"),
    };
    plan_version(tool, environment, architecture, version)
}
pub(super) fn plan_version(
    tool: ToolId,
    environment: &InstallationEnvironment,
    architecture: &str,
    version: &str,
) -> Result<InstallPlan> {
    if !matches!(architecture, "aarch64" | "x86_64") {
        bail!("官方安装方案不支持当前架构 {architecture}")
    }
    let windows = environment.platform == HostPlatform::Windows;
    let (command, origin) = match tool {
        ToolId::ClaudeCode => ("claude", "https://claude.ai"),
        ToolId::Codex => ("codex", "https://chatgpt.com/codex"),
        _ => bail!("该工具没有原生 CLI 安装方案"),
    };
    let extension = if windows { "ps1" } else { "sh" };
    let installer = environment
        .home
        .join(".tokenmarket/installers")
        .join(tool.to_string())
        .join(format!("install.{extension}"));
    let mut args = if windows {
        vec![
            "-NoProfile".into(),
            "-NonInteractive".into(),
            "-ExecutionPolicy".into(),
            "Bypass".into(),
            "-File".into(),
            installer.to_string_lossy().into_owned(),
        ]
    } else {
        vec![installer.to_string_lossy().into_owned()]
    };
    let mut process_environment = vec![];
    if tool == ToolId::Codex {
        // The official POSIX script accepts --release; PowerShell uses -Release.
        args.push(if windows { "-Release" } else { "--release" }.into());
        // Only redirect the visible command. CODEX_HOME and its standalone
        // package tree remain vendor-owned, including bundled helper programs.
        process_environment.extend([
            ("CODEX_NON_INTERACTIVE".into(), "1".into()),
            (
                "CODEX_INSTALL_DIR".into(),
                environment
                    .home
                    .join(".tokenmarket/tools/codex/bin")
                    .to_string_lossy()
                    .into_owned(),
            ),
        ]);
    }
    args.push(version.into());
    Ok(InstallPlan {
        tool,
        version: version.into(),
        command: command.into(),
        steps: vec![
            InstallStep {
                stage: InstallStage::Downloading,
                program: String::new(),
                args: vec![],
                environment: vec![],
                download: Some((format!("{origin}/install.{extension}"), installer)),
            },
            InstallStep {
                stage: InstallStage::Installing,
                program: if windows {
                    "powershell.exe"
                } else if tool == ToolId::ClaudeCode {
                    "bash"
                } else {
                    "sh"
                }
                .into(),
                args,
                environment: process_environment,
                download: None,
            },
        ],
    })
}
