//! Official 1.0.89 artifacts and SHA512 from the vendor's update feed, checked 2026-11-03.
use super::{InstallPlan, InstallStage, InstallStep};
use crate::{
    inventory::{HostPlatform, InstallationEnvironment},
    ToolId,
};
use anyhow::{bail, Result};
use base64::{engine::general_purpose::STANDARD, Engine};

pub(super) const VERSION: &str = "1.0.89";
pub(super) fn plan(
    environment: &InstallationEnvironment,
    architecture: &str,
) -> Result<InstallPlan> {
    let (filename, digest) = match (environment.platform, architecture) {
        (HostPlatform::MacOs, "aarch64") => ("DuMate-1.0.89-arm64.zip", "ybCfGYUN4TbpSj/mSTNVifVnT+FvuqpNnu0G4ophCKKkp0oFT1cIJiVi3EchlQEjz4u8dKRp24slzAykzSDgPA=="),
        (HostPlatform::Windows, "x86_64") => ("DuMate-Setup-1.0.89-x64.exe", "tPeZQ3mTubmQhMFP5AtYqCDD/KWOkLYNpfPyYUU/lohXoXtd1HMnsrtCb7VDePdzQ35C67HprDMVpgr52FEBbw=="),
        _ => bail!("DuMate 官方个人版安装包仅支持 macOS Apple Silicon（macOS 14+）和 Windows x64（Windows 10 1809+）"),
    };
    let digest = STANDARD
        .decode(digest)?
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let package = environment
        .home
        .join(".tokenmarket/installers/dumate")
        .join(VERSION)
        .join(filename);
    let step = if environment.platform == HostPlatform::MacOs {
        InstallStep {
            stage: InstallStage::Installing,
            program: "/bin/sh".into(),
            args: vec!["-c".into(), "set -eu; actual=$(/usr/bin/shasum -a 512 \"$1\"); actual=${actual%% *}; [ \"$actual\" = \"$2\" ] || { echo 'DuMate SHA512 校验失败' >&2; exit 1; }; /bin/mkdir -p \"$3\"; /usr/bin/ditto -x -k \"$1\" \"$3\"".into(), "tokenmarket-dumate-verify".into(), package.to_string_lossy().into(), digest, package.parent().unwrap().join("unpacked").to_string_lossy().into()],
            environment: vec![], download: None,
        }
    } else {
        InstallStep {
            stage: InstallStage::Installing,
            program: "powershell.exe".into(),
            args: vec!["-NoProfile".into(), "-NonInteractive".into(), "-Command".into(), format!("$ErrorActionPreference = 'Stop'; if ((Get-FileHash -LiteralPath '{}' -Algorithm SHA512).Hash -ne '{digest}') {{ throw 'DuMate SHA512 校验失败' }}", package.to_string_lossy().replace('\'', "''"))],
            environment: vec![], download: None,
        }
    };
    let handoff = if environment.platform == HostPlatform::MacOs {
        InstallStep {
            stage: InstallStage::WaitingForUser,
            program: "/usr/bin/open".into(),
            args: vec![package
                .parent()
                .unwrap()
                .join("unpacked")
                .to_string_lossy()
                .into()],
            environment: vec![],
            download: None,
        }
    } else {
        InstallStep {
            stage: InstallStage::WaitingForUser,
            program: "powershell.exe".into(),
            args: vec![
                "-NoProfile".into(),
                "-NonInteractive".into(),
                "-Command".into(),
                format!(
                    "$ErrorActionPreference = 'Stop'; Start-Process -FilePath '{}'",
                    package.to_string_lossy().replace('\'', "''")
                ),
            ],
            environment: vec![],
            download: None,
        }
    };
    Ok(InstallPlan {tool: ToolId::DuMate, version: VERSION.into(), command: "dumate".into(), steps: vec![
        InstallStep {stage: InstallStage::Downloading, program: String::new(), args: vec![], environment: vec![], download: Some((format!("https://dumate-app-public.cdn.bcebos.com/release-info/artifacts-update/{filename}"), package))},
        step, handoff,
    ]})
}
