//! Guardian protocol. The native adapter owns immutable verified bytes and an
//! exact parent process handle before entering this operation.
use super::{ClientUpdateGuard, ClientUpdateJournal, ClientUpdatePhase as Phase};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    io::{BufRead, Write},
    path::PathBuf,
    process::Child,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NativeProcessIdentity {
    pub pid: u32,
    /// Native process creation time; a PID alone is never an identity.
    pub created: u64,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct GuardianPlan {
    pub home: PathBuf,
    pub origin: String,
    pub from_version: String,
    pub target_version: String,
    pub application: PathBuf,
    pub package: PathBuf,
    pub package_sha256: String,
    pub guardian: NativeProcessIdentity,
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct ExecutionIdentity {
    guardian: NativeProcessIdentity,
    installer: Option<NativeProcessIdentity>,
    package: PathBuf,
    package_sha256: String,
}

/// Only operating-system effects belong here. The guardian, never the exiting
/// desktop, owns the whole legacy guard and the returned installer Child.
pub trait GuardianEnvironment {
    fn wait_for_parent(&mut self) -> Result<()>;
    fn launch_installer(&mut self) -> Result<Child>;
    fn installer_identity(&self, child: &Child) -> Result<NativeProcessIdentity>;
    fn relaunch(&mut self) -> Result<()>;
}

fn send(output: &mut impl Write, value: serde_json::Value) -> Result<()> {
    serde_json::to_writer(&mut *output, &value)?;
    output.write_all(b"\n")?;
    output.flush()?;
    Ok(())
}

/// READY is an owned idle decision. EOF/cancel before a matching COMMIT never
/// installs. After COMMIT no timeout can release protection around a live child.
pub fn run_guardian(
    plan: GuardianPlan,
    input: &mut impl BufRead,
    output: &mut impl Write,
    environment: &mut impl GuardianEnvironment,
) -> Result<()> {
    let _guard = ClientUpdateGuard::acquire(&plan.home, &plan.origin)?;
    let mut journal = ClientUpdateJournal::begin(
        &plan.home,
        &plan.from_version,
        &plan.target_version,
        &plan.application,
    )?;
    let mut execution = ExecutionIdentity {
        guardian: plan.guardian,
        installer: None,
        package: plan.package,
        package_sha256: plan.package_sha256,
    };
    journal.execution(execution.clone())?;
    journal.record(Phase::Guarded)?;
    if let Err(error) = send(
        output,
        serde_json::json!({"ready":true,"nonce":journal.nonce(),"guardian":execution.guardian}),
    ) {
        journal.record(Phase::Failed)?;
        return Err(error);
    }
    let mut command = String::new();
    let read = input.read_line(&mut command);
    let commit = serde_json::from_str::<serde_json::Value>(&command).ok();
    if !matches!(read, Ok(n) if n > 0)
        || commit
            .as_ref()
            .and_then(|c| c.get("commit"))
            .and_then(|v| v.as_str())
            != Some(journal.nonce())
    {
        journal.record(Phase::Failed)?;
        return Ok(());
    }
    journal.record(Phase::Committed)?;
    // Failure to acknowledge after durable commit is not a cancellation.
    let _ = send(output, serde_json::json!({"committed":true}));
    if let Err(error) = environment.wait_for_parent() {
        journal.record(Phase::Failed)?; // No installer has been started.
        return Err(error.context("无法确认原客户端已退出；未启动安装器"));
    }
    let mut installer = match environment.launch_installer() {
        Ok(child) => child,
        Err(error) => {
            journal.record_failure(&format!("无法启动安装器，请重试或使用官方安装包：{error}"))?;
            // The original desktop has already exited. Reopen whatever is now
            // at the native application path for recovery; this is not rollback
            // or proof that an update completed.
            let _ = environment.relaunch();
            return Err(error.context("无法启动安装器，请重试或使用官方安装包"));
        }
    };
    // Every failure after spawn must first observe termination using this exact
    // Child handle. Never drop the legacy guard on a journal/identity I/O error.
    let identity_result = environment
        .installer_identity(&installer)
        .and_then(|identity| {
            execution.installer = Some(identity);
            journal.execution(execution)?;
            journal.record(Phase::InstallerRunning)
        });
    let status = loop {
        match installer.wait() {
            Ok(status) => break status,
            Err(_) => std::thread::sleep(std::time::Duration::from_secs(1)),
        }
    };
    if let Err(error) = identity_result {
        // Known terminated, but do not assert startup completion without durable
        // trustworthy process identity. The prior unresolved record is recovery.
        let _ = journal.record_failure("安装器已结束，但无法保存恢复信息，请使用官方安装包检查");
        return Err(error.context("安装器已结束，但无法保存恢复信息，请使用官方安装包检查"));
    }
    if !status.success() {
        let message = format!(
            "安装器未完成（退出码 {:?}），请重试或使用官方安装包恢复",
            status.code()
        );
        journal.record_failure(&message)?;
        let _ = environment.relaunch();
        bail!("{message}");
    }
    journal.record(Phase::AwaitingStartup)?;
    // No journal writes after launch: only actual compiled target startup may
    // acknowledge success. A failed relaunch intentionally remains pending.
    environment
        .relaunch()
        .context("安装器已结束，但重启失败，请手动打开客户端或使用官方安装包恢复")?;
    Ok(())
}
