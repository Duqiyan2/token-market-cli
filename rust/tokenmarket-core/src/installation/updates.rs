//! Explicit version probes and public release metadata. Inventory stays passive.
use super::{attempt, installation_lock, upgrades, InstallPlan, InstallationManager};
use crate::{
    adapters::adapters,
    inventory::{HostPlatform, ToolInventory},
    ToolId,
};
use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::{cmp::Ordering, time::Duration};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolUpdate {
    pub tool: ToolId,
    pub current_version: Option<String>,
    pub target_version: Option<String>,
    pub latest_version: Option<String>,
    pub can_upgrade: bool,
    pub manual_upgrade: bool,
    pub state: String,
    pub note: String,
}

// Vendor --version output may include a command name or tag prefix. Refuse
// ambiguous/non-version output rather than offering a downgrade.
fn version(value: &str) -> Option<(Vec<u64>, Option<Vec<String>>)> {
    value.split_whitespace().find_map(|word| {
        let word = word.rsplit(['/', '@']).next()?.trim_start_matches('v');
        let word = word.split('+').next()?;
        let (core, pre) = word
            .split_once('-')
            .map_or((word, None), |(c, p)| (c, Some(p)));
        let parts: Option<Vec<u64>> = core.split('.').map(|s| s.parse().ok()).collect();
        let parts = parts?;
        if parts.len() < 3 {
            return None;
        }
        let pre = pre.map(|p| p.split('.').map(str::to_owned).collect());
        Some((parts, pre))
    })
}
pub fn compare_versions(left: &str, right: &str) -> Option<Ordering> {
    let (mut l, lp) = version(left)?;
    let (mut r, rp) = version(right)?;
    let count = l.len().max(r.len());
    l.resize(count, 0);
    r.resize(count, 0);
    let order = l.cmp(&r);
    if order != Ordering::Equal {
        return Some(order);
    }
    Some(match (lp, rp) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(l), Some(r)) => {
            for (a, b) in l.iter().zip(&r) {
                let order = match (a.parse::<u64>(), b.parse::<u64>()) {
                    (Ok(a), Ok(b)) => a.cmp(&b),
                    (Ok(_), Err(_)) => Ordering::Less,
                    (Err(_), Ok(_)) => Ordering::Greater,
                    _ => a.cmp(b),
                };
                if order != Ordering::Equal {
                    return Some(order);
                }
            }
            l.len().cmp(&r.len())
        }
    })
}
impl InstallationManager {
    pub(super) fn validate_upgrade(&self, plan: &InstallPlan) -> Result<()> {
        let probe = self
            .environment
            .command_path(plan.tool.command())
            .context("未检测到当前工具")?;
        let repairing = attempt::upgrade_pending(&self.environment, plan, &probe)?;
        match self.installed_version(plan.tool) {
            Ok(Some(current)) => match compare_versions(&current, &plan.version) {
                Some(Ordering::Less) => Ok(()),
                Some(Ordering::Equal) if repairing => Ok(()),
                _ => bail!(
                    "当前版本 {current} 无需升级到 {}；请重新检查更新",
                    plan.version
                ),
            },
            _ if repairing => Ok(()),
            _ => bail!("无法确认当前版本，请先检查工具安装"),
        }
    }
    /// Fixture-friendly release boundary: production obtains metadata from official endpoints.
    pub fn inspect_update(&self, tool: ToolId, latest: Result<String>) -> ToolUpdate {
        self.inspect_release(tool, latest.map(|v| (v, None)))
    }
    fn inspect_release(
        &self,
        tool: ToolId,
        latest: Result<(String, Option<String>)>,
    ) -> ToolUpdate {
        self.upgrade_plans.lock().unwrap().remove(&tool.to_string());
        let mut result = ToolUpdate {
            tool,
            current_version: None,
            target_version: None,
            latest_version: None,
            can_upgrade: false,
            manual_upgrade: false,
            state: "not_installed".into(),
            note: "未安装".into(),
        };
        let inventory = ToolInventory::new(self.environment.clone());
        if !inventory.is_installed(tool) && self.environment.command_path(tool.command()).is_none()
        {
            return result;
        }
        let Ok(_guard) = installation_lock(&self.environment, tool) else {
            result.state = "busy".into();
            result.note = "工具正在安装、升级或接入，请完成后重试".into();
            return result;
        };
        result.current_version = self
            .installed_version(tool)
            .ok()
            .flatten()
            .filter(|v| version(v).is_some());
        if tool == ToolId::QwenWork {
            result.state = "manual_update".into();
            result.manual_upgrade = true;
            result.note =
                "请通过官网或客户端更新千问办公；升级、降级或重新安装后，请点击“重新接入”，程序会自动检查并修复配置；如提示尚未适配，请联系管理员适配"
                    .into();
            return result;
        }
        let (latest, digest) =
            match latest.and_then(|(v, d)| Ok((upgrades::normalized_version(&v)?, d))) {
                Ok(release) => release,
                Err(_) => {
                    result.state = "check_failed".into();
                    result.note = "无法获取官方最新版，请稍后重新检查".into();
                    return result;
                }
            };
        result.target_version = Some(latest.clone());
        result.latest_version = Some(latest.clone());
        let route = upgrades::route(
            tool,
            &latest,
            &self.environment,
            &self.architecture,
            digest.as_deref(),
        );
        let pending = match &route {
            Ok(upgrades::Route::Automatic(target)) => {
                attempt::upgrade_pending(&self.environment, &target.plan, &target.probe)
                    .unwrap_or(false)
            }
            _ => false,
        };
        let order = result
            .current_version
            .as_deref()
            .and_then(|v| compare_versions(v, &latest));
        if order.is_none() && !pending {
            result.state = "unknown".into();
            result.note = "无法读取当前版本，请检查工具安装".into();
            return result;
        }
        if matches!(order, Some(Ordering::Greater)) || order == Some(Ordering::Equal) && !pending {
            result.state = "current".into();
            result.note = if order == Some(Ordering::Greater) {
                "当前版本高于官方最新稳定版，不会降级"
            } else {
                "已是官方最新版"
            }
            .into();
            return result;
        }
        match route {
            Ok(upgrades::Route::Automatic(target)) => {
                result.can_upgrade = true;
                result.state = "available".into();
                result.note = if pending {
                    "可继续完成上次升级"
                } else {
                    "可升级至官方最新版；升级前请退出工具"
                }
                .into();
                self.upgrade_plans
                    .lock()
                    .unwrap()
                    .insert(tool.to_string(), target);
            }
            Ok(upgrades::Route::Archive) => {
                result.state = "check_failed".into();
                result.note = "官方安装包校验信息尚未就绪，请重新检查更新".into();
            }
            Ok(upgrades::Route::Manual(reason)) => {
                result.manual_upgrade = true;
                result.state = "manual_update".into();
                result.note = reason;
            }
            Ok(upgrades::Route::Restricted(reason)) => {
                result.state = "unsupported_update".into();
                result.note = reason;
            }
            Err(error) => {
                result.manual_upgrade = true;
                result.state = "manual_update".into();
                result.note = error.to_string();
            }
        }
        result
    }
    pub async fn check_updates(&self) -> Vec<ToolUpdate> {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(4))
            .timeout(Duration::from_secs(12))
            .user_agent("TokenMarket-ToolUpdates/1.0")
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() >= 4 || attempt.url().scheme() != "https" {
                    attempt.stop()
                } else {
                    attempt.follow()
                }
            }))
            .build()
            .expect("static TLS client configuration");
        let mut tasks = tokio::task::JoinSet::new();
        let permits = std::sync::Arc::new(tokio::sync::Semaphore::new(4));
        for adapter in adapters() {
            let tool = adapter.id;
            // Both DuMate account modes refer to one application and one OS
            // operation lock. Probe it once rather than racing ourselves.
            if tool == ToolId::DuMateEnterprise {
                continue;
            }
            let manager = self.clone();
            let client = client.clone();
            let permits = permits.clone();
            tasks.spawn(async move {
                let _permit = permits.acquire_owned().await.unwrap();
                let installed = ToolInventory::new(manager.environment.clone()).is_installed(tool)
                    || manager.environment.command_path(tool.command()).is_some();
                let latest = if installed {
                    fetch_release(&client, tool, &manager.environment, &manager.architecture).await
                } else {
                    Ok((String::new(), None))
                };
                tokio::task::spawn_blocking(move || manager.inspect_release(tool, latest))
                    .await
                    .unwrap_or_else(|_| ToolUpdate {
                        tool,
                        current_version: None,
                        target_version: None,
                        latest_version: None,
                        can_upgrade: false,
                        manual_upgrade: false,
                        state: "check_failed".into(),
                        note: "版本检测未完成，请稍后重试".into(),
                    })
            });
        }
        let mut results = vec![];
        while let Some(result) = tasks.join_next().await {
            if let Ok(result) = result {
                results.push(result);
            }
        }
        if let Some(mut enterprise) = results.iter().find(|r| r.tool == ToolId::DuMate).cloned() {
            enterprise.tool = ToolId::DuMateEnterprise;
            results.push(enterprise);
        }
        results.sort_by_key(|r| r.tool.to_string());
        results
    }
}

