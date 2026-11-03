//! Shared rootless Git Bash dependency for Pi and Kimi on Windows.
use super::{InstallStage, InstallStep};
use crate::inventory::{HostPlatform, InstallationEnvironment};
use anyhow::{bail, Result};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
fn ps(path: &Path) -> String {
    path.to_string_lossy().replace('\'', "''")
}
fn download(url: String, path: PathBuf) -> InstallStep {
    InstallStep {
        stage: InstallStage::Downloading,
        program: String::new(),
        args: vec![],
        environment: vec![],
        download: Some((url, path)),
    }
}
pub(crate) fn bash_path(environment: &InstallationEnvironment) -> Option<PathBuf> {
    environment
        .command_path("bash")
        .filter(|path| {
            !path.starts_with(environment.home.join(".tokenmarket/runtime/git"))
                || environment
                    .home
                    .join(".tokenmarket/runtime/git/.tokenmarket-ready-2.56.0.4")
                    .is_file()
        })
        .or_else(|| {
            [
                environment
                    .home
                    .join(".tokenmarket/runtime/git/bin/bash.exe"),
                environment
                    .home
                    .join(".tokenmarket/tools/pi/git/bin/bash.exe"),
            ]
            .into_iter()
            .chain(
                environment
                    .application_directories
                    .iter()
                    .map(|p| p.join("Git/bin/bash.exe")),
            )
            .find(|p| {
                p.is_file()
                    && (!p.starts_with(environment.home.join(".tokenmarket/runtime/git"))
                        || environment
                            .home
                            .join(".tokenmarket/runtime/git/.tokenmarket-ready-2.56.0.4")
                            .is_file())
            })
        })
}
pub(super) fn steps(
    environment: &InstallationEnvironment,
    architecture: &str,
    consumer: &str,
) -> Result<Vec<InstallStep>> {
    if environment.platform != HostPlatform::Windows || bash_path(environment).is_some() {
        return Ok(vec![]);
    }
    if !matches!(architecture, "aarch64" | "x86_64") {
        bail!("Git for Windows 不支持当前架构")
    }
    let git = environment.home.join(".tokenmarket/runtime/git");
    let installers = environment
        .home
        .join(".tokenmarket/installers")
        .join(consumer)
        .join("portable-git");
    let mut steps = vec![];
    let (asset, sha) = if architecture == "aarch64" {
        (
            "PortableGit-2.56.0.4-arm64.7z.exe",
            "044110f9d951858452ddb5e4b5415ea0f2b5b2598e794d6fec77fe28b9aa8cc0",
        )
    } else {
        (
            "PortableGit-2.56.0.4-64-bit.7z.exe",
            "b1f3e52216e2ed6882d1bebfd4d3fdf319bec40d2edbbf9fd26ec4fd3fce7ed1",
        )
    };
    let package = installers.join(asset);
    steps.push(download(
        format!(
            "https://github.com/git-for-windows/git/releases/download/v2.56.0.windows.4/{asset}"
        ),
        package.clone(),
    ));
    let ready = git.join(".tokenmarket-ready-2.56.0.4");
    let mutex_name = format!(
        "Local\\TokenMarketPortableGit-{:x}",
        Sha256::digest(git.to_string_lossy().as_bytes())
    );
    let script=format!("$ErrorActionPreference='Stop'; $mutex=[System.Threading.Mutex]::new($false,'{}'); $locked=$false; try {{ try {{ [void]$mutex.WaitOne(); $locked=$true }} catch [System.Threading.AbandonedMutexException] {{ $locked=$true }}; if (-not ((Test-Path -LiteralPath '{}') -and (Test-Path -LiteralPath '{}'))) {{ if ((Get-FileHash -Algorithm SHA256 -LiteralPath '{}').Hash.ToLowerInvariant() -ne '{}') {{ throw 'Git for Windows checksum mismatch' }}; & '{}' '-y' '-o{}'; if ($LASTEXITCODE -ne 0) {{ throw 'Git for Windows extraction failed' }}; & '{}' --version; if ($LASTEXITCODE -ne 0) {{ throw 'Git Bash dependency cannot start' }}; [System.IO.File]::WriteAllText('{}','2.56.0.4') }} }} finally {{ if ($locked) {{ $mutex.ReleaseMutex() }}; $mutex.Dispose() }}",mutex_name,ps(&ready),ps(&git.join("bin/bash.exe")),ps(&package),sha,ps(&package),ps(&git),ps(&git.join("bin/bash.exe")),ps(&ready));
    steps.push(InstallStep {
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
        environment: vec![],
        download: None,
    });
    Ok(steps)
}
