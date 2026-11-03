//! DuMate 1.0.89 personal-account configuration, distinct from generated OpenCode files.
use crate::inventory::HostPlatform;
use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
};

pub(crate) fn user_data(home: &Path, platform: HostPlatform) -> PathBuf {
    let base = match platform {
        HostPlatform::MacOs => home.join("Library/Application Support"),
        HostPlatform::Windows => {
            if home == crate::config::home_dir() {
                std::env::var_os("APPDATA")
                    .filter(|v| !v.is_empty())
                    .map(PathBuf::from)
                    .unwrap_or_else(|| home.join("AppData/Roaming"))
            } else {
                home.join("AppData/Roaming")
            }
        }
        HostPlatform::Unix => home.join(".config"),
    };
    base.join("qianfan-desktop-app")
}

pub(crate) fn personal_config(home: &Path, platform: HostPlatform) -> Result<PathBuf> {
    if platform == HostPlatform::Unix {
        bail!("DuMate 个人版目前仅支持已验证的 macOS Apple Silicon / Windows x64 客户端")
    }
    let root = user_data(home, platform);
    let bytes =
        fs::read(root.join("auth.json")).context("请先打开 DuMate 并登录个人版，再返回重新接入")?;
    let auth: Value =
        serde_json::from_slice(&bytes).context("无法识别 DuMate 登录配置，请重新登录个人版")?;
    if auth["activeProviderType"] != "bce" {
        bail!("此接入仅支持 DuMate 个人版，请在 DuMate 中切换到个人版并登录")
    }
    let active = auth["activeProfileId"]
        .as_str()
        .filter(|v| !v.is_empty())
        .context("请先在 DuMate 个人版完成登录，再返回接入")?;
    let profile = auth["accountProfiles"]
        .as_array()
        .and_then(|rows| {
            rows.iter()
                .find(|row| row["profileId"] == active && row["loginType"] == "bce")
        })
        .context("DuMate 活动个人账号无法确认，请重新登录")?;
    let user = profile["bceUserId"]
        .as_str()
        .filter(|id| {
            !id.is_empty()
                && id.len() <= 256
                && id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
        })
        .context("DuMate 个人账号目录标识无效，请重新登录")?;
    Ok(root
        .join("qianfan_desk_xdg")
        .join(user)
        .join("config/desktop-main/dumate-app.json"))
}

pub(crate) fn display_config(home: &Path, platform: HostPlatform) -> PathBuf {
    personal_config(home, platform).unwrap_or_else(|_| {
        user_data(home, platform).join("qianfan_desk_xdg/config/desktop-main/dumate-app.json")
    })
}

/// Explicit-operation check only: read-only inventory never executes process probes.
/// DuMate caches the entire preference document, so writing while it runs can lose edits.
pub(crate) fn require_stopped(config: &Path) -> Result<()> {
    // InstallationEnvironment supplies isolated homes to fixtures. A process in
    // this user's real profile cannot cache or overwrite those separate files.
    let platform = if cfg!(target_os = "macos") {
        HostPlatform::MacOs
    } else if cfg!(windows) {
        HostPlatform::Windows
    } else {
        HostPlatform::Unix
    };
    if !config.starts_with(user_data(&crate::config::home_dir(), platform)) {
        return Ok(());
    }
    #[cfg(target_os = "macos")]
    {
        for name in ["DuMate", "dumate-main-server"] {
            let status = std::process::Command::new("/usr/bin/pgrep")
                .args(["-x", name])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .context("无法确认 DuMate 是否已退出，请完全退出后重试")?;
            if status.success() {
                bail!("请先完全退出 DuMate（含后台进程）后重试；接入/取消接入后再打开以读取配置")
            }
            if status.code() != Some(1) {
                bail!("无法确认 DuMate 是否已退出，请完全退出后重试")
            }
        }
    }
    #[cfg(windows)]
    {
        let status = std::process::Command::new("powershell.exe").args([
            "-NoProfile", "-NonInteractive", "-Command",
            "$ErrorActionPreference='Stop'; if (Get-Process -Name DuMate,dumate-main-server -ErrorAction SilentlyContinue) { exit 2 }; exit 0",
        ]).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status()
            .context("无法确认 DuMate 是否已退出，请完全退出后重试")?;
        if !status.success() {
            bail!("请先完全退出 DuMate（含后台进程）后重试；接入/取消接入后再打开以读取配置")
        }
    }
    Ok(())
}