fn release_source(
    tool: ToolId,
    platform: HostPlatform,
    architecture: &str,
) -> (String, &'static str) {
    let package = match tool {
        ToolId::ClaudeCode => Some("@anthropic-ai%2Fclaude-code"),
        ToolId::Codex => Some("@openai%2Fcodex"),
        ToolId::OpenClaw => Some("openclaw"),
        ToolId::OpenCode => Some("opencode-ai"),
        ToolId::GrokBuild => Some("@xai-official%2Fgrok"),
        ToolId::Pi => Some("@earendil-works%2Fpi-coding-agent"),
        ToolId::MiniMaxCode => Some("@minimax-ai%2Fcode"),
        ToolId::KimiCode => Some("@moonshot-ai%2Fkimi-code"),
        _ => None,
    };
    if let Some(package) = package {
        return (
            format!("https://registry.npmjs.org/{package}/latest"),
            "json",
        );
    }
    match tool {
        ToolId::Hermes => (
            "https://api.github.com/repos/NousResearch/hermes-agent/releases/latest".into(),
            "tag",
        ),
        ToolId::OhMyPi => (
            "https://api.github.com/repos/can1357/oh-my-pi/releases/latest".into(),
            "tag",
        ),
        ToolId::QwenWork => ("https://qwenwork.cn/download".into(), "manual"),
        ToolId::WorkBuddy => (
            "https://www.codebuddy.cn/docs/workbuddy/Download-History".into(),
            "workbuddy",
        ),
        ToolId::DeepSeekHarness
            if platform == HostPlatform::Unix
                || (platform == HostPlatform::MacOs && architecture == "x86_64") =>
        {
            (
                "https://registry.npmjs.org/@deepseek-ai%2Fdsh/latest".into(),
                "json",
            )
        }
        ToolId::DeepSeekHarness => {
            let (platform, feed) = if platform == HostPlatform::MacOs {
                ("mac-arm64", "latest-mac.yml")
            } else {
                ("win-x64", "latest.yml")
            };
            (
                format!("https://download.deepseek.com/dsh-desk/bin/{platform}/{feed}"),
                "yaml",
            )
        }
        ToolId::DuMate | ToolId::DuMateEnterprise => {
            let feed = if platform == HostPlatform::MacOs {
                "latest-mac.yml"
            } else {
                "latest.yml"
            };
            (
                format!(
                    "https://dumate-app-public.cdn.bcebos.com/release-info/artifacts-update/{feed}"
                ),
                "yaml",
            )
        }
        _ => unreachable!(),
    }
}
async fn fetch_release(
    client: &reqwest::Client,
    tool: ToolId,
    env: &crate::inventory::InstallationEnvironment,
    arch: &str,
) -> Result<(String, Option<String>)> {
    if tool == ToolId::QwenWork {
        bail!("千问办公版本请通过官网或客户端查看")
    }
    let latest =
        upgrades::normalized_version(&fetch_latest(client, tool, env.platform, arch).await?)?;
    let digest = if matches!(
        upgrades::route(tool, &latest, env, arch, None),
        Ok(upgrades::Route::Archive)
    ) {
        let artifact = async {
            let (url, asset) = upgrades::archive_request(tool, &latest, env, arch)?;
            let body = fetch_body(client, &url).await?;
            upgrades::archive_digest(&serde_json::from_str(&body)?, tool, &latest, &asset)
        }
        .await;
        // The version lookup succeeded even if its platform artifact is not
        // ready. Keep displaying that version, without enabling execution.
        artifact.ok()
    } else {
        None
    };
    Ok((latest, digest))
}
async fn fetch_body(client: &reqwest::Client, url: &str) -> Result<String> {
    let mut response = client.get(url).send().await?.error_for_status()?;
    let mut bytes = vec![];
    while let Some(chunk) = response.chunk().await? {
        if bytes.len() + chunk.len() > 2 * 1024 * 1024 {
            bail!("版本信息过大");
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(String::from_utf8(bytes)?)
}
async fn fetch_latest(
    client: &reqwest::Client,
    tool: ToolId,
    platform: HostPlatform,
    architecture: &str,
) -> Result<String> {
    let (url, format) = release_source(tool, platform, architecture);
    parse_release(format, &fetch_body(client, &url).await?)
}

fn parse_release(format: &str, body: &str) -> Result<String> {
    let value = match format {
        "json" | "tag" => serde_json::from_str::<serde_json::Value>(body)?[if format == "tag" {
            "tag_name"
        } else {
            "version"
        }]
        .as_str()
        .map(str::to_owned),
        "yaml" => serde_yaml::from_str::<serde_yaml::Value>(body)?["version"]
            .as_str()
            .map(str::to_owned),
        "workbuddy" => body
            .split("WorkBuddy-")
            .filter_map(|part| {
                part.split('-')
                    .find(|word| word.chars().next().is_some_and(|c| c.is_ascii_digit()))
                    .and_then(|word| {
                        let numbers = word.split('.').take(3).collect::<Vec<_>>().join(".");
                        version(&numbers).map(|_| numbers)
                    })
            })
            .max_by(|a, b| compare_versions(a, b).unwrap_or(Ordering::Equal)),
        _ => None,
    }
    .context("无法读取官方版本信息")?;
    if version(&value).is_none() {
        bail!("官方版本格式无法识别");
    }
    Ok(value)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn versions_are_numeric_and_prerelease_aware() {
        for (a, b) in [
            ("codex-cli 0.99.0", "0.166.0"),
            ("omp/18.9.0", "18.15.0"),
            ("v1.0.0-rc.9", "1.0.0-rc.10"),
            ("0.2.1-rc.5", "0.2.1"),
            ("1.0.89", "1.0.100"),
        ] {
            assert_eq!(compare_versions(a, b), Some(Ordering::Less));
        }
        assert_eq!(compare_versions("error", "1.0.0"), None);
        assert_eq!(
            compare_versions("1.2.3+build", "1.2.3"),
            Some(Ordering::Equal)
        );
    }
    #[test]
    fn metadata_uses_product_versions() {
        assert_eq!(
            parse_release("json", r#"{"version":"2.1.309"}"#).unwrap(),
            "2.1.309"
        );
        assert_eq!(
            parse_release("tag", r#"{"tag_name":"v0.22.5"}"#).unwrap(),
            "v0.22.5"
        );
        assert_eq!(
            parse_release("yaml", "version: 1.0.89\nfiles: []").unwrap(),
            "1.0.89"
        );
        assert_eq!(parse_release("workbuddy","WorkBuddy-darwin-arm64-5.9.3.42398134-d4692421.dmg WorkBuddy-win32-x64-user-5.8.0.5-abc.exe").unwrap(),"5.9.3");
        assert!(parse_release("json", r#"{"error":"unavailable"}"#).is_err());
    }
}
