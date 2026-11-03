//! DuMate 1.0.89 account configuration and read-only enterprise permission checks.
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

fn enterprise_profile(home: &Path, platform: HostPlatform) -> Result<(PathBuf, Value, url::Url)> {
    if platform == HostPlatform::Unix {
        bail!("DuMate 企业版目前仅支持 macOS Apple Silicon / Windows x64 客户端")
    }
    let root = user_data(home, platform);
    let auth: Value = serde_json::from_slice(
        &fs::read(root.join("auth.json"))
            .context("请先在 DuMate 登录企业版，再完全退出应用后接入")?,
    )
    .context("无法识别 DuMate 企业登录配置，请重新登录")?;
    if auth["activeProviderType"] != "enterprise" {
        bail!("请先在 DuMate 切换到企业版并登录，再完全退出应用后接入")
    }
    let active = auth["activeProfileId"]
        .as_str()
        .filter(|s| !s.is_empty())
        .context("DuMate 活动企业账号无法确认，请重新登录")?;
    let mut profile = auth["accountProfiles"]
        .as_array()
        .and_then(|rows| {
            rows.iter()
                .find(|row| row["profileId"] == active && row["loginType"] == "enterprise")
        })
        .context("DuMate 活动企业账号无法确认，请重新登录")?
        .clone();
    // The active slot can be refreshed or logged out independently of the saved
    // account pool. Never revive an archived profile's cookie after logout.
    let active_cookies = auth["cookies"]
        .as_str()
        .filter(|s| !s.is_empty())
        .context("DuMate 企业登录已退出，请重新登录")?;
    profile["encryptedCookies"] = Value::String(active_cookies.to_owned());
    profile["bceUserId"]
        .as_str()
        .filter(|s| {
            !s.is_empty()
                && s.len() <= 256
                && s.bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
        })
        .context("DuMate 企业账号目录标识无效，请重新登录")?;
    let origin = url::Url::parse(
        profile["enterpriseApiBaseUrl"]
            .as_str()
            .context("DuMate 企业服务地址缺失，请重新登录")?,
    )
    .map_err(|_| anyhow::anyhow!("DuMate 企业服务地址无效，请重新登录"))?;
    let fixture_local = home != crate::config::home_dir()
        && matches!(origin.host_str(), Some("localhost" | "127.0.0.1"));
    if origin.host_str().is_none()
        || !origin.username().is_empty()
        || origin.password().is_some()
        || origin.query().is_some()
        || origin.fragment().is_some()
        || origin.path() != "/"
        || !(origin.scheme() == "https" || (fixture_local && origin.scheme() == "http"))
    {
        bail!("DuMate 企业服务必须是有效 HTTPS 来源地址，请重新登录")
    }
    if let Some(mirror) = auth["enterpriseApiBaseUrl"].as_str() {
        if mirror.trim_end_matches('/') != origin.as_str().trim_end_matches('/') {
            bail!("DuMate 活动企业服务地址不一致，请重新登录")
        }
    }
    Ok((root, profile, origin))
}

pub(crate) fn enterprise_config(home: &Path, platform: HostPlatform) -> Result<PathBuf> {
    let (root, profile, _) = enterprise_profile(home, platform)?;
    Ok(root
        .join("qianfan_desk_xdg")
        .join(profile["bceUserId"].as_str().unwrap())
        .join("config/desktop-main/personal-models.json"))
}
pub(crate) fn enterprise_display_config(home: &Path, platform: HostPlatform) -> PathBuf {
    enterprise_config(home, platform).unwrap_or_else(|_| {
        user_data(home, platform).join("qianfan_desk_xdg/config/desktop-main/personal-models.json")
    })
}
pub(crate) fn enterprise_fingerprint(home: &Path, platform: HostPlatform) -> Result<String> {
    use sha2::{Digest, Sha256};
    let (_, profile, origin) = enterprise_profile(home, platform)?;
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&serde_json::json!([
            profile["profileId"],
            profile["loginType"],
            profile["bceUserId"],
            profile["bceAccountId"],
            origin.as_str()
        ]))?)
    ))
}

