//! Official Pi release archives include their Bun runtime and native helpers.
use super::{InstallPlan, InstallStage, InstallStep};
use crate::{
    inventory::{HostPlatform, InstallationEnvironment},
    ToolId,
};
use anyhow::{bail, Result};
use std::path::Path;
const VERSION: &str = "1.4.0";
fn ps(path: &Path) -> String {
    path.to_string_lossy().replace('\'', "''")
}
fn download(url: String, path: std::path::PathBuf) -> InstallStep {
    InstallStep {
        stage: InstallStage::Downloading,
        program: String::new(),
        args: vec![],
        environment: vec![],
        download: Some((url, path)),
    }
}
pub(super) fn plan(
    environment: &InstallationEnvironment,
    architecture: &str,
) -> Result<InstallPlan> {
    let (platform, arch, hash) = match (environment.platform, architecture) {
        (HostPlatform::MacOs, "aarch64") => (
            "darwin",
            "arm64",
            "79d9979d76a2a5adb7c28e27e9b44629928ccd0dc9fbb6ae7217860bd5e8be5d",
        ),
        (HostPlatform::MacOs, "x86_64") => (
            "darwin",
            "x64",
            "1bdefc9e1074c1de458ea9f2e43e6013ce49cd05326009b3de144037036585e2",
        ),
        (HostPlatform::Unix, "aarch64") => (
            "linux",
            "arm64",
            "fa5829c7ef83ffeaebba015715c8306220f89b2aa8a00154efbba98dd5440156",
        ),
        (HostPlatform::Unix, "x86_64") => (
            "linux",
            "x64",
            "f2e4e4329603223d03260cdb8bd5033dbdc1f73ba58ab8939db2e56546e115d2",
        ),
        (HostPlatform::Windows, "aarch64") => (
            "windows",
            "arm64",
            "17ab17ee0e9088545c7dfa56d29490c45bd915425610b0db0cd937632f2df714",
        ),
        (HostPlatform::Windows, "x86_64") => (
            "windows",
            "x64",
            "0f7256ea8bdac3af20012b94fdc1252a31cc6dc0d2f9b9eb539555d22e3e321b",
        ),
        _ => bail!("Pi 官方独立安装包不支持当前架构 {architecture}"),
    };
    let windows = environment.platform == HostPlatform::Windows;
    let runtime = environment.home.join(".tokenmarket/tools/pi/runtime");
    let installers = environment.home.join(".tokenmarket/installers/pi");
    let asset = format!(
        "pi-{platform}-{arch}.{}",
        if windows { "zip" } else { "tar.gz" }
    );
    let archive = installers.join(&asset);
    let mut steps = super::portable_git::steps(environment, architecture, "pi")?;
    steps.push(download(
        format!("https://github.com/earendil-works/pi/releases/download/v{VERSION}/{asset}"),
        archive.clone(),
    ));
    let (program, args) = if windows {
        let script=format!("$ErrorActionPreference='Stop'; if ((Get-FileHash -Algorithm SHA256 -LiteralPath '{}').Hash.ToLowerInvariant() -ne '{}') {{ throw 'Pi checksum mismatch' }}; New-Item -ItemType Directory -Force -Path '{}' | Out-Null; Expand-Archive -LiteralPath '{}' -DestinationPath '{}' -Force; if (-not (Test-Path -LiteralPath '{}')) {{ throw 'Pi executable missing' }}",ps(&archive),hash,ps(&runtime),ps(&archive),ps(&runtime),ps(&runtime.join("pi.exe")));
        (
            "powershell.exe",
            vec![
                "-NoProfile".into(),
                "-NonInteractive".into(),
                "-ExecutionPolicy".into(),
                "Bypass".into(),
                "-Command".into(),
                script,
            ],
        )
    } else {
        let script = r#"set -eu
command -v bash >/dev/null 2>&1 || { printf '%s\n' 'Pi requires Bash; install Bash and retry.' >&2; exit 1; }
if command -v sha256sum >/dev/null 2>&1; then actual=$(sha256sum "$1"); else actual=$(shasum -a 256 "$1"); fi
actual=${actual%% *}
[ "$actual" = "$3" ] || { printf '%s\n' 'Pi checksum mismatch' >&2; exit 1; }
mkdir -p "$2"
tar -xzf "$1" -C "$2" --strip-components=1
chmod u+x "$2/pi"
"#;
        (
            "sh",
            vec![
                "-c".into(),
                script.into(),
                "pi-install".into(),
                archive.to_string_lossy().into(),
                runtime.to_string_lossy().into(),
                hash.into(),
            ],
        )
    };
    steps.push(InstallStep {
        stage: InstallStage::Installing,
        program: program.into(),
        args,
        environment: vec![],
        download: None,
    });
    Ok(InstallPlan {
        tool: ToolId::Pi,
        version: VERSION.into(),
        command: "pi".into(),
        steps,
    })
}
