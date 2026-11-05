//! Durable lifecycle facts, not a replacement for live legacy lock ownership.
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ClientUpdatePhase {
    Preparing,
    Guarded,
    Committed,
    InstallerRunning,
    AwaitingStartup,
    Failed,
    Succeeded,
}

#[derive(Serialize, Deserialize)]
struct Record {
    nonce: String,
    from_version: String,
    target_version: String,
    application: PathBuf,
    phase: ClientUpdatePhase,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    execution: Option<super::guardian::ExecutionIdentity>,
}

pub struct ClientUpdateJournal {
    path: PathBuf,
    record: Record,
}

impl ClientUpdateJournal {
    /// The lock-owning installer/guardian serializes journal writers. Never put
    /// renderer paths, credentials or package bytes in this record.
    pub fn begin(home: &Path, from: &str, target: &str, application: &Path) -> Result<Self> {
        if !application.is_absolute() || from.is_empty() || target.is_empty() {
            bail!("客户端更新恢复信息无效");
        }
        if let Some(previous) = Self::read(home)? {
            if !matches!(
                previous.phase(),
                ClientUpdatePhase::Failed | ClientUpdatePhase::Succeeded
            ) {
                bail!("上次客户端更新交接尚未确认结束，请先检查安装或使用官方安装包恢复");
            }
        }
        let journal = Self {
            path: Self::path(home),
            record: Record {
                nonce: format!("{:032x}", rand::random::<u128>()),
                from_version: from.into(),
                target_version: target.into(),
                application: application.into(),
                phase: ClientUpdatePhase::Preparing,
                execution: None,
            },
        };
        journal.save()?;
        Ok(journal)
    }

    fn path(home: &Path) -> PathBuf {
        home.join(".tokenmarket/client-update-attempt.json")
    }

    pub fn read(home: &Path) -> Result<Option<Self>> {
        let path = Self::path(home);
        match fs::read(&path) {
            Ok(bytes) => Ok(Some(Self {
                path,
                record: serde_json::from_slice(&bytes)
                    .context("无法读取客户端更新恢复记录，请使用官方安装包检查恢复")?,
            })),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error).context("无法读取客户端更新恢复记录"),
        }
    }

    pub fn nonce(&self) -> &str {
        &self.record.nonce
    }
    pub fn phase(&self) -> ClientUpdatePhase {
        self.record.phase
    }

    pub(super) fn execution(
        &mut self,
        execution: super::guardian::ExecutionIdentity,
    ) -> Result<()> {
        self.record.execution = Some(execution);
        self.save()
    }

    /// Record only observed facts. AwaitingStartup requires known installer
    /// termination; a timeout or missing parent PID does not establish it.
    pub fn record(&mut self, next: ClientUpdatePhase) -> Result<()> {
        use ClientUpdatePhase::*;
        if !matches!(
            (self.phase(), next),
            (Preparing, Guarded | Failed)
                | (Guarded, Committed | Failed)
                | (Committed, InstallerRunning | Failed)
                | (InstallerRunning, AwaitingStartup | Failed)
        ) {
            bail!("客户端更新交接阶段无效，请检查安装恢复状态");
        }
        self.set_phase(next)
    }

    /// Call only from the actual new process using compiled version/current_exe.
    /// Neither launch success nor installer exit code alone reports success.
    pub fn acknowledge_startup(&mut self, running_version: &str, executable: &Path) -> Result<()> {
        if self.phase() != ClientUpdatePhase::AwaitingStartup
            || running_version != self.record.target_version
            || executable != self.record.application
        {
            bail!("尚未确认目标客户端版本启动，请检查安装或使用官方安装包恢复");
        }
        self.set_phase(ClientUpdatePhase::Succeeded)
    }

    fn set_phase(&mut self, phase: ClientUpdatePhase) -> Result<()> {
        let previous = self.record.phase;
        self.record.phase = phase;
        if let Err(error) = self.save() {
            self.record.phase = previous;
            return Err(error);
        }
        Ok(())
    }

    fn save(&self) -> Result<()> {
        let temporary = self
            .path
            .with_extension(format!("{}.tmp", self.record.nonce));
        crate::config::write_private_json(&temporary, &self.record)?;
        fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&temporary)?
            .sync_all()?;
        fs::rename(&temporary, &self.path).context("无法保存客户端更新恢复记录")?;
        #[cfg(unix)]
        fs::File::open(self.path.parent().unwrap())?.sync_all()?;
        Ok(())
    }
}