/// Read only the active enterprise session. Never repair its key, persist its
/// plaintext, or send DuMate credentials to a Token Market endpoint.
pub(crate) async fn authorize_enterprise(home: &Path, platform: HostPlatform) -> Result<()> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
    let (root, profile, origin) = enterprise_profile(home, platform)?;
    let key_bytes = fs::read(root.join(".cookie-key"))
        .context("无法读取 DuMate 企业登录密钥，请在 DuMate 重新登录后重试")?;
    let key = UnboundKey::new(&AES_256_GCM, &key_bytes)
        .map_err(|_| anyhow::anyhow!("DuMate 企业登录密钥无效，请重新登录"))?;
    let encoded = profile["encryptedCookies"]
        .as_str()
        .filter(|s| s.len() <= 1024 * 1024)
        .context("DuMate 企业登录凭据缺失，请重新登录")?;
    let blob = STANDARD
        .decode(encoded)
        .map_err(|_| anyhow::anyhow!("DuMate 企业登录凭据无效，请重新登录"))?;
    if blob.len() < 28 {
        bail!("DuMate 企业登录凭据无效，请重新登录")
    }
    let nonce = Nonce::try_assume_unique_for_key(&blob[..12])
        .map_err(|_| anyhow::anyhow!("DuMate 企业登录凭据无效"))?;
    let mut ciphertext = blob[28..].to_vec();
    ciphertext.extend_from_slice(&blob[12..28]);
    let plaintext = LessSafeKey::new(key)
        .open_in_place(nonce, Aad::empty(), &mut ciphertext)
        .map_err(|_| anyhow::anyhow!("无法解密 DuMate 企业登录凭据，请重新登录"))?;
    let cookies: Value = serde_json::from_slice(plaintext)
        .map_err(|_| anyhow::anyhow!("DuMate 企业登录凭据格式无效，请重新登录"))?;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(15))
        .build()?;
    for (path, pointer) in [
        (
            "/api/dumate-enterprise/app/v1/model/config",
            "/appCreateModelEnabled",
        ),
        (
            "/api/dumate-enterprise/app/v1/modelwhite/config",
            "/switchs/model_controller",
        ),
    ] {
        let target = origin.join(path)?;
        let (cookie, csrf) = enterprise_cookie_headers(&cookies, &target)?;
        let mut response = client
            .get(target)
            .header("Cookie", cookie)
            .header("csrftoken", csrf)
            .header("Content-Type", "application/json")
            .header("X-Requested-With", "XMLHttpRequest")
            .header("Origin", origin.origin().ascii_serialization())
            .header("Referer", origin.as_str())
            .header("dumate-version", "1.0.89")
            .header("X-Dumate-Client-Type", "desktop")
            .send()
            .await
            .map_err(|_| anyhow::anyhow!("无法确认 DuMate 企业权限，请检查企业网络和登录后重试"))?;
        if response.status() != reqwest::StatusCode::OK {
            bail!("DuMate 企业权限校验未通过，请重新登录并确认管理员已开放自定义模型与模型选择权限")
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| anyhow::anyhow!("无法读取 DuMate 企业权限响应"))?
        {
            if bytes.len() + chunk.len() > 1024 * 1024 {
                bail!("DuMate 企业权限响应无效")
            }
            bytes.extend_from_slice(&chunk);
        }
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|_| anyhow::anyhow!("DuMate 企业登录已失效或权限响应无效，请重新登录"))?;
        if value.pointer(pointer) != Some(&Value::Bool(true))
            || value["success"] == false
            || value["needLogin"] == true
            || value.get("redirect").is_some()
            || value.get("code").is_some_and(|v| v != 0 && v != "0")
        {
            bail!(
                "企业管理员尚未开放自定义模型与模型选择权限，或登录已失效；请在 DuMate 确认后重试"
            )
        }
    }
    Ok(())
}

fn enterprise_cookie_headers(cookies: &Value, target: &url::Url) -> Result<(String, String)> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs_f64();
    let host = target
        .host_str()
        .context("企业服务缺少主机")?
        .to_ascii_lowercase();
    let mut matching = Vec::new();
    for cookie in cookies
        .as_array()
        .context("DuMate 企业登录 Cookie 格式无效")?
    {
        let Some(domain) = cookie["domain"].as_str() else {
            continue;
        };
        let domain = domain.trim_start_matches('.').to_ascii_lowercase();
        if domain.is_empty()
            || !(host == domain
                || (cookie["hostOnly"] != true && host.ends_with(&format!(".{domain}"))))
        {
            continue;
        }
        if cookie["secure"] == true && target.scheme() != "https" {
            continue;
        }
        if cookie["expirationDate"]
            .as_f64()
            .is_some_and(|expiry| expiry <= now)
        {
            continue;
        }
        let path = cookie["path"].as_str().unwrap_or("/");
        if !path.starts_with('/')
            || !(target.path() == path
                || (target.path().starts_with(path)
                    && (path.ends_with('/')
                        || target.path().as_bytes().get(path.len()) == Some(&b'/'))))
        {
            continue;
        }
        let (Some(name), Some(value)) = (cookie["name"].as_str(), cookie["value"].as_str()) else {
            continue;
        };
        if name.is_empty()
            || !name
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&c))
            || !value
                .bytes()
                .all(|c| (0x21..=0x7e).contains(&c) && c != b';')
        {
            bail!("DuMate 企业登录 Cookie 无效，请重新登录")
        }
        matching.push((path.len(), name, value));
    }
    matching.sort_by_key(|v| std::cmp::Reverse(v.0));
    if matching.is_empty() {
        bail!("DuMate 企业登录 Cookie 已失效，请重新登录")
    }
    let csrf = matching
        .iter()
        .find(|(_, name, _)| *name == "bce-user-info")
        .map(|(_, _, value)| value.trim_matches(['\'', '"']).to_owned())
        .unwrap_or_default();
    let cookie = matching
        .iter()
        .map(|(_, name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join("; ");
    Ok((cookie, csrf))
}
