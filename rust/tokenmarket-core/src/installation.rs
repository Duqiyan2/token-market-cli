//! Explicit tool installation, independent of credentials and Tool Connections.
pub(crate) mod attempt;
pub(crate) mod discovery;
mod dumate;
mod gui;
mod harness_cli;
mod hermes;
mod kimi;
mod minimax;
mod native_cli;
mod node;
mod node_requirement;
mod npm;
mod oh_my_pi;
pub mod opencode;
mod pi;
mod policy;
pub(crate) mod portable_git;
pub mod recipes;
pub mod uninstall;
pub mod updates;
mod upgrades;
use crate::{
    inventory::{InstallationEnvironment, ToolInventory},
    ToolId,
};
use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallStage {
    Preparing,
    Downloading,
    Dependencies,
    Installing,
    WaitingForUser,
    Detecting,
    Succeeded,
    Failed,
    Cancelled,
}
impl InstallStage {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallationAvailability {
    pub supported: bool,
    pub reason: Option<String>,
    pub version: Option<String>,
    pub method: Option<InstallMethod>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallMethod {
    Npm,
    NpmPreferred,
    ScriptWithNpmFallback,
    Script,
    Standalone,
    Desktop,
    Homebrew,
}
impl InstallMethod {
    pub fn label(self, platform: crate::inventory::HostPlatform) -> &'static str {
        match self {
            Self::Npm => "通过 npm 安装",
            Self::NpmPreferred => "优先通过 npm 安装",
            Self::ScriptWithNpmFallback => "运行官方安装脚本，失败时改用 npm",
            Self::Script if platform == crate::inventory::HostPlatform::Windows => {
                "通过官方 PowerShell 脚本安装"
            }
            Self::Script => "通过官方安装脚本安装",
            Self::Standalone => "自动安装命令行工具",
            Self::Desktop => "通过桌面安装器安装",
            Self::Homebrew => "通过 Homebrew 安装",
        }
    }
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallStatus {
    pub tool: ToolId,
    pub stage: InstallStage,
    pub output: Vec<String>,
    pub output_sequence: u64,
    pub detail: String,
    pub installed: bool,
    pub version: Option<String>,
}
#[derive(Debug, Clone)]
pub struct InstallStep {
    pub stage: InstallStage,
    pub program: String,
    pub args: Vec<String>,
    pub environment: Vec<(String, String)>,
    pub download: Option<(String, PathBuf)>,
}
#[derive(Debug, Clone)]
pub struct InstallPlan {
    pub tool: ToolId,
    pub version: String,
    pub command: String,
    pub steps: Vec<InstallStep>,
    pub method: InstallMethod,
}

/// Real downloads and processes use this boundary; tests supply fixture installers.
pub trait InstallExecutor: Send + Sync {
    fn execute(
        &self,
        step: &InstallStep,
        cancelled: &AtomicBool,
        output: &mut dyn FnMut(String),
    ) -> Result<()>;
    fn version(&self, command: &Path) -> Result<String>;
    fn runtime_version(&self, command: &Path, _node: &Path) -> Result<String> {
        self.version(command)
    }
    /// Read application version metadata without launching the application.
    fn application_version(&self, _application: &Path) -> Result<Option<String>> {
        Ok(None)
    }
    /// Retain full product versions when numeric OS metadata omits a prerelease.
    fn tool_application_version(&self, tool: ToolId, application: &Path) -> Result<String> {
        if tool == ToolId::DeepSeekHarness {
            return gui::harness_application_version(application);
        }
        self.application_version(application)?
            .context("无法读取应用版本元数据")
    }
}
pub struct NativeInstallExecutor;
impl InstallExecutor for NativeInstallExecutor {
    fn execute(
        &self,
        step: &InstallStep,
        cancelled: &AtomicBool,
        output: &mut dyn FnMut(String),
    ) -> Result<()> {
        if let Some((url, path)) = &step.download {
            std::fs::create_dir_all(path.parent().context("安装下载目录无效")?)?;
            let args = vec![
                "--fail".into(),
                "--location".into(),
                "--silent".into(),
                "--show-error".into(),
                "--proto".into(),
                "=https".into(),
                "--proto-redir".into(),
                "=https".into(),
                "--tlsv1.2".into(),
                "--connect-timeout".into(),
                "30".into(),
                // Large official packages can make healthy progress for much
                // longer than five minutes. Abort stalled transfers, not slow
                // successful ones; curl retries transient errors from scratch.
                "--speed-limit".into(),
                "1024".into(),
                "--speed-time".into(),
                "120".into(),
                "--retry".into(),
                "2".into(),
                "--retry-max-time".into(),
                "300".into(),
                "--output".into(),
                path.to_string_lossy().into(),
                url.clone(),
            ];
            run_process(
                if cfg!(windows) { "curl.exe" } else { "curl" },
                &args,
                &[],
                cancelled,
                output,
                None,
            )?;
        }
        if !step.program.is_empty() {
            run_process(
                &step.program,
                &step.args,
                &step.environment,
                cancelled,
                output,
                Some(Duration::from_secs(1800)),
            )?;
        }
        Ok(())
    }
    fn version(&self, command: &Path) -> Result<String> {
        self.runtime_version(command, command)
    }
    fn runtime_version(&self, command: &Path, node: &Path) -> Result<String> {
        let mut lines = vec![];
        // Rust handles Windows CMD/BAT launchers and their nonstandard quoting.
        // Manually embedding a quoted script in `cmd /c` breaks paths with spaces.
        let (program, args) = (
            command.to_string_lossy().into_owned(),
            vec!["--version".into()],
        );
        run_process(
            &program,
            &args,
            &[("PATH".into(), npm::runtime_path(node))],
            &AtomicBool::new(false),
            &mut |line| lines.push(line),
            Some(Duration::from_secs(15)),
        )?;
        lines
            .into_iter()
            .find(|line| !line.trim().is_empty())
            .context("无法读取已安装版本")
    }
    fn application_version(&self, application: &Path) -> Result<Option<String>> {
        // Metadata belongs to the detected artifact, which can differ from the
        // host platform in a replaceable installation environment.
        if application.is_dir() {
            let metadata = application.join("Contents/Info.plist");
            if !metadata.exists() {
                return Ok(None);
            }
            let plist = plist::Value::from_file(metadata)?;
            return Ok(plist
                .as_dictionary()
                .and_then(|properties| properties.get("CFBundleShortVersionString"))
                .and_then(plist::Value::as_string)
                .map(str::to_owned));
        }
        #[cfg(windows)]
        {
            let mut lines = vec![];
            let script = format!("$ErrorActionPreference = 'Stop'; $v = [System.Diagnostics.FileVersionInfo]::GetVersionInfo('{}'); if ($v.ProductVersion) {{ Write-Output $v.ProductVersion }} else {{ Write-Output $v.FileVersion }}", application.to_string_lossy().replace('\'', "''"));
            run_process(
                "powershell.exe",
                &[
                    "-NoProfile".into(),
                    "-NonInteractive".into(),
                    "-Command".into(),
                    script,
                ],
                &[],
                &AtomicBool::new(false),
                &mut |line| lines.push(line),
                Some(Duration::from_secs(15)),
            )?;
            Ok(lines.into_iter().find(|line| !line.trim().is_empty()))
        }
        #[cfg(not(windows))]
        {
            Ok(None)
        }
    }
}

fn background_command(program: &str) -> Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let mut command = Command::new(program);
        // The desktop has no console. Piped output alone still lets Windows
        // create one for EXE, CMD/BAT and PowerShell version probes.
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        command
    }
    #[cfg(not(windows))]
    {
        Command::new(program)
    }
}

