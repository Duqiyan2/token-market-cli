//! OMP's official standalone binary embeds Bun, native helpers and Brush shell.
use super::{InstallPlan, InstallStage, InstallStep};
use crate::{
    inventory::{HostPlatform, InstallationEnvironment},
    ToolId,
};
use anyhow::{bail, Result};
use std::path::Path;
const VERSION: &str = "18.15.0";
fn ps(path: &Path) -> String {
    path.to_string_lossy().replace('\'', "''")
}
pub(super) fn plan(
    environment: &InstallationEnvironment,
    architecture: &str,
) -> Result<InstallPlan> {
    plan_release(environment, architecture, VERSION, None)
}
pub(super) fn plan_release(
    environment: &InstallationEnvironment,
    architecture: &str,
    version: &str,
    digest: Option<&str>,
) -> Result<InstallPlan> {
    // Read-only libc detection: inventory never launches ldd or another executable.
    let musl = environment.platform == HostPlatform::Unix
        && (cfg!(target_env = "musl")
            || Path::new("/etc/alpine-release").exists()
            || Path::new("/lib/ld-musl-x86_64.so.1").exists()
            || Path::new("/lib/ld-musl-aarch64.so.1").exists());
    let (asset, hash) = match (environment.platform, architecture, musl) {
        (HostPlatform::MacOs, "aarch64", _) => (
            "omp-darwin-arm64",
            "d36dfabf0ee52d6f08efadb59c46f0349545c6f6975522417adae29646e73e34",
        ),
        (HostPlatform::MacOs, "x86_64", _) => (
            "omp-darwin-x64",
            "dc86c67ff66418403dcf62a62bf8129fae8f24e50a1b4099716c8d36914fe204",
        ),
        (HostPlatform::Unix, "aarch64", false) => (
            "omp-linux-arm64",
            "6d7d65030ba562d73cb0687b9710e0ac597b362a185d1911e7b211706dadd4ac",
        ),
        (HostPlatform::Unix, "x86_64", false) => (
            "omp-linux-x64",
            "6737ee2415555a3656cc7b89c14d1c25c18658ee4c0fd4f7309705149ccb7b0f",
        ),
        (HostPlatform::Unix, "aarch64", true) => (
            "omp-linux-musl-arm64",
            "f20148c4765287a9cb473ad7337ceb3841ecf602e0478c220e02e30d8cfc1ab2",
        ),
        (HostPlatform::Unix, "x86_64", true) => (
            "omp-linux-musl-x64",
            "f3e8152c17b1ea7ce054103c55dadfe2c18991c76dbf4057cd0d2049345ba05c",
        ),
        (HostPlatform::Windows, "aarch64", _) => (
            "omp-windows-arm64.exe",
            "a9a4d83c509322f52fe9f3b187163d9c0cbe878d8a1ca4883032820c697a8184",
        ),
        (HostPlatform::Windows, "x86_64", _) => (
            "omp-windows-x64.exe",
            "978846a816cb77f44a55e32b16493375ca837fbf87e0aec35a9c1c934bff0cd0",
        ),
        _ => bail!("Oh My Pi 官方独立二进制不支持当前架构 {architecture}"),
    };
    let hash = digest.unwrap_or(hash);
    let windows = environment.platform == HostPlatform::Windows;
    let binary = environment
        .home
        .join(".tokenmarket/installers/oh-my-pi")
        .join(asset);
    let prefix = environment.home.join(".tokenmarket/tools/oh-my-pi/bin");
    let command = prefix.join(if windows { "omp.exe" } else { "omp" });
    let (program, args) = if windows {
        let script=format!("$ErrorActionPreference='Stop'; if ($PSVersionTable.PSVersion -lt [version]'5.1') {{ throw 'Oh My Pi requires PowerShell 5.1 or newer' }}; if ((Get-FileHash -Algorithm SHA256 -LiteralPath '{}').Hash.ToLowerInvariant() -ne '{}') {{ throw 'Oh My Pi checksum mismatch' }}; & '{}' --version; if ($LASTEXITCODE -ne 0) {{ throw 'Oh My Pi binary cannot start; no command was installed' }}; New-Item -ItemType Directory -Force -Path '{}' | Out-Null; Copy-Item -LiteralPath '{}' -Destination '{}' -Force",ps(&binary),hash,ps(&binary),ps(&prefix),ps(&binary),ps(&command));
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
if command -v sha256sum >/dev/null 2>&1; then actual=$(sha256sum "$1"); else actual=$(shasum -a 256 "$1"); fi
actual=${actual%% *}
[ "$actual" = "$3" ] || { printf '%s\n' 'Oh My Pi checksum mismatch' >&2; exit 1; }
chmod u+x "$1"
if ! "$1" --version; then
  printf '%s\n' 'Oh My Pi binary cannot start. On Alpine/musl install libstdc++ and libgcc (apk add libstdc++ libgcc), then retry; no command was installed.' >&2
  exit 1
fi
mkdir -p "$2"
cp -f "$1" "$2/omp"
chmod u+x "$2/omp"
"#;
        (
            "sh",
            vec![
                "-c".into(),
                script.into(),
                "omp-install".into(),
                binary.to_string_lossy().into(),
                prefix.to_string_lossy().into(),
                hash.into(),
            ],
        )
    };
    Ok(InstallPlan {
        tool: ToolId::OhMyPi,
        version: version.into(),
        command: "omp".into(),
        steps: vec![
            InstallStep {
                stage: InstallStage::Downloading,
                program: String::new(),
                args: vec![],
                environment: vec![],
                download: Some((
                    format!(
                        "https://github.com/can1357/oh-my-pi/releases/download/v{version}/{asset}"
                    ),
                    binary,
                )),
            },
            InstallStep {
                stage: InstallStage::Installing,
                program: program.into(),
                args,
                environment: vec![],
                download: None,
            },
        ],
    })
}
