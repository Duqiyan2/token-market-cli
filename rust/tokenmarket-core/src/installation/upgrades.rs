//! Latest-release upgrade routes. Keep the effective install channel and prefix.
use super::{recipes, InstallPlan, InstallStage, InstallStep};
use crate::{
    inventory::{HostPlatform, InstallationEnvironment, ToolInventory},
    ToolId,
};
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub(super) struct UpgradePlan {
    pub plan: InstallPlan,
    pub probe: PathBuf,
    pub managed: bool,
}
pub(super) enum Route {
    Automatic(UpgradePlan),
    Archive,
    Manual(String),
    Restricted(String),
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
fn step(
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
/// Only update a proven global prefix. Never run npm against a project install.
fn npm_prefix(
    env: &InstallationEnvironment,
    tool: ToolId,
    probe: &Path,
    name: &str,
) -> Option<(PathBuf, PathBuf)> {
    let canonical = probe.canonicalize().ok()?;
    for root in canonical.ancestors().take(10) {
        if !package_identity(root, name) {
            continue;
        }
        let modules = root
            .ancestors()
            .find(|p| p.file_name().is_some_and(|n| n == "node_modules"))?;
        if modules.to_string_lossy().contains(".pnpm")
            || modules.to_string_lossy().contains(".yarn")
        {
            return None;
        }
        let parent = modules.parent()?;
        let prefix = if parent.file_name().is_some_and(|n| n == "lib") {
            parent.parent()?
        } else {
            parent
        };
        let parent = probe.parent().and_then(|p| p.canonicalize().ok());
        if parent != prefix.join("bin").canonicalize().ok()
            && !(env.platform == HostPlatform::Windows && parent == prefix.canonicalize().ok())
        {
            continue;
        }
        return Some((prefix.into(), PathBuf::new()));
    }
    if recipes::owned_command_paths(tool, env).contains(&probe.to_path_buf()) {
        let root = env.home.join(".tokenmarket/tools").join(tool.to_string());
        for suffix in ["package", "runtime", ""] {
            let prefix = root.join(suffix);
            if package_identity(&prefix.join("lib/node_modules").join(name), name)
                || package_identity(&prefix.join("node_modules").join(name), name)
            {
                let node = std::fs::read_to_string(root.join("node-path"))
                    .ok()
                    .map(|s| PathBuf::from(s.trim()))
                    .filter(|p| p.is_absolute())
                    .unwrap_or_default();
                return Some((prefix, node));
            }
        }
    }
    None
}
pub(super) fn route(
    tool: ToolId,
    latest: &str,
    env: &InstallationEnvironment,
    arch: &str,
    digest: Option<&str>,
) -> Result<Route> {
    let latest = normalized_version(latest)?;
    if tool == ToolId::OpenCode
        && latest
            .split('.')
            .next()
            .and_then(|v| v.parse::<u64>().ok())
            .is_some_and(|v| v >= 2)
    {
        return Ok(Route::Restricted(
            "OpenCode 2 的接入格式与当前 V1 适配不同，需要更新接入适配后再升级".into(),
        ));
    }
    if matches!(tool, ToolId::DuMate | ToolId::DuMateEnterprise) && latest != "1.0.89" {
        return Ok(Route::Restricted(
            "DuMate 新版的接入适配尚未完成，请先通过官方应用查看更新说明".into(),
        ));
    }
    let inventory = ToolInventory::new(env.clone());
    if inventory.application_path(tool).is_some() {
        return Ok(Route::Manual(
            "请通过应用内的检查更新入口升级到官方最新版".into(),
        ));
    }
    let probe = env
        .command_path(tool.command())
        .context("未检测到当前安装")?;
    let managed = recipes::owned_command_paths(tool, env).contains(&probe);
    if let Some(name) = package(tool) {
        if let Some((prefix, node)) = npm_prefix(env, tool, &probe, name) {
            let sibling = node
                .parent()
                .map(|p| {
                    p.join(if env.platform == HostPlatform::Windows {
                        "npm.cmd"
                    } else {
                        "npm"
                    })
                })
                .filter(|p| p.is_file());
            let npm = sibling
                .or_else(|| env.command_path("npm"))
                .context("当前 npm 安装渠道的运行环境不可用，请修复 Node.js/npm 后重试")?;
            let dirs = npm
                .parent()
                .map(|p| vec![p.to_path_buf()])
                .unwrap_or_default();
            let install = step(
                env,
                &npm,
                vec![
                    "install".into(),
                    "--global".into(),
                    "--prefix".into(),
                    prefix.to_string_lossy().into_owned(),
                    format!("{name}@{latest}"),
                    "--no-audit".into(),
                    "--no-fund".into(),
                ],
                dirs,
            );
            return Ok(Route::Automatic(UpgradePlan {
                plan: InstallPlan {
                    tool,
                    version: latest,
                    command: tool.command().into(),
                    steps: vec![install],
                },
                probe,
                managed: false,
            }));
        }
    }
    let canonical = probe.canonicalize().unwrap_or_else(|_| probe.clone());
    let brew_channel = canonical
        .components()
        .any(|c| c.as_os_str() == "Cellar" || c.as_os_str() == "Caskroom");
    if brew_channel {
        let name = match tool {
            ToolId::ClaudeCode => "claude-code",
            ToolId::Codex => "codex",
            _ => return Ok(Route::Manual("请使用 Homebrew 的原安装包更新此工具".into())),
        };
        let brew = env
            .command_path("brew")
            .context("未找到原安装渠道 Homebrew")?;
        let kind = if canonical.components().any(|c| c.as_os_str() == "Caskroom") {
            "--cask"
        } else {
            "--formula"
        };
        let install = step(
            env,
            &brew,
            vec!["upgrade".into(), kind.into(), name.into()],
            vec![],
        );
        return Ok(Route::Automatic(UpgradePlan {
            plan: InstallPlan {
                tool,
                version: latest,
                command: tool.command().into(),
                steps: vec![install],
            },
            probe,
            managed: false,
        }));
    }
    if managed && matches!(tool, ToolId::ClaudeCode | ToolId::Codex) {
        return Ok(Route::Automatic(UpgradePlan {
            plan: super::native_cli::plan_version(tool, env, arch, &latest)?,
            probe,
            managed: true,
        }));
    }
    if matches!(tool, ToolId::Hermes) || (managed && tool == ToolId::OpenClaw) {
        let args = if tool == ToolId::Hermes {
            vec!["update".into()]
        } else {
            vec![
                "update".into(),
                "--tag".into(),
                latest.clone(),
                "--yes".into(),
                "--no-restart".into(),
            ]
        };
        let install = step(
            env,
            &probe,
            args,
            probe.parent().map(|p| vec![p.into()]).unwrap_or_default(),
        );
        return Ok(Route::Automatic(UpgradePlan {
            plan: InstallPlan {
                tool,
                version: latest,
                command: tool.command().into(),
                steps: vec![install],
            },
            probe,
            managed,
        }));
    }
    if managed && matches!(tool, ToolId::Pi | ToolId::OhMyPi | ToolId::KimiCode) {
        let Some(digest) = digest else {
            return Ok(Route::Archive);
        };
        if digest.len() != 64 || !digest.chars().all(|c| c.is_ascii_hexdigit()) {
            bail!("官方安装包缺少有效 SHA256 校验信息");
        }
        let plan = match tool {
            ToolId::Pi => super::pi::plan_release(env, arch, &latest, Some(digest))?,
            ToolId::OhMyPi => super::oh_my_pi::plan_release(env, arch, &latest, Some(digest))?,
            ToolId::KimiCode => super::kimi::plan_release(env, arch, &latest, Some(digest))?,
            _ => unreachable!(),
        };
        return Ok(Route::Automatic(UpgradePlan {
            plan,
            probe,
            managed: true,
        }));
    }
    Ok(Route::Manual(
        "此安装渠道需通过工具自身或原包管理器更新至官方最新版".into(),
    ))
}

/// Select the checksum for this exact platform artifact from its official release.
pub(super) fn archive_request(
    tool: ToolId,
    latest: &str,
    env: &InstallationEnvironment,
    arch: &str,
) -> Result<(String, String)> {
    let latest = normalized_version(latest)?;
    let pinned = recipes::plan(tool, env, arch)?;
    let repo = match tool {
        ToolId::Pi => "earendil-works/pi",
        ToolId::OhMyPi => "can1357/oh-my-pi",
        ToolId::KimiCode => "MoonshotAI/kimi-code",
        _ => bail!("该工具没有独立发行包升级方案"),
    };
    let asset = pinned
        .steps
        .iter()
        .filter_map(|s| s.download.as_ref())
        .find(|(url, _)| url.contains(&format!("github.com/{repo}/releases/download/")))
        .and_then(|(url, _)| url.rsplit('/').next())
        .context("无法确认当前平台安装包")?
        .to_owned();
    let tag = if tool == ToolId::KimiCode {
        format!("%40moonshot-ai%2Fkimi-code%40{latest}")
    } else {
        format!("v{latest}")
    };
    Ok((
        format!("https://api.github.com/repos/{repo}/releases/tags/{tag}"),
        asset,
    ))
}
pub(super) fn archive_digest(
    body: &serde_json::Value,
    tool: ToolId,
    latest: &str,
    asset: &str,
) -> Result<String> {
    let tag = body["tag_name"].as_str().context("官方发行版本缺失")?;
    if normalized_version(tag)? != normalized_version(latest)?
        || body["draft"] == true
        || body["prerelease"] == true
    {
        bail!("官方安装包版本与目标稳定版不符");
    }
    let repo = match tool {
        ToolId::Pi => "earendil-works/pi",
        ToolId::OhMyPi => "can1357/oh-my-pi",
        ToolId::KimiCode => "MoonshotAI/kimi-code",
        _ => bail!("未知独立发行包"),
    };
    let item = body["assets"]
        .as_array()
        .and_then(|a| a.iter().find(|a| a["name"] == asset))
        .context("官方发行版缺少当前平台安装包")?;
    if !item["browser_download_url"]
        .as_str()
        .is_some_and(|u| u.starts_with(&format!("https://github.com/{repo}/releases/download/")))
    {
        bail!("官方安装包来源无效");
    }
    let digest = item["digest"]
        .as_str()
        .and_then(|s| s.strip_prefix("sha256:"))
        .context("官方安装包未提供 SHA256 摘要")?;
    if digest.len() != 64 || !digest.chars().all(|c| c.is_ascii_hexdigit()) {
        bail!("官方安装包摘要格式无效");
    }
    Ok(digest.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn executable(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "inert fixture").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
    }
    fn env(home: &Path) -> InstallationEnvironment {
        InstallationEnvironment {
            home: home.into(),
            platform: HostPlatform::MacOs,
            command_directories: recipes::managed_command_directories(home),
            application_directories: vec![],
            executable_extensions: vec![],
        }
    }
    #[test]
    fn latest_native_archives_use_the_requested_release_and_official_digest() {
        for (tool, path, version) in [
            (ToolId::Pi, ".tokenmarket/tools/pi/runtime/pi", "1.5.0"),
            (
                ToolId::OhMyPi,
                ".tokenmarket/tools/oh-my-pi/bin/omp",
                "18.16.0",
            ),
            (
                ToolId::KimiCode,
                ".tokenmarket/tools/kimi-code/bin/kimi",
                "2.5.0",
            ),
        ] {
            let t = tempfile::tempdir().unwrap();
            let e = env(t.path());
            executable(&t.path().join(path));
            let digest = "a".repeat(64);
            assert!(matches!(
                route(tool, version, &e, "aarch64", None).unwrap(),
                Route::Archive
            ));
            let Route::Automatic(plan) =
                route(tool, version, &e, "aarch64", Some(&digest)).unwrap()
            else {
                panic!("no archive plan")
            };
            assert_eq!(plan.plan.version, version);
            assert!(plan
                .plan
                .steps
                .iter()
                .filter_map(|s| s.download.as_ref())
                .any(|(url, _)| url.contains(version)));
            assert!(plan
                .plan
                .steps
                .iter()
                .flat_map(|s| &s.args)
                .any(|a| a.contains(&digest)));
        }
    }
    #[test]
    fn release_artifact_identity_and_checksums_are_checked_before_execution() {
        let digest = "b".repeat(64);
        let mut release = serde_json::json!({"tag_name":"v1.5.0","draft":false,"prerelease":false,"assets":[{"name":"pi-darwin-arm64.tar.gz","browser_download_url":"https://github.com/earendil-works/pi/releases/download/v1.5.0/pi-darwin-arm64.tar.gz","digest":format!("sha256:{digest}")}]});
        assert_eq!(
            archive_digest(&release, ToolId::Pi, "1.5.0", "pi-darwin-arm64.tar.gz").unwrap(),
            digest
        );
        assert!(archive_digest(&release, ToolId::Pi, "1.6.0", "pi-darwin-arm64.tar.gz").is_err());
        release["assets"][0]["digest"] = serde_json::Value::Null;
        assert!(archive_digest(&release, ToolId::Pi, "1.5.0", "pi-darwin-arm64.tar.gz").is_err());
        release["assets"][0]["digest"] = format!("sha256:{digest}").into();
        release["assets"][0]["browser_download_url"] = "https://example.invalid/pi.tar.gz".into();
        assert!(archive_digest(&release, ToolId::Pi, "1.5.0", "pi-darwin-arm64.tar.gz").is_err());
    }
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