fn run_process(
    program: &str,
    args: &[String],
    environment: &[(String, String)],
    cancelled: &AtomicBool,
    output: &mut dyn FnMut(String),
    timeout: Option<Duration>,
) -> Result<()> {
    if cancelled.load(Ordering::SeqCst) {
        bail!("安装已取消")
    }
    let mut command = background_command(program);
    command
        .args(args)
        .envs(environment.iter().map(|(key, value)| (key, value)))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // A client launched from pwsh 7 inherits its incompatible module paths.
    // Windows PowerShell only repairs those paths when started directly by
    // pwsh, not through this Rust/cmd intermediate. Let each installation child
    // construct its defaults; preserve the parent and all other environment.
    #[cfg(windows)]
    command.env_remove("PSModulePath");
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command
        .spawn()
        .with_context(|| format!("无法启动安装程序 {program}"))?;
    let (sender, receiver) = mpsc::channel();
    for reader in [
        child
            .stdout
            .take()
            .map(|pipe| Box::new(pipe) as Box<dyn std::io::Read + Send>),
        child
            .stderr
            .take()
            .map(|pipe| Box::new(pipe) as Box<dyn std::io::Read + Send>),
    ]
    .into_iter()
    .flatten()
    {
        let sender = sender.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(reader)
                .lines()
                .map_while(std::result::Result::ok)
            {
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
    }
    drop(sender);
    let deadline = timeout.map(|timeout| Instant::now() + timeout);
    loop {
        while let Ok(line) = receiver.try_recv() {
            output(line);
        }
        if cancelled.load(Ordering::SeqCst)
            || deadline.is_some_and(|deadline| Instant::now() >= deadline)
        {
            #[cfg(unix)]
            {
                let _ = Command::new("/bin/kill")
                    // procps versions can parse an unseparated negative PGID
                    // as options and signal our own group instead.
                    .args(["-TERM", "--", &format!("-{}", child.id())])
                    .status();
            }
            #[cfg(windows)]
            {
                let _ = background_command("taskkill.exe")
                    .args(["/PID", &child.id().to_string(), "/T", "/F"])
                    .output();
            }
            let _ = child.kill();
            let _ = child.wait();
            bail!(if cancelled.load(Ordering::SeqCst) {
                "安装已取消；已准备的依赖保留"
            } else {
                "安装程序超时，请检查网络后重试"
            });
        }
        if let Some(status) = child.try_wait()? {
            // Reader threads close their senders after draining both pipes.
            for line in receiver.iter() {
                output(line);
            }
            if !status.success() {
                bail!("安装程序失败（{status}），请查看输出后重试")
            }
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(40));
    }
}

/// A GUI inherits a different PATH from an interactive terminal. Resolve the
/// login-shell PATH once, with the same bounded process runner as version probes.
pub(crate) fn effective_path() -> Option<std::ffi::OsString> {
    static PATH: std::sync::OnceLock<Option<std::ffi::OsString>> = std::sync::OnceLock::new();
    PATH.get_or_init(|| {
        #[cfg(unix)]
        {
            let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
            let mut found = None;
            let result = run_process(
                &shell,
                &[
                    "-lic".into(),
                    "printf '\\n__TOKENMARKET_PATH__%s\\n' \"$PATH\"".into(),
                ],
                &[],
                &AtomicBool::new(false),
                &mut |line| {
                    if let Some(path) = line.strip_prefix("__TOKENMARKET_PATH__") {
                        found = Some(std::ffi::OsString::from(path));
                    }
                },
                Some(Duration::from_secs(3)),
            );
            if result.is_ok() && found.is_some() {
                return found;
            }
        }
        std::env::var_os("PATH")
    })
    .clone()
}

struct Job {
    status: InstallStatus,
    cancelled: Arc<AtomicBool>,
}
/// Explicitly release the OS lock even if a concurrent process probe briefly
/// inherited a descriptor. Closing only this copy can leave that lock alive.
pub struct InstallationGuard {
    file: std::fs::File,
}
impl Drop for InstallationGuard {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}
#[derive(Clone)]
pub struct InstallationManager {
    environment: InstallationEnvironment,
    architecture: String,
    executor: Arc<dyn InstallExecutor>,
    jobs: Arc<Mutex<BTreeMap<String, Job>>>,
    upgrade_plans: Arc<Mutex<BTreeMap<String, upgrades::UpgradePlan>>>,
    node_requirements: Arc<Mutex<BTreeMap<String, String>>>,
}
impl InstallationManager {
    pub fn current() -> Self {
        Self::new(
            InstallationEnvironment::current(),
            std::env::consts::ARCH.into(),
            Arc::new(NativeInstallExecutor),
        )
    }
    pub fn new(
        mut environment: InstallationEnvironment,
        architecture: String,
        executor: Arc<dyn InstallExecutor>,
    ) -> Self {
        environment
            .command_directories
            .extend(recipes::managed_command_directories(&environment.home));
        Self {
            environment,
            architecture,
            executor,
            jobs: Arc::new(Mutex::new(BTreeMap::new())),
            upgrade_plans: Arc::new(Mutex::new(BTreeMap::new())),
            node_requirements: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }
    pub fn availability(&self, tool: ToolId) -> InstallationAvailability {
        recipes::availability(tool, &self.environment, &self.architecture)
    }
    /// Explicit-operation probe only. Read-only inventory must never call this.
    /// Adapters can compare this with their documented supported version range
    /// before configuration mutation; the installer pin is not that whole range.
    pub fn installed_version(&self, tool: ToolId) -> Result<Option<String>> {
        // Installed GUI metadata is readable even on a fixture host without a
        // matching installer architecture. DuMate has no official CLI identity.
        if matches!(
            tool,
            ToolId::DuMate | ToolId::DuMateEnterprise | ToolId::QwenWork
        ) {
            return ToolInventory::new(self.environment.clone())
                .application_path(tool)
                .map(|path| self.executor.tool_application_version(tool, &path))
                .transpose();
        }
        let plan = recipes::plan(tool, &self.environment, &self.architecture)?;
        if gui::is_gui(tool) && self.environment.command_path(tool.command()).is_none() {
            if let Some(path) = ToolInventory::new(self.environment.clone()).application_path(tool)
            {
                return self
                    .executor
                    .tool_application_version(tool, &path)
                    .map(Some);
            }
        }
        self.environment
            .command_path(&plan.command)
            .map(|path| self.executor.version(&path))
            .transpose()
    }
    /// Held throughout an explicit Connect/Reconnect so installation cannot race
    /// with configuration mutation. This is never part of read-only discovery.
    pub fn connection_guard(&self, tool: ToolId) -> Result<InstallationGuard> {
        let guard = installation_lock(&self.environment, tool)?;
        if matches!(tool, ToolId::DuMate | ToolId::DuMateEnterprise) {
            let version = self
                .installed_version(tool)?
                .context("无法确认 DuMate 版本，请先安装官方 1.0.89 客户端")?;
            if !gui::matches_version(tool, &version, dumate::VERSION) {
                bail!("当前 DuMate 接入仅验证 1.0.89（检测到 {version}），请按官方说明选择该版本；未修改配置或自动升级")
            }
        }
        if tool == ToolId::KimiCode {
            if let Some(version) = self
                .installed_version(tool)
                .ok()
                .flatten()
                .filter(|version| kimi::is_legacy_version(version))
            {
                bail!("检测到 Kimi 旧主版本（{version}）；当前接入适配新版 Kimi Code 2.x。请按官方说明手动选择兼容版本，未修改配置或自动迁移/升级")
            }
        }
        if tool == ToolId::OpenCode {
            let command_version = self.installed_version(tool).ok().flatten();
            let application_version = ToolInventory::new(self.environment.clone())
                .application_path(tool)
                .and_then(|application| {
                    self.executor
                        .application_version(&application)
                        .ok()
                        .flatten()
                });
            // Unknown metadata does not prove incompatibility. Check both detected
            // identities because the desktop prefers an application while CLI
            // management can still use a command on PATH.
            if let Some(version) = command_version
                .into_iter()
                .chain(application_version)
                .find(|version| opencode::major_version(version).is_some_and(|major| major >= 2))
            {
                bail!("检测到 OpenCode 2 或更新主版本（{version}）；当前接入仅支持 OpenCode V1。请按官方说明选择兼容版本，未修改配置或自动升级/降级")
            }
        }
        Ok(guard)
    }
    pub fn status(&self, tool: ToolId) -> Option<InstallStatus> {
        self.jobs
            .lock()
            .unwrap()
            .get(&tool.to_string())
            .map(|job| job.status.clone())
    }
    pub fn cancel(&self, tool: ToolId) -> Result<()> {
        let jobs = self.jobs.lock().unwrap();
        let job = jobs.get(&tool.to_string()).context("没有正在进行的安装")?;
        if !job.status.stage.is_terminal() {
            job.cancelled.store(true, Ordering::SeqCst);
        }
        Ok(())
    }
    pub fn start(&self, tool: ToolId) -> Result<InstallStatus> {
        self.start_operation(tool)
    }
    /// Targets come from the last successful official release check, never a UI command or URL.
    pub fn start_upgrade(&self, tool: ToolId) -> Result<InstallStatus> {
        let target = self
            .upgrade_plans
            .lock()
            .unwrap()
            .get(&tool.to_string())
            .map(|p| p.plan.version.clone())
            .context("请先检查更新，获取官方最新版本")?;
        self.start_upgrade_to(tool, &target)
    }
    pub fn start_upgrade_to(&self, tool: ToolId, expected_version: &str) -> Result<InstallStatus> {
        self.start_upgrade_checked(tool, expected_version, None)
    }
    pub fn start_upgrade_confirmed(
        &self,
        tool: ToolId,
        expected_version: &str,
        revision: &str,
    ) -> Result<InstallStatus> {
        self.start_upgrade_checked(tool, expected_version, Some(revision))
    }
    fn start_upgrade_checked(
        &self,
        tool: ToolId,
        expected_version: &str,
        revision: Option<&str>,
    ) -> Result<InstallStatus> {
        let target = self
            .upgrade_plans
            .lock()
            .unwrap()
            .get(&tool.to_string())
            .cloned()
            .context("更新信息不可用，请先检查更新")?;
        if target.plan.version != expected_version {
            bail!("官方版本信息已改变，请重新检查并确认升级");
        }
        if revision.is_some_and(|revision| revision != target.revision()) {
            bail!("升级入口或方案已改变，请重新检查并确认")
        }
        let operation_lock = installation_lock(&self.environment, tool)?;
        if self.environment.command_path(tool.command()).as_ref() != Some(&target.probe) {
            bail!("工具安装路径已改变，请重新检查更新");
        }
        self.ensure_node_requirement(tool)?;
        let current_version = self.installed_version(tool).ok().flatten();
        match upgrades::route(
            tool,
            expected_version,
            &self.environment,
            &self.architecture,
            None,
            current_version.as_deref(),
        )? {
            upgrades::Route::Automatic(fresh)
                if fresh.probe == target.probe
                    && fresh
                        .plan
                        .steps
                        .iter()
                        .map(|s| (&s.program, &s.args))
                        .eq(target.plan.steps.iter().map(|s| (&s.program, &s.args))) => {}
            _ => bail!("工具安装渠道已改变，请重新检查更新"),
        }
        self.validate_upgrade(&target.plan)?;
        let cancelled = Arc::new(AtomicBool::new(false));
        let status = InstallStatus {
            tool,
            stage: InstallStage::Preparing,
            output: vec![],
            output_sequence: 0,
            detail: format!("正在准备升级至官方最新版 {}", target.plan.version),
            installed: true,
            version: Some(target.plan.version.clone()),
        };
        {
            let mut jobs = self.jobs.lock().unwrap();
            if jobs
                .get(&tool.to_string())
                .is_some_and(|j| !j.status.stage.is_terminal())
            {
                bail!("该工具正在安装或升级，请稍后重试");
            }
            jobs.insert(
                tool.to_string(),
                Job {
                    status: status.clone(),
                    cancelled: cancelled.clone(),
                },
            );
        }
        let manager = self.clone();
        std::thread::spawn(move || {
            let result = (|| -> Result<String> {
                if target.managed {
                    attempt::begin_operation(&manager.environment, &target.plan, true)?;
                }
                manager.execute_steps(&target.plan, &cancelled, true)?;
                if cancelled.load(Ordering::SeqCst) {
                    bail!("升级已取消；可以重新检查更新后重试");
                }
                manager.update(tool, |state| {
                    state.stage = InstallStage::Detecting;
                    state.detail = "正在检测升级后的实际版本".into();
                });
                let actual = manager.executor.version(&target.probe)?;
                if !matches!(
                    updates::compare_versions(&actual, &target.plan.version),
                    Some(std::cmp::Ordering::Equal | std::cmp::Ordering::Greater)
                ) {
                    bail!(
                        "更新程序结束后检测到 {actual}，尚未达到目标版本 {}，请检查更新输出",
                        target.plan.version
                    );
                }
                if target.managed {
                    attempt::complete(&manager.environment, tool)?;
                }
                Ok(actual)
            })();
            let installed = ToolInventory::new(manager.environment.clone()).is_installed(tool);
            drop(operation_lock);
            manager.update(tool, |state| {
                state.installed = installed;
                match result {
                    Ok(actual) => {
                        state.stage = InstallStage::Succeeded;
                        state.version = Some(actual);
                        state.detail = "升级完成，请重新打开工具；原有连接保持不变".into();
                    }
                    Err(error) => {
                        state.stage = if cancelled.load(Ordering::SeqCst) {
                            InstallStage::Cancelled
                        } else {
                            InstallStage::Failed
                        };
                        state.detail = error.to_string();
                    }
                }
            });
        });
        Ok(status)
    }
    fn start_operation(&self, tool: ToolId) -> Result<InstallStatus> {
        let plan = recipes::plan(tool, &self.environment, &self.architecture)?;
        let operation_lock = installation_lock(&self.environment, tool)?;
        let cancelled = Arc::new(AtomicBool::new(false));
        let status = InstallStatus {
            tool,
            stage: InstallStage::Preparing,
            output: vec![],
            output_sequence: 0,
            detail: "正在检查已有安装".into(),
            installed: false,
            version: Some(plan.version.clone()),
        };
        {
            let mut jobs = self.jobs.lock().unwrap();
            if jobs
                .get(&tool.to_string())
                .is_some_and(|job| !job.status.stage.is_terminal())
            {
                bail!("该工具正在安装，请等待完成或取消")
            }
            jobs.insert(
                tool.to_string(),
                Job {
                    status: status.clone(),
                    cancelled: cancelled.clone(),
                },
            );
        }
        let manager = self.clone();
        std::thread::spawn(move || {
            let result = manager.install(&plan, &cancelled);
            let installed = ToolInventory::new(manager.environment.clone()).is_installed(tool);
            drop(operation_lock);
            manager.update(tool, |state| {
                state.installed = installed;
                match result {
                    Ok(()) => {
                        state.stage = InstallStage::Succeeded;
                        state.detail = "已检测到安装；请按需接入或打开工具".into();
                    }
                    Err(error) => {
                        state.stage = if cancelled.load(Ordering::SeqCst) {
                            InstallStage::Cancelled
                        } else {
                            InstallStage::Failed
                        };
                        state.detail = error.to_string();
                    }
                }
            });
        });
        Ok(status)
    }
    fn install(&self, plan: &InstallPlan, cancelled: &AtomicBool) -> Result<()> {
        if gui::is_gui(plan.tool) {
            if let Some(path) =
                ToolInventory::new(self.environment.clone()).application_path(plan.tool)
            {
                let version = self.executor.tool_application_version(plan.tool, &path)?;
                if !gui::matches_version(plan.tool, &version, &plan.version) {
                    bail!("已安装应用版本 {version} 尚未验证兼容（当前支持 {}）；未自动升级，请按官方说明处理", plan.version)
                }
                return Ok(());
            }
        }
        let repairing_owned;
        let command = if matches!(plan.tool, ToolId::DuMate | ToolId::DuMateEnterprise) {
            None
        } else {
            self.environment.command_path(&plan.command)
        };
        if let Some(path) = command {
            let repair = attempt::repair(&self.environment, plan, &path)?;
            let version = self.executor.version(&path);
            if repair {
                repairing_owned = true;
                if version.as_ref().is_ok_and(|version| {
                    !recipes::matches_pinned_version(plan.tool, version, &plan.version)
                }) {
                    bail!("未完成安装的版本已改变；请按官方说明处理，未自动升级或覆盖")
                }
                self.update(plan.tool, |state| {
                    state.detail = "正在恢复上次未完成的安装；保留已有依赖".into()
                });
            } else {
                let version = version?;
                if !recipes::compatible_existing_version(plan.tool, &version, &plan.version) {
                    bail!("已安装版本 {version} 尚未验证兼容（当前支持 {}）；未自动升级，请按官方说明处理", plan.version)
                }
                self.update(plan.tool, |state| state.version = Some(version));
                return Ok(());
            }
        } else {
            repairing_owned = attempt::pending(&self.environment, plan)?;
        }
        // An independently installed GUI must not be overwritten by a CLI installer.
        if !repairing_owned
            && ToolInventory::new(self.environment.clone())
                .application_path(plan.tool)
                .is_some()
        {
            bail!("已检测到应用安装，请使用原有安装；未自动替换或升级")
        }
        self.update(plan.tool, |state| {
            state
                .output
                .push(plan.method.label(self.environment.platform).into());
            state.output_sequence += 1;
        });
        let native_gui = plan
            .steps
            .iter()
            .any(|step| step.stage == InstallStage::WaitingForUser);
        if !native_gui {
            attempt::begin_operation(&self.environment, plan, false)?;
        }
        self.execute_steps(plan, cancelled, false)?;
        if plan
            .steps
            .iter()
            .any(|step| step.stage == InstallStage::WaitingForUser)
        {
            return self.wait_for_application(plan, cancelled);
        }
        if cancelled.load(Ordering::SeqCst) {
            bail!("安装已取消；已准备的依赖保留")
        }
        self.update(plan.tool, |state| {
            state.stage = InstallStage::Detecting;
            state.detail = "正在重新检测实际安装".into();
        });
        let path = self
            .environment
            .command_path(&plan.command)
            .context("安装程序已结束，但未检测到有效工具；请查看输出并重试")?;
        let version = self.executor.version(&path)?;
        if !recipes::matches_pinned_version(plan.tool, &version, &plan.version) {
            bail!(
                "安装后的版本 {version} 与目标版本 {} 不符，请检查安装渠道",
                plan.version
            )
        }
        attempt::complete(&self.environment, plan.tool)?;
        self.update(plan.tool, |state| state.version = Some(version));
        Ok(())
    }
    fn execute_steps(
        &self,
        plan: &InstallPlan,
        cancelled: &AtomicBool,
        upgrading: bool,
    ) -> Result<()> {
        for step in &plan.steps {
            if cancelled.load(Ordering::SeqCst) {
                bail!("安装已取消；已准备的依赖保留")
            }
            self.update(plan.tool, |state| {
                state.stage = if step.download.is_some() {
                    InstallStage::Downloading
                } else {
                    step.stage
                };
                state.detail = if step.stage == InstallStage::WaitingForUser {
                    gui::instructions(plan.tool, self.environment.platform).into()
                } else {
                    if upgrading {
                        "升级进行中，可查看输出"
                    } else {
                        "安装进行中，可查看输出"
                    }
                    .into()
                };
            });
            // Native GUI installers own their own cancellation. Stopping this
            // client's waiting must not terminate a detached installer process.
            let native_handoff = AtomicBool::new(false);
            let cancellation = if step.stage == InstallStage::WaitingForUser {
                &native_handoff
            } else {
                cancelled
            };
            if upgrading && !step.program.is_empty() {
                if let Some(path) = self.environment.command_path(&plan.command) {
                    if let Ok(actual) = self.executor.version(&path) {
                        if updates::compare_versions(&actual, &plan.version)
                            == Some(std::cmp::Ordering::Greater)
                        {
                            bail!("检测到更高版本 {actual}，已停止本次更新，请重新检查更新");
                        }
                    }
                }
            }
            let result = self.executor.execute(step, cancellation, &mut |line| {
                self.update(plan.tool, |state| {
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) {
                        if value["name"] == "openclaw" {
                            state.stage = InstallStage::Installing;
                        } else if value["name"]
                            .as_str()
                            .is_some_and(|name| name.contains("node") || name == "dependencies")
                        {
                            state.stage = InstallStage::Dependencies;
                        }
                    }
                    state.output.push(safe_output(&line));
                    state.output_sequence += 1;
                    if state.output.len() > 200 {
                        state.output.remove(0);
                    }
                });
            });
            let observed = attempt::observe(&self.environment, plan.tool);
            result?;
            observed?;
        }
        Ok(())
    }
    fn wait_for_application(&self, plan: &InstallPlan, cancelled: &AtomicBool) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(1800);
        loop {
            if cancelled.load(Ordering::SeqCst) {
                bail!("已停止等待；请在原生安装器中取消安装。已下载文件与已完成安装保留，可重新检测后重试")
            }
            if let Some(path) =
                ToolInventory::new(self.environment.clone()).application_path(plan.tool)
            {
                if let Ok(version) = self.executor.tool_application_version(plan.tool, &path) {
                    if gui::matches_version(plan.tool, &version, &plan.version) {
                        return Ok(());
                    }
                    bail!(
                        "检测到应用版本 {version} 与目标版本 {} 不匹配，请检查安装来源",
                        plan.version
                    )
                }
            }
            if Instant::now() >= deadline {
                bail!("尚未检测到完整应用安装；请完成原生安装器步骤后重新检测或重试")
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    fn update(&self, tool: ToolId, action: impl FnOnce(&mut InstallStatus)) {
        if let Some(job) = self.jobs.lock().unwrap().get_mut(&tool.to_string()) {
            action(&mut job.status);
        }
    }
}

fn installation_lock(
    environment: &InstallationEnvironment,
    tool: ToolId,
) -> Result<InstallationGuard> {
    let tool = if tool == ToolId::DuMateEnterprise {
        ToolId::DuMate
    } else {
        tool
    };
    let directory = environment.home.join(".tokenmarket/install-locks");
    std::fs::create_dir_all(&directory)?;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(directory.join(format!("{tool}.lock")))?;
    file.try_lock()
        .map_err(|_| anyhow::anyhow!("该工具正在安装或接入，请等待另一个客户端操作完成"))?;
    Ok(InstallationGuard { file })
}

fn safe_output(line: &str) -> String {
    // Installers receive no client credentials. Redact common inherited-secret output as defense in depth.
    let lower = line.to_ascii_lowercase();
    if [
        "api_key",
        "apikey",
        "api-key",
        "authorization:",
        "bearer ",
        "sk-",
    ]
    .iter()
    .any(|key| lower.contains(key))
    {
        return "[已隐藏可能包含凭据的安装输出]".into();
    }
    line.chars()
        .filter(|character| !character.is_control() || *character == '\t')
        .take(2048)
        .collect()
}
