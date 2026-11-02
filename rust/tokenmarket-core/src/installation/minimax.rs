//! Official 0.7.2 release archive, paired with its tagged public schema.
use super::{InstallPlan, InstallStage, InstallStep};
use crate::{
    inventory::{HostPlatform, InstallationEnvironment},
    ToolId,
};
use anyhow::{bail, Result};
use std::path::Path;
const VERSION: &str = "0.7.2";
const SHA256: &str = "aa055b696945286cbab62f8b748835de1428e4d08a598bbf5de4c8a5de71f4e4";
fn ps(path: &Path) -> String {
    path.to_string_lossy().replace('\'', "''")
}
pub(super) fn plan(
    environment: &InstallationEnvironment,
    architecture: &str,
) -> Result<InstallPlan> {
    let asset = super::node::asset(environment.platform, architecture)?;
    let target = asset.target;
    let sha = asset.sha256;
    if environment.platform == HostPlatform::Unix && cfg!(target_env = "musl") {
        bail!("MiniMax Code 官方一键安装不支持 Alpine/musl；请在受支持的 glibc Linux 环境安装")
    }
    let windows = environment.platform == HostPlatform::Windows;
    let prefix = environment.home.join(".tokenmarket/tools/minimax-code");
    let downloads = environment
        .home
        .join(".tokenmarket/installers/minimax-code");
    let package = downloads.join("minimax-code-0.7.2.tar.gz");
    let node_archive = downloads.join(&asset.filename);
    let node_url = asset.url;
    let path = environment
        .command_directories
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .chain(std::env::var("PATH").ok())
        .collect::<Vec<_>>()
        .join(if windows { ";" } else { ":" });
    let env = vec![("PATH".into(), path)];
    let mut steps=vec![InstallStep{stage:InstallStage::Downloading,program:String::new(),args:vec![],environment:vec![],download:Some((format!("https://github.com/MiniMax-AI/minimax-code/releases/download/v{VERSION}/minimax-code-{VERSION}.tar.gz"),package.clone()))}];
    if windows {
        let runtime = prefix.join("runtime");
        let prepare = format!(
            r#"$ErrorActionPreference='Stop'
if ((Get-FileHash -Algorithm SHA256 -LiteralPath '{package}').Hash.ToLowerInvariant() -ne '{SHA256}') {{ throw 'MiniMax Code SHA256 verification failed' }}
function Usable-Node($candidate) {{
 if (-not $candidate -or -not (Test-Path -LiteralPath $candidate) -or -not (Test-Path -LiteralPath (Join-Path (Split-Path $candidate) 'npm.cmd'))) {{ return $false }}
 & $candidate -e 'const [a,b]=process.versions.node.split(".").map(Number);process.exit((a===22 && b>=19)||(a===24 && b>=2)||a===25||a===26?0:1)'
 return $LASTEXITCODE -eq 0
}}
$node=(Get-Command node.exe -ErrorAction SilentlyContinue).Source
if (-not (Usable-Node $node)) {{
 $node='{node}'
 if (-not (Usable-Node $node)) {{
  Write-Output 'Preparing private Node.js {node_version} runtime'
  & curl.exe --fail --location --silent --show-error --proto '=https' --proto-redir '=https' --connect-timeout 30 --max-time 300 --output '{archive}' '{url}'
  if ($LASTEXITCODE -ne 0) {{ throw 'Node.js download failed' }}
  if ((Get-FileHash -Algorithm SHA256 -LiteralPath '{archive}').Hash.ToLowerInvariant() -ne '{sha}') {{ throw 'Node.js SHA256 verification failed' }}
  New-Item -ItemType Directory -Force -Path '{runtime}' | Out-Null
  Expand-Archive -LiteralPath '{archive}' -DestinationPath '{staging}' -Force
  Copy-Item -Path '{staging}/node-v{node_version}-{target}/*' -Destination '{runtime}' -Recurse -Force
  if (-not (Usable-Node $node)) {{ throw 'Private Node.js runtime was not installed' }}
 }}
}}
New-Item -ItemType Directory -Force -Path '{prefix}' | Out-Null
Set-Content -LiteralPath '{node_path}' -Value $node -Encoding UTF8
"#,
            node_version = super::node::VERSION,
            package = ps(&package),
            node = ps(&runtime.join("node.exe")),
            archive = ps(&node_archive),
            url = node_url,
            runtime = ps(&runtime),
            staging = ps(&downloads.join("node-staging")),
            prefix = ps(&prefix),
            node_path = ps(&prefix.join("node-path"))
        );
        let install = format!(
            r#"$ErrorActionPreference='Stop'
if ((Get-FileHash -Algorithm SHA256 -LiteralPath '{package}').Hash.ToLowerInvariant() -ne '{SHA256}') {{ throw 'MiniMax Code SHA256 verification failed' }}
$node=(Get-Content -LiteralPath '{node_path}' -Raw).Trim()
$env:PATH=(Split-Path $node)+';'+$env:PATH
& (Join-Path (Split-Path $node) 'npm.cmd') install --global --prefix '{npm_prefix}' '{package}' --registry=https://registry.npmjs.org/ --include=optional --ignore-scripts=false --allow-scripts=better-sqlite3 --no-audit --no-fund
if ($LASTEXITCODE -ne 0) {{ throw 'MiniMax Code npm/native dependency installation failed; retain runtime and retry after checking the output or installing the official Python/C++ build prerequisites' }}
New-Item -ItemType Directory -Force -Path '{bin}' | Out-Null
@'
$ErrorActionPreference='Stop'
$prefix=Split-Path $PSScriptRoot
$node=(Get-Content -LiteralPath (Join-Path $prefix 'node-path') -Raw).Trim()
$env:PATH=(Split-Path $node)+';'+$env:PATH
& $node (Join-Path $prefix 'package/node_modules/@minimax-ai/code/cli.js') @args
exit $LASTEXITCODE
'@ | Set-Content -LiteralPath '{launcher}' -Encoding UTF8
@'
@echo off
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0mcode.ps1" %*
exit /b %errorlevel%
'@ | Set-Content -LiteralPath '{cmd}' -Encoding ASCII
Write-Output 'MiniMax Code installed; connect separately'
"#,
            package = ps(&package),
            node_path = ps(&prefix.join("node-path")),
            npm_prefix = ps(&prefix.join("package")),
            bin = ps(&prefix.join("bin")),
            launcher = ps(&prefix.join("bin/mcode.ps1")),
            cmd = ps(&prefix.join("bin/mcode.cmd"))
        );
        for (stage, script) in [
            (InstallStage::Dependencies, prepare),
            (InstallStage::Installing, install),
        ] {
            steps.push(InstallStep {
                stage,
                program: "powershell.exe".into(),
                args: vec![
                    "-NoProfile".into(),
                    "-NonInteractive".into(),
                    "-ExecutionPolicy".into(),
                    "Bypass".into(),
                    "-Command".into(),
                    script,
                ],
                environment: env.clone(),
                download: None,
            });
        }
    } else {
        let prepare = r#"set -eu
prefix=$1; package=$2; package_sha=$3; archive=$4; origin=$5; checksum=$6
verify() { if command -v sha256sum >/dev/null 2>&1; then actual=$(sha256sum "$1"); else actual=$(shasum -a 256 "$1"); fi; actual=${actual%% *}; [ "$actual" = "$2" ] || { echo 'MiniMax Code/Node.js SHA256 verification failed' >&2; exit 1; }; }
verify "$package" "$package_sha"
if [ -f /etc/alpine-release ] || ldd --version 2>&1 | grep -qi musl; then echo 'MiniMax Code official installer does not support Alpine/musl' >&2; exit 1; fi
__NODE_PREPARE__
mkdir -p "$prefix"
printf '%s\n' "$node" > "$prefix/node-path"
"#.replace("__NODE_PREPARE__",&super::node::posix_prepare("const [a,b]=process.versions.node.split(\".\").map(Number);process.exit((a===22 && b>=19)||(a===24 && b>=2)||a===25||a===26?0:1)"));
        steps.push(InstallStep {
            stage: InstallStage::Dependencies,
            program: "/bin/sh".into(),
            args: vec![
                "-c".into(),
                prepare.into(),
                "tokenmarket-minimax-dependencies".into(),
                prefix.to_string_lossy().into(),
                package.to_string_lossy().into(),
                SHA256.into(),
                node_archive.to_string_lossy().into(),
                node_url,
                sha.into(),
            ],
            environment: env.clone(),
            download: None,
        });
        let install = r#"set -eu
prefix=$1; package=$2; package_sha=$3
if command -v sha256sum >/dev/null 2>&1; then actual=$(sha256sum "$package"); else actual=$(shasum -a 256 "$package"); fi
actual=${actual%% *}; [ "$actual" = "$package_sha" ] || { echo 'MiniMax Code SHA256 verification failed' >&2; exit 1; }
node=$(cat "$prefix/node-path")
PATH="$(dirname "$node"):$PATH"; export PATH
mkdir -p "$prefix/package" "$prefix/bin"
"$(dirname "$node")/npm" install --global --prefix "$prefix/package" "$package" --registry=https://registry.npmjs.org/ --include=optional --ignore-scripts=false --allow-scripts=better-sqlite3 --no-audit --no-fund || { echo 'MiniMax Code native dependency installation failed; retained runtime. Check the output and official Python/C++ prerequisites before retrying.' >&2; exit 1; }
cat > "$prefix/bin/mcode" <<'TOKENMARKET_MCODE'
#!/bin/sh
set -eu
prefix=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
node=$(cat "$prefix/node-path")
PATH="$(dirname "$node"):$PATH"; export PATH
exec "$node" "$prefix/package/lib/node_modules/@minimax-ai/code/cli.js" "$@"
TOKENMARKET_MCODE
chmod 755 "$prefix/bin/mcode"
echo 'MiniMax Code installed; connect separately'
"#;
        steps.push(InstallStep {
            stage: InstallStage::Installing,
            program: "/bin/sh".into(),
            args: vec![
                "-c".into(),
                install.into(),
                "tokenmarket-minimax-install".into(),
                prefix.to_string_lossy().into(),
                package.to_string_lossy().into(),
                SHA256.into(),
            ],
            environment: env,
            download: None,
        });
    }
    Ok(InstallPlan {
        tool: ToolId::MiniMaxCode,
        version: VERSION.into(),
        command: "mcode".into(),
        steps,
    })
}
