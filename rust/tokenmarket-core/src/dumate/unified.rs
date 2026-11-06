//! Cold-start-only adapter. No external live updates, queue or control server.
use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::{fs, io::Read, path::Path};

pub(crate) const PLUGIN_NAME: &str = "tokenmarket-dumate-unified.js";
pub(crate) const POLICY_NAME: &str = "tokenmarket-unified.json";
pub(crate) const SOURCE: &str = include_str!("unified/plugin.js");
pub(crate) const MARKER: &str = "// Token Market DuMate unified cold-start adapter v2.";

pub(crate) fn validate_engine(
    application: &Path,
    platform: crate::inventory::HostPlatform,
) -> Result<()> {
    let (path, expected) = match platform {
        crate::inventory::HostPlatform::MacOs => (
            application.join("Contents/Resources/extra-resource/opencode/bin/dumate-opencode"),
            "e5f9eee34333afa45a2cdafcd634b7c1a583334f55e018f5126eb8387f7b6763",
        ),
        crate::inventory::HostPlatform::Windows => (
            application
                .parent()
                .context("搭子安装路径无效")?
                .join("resources/extra-resource/opencode/bin/dumate-opencode.exe"),
            "4cf28953c1f38f56451f43eff55707eb7593e43aed6cb49a43d3384c44cc343d",
        ),
        _ => bail!("当前平台尚不支持搭子统一接管"),
    };
    let mut file = fs::File::open(path).context("无法确认搭子引擎，请先完成安装后重试")?;
    let mut hash = Sha256::new();
    let mut chunk = [0u8; 65536];
    loop {
        let size = file.read(&mut chunk)?;
        if size == 0 {
            break;
        }
        hash.update(&chunk[..size]);
    }
    if format!("{:x}", hash.finalize()) != expected {
        bail!("当前搭子引擎版本尚未适配统一接管，请更新管理工具后重试；原配置已保留")
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn an_unrecognized_engine_never_passes_unified_preflight() {
        let home = tempfile::tempdir().unwrap();
        let app = home.path().join("DuMate.app");
        let engine = app.join("Contents/Resources/extra-resource/opencode/bin/dumate-opencode");
        fs::create_dir_all(engine.parent().unwrap()).unwrap();
        fs::write(engine, b"unexpected engine").unwrap();
        assert!(validate_engine(&app, crate::inventory::HostPlatform::MacOs)
            .unwrap_err()
            .to_string()
            .contains("原配置已保留"));
    }
}
