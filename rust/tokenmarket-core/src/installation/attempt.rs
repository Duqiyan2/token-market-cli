//! A version-printing launcher is not completion evidence for our interrupted work.
use super::InstallPlan;
use crate::{
    inventory::{HostPlatform, InstallationEnvironment},
    ToolId,
};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize)]
pub(super) struct Attempt {
    version: String,
    commands: Vec<PathBuf>,
}
fn path(environment: &InstallationEnvironment, tool: ToolId) -> PathBuf {
    environment
        .home
        .join(".tokenmarket/installations")
        .join(format!("{tool}.incomplete.json"))
}
fn matches(environment: &InstallationEnvironment, left: &Path, right: &Path) -> bool {
    if environment.platform == HostPlatform::Windows {
        left.to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy())
    } else {
        left == right
    }
}
fn read(environment: &InstallationEnvironment, tool: ToolId) -> Result<Option<Attempt>> {
    match std::fs::read(path(environment, tool)) {
        Ok(bytes) => Ok(Some(
            serde_json::from_slice(&bytes)
                .context("安装恢复记录损坏，请保留现有文件并检查安装目录")?,
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).context("无法读取安装恢复记录"),
    }
}
pub(super) fn repair(
    environment: &InstallationEnvironment,
    plan: &InstallPlan,
    command: &Path,
) -> Result<bool> {
    let Some(attempt) = read(environment, plan.tool)? else {
        return Ok(false);
    };
    if !attempt
        .commands
        .iter()
        .any(|p| matches(environment, p, command))
    {
        return Ok(false);
    }
    if attempt.version != plan.version {
        bail!("上次安装尚未完成且目标版本已改变；请按官方说明检查现有目录，未自动升级或覆盖")
    }
    Ok(true)
}
/// Read-only and path-scoped: a newly discovered external install is still usable.
pub(crate) fn incomplete(
    environment: &InstallationEnvironment,
    tool: ToolId,
    command: &Path,
) -> bool {
    match read(environment, tool) {
        Ok(Some(attempt)) => attempt
            .commands
            .iter()
            .any(|p| matches(environment, p, command)),
        Ok(None) => false,
        Err(_) => super::recipes::owned_command_paths(tool, environment)
            .iter()
            .any(|p| matches(environment, p, command)),
    }
}
pub(super) fn begin(environment: &InstallationEnvironment, plan: &InstallPlan) -> Result<()> {
    if let Some(attempt) = read(environment, plan.tool)? {
        if attempt.version != plan.version {
            bail!("上次安装尚未完成且目标版本已改变；未自动升级，请先检查现有安装")
        }
        return Ok(());
    }
    let destination = path(environment, plan.tool);
    let temporary = destination.with_extension("tmp");
    let attempt = Attempt {
        version: plan.version.clone(),
        commands: super::recipes::owned_command_paths(plan.tool, environment)
            .into_iter()
            .filter(|p| !p.exists())
            .collect(),
    };
    crate::config::write_private_json(&temporary, &attempt)?;
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&temporary)?
        .sync_all()?;
    std::fs::rename(temporary, destination).context("无法保存安装恢复记录")?;
    Ok(())
}
pub(super) fn complete(environment: &InstallationEnvironment, tool: ToolId) -> Result<()> {
    match std::fs::remove_file(path(environment, tool)) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e).context("安装已完成，但无法更新恢复记录；请重试"),
    }
}
