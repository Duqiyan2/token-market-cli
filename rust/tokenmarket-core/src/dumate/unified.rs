//! Cold-start-only adapter. No external live updates, queue or control server.
use anyhow::{bail, Context, Result};
use std::{fs, io::Read, path::Path};

pub(crate) const PLUGIN_NAME: &str = "tokenmarket-dumate-unified.js";
pub(crate) const POLICY_NAME: &str = "tokenmarket-unified.json";
pub(crate) const SOURCE: &str = include_str!("unified/plugin.js");
pub(crate) const MARKER: &str = "// Token Market DuMate unified cold-start adapter v2.";

pub(crate) fn validate_engine(
    application: &Path,
    platform: crate::inventory::HostPlatform,
) -> Result<()> {
    let path = match platform {
        crate::inventory::HostPlatform::MacOs => {
            application.join("Contents/Resources/extra-resource/opencode/bin/dumate-opencode")
        }
        crate::inventory::HostPlatform::Windows => application
            .parent()
            .context("搭子安装路径无效")?
            .join("resources/extra-resource/opencode/bin/dumate-opencode.exe"),
        _ => bail!("当前平台尚不支持搭子统一接管"),
    };
    let mut file = fs::File::open(path).context("无法确认搭子引擎，请先完成安装后重试")?;
    validate_capabilities(&mut file)
}

// Inspect the installed engine's embedded JavaScript without executing it or
// reading a live account. Version/build/hash changes are not incompatibility.
// This is a structural preflight, not proof of semantics: the plugin additionally
// validates the actual provider/configuration before enabling model forwarding.
fn validate_capabilities(engine: &mut impl Read) -> Result<()> {
    let required = [
        ("{plugin,plugins}/*.{ts,js}", "账号级扩展加载入口"),
        ("config?.(", "扩展配置入口"),
        ("\"chat.message\"", "消息扩展入口"),
        ("\"chat.params\"", "模型参数扩展入口"),
        ("QianfanPersonalQuota", "个人版文本模型来源"),
        ("qianfan-multimodal", "原生媒体模型来源"),
        ("modelSelection", "固定模型选择"),
        ("modelSettings", "消息模型设置"),
        ("ROUTING_DECIDE_API_URL", "任务路由接口"),
        ("XDG_CONFIG_HOME", "账号配置目录"),
        ("globalThis.fetch", "请求拦截入口"),
    ];
    let overlap = required
        .iter()
        .map(|(needle, _)| needle.len())
        .max()
        .unwrap()
        - 1;
    let finders = required
        .iter()
        .map(|(needle, _)| memchr::memmem::Finder::new(needle.as_bytes()))
        .collect::<Vec<_>>();
    let mut found = vec![false; required.len()];
    let mut buffer = vec![0u8; 65536 + overlap];
    let mut retained = 0;
    loop {
        let size = engine.read(&mut buffer[retained..])?;
        if size == 0 {
            break;
        }
        let end = retained + size;
        // Binary substring search avoids decoding a large native executable.
        // Retain raw bytes across reads to catch tokens split at a boundary.
        for (index, finder) in finders.iter().enumerate() {
            if !found[index] {
                found[index] = finder.find(&buffer[..end]).is_some();
            }
        }
        if found.iter().all(|found| *found) {
            return Ok(());
        }
        retained = overlap.min(end);
        buffer.copy_within(end - retained..end, 0);
    }
    let missing = required
        .iter()
        .zip(found)
        .filter_map(|((_, name), found)| (!found).then_some(*name))
        .collect::<Vec<_>>();
    bail!(
        "当前搭子引擎未检测到统一接管所需能力：{}；暂不支持接入，原配置已保留",
        missing.join("、")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    const COMPATIBLE: &[u8] = br#"{plugin,plugins}/*.{ts,js} config?.( "chat.message" "chat.params" QianfanPersonalQuota qianfan-multimodal modelSelection modelSettings ROUTING_DECIDE_API_URL XDG_CONFIG_HOME globalThis.fetch"#;
    #[test]
    #[ignore = "read-only capability check of an engine supplied via TOKEN_MARKET_DUMATE_ENGINE"]
    fn installed_engine_capabilities() {
        let path = std::env::var_os("TOKEN_MARKET_DUMATE_ENGINE").expect("engine path");
        validate_capabilities(&mut fs::File::open(path).unwrap()).unwrap();
    }
    #[test]
    fn compatible_engine_is_accepted_across_build_changes_and_chunk_boundaries() {
        for build in ["1.0.88.400", "1.0.89.0", "2.0.0.500"] {
            // Put the first token across the scanner's first read boundary.
            let mut bytes = vec![0xff; 65536 + 26 - 6];
            bytes.extend_from_slice(COMPATIBLE);
            bytes.extend_from_slice(build.as_bytes());
            for platform in [
                crate::inventory::HostPlatform::MacOs,
                crate::inventory::HostPlatform::Windows,
            ] {
                let home = tempfile::tempdir().unwrap();
                let (app, engine) = match platform {
                    crate::inventory::HostPlatform::MacOs => {
                        let app = home.path().join("DuMate.app");
                        let engine = app
                            .join("Contents/Resources/extra-resource/opencode/bin/dumate-opencode");
                        (app, engine)
                    }
                    _ => (
                        home.path().join("DuMate.exe"),
                        home.path()
                            .join("resources/extra-resource/opencode/bin/dumate-opencode.exe"),
                    ),
                };
                fs::create_dir_all(engine.parent().unwrap()).unwrap();
                fs::write(&engine, &bytes).unwrap();
                validate_engine(&app, platform).unwrap();
            }
        }
    }
    #[test]
    fn missing_capability_reports_the_incompatible_interface() {
        let bytes = String::from_utf8(COMPATIBLE.to_vec())
            .unwrap()
            .replace("chat.message", "gone.message");
        let error = validate_capabilities(&mut bytes.as_bytes())
            .unwrap_err()
            .to_string();
        assert!(error.contains("消息扩展入口"));
        assert!(error.contains("原配置已保留"));
    }
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
