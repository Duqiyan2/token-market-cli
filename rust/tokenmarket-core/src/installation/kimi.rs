//! Official stable Kimi Code standalone artifacts include Node SEA and native assets.
use super::{InstallPlan, InstallStage, InstallStep};
use crate::{
    inventory::{HostPlatform, InstallationEnvironment},
    ToolId,
};
use anyhow::{bail, Result};
use std::path::Path;
fn ps(path: &Path) -> String {
    path.to_string_lossy().replace('\'', "''")
}
pub(super) fn plan(
    environment: &InstallationEnvironment,
    architecture: &str,
) -> Result<InstallPlan> {
    plan_release(environment, architecture, "2.4.1", None)
}
pub(super) fn plan_release(
    environment: &InstallationEnvironment,
    architecture: &str,
    version: &str,
    digest: Option<&str>,
) -> Result<InstallPlan> {
    let (platform, arch, hash) = match (environment.platform, architecture) {
        (HostPlatform::MacOs, "aarch64") => (
            "darwin",
            "arm64",
            "17f848e7b9b159aed9b2ad0df917c4be3c484393d924a6540171ede7b87cc66d",
        ),
        (HostPlatform::MacOs, "x86_64") => (
            "darwin",
            "x64",
            "63c1120c7e75e379c805d9f13f45f0a1163f955b20ddcb16344440271b30d159",
        ),
        (HostPlatform::Unix, "aarch64") => (
            "linux",
            "arm64",
            "dffb4beb5327366de810f73fc8dd1eda73017f3fdd55dec49904a70692d20c51",
        ),
        (HostPlatform::Unix, "x86_64") => (
            "linux",
            "x64",
            "f4a225fb8afe0394151e721bc3cd94d71e314fbea7f75900453556da7d8738de",
        ),
        (HostPlatform::Windows, "aarch64") => (
            "win32",
            "arm64",
            "af2ce55589d1dbd0205ef6174cd91e41d79ad1704d7bd812ec710e379311ffb0",
        ),
        (HostPlatform::Windows, "x86_64") => (
            "win32",
            "x64",
            "23d74ca9454755b9dac676ae8cac8e0a39b2bb96b9d9adc2093895eac1686557",
        ),
        _ => bail!("Kimi Code 官方独立安装包不支持当前架构 {architecture}"),
    };
    let hash = digest.unwrap_or(hash);
    let windows = environment.platform == HostPlatform::Windows;
    let shell = std::env::var_os("KIMI_SHELL_PATH")
        .filter(|v| !v.is_empty())
        .map(std::path::PathBuf::from);
    if windows
        && shell
            .as_ref()
            .is_some_and(|p| !p.is_absolute() || !p.is_file())
    {
        bail!("KIMI_SHELL_PATH 必须指向已有 Git Bash 的绝对路径，请先修复该覆盖")
    }
    let mut steps = if windows && shell.is_some() {
        vec![]
    } else {
        super::portable_git::steps(environment, architecture, "kimi-code")?
    };
    let scripts = environment.home.join(".tokenmarket/installers/kimi-code");
    let staging = scripts.join("staging");
    let prefix = environment.home.join(".tokenmarket/tools/kimi-code/bin");
    let asset = format!(
        "kimi-code-{platform}-{arch}.{}",
        if windows { "zip" } else { "tar.gz" }
    );
    let archive = scripts.join(&asset);
    let (program, args) = if windows {
        let script=format!("$ErrorActionPreference='Stop'; if ((Get-FileHash -Algorithm SHA256 -LiteralPath '{}').Hash.ToLowerInvariant() -ne '{}') {{ throw 'Kimi Code checksum mismatch' }}; New-Item -ItemType Directory -Force -Path '{}' | Out-Null; Expand-Archive -LiteralPath '{}' -DestinationPath '{}' -Force; & '{}' --version; if ($LASTEXITCODE -ne 0) {{ throw 'Kimi Code native binary cannot start; no command was installed' }}; New-Item -ItemType Directory -Force -Path '{}' | Out-Null; Copy-Item -LiteralPath '{}' -Destination '{}' -Force",ps(&archive),hash,ps(&staging),ps(&archive),ps(&staging),ps(&staging.join("kimi.exe")),ps(&prefix),ps(&staging.join("kimi.exe")),ps(&prefix.join("kimi.exe")));
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
command -v bash >/dev/null 2>&1 || { printf '%s\n' 'Kimi Code requires Bash; install Bash and retry.' >&2; exit 1; }
if command -v sha256sum >/dev/null 2>&1; then actual=$(sha256sum "$1"); else actual=$(shasum -a 256 "$1"); fi
actual=${actual%% *}
[ "$actual" = "$4" ] || { printf '%s\n' 'Kimi Code checksum mismatch' >&2; exit 1; }
mkdir -p "$2"
tar -xzf "$1" -C "$2"
chmod u+x "$2/kimi"
"$2/kimi" --version || { printf '%s\n' 'Kimi Code native binary cannot start; no command was installed.' >&2; exit 1; }
mkdir -p "$3"
cp -f "$2/kimi" "$3/kimi"
chmod u+x "$3/kimi"
"#;
        (
            "sh",
            vec![
                "-c".into(),
                script.into(),
                "kimi-install".into(),
                archive.to_string_lossy().into(),
                staging.to_string_lossy().into(),
                prefix.to_string_lossy().into(),
                hash.into(),
            ],
        )
    };
    steps.push(InstallStep{stage:InstallStage::Downloading,program:String::new(),args:vec![],environment:vec![],download:Some((format!("https://github.com/MoonshotAI/kimi-code/releases/download/%40moonshot-ai/kimi-code%40{version}/{asset}"),archive))});
    let process_environment = if windows {
        vec![(
            "KIMI_SHELL_PATH".into(),
            shell
                .or_else(|| super::portable_git::bash_path(environment))
                .unwrap_or_else(|| {
                    environment
                        .home
                        .join(".tokenmarket/runtime/git/bin/bash.exe")
                })
                .to_string_lossy()
                .into(),
        )]
    } else {
        vec![]
    };
    steps.push(InstallStep {
        stage: InstallStage::Installing,
        program: program.into(),
        args,
        environment: process_environment,
        download: None,
    });
    Ok(InstallPlan {
        method: super::InstallMethod::Standalone,
        tool: ToolId::KimiCode,
        version: version.into(),
        command: "kimi".into(),
        steps,
    })
}

/// Recognize a positive old-product identity, never incidental runtime versions.
pub(crate) fn is_legacy_version(output: &str) -> bool {
    let output = output.trim();
    let parts: Vec<_> = output.split_whitespace().collect();
    let lower = output.to_ascii_lowercase();
    let token = if lower.starts_with("kimi") {
        if let Some(index) = parts
            .iter()
            .position(|part| part.eq_ignore_ascii_case("version"))
        {
            parts.get(index + 1).copied()
        } else {
            parts
                .iter()
                .skip(1)
                .skip_while(|part| {
                    part.eq_ignore_ascii_case("code") || part.eq_ignore_ascii_case("cli")
                })
                .next()
                .copied()
        }
    } else if parts.len() == 1 {
        parts.first().copied()
    } else {
        None
    };
    token
        .and_then(super::opencode::major_version)
        .is_some_and(|major| major < 2)
}
