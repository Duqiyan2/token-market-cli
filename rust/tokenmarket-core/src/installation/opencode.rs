//! OpenCode V1 installation. V2 uses a different configuration contract.
use super::{InstallPlan, InstallStage, InstallStep};
use crate::{
    inventory::{HostPlatform, InstallationEnvironment},
    ToolId,
};
use anyhow::{bail, Result};

pub const VERSION: &str = "1.19.5";

pub fn plan(environment: &InstallationEnvironment, architecture: &str) -> Result<InstallPlan> {
    let (archive, digest) = match (environment.platform, architecture) {
        (HostPlatform::MacOs, "aarch64") => (
            "opencode-darwin-arm64.zip",
            "c1c9e62521192192ef4c3ed1355ada2144055bddb61682217e997b03c55203c1",
        ),
        (HostPlatform::MacOs, "x86_64") => (
            "opencode-darwin-x64-baseline.zip",
            "e1235374cb7e712cdc15e21ba915a029a5d22312b460606d0783754739706775",
        ),
        (HostPlatform::Unix, "aarch64") => (
            "opencode-linux-arm64.tar.gz",
            "fae5be262d693b7c5e562856cc473e59e741772936db0c0600ac0d5ed5bd088b",
        ),
        (HostPlatform::Unix, "x86_64") => (
            "opencode-linux-x64-baseline.tar.gz",
            "17c200fd4b2289954cc01552559f9b6d03ec157b2ddfb5eb54144f491656333d",
        ),
        (HostPlatform::Windows, "x86_64") => (
            "opencode-windows-x64-baseline.zip",
            "e4dac5cd38a182a24e7b3e18853f01d08cdb137e50b3bd1397c1a89e54d28c63",
        ),
        _ => bail!("当前系统/架构没有已验证的 OpenCode V1 官方安装方案"),
    };
    let directory = environment.home.join(".tokenmarket/installers/opencode");
    let archive_path = directory.join(archive);
    let destination = environment.home.join(".tokenmarket/tools/opencode/bin");
    let download = InstallStep {
        stage: InstallStage::Downloading,
        program: String::new(),
        args: vec![],
        environment: vec![],
        download: Some((
            format!("https://github.com/anomalyco/opencode/releases/download/v{VERSION}/{archive}"),
            archive_path.clone(),
        )),
    };
    let execution = if environment.platform == HostPlatform::Windows {
        let script = format!("$ErrorActionPreference = 'Stop'; $archive = '{}'; $destination = '{}'; if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant() -ne '{}') {{ throw 'OpenCode SHA256 校验失败，未运行或安装下载内容' }}; New-Item -ItemType Directory -Force -Path $destination | Out-Null; $stage = Join-Path $destination ('.stage-' + [guid]::NewGuid().ToString('N')); try {{ Expand-Archive -LiteralPath $archive -DestinationPath $stage; $binary = Join-Path $stage 'opencode.exe'; if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) {{ throw 'OpenCode 安装包缺少有效可执行文件' }}; Move-Item -LiteralPath $binary -Destination (Join-Path $destination 'opencode.exe') -Force }} finally {{ if (Test-Path -LiteralPath $stage) {{ Remove-Item -LiteralPath $stage -Recurse -Force }} }}", ps(&archive_path), ps(&destination), digest);
        InstallStep {
            stage: InstallStage::Installing,
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
        }
    } else {
        InstallStep {
            stage: InstallStage::Installing,
            program: "sh".into(),
            args: vec![
                "-c".into(),
                POSIX_INSTALL.into(),
                "tokenmarket-opencode-install".into(),
                archive_path.to_string_lossy().into(),
                destination.to_string_lossy().into(),
                digest.into(),
                if environment.platform == HostPlatform::MacOs {
                    "zip".into()
                } else {
                    "tar.gz".into()
                },
            ],
            environment: vec![],
            download: None,
        }
    };
    Ok(InstallPlan {
        method: super::InstallMethod::Standalone,
        tool: ToolId::OpenCode,
        version: VERSION.into(),
        command: "opencode".into(),
        steps: vec![download, execution],
    })
}

const POSIX_INSTALL: &str = r#"set -eu
archive=$1
destination=$2
expected=$3
kind=$4
if command -v shasum >/dev/null 2>&1; then
  actual=$(shasum -a 256 "$archive" | cut -d ' ' -f 1)
elif command -v sha256sum >/dev/null 2>&1; then
  actual=$(sha256sum "$archive" | cut -d ' ' -f 1)
else
  echo '缺少系统 SHA256 校验工具，请按官方安装说明准备后重试' >&2
  exit 1
fi
if [ "$actual" != "$expected" ]; then
  echo 'OpenCode SHA256 校验失败，未运行或安装下载内容' >&2
  exit 1
fi
mkdir -p "$destination"
stage=$(mktemp -d "$destination/.stage.XXXXXXXX")
trap 'rm -rf -- "$stage"' EXIT HUP INT TERM
if [ "$kind" = zip ]; then
  /usr/bin/ditto -x -k "$archive" "$stage"
else
  tar -xzf "$archive" -C "$stage"
fi
if [ ! -f "$stage/opencode" ] || [ -L "$stage/opencode" ]; then
  echo 'OpenCode 安装包缺少有效可执行文件' >&2
  exit 1
fi
chmod 755 "$stage/opencode"
mv -f "$stage/opencode" "$destination/opencode"
echo 'OpenCode V1 已安装，正在重新检测'
"#;

fn ps(path: &std::path::Path) -> String {
    path.to_string_lossy().replace('\'', "''")
}

pub fn major_version(version: &str) -> Option<u64> {
    version.split_whitespace().find_map(|part| {
        let token = part.trim_start_matches('v');
        let mut numbers = token.split('.');
        let major = numbers.next()?.parse::<u64>().ok()?;
        numbers.next()?.parse::<u64>().ok()?;
        if let Some(patch) = numbers.next() {
            patch.split(['-', '+']).next()?.parse::<u64>().ok()?;
        }
        Some(major)
    })
}
pub fn compatible_existing_version(version: &str) -> bool {
    major_version(version) == Some(1) && !version.contains('-')
}
