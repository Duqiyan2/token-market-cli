//! Retire our experimental bridge without changing DuMate's native provider settings.
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

pub(crate) fn retire(config: &Path) -> Result<usize> {
    let config_dir = config
        .parent()
        .and_then(Path::parent)
        .context("搭子配置目录无效")?;
    let script = config_dir.join("opencode/plugins/tokenmarket-dumate.js");
    let state = config_dir
        .parent()
        .context("搭子账号目录无效")?
        .join("state/tokenmarket-dumate");
    // The old extension is never newly installed by this version. Preserve foreign files.
    let owned = match fs::symlink_metadata(&script) {
        Ok(meta)
            if meta.is_file() && !meta.file_type().is_symlink() && meta.len() <= 256 * 1024 =>
        {
            let bytes = fs::read(&script)?;
            bytes
                .starts_with(b"// Token Market DuMate 1.0.89 bridge.")
                .then_some(bytes)
        }
        Ok(_) => None,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    if owned.is_none() {
        return Ok(0);
    }
    for path in [state.parent().unwrap(), state.as_path()] {
        if fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink()) {
            bail!("旧接管扩展的状态目录为符号链接，请手动移除扩展后重试")
        }
        fs::create_dir_all(path)?;
    }
    let policy = state.join("policy.json");
    if let Ok(meta) = fs::symlink_metadata(&policy) {
        if meta.file_type().is_symlink() || !meta.is_file() || meta.len() > 65536 {
            bail!("旧接管扩展的策略文件无法安全清理")
        }
        let mut value: Value = serde_json::from_slice(&fs::read(&policy)?)
            .context("旧接管策略无法识别，请先移除扩展并重启搭子")?;
        if value["protocol"] == 1 {
            // Invalidate an old queued update so it cannot later replace the saved native config.
            value["enabled"] = json!(false);
            value["revision"] = json!(rand::random::<[u8; 16]>()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>());
            super::super::connections::atomic_write(&policy, &serde_json::to_vec_pretty(&value)?)?;
        }
    }
    let bytes = owned.unwrap();
    let hash: String = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let backup = state.join(format!("retired-bridge-{hash}.js.disabled"));
    if backup.exists() {
        let meta = fs::symlink_metadata(&backup)?;
        if meta.file_type().is_symlink() || fs::read(&backup)? != bytes {
            bail!("旧扩展备份已改变，请检查后重试")
        }
        fs::remove_file(&script)?;
    } else {
        fs::rename(&script, &backup)?;
    }
    Ok(1)
}

pub(crate) fn retire_all(home: &Path, platform: crate::inventory::HostPlatform) -> Result<usize> {
    let accounts = super::user_data(home, platform).join("qianfan_desk_xdg");
    let dirs = match fs::read_dir(accounts) {
        Ok(dirs) => dirs,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(error.into()),
    };
    let mut count = 0;
    for entry in dirs {
        let entry = entry?;
        if entry.file_type()?.is_dir()
            && entry.file_name().to_str().is_some_and(|name| {
                !name.is_empty()
                    && name
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
            })
        {
            count += retire(&entry.path().join("config/desktop-main/dumate-app.json"))?;
        }
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retiring_preserves_native_settings_and_disables_owned_code_without_deleting_user_edits() {
        let root = tempfile::tempdir().unwrap();
        let config = root
            .path()
            .join("account/config/desktop-main/dumate-app.json");
        fs::create_dir_all(config.parent().unwrap()).unwrap();
        fs::write(
            &config,
            b"{\"customLLMProvider\":{\"model\":\"keep\"},\"theme\":\"dark\"}",
        )
        .unwrap();
        let script = root
            .path()
            .join("account/config/opencode/plugins/tokenmarket-dumate.js");
        fs::create_dir_all(script.parent().unwrap()).unwrap();
        let bytes = b"// Token Market DuMate 1.0.89 bridge.\n// user amended this bridge\n";
        fs::write(&script, bytes).unwrap();
        let state = root.path().join("account/state/tokenmarket-dumate");
        fs::create_dir_all(&state).unwrap();
        fs::write(
            state.join("policy.json"),
            b"{\"protocol\":1,\"enabled\":true,\"revision\":\"old\"}",
        )
        .unwrap();
        let before = fs::read(&config).unwrap();
        assert_eq!(retire(&config).unwrap(), 1);
        assert!(!script.exists());
        assert_eq!(fs::read(&config).unwrap(), before);
        let policy: Value =
            serde_json::from_slice(&fs::read(state.join("policy.json")).unwrap()).unwrap();
        assert_eq!(policy["enabled"], false);
        assert_ne!(policy["revision"], "old");
        let backup = fs::read_dir(state)
            .unwrap()
            .flatten()
            .find(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .ends_with(".js.disabled")
            })
            .unwrap();
        assert_eq!(fs::read(backup.path()).unwrap(), bytes);
        assert_eq!(retire(&config).unwrap(), 0);
        fs::write(&script, b"user unrelated plugin").unwrap();
        assert_eq!(retire(&config).unwrap(), 0);
        assert_eq!(fs::read(script).unwrap(), b"user unrelated plugin");
    }
}
