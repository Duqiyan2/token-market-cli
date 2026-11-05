//! Latest-release upgrade routes. Keep the effective install channel and prefix.
use super::{InstallPlan, InstallStage, InstallStep};
use crate::{
    inventory::{HostPlatform, InstallationEnvironment},
    ToolId,
};
use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub(super) struct UpgradePlan {
    pub plan: InstallPlan,
    pub probe: PathBuf,
    pub managed: bool,
    pub anchored: bool,
}
impl UpgradePlan {
    pub(super) fn revision(&self) -> String {
        use sha2::{Digest, Sha256};
        let mut digest = Sha256::new();
        digest.update(format!("{:?}", self).as_bytes());
        digest.update(format!("{:?}", self.probe.canonicalize().ok()).as_bytes());
        if let Ok(meta) = std::fs::metadata(&self.probe) {
            digest.update(meta.len().to_le_bytes());
            digest.update(format!("{:?}", meta.modified().ok()).as_bytes());
        }
        format!("{:x}", digest.finalize())
    }
}
pub(super) enum Route {
    Automatic(UpgradePlan),
    Manual(String),
}
pub(super) fn normalized_version(value: &str) -> Result<String> {
    let value = value
        .strip_prefix("@moonshot-ai/kimi-code@")
        .unwrap_or(value);
    let value = value.strip_prefix('v').unwrap_or(value);
    if value.len() > 80
        || value.is_empty()
        || !value.starts_with(|c: char| c.is_ascii_digit())
        || !value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'))
        || super::updates::compare_versions(value, value).is_none()
    {
        bail!("官方版本格式无效");
    }
    Ok(value.into())
}
pub(super) fn package(tool: ToolId) -> Option<&'static str> {
    Some(match tool {
        ToolId::ClaudeCode => "@anthropic-ai/claude-code",
        ToolId::Codex => "@openai/codex",
        ToolId::OpenClaw => "openclaw",
        ToolId::OpenCode => "opencode-ai",
        ToolId::GrokBuild => "@xai-official/grok",
        ToolId::Hermes => return None,
        ToolId::Pi => "@earendil-works/pi-coding-agent",
        ToolId::MiniMaxCode => "@minimax-ai/code",
        ToolId::OhMyPi => "@oh-my-pi/pi-coding-agent",
        ToolId::KimiCode => "@moonshot-ai/kimi-code",
        ToolId::DeepSeekHarness => "@deepseek-ai/dsh",
        _ => return None,
    })
}
fn ps(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
pub(super) fn step(
    env: &InstallationEnvironment,
    program: &Path,
    args: Vec<String>,
    extra_path: Vec<PathBuf>,
) -> InstallStep {
    let mut dirs = extra_path;
    dirs.extend(env.command_directories.clone());
    let mut path = dirs
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(if env.platform == HostPlatform::Windows {
            ";"
        } else {
            ":"
        });
    if let Ok(inherited) = std::env::var("PATH") {
        path.push(if env.platform == HostPlatform::Windows {
            ';'
        } else {
            ':'
        });
        path.push_str(&inherited);
    }
    let (program, args) = if env.platform == HostPlatform::Windows
        && program
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("cmd") || e.eq_ignore_ascii_case("bat"))
    {
        let script = format!(
            "& {} {}; if ($LASTEXITCODE -ne 0) {{ exit $LASTEXITCODE }}",
            ps(&program.to_string_lossy()),
            args.iter().map(|v| ps(v)).collect::<Vec<_>>().join(" ")
        );
        (
            "powershell.exe".into(),
            vec![
                "-NoProfile".into(),
                "-NonInteractive".into(),
                "-Command".into(),
                script,
            ],
        )
    } else {
        (program.to_string_lossy().into_owned(), args)
    };
    InstallStep {
        stage: InstallStage::Installing,
        program,
        args,
        environment: vec![("PATH".into(), path), ("CI".into(), "1".into())],
        download: None,
    }
}
pub(super) fn package_identity(path: &Path, name: &str) -> bool {
    std::fs::read(path.join("package.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        .is_some_and(|v| v["name"] == name)
}
pub(super) fn route(
    tool: ToolId,
    latest: &str,
    env: &InstallationEnvironment,
    arch: &str,
    digest: Option<&str>,
    current_version: Option<&str>,
) -> Result<Route> {
    let latest = normalized_version(latest)?;
    let _ = (arch, digest);
    super::policy::upgrade(tool, &latest, env, current_version)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_versions_cannot_inject_a_command_or_download_path() {
        for version in [
            "1.0.0; touch /tmp/x",
            "../../1.0.0",
            "--help",
            "1.0.0\nmalicious",
        ] {
            assert!(normalized_version(version).is_err());
        }
    }
}
