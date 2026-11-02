//! Official version-bound installer recipes. Adding a tool does not alter connection policy.
use super::{InstallPlan, InstallStage, InstallStep, InstallationAvailability};
use crate::{
    inventory::{HostPlatform, InstallationEnvironment},
    ToolId,
};
use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

pub const OPENCLAW_VERSION: &str = "2026.10.9";
pub const GROK_BUILD_VERSION: &str = "1.0.70";
pub fn managed_command_directories(home: &Path) -> Vec<PathBuf> {
    vec![
        home.join(".local/bin"),
        home.join(".tokenmarket/tools/codex/bin"),
        home.join(".tokenmarket/tools/deepseek-harness/bin"),
        home.join(".tokenmarket/tools/deepseek-harness/runtime/bin"),
        home.join(".tokenmarket/tools/openclaw/bin"),
        home.join(".tokenmarket/tools/openclaw"),
        home.join(".tokenmarket/tools/openclaw/runtime"),
        home.join(".tokenmarket/tools/grok-build/bin"),
        home.join(".grok/bin"),
    ]
}
pub fn availability(
    tool: ToolId,
    environment: &InstallationEnvironment,
    architecture: &str,
) -> InstallationAvailability {
    match plan(tool, environment, architecture) {
        Ok(plan) => InstallationAvailability {
            supported: true,
            reason: None,
            version: Some(plan.version),
        },
        Err(error) => InstallationAvailability {
            supported: false,
            reason: Some(error.to_string()),
            version: None,
        },
    }
}
pub fn plan(
    tool: ToolId,
    environment: &InstallationEnvironment,
    architecture: &str,
) -> Result<InstallPlan> {
    if super::gui::is_gui(tool) {
        return super::gui::plan(tool, environment, architecture);
    }
    if matches!(tool, ToolId::ClaudeCode | ToolId::Codex) {
        return super::native_cli::plan(tool, environment, architecture);
    }
    if tool == ToolId::GrokBuild {
        return grok_plan(environment, architecture);
    }
    if tool != ToolId::OpenClaw {
        bail!("该工具的自动安装流程尚未配置，请查看官方安装说明")
    }
    if !matches!(architecture, "aarch64" | "x86_64") {
        bail!("官方安装方案不支持当前架构 {architecture}")
    }
    let prefix = environment.home.join(".tokenmarket/tools/openclaw");
    let scripts = environment.home.join(".tokenmarket/installers/openclaw");
    let step = if environment.platform == HostPlatform::Windows {
        let installer = scripts.join("install.ps1");
        let runtime = prefix.join("runtime");
        let script = format!("$runtimeReady = $false; if (Test-Path -LiteralPath '{}') {{ $nodeVersion = & '{}' --version; if ($LASTEXITCODE -eq 0 -and $nodeVersion -match '^v(24|26)\\.(\\d+)\\.(\\d+)$') {{ $runtimeReady = ([int]$Matches[1] -eq 24 -and ([int]$Matches[2] -gt 22 -or ([int]$Matches[2] -eq 22 -and [int]$Matches[3] -ge 1))) -or ([int]$Matches[1] -eq 26 -and ([int]$Matches[2] -gt 12 -or ([int]$Matches[2] -eq 12 -and [int]$Matches[3] -ge 1))) }} }}; if (-not $runtimeReady) {{ & '{}' -NodeOnly -NodePrefix '{}'; if (-not $?) {{ exit 1 }} }}; $env:PATH = '{};' + $env:PATH; & '{}' install --global --prefix '{}' 'openclaw@{}'; if ($LASTEXITCODE -ne 0) {{ exit $LASTEXITCODE }}", ps(&runtime.join("node.exe")), ps(&runtime.join("node.exe")), ps(&installer), ps(&runtime), ps(&runtime), ps(&runtime.join("npm.cmd")), ps(&runtime), OPENCLAW_VERSION);
        InstallStep {
            stage: InstallStage::Dependencies,
            program: "powershell.exe".into(),
            args: vec![
                "-NoProfile".into(),
                "-NonInteractive".into(),
                "-ExecutionPolicy".into(),
                "Bypass".into(),
                "-Command".into(),
                script,
            ],
            environment: vec![("OPENCLAW_NO_ONBOARD".into(), "1".into())],
            download: Some(("https://openclaw.ai/install.ps1".into(), installer)),
        }
    } else {
        let installer = scripts.join("install-cli.sh");
        InstallStep {
            stage: InstallStage::Dependencies,
            program: "bash".into(),
            args: vec![
                installer.to_string_lossy().into(),
                "--prefix".into(),
                prefix.to_string_lossy().into(),
                "--version".into(),
                OPENCLAW_VERSION.into(),
                "--npm".into(),
                "--runtime-only".into(),
                "--no-onboard".into(),
                "--json".into(),
            ],
            environment: vec![("OPENCLAW_NO_ONBOARD".into(), "1".into())],
            download: Some(("https://openclaw.ai/install-cli.sh".into(), installer)),
        }
    };
    let mut execution = step.clone();
    execution.download = None;
    let download = InstallStep {
        stage: InstallStage::Downloading,
        program: String::new(),
        args: vec![],
        environment: vec![],
        download: step.download,
    };
    Ok(InstallPlan {
        tool,
        version: OPENCLAW_VERSION.into(),
        command: "openclaw".into(),
        steps: vec![download, execution],
    })
}
fn grok_plan(environment: &InstallationEnvironment, architecture: &str) -> Result<InstallPlan> {
    if !matches!(architecture, "aarch64" | "x86_64") {
        bail!("Grok Build 官方安装方案不支持当前架构 {architecture}")
    }
    let windows = environment.platform == HostPlatform::Windows;
    let prefix = environment.home.join(".tokenmarket/tools/grok-build/bin");
    let installer = environment
        .home
        .join(".tokenmarket/installers/grok-build")
        .join(if windows { "install.ps1" } else { "install.sh" });
    let source = if windows {
        "https://x.ai/cli/install.ps1"
    } else {
        "https://x.ai/cli/install.sh"
    };
    let execution = InstallStep {
        stage: InstallStage::Installing,
        program: if windows { "powershell.exe" } else { "bash" }.into(),
        args: if windows {
            vec![
                "-NoProfile".into(),
                "-NonInteractive".into(),
                "-ExecutionPolicy".into(),
                "Bypass".into(),
                "-File".into(),
                installer.to_string_lossy().into(),
                "-Version".into(),
                GROK_BUILD_VERSION.into(),
            ]
        } else {
            vec![
                installer.to_string_lossy().into(),
                GROK_BUILD_VERSION.into(),
            ]
        },
        // Deployment keys make the upstream installer write inference configuration.
        // Install only the runtime; an existing client connection remains untouched.
        environment: vec![
            ("GROK_BIN_DIR".into(), prefix.to_string_lossy().into()),
            ("GROK_CHANNEL".into(), "stable".into()),
            ("GROK_DEPLOYMENT_KEY".into(), String::new()),
        ],
        download: None,
    };
    Ok(InstallPlan {
        tool: ToolId::GrokBuild,
        version: GROK_BUILD_VERSION.into(),
        command: "grok".into(),
        steps: vec![
            InstallStep {
                stage: InstallStage::Downloading,
                program: String::new(),
                args: vec![],
                environment: vec![],
                download: Some((source.into(), installer)),
            },
            execution,
        ],
    })
}
fn ps(path: &Path) -> String {
    path.to_string_lossy().replace('\'', "''")
}
