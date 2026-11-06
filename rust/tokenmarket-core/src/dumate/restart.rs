//! Explicitly confirmed restart; never inspect task state or runtime credentials.
#[cfg(any(test, not(all(windows, target_arch = "x86_64"))))]
use anyhow::bail;
use anyhow::{Context, Result};
use serde::Serialize;
use std::path::Path;
#[cfg(target_os = "macos")]
use std::{
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;
#[cfg(any(windows, test))]
mod windows_contract;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestartOutcome {
    pub state: &'static str,
    pub message: &'static str,
}
pub const WARNING: &str = "所有正在运行的任务都将被中断，定时任务也将无法调度。";
const UNKNOWN: &str =
    "无法确认搭子进程身份，未执行重启。请手动退出后重新打开；已保存的配置不受影响。";
fn confirmation_required() -> RestartOutcome {
    RestartOutcome {
        state: "confirmation_required",
        message: WARNING,
    }
}
pub fn cleanup_legacy_extensions() -> Result<usize> {
    let platform = if cfg!(target_os = "macos") {
        crate::inventory::HostPlatform::MacOs
    } else if cfg!(windows) {
        crate::inventory::HostPlatform::Windows
    } else {
        crate::inventory::HostPlatform::Unix
    };
    super::legacy::retire_all(&crate::config::home_dir(), platform)
}
pub fn is_running() -> Result<bool> {
    super::is_running()
}

trait Driver {
    fn running(&mut self) -> impl std::future::Future<Output = Result<bool>> + Send;
    fn quit(&mut self) -> impl std::future::Future<Output = Result<()>> + Send;
    fn wait_stopped(&mut self) -> impl std::future::Future<Output = Result<()>> + Send;
    fn open(&mut self) -> impl std::future::Future<Output = Result<()>> + Send;
}
async fn execute(driver: &mut impl Driver, force: bool) -> Result<RestartOutcome> {
    if !force {
        return Ok(confirmation_required());
    }
    let running = driver.running().await?;
    if running {
        driver.quit().await?;
        driver.wait_stopped().await?;
    }
    driver
        .open()
        .await
        .context("未能重新打开搭子，请手动打开；已保存的配置不受影响")?;
    Ok(RestartOutcome {
        state: if running { "restarted" } else { "opened" },
        message: "已打开搭子，配置将在启动时加载。",
    })
}

static OPERATION: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
pub(crate) fn configuration_lock(
    config: &Path,
) -> Result<Option<tokio::sync::MutexGuard<'static, ()>>> {
    let platform = if cfg!(target_os = "macos") {
        crate::inventory::HostPlatform::MacOs
    } else if cfg!(windows) {
        crate::inventory::HostPlatform::Windows
    } else {
        crate::inventory::HostPlatform::Unix
    };
    if !config.starts_with(super::user_data(&crate::config::home_dir(), platform)) {
        return Ok(None);
    }
    Ok(Some(OPERATION.try_lock().map_err(|_| {
        anyhow::anyhow!("搭子配置或重启操作正在进行，请稍后重试")
    })?))
}

pub async fn restart_current(force: bool) -> Result<RestartOutcome> {
    // A caller without explicit confirmation cannot inspect or terminate processes.
    if !force {
        return Ok(confirmation_required());
    }
    let _guard = OPERATION
        .try_lock()
        .map_err(|_| anyhow::anyhow!("搭子配置或重启操作正在进行，请稍后重试"))?;
    let inventory = crate::inventory::ToolInventory::current();
    inventory.require_installed(crate::ToolId::DuMate)?;
    let app = inventory
        .application_path(crate::ToolId::DuMate)
        .context("未找到百度搭子应用")?
        .canonicalize()?;
    #[cfg(target_os = "macos")]
    {
        return execute(&mut NativeDriver::new(app)?, true).await;
    }
    #[cfg(all(windows, target_arch = "x86_64"))]
    {
        return execute(&mut windows::WindowsDriver::new(app)?, true).await;
    }
    #[cfg(not(any(target_os = "macos", all(windows, target_arch = "x86_64"))))]
    {
        let _ = app;
        bail!("当前平台请手动退出并重新打开搭子");
    }
}

#[cfg(target_os = "macos")]
struct NativeDriver {
    app: PathBuf,
    closing: Vec<macos::Process>,
}
#[cfg(target_os = "macos")]
impl NativeDriver {
    fn new(app: PathBuf) -> Result<Self> {
        macos::validate_app(&app)?;
        Ok(Self {
            app,
            closing: Vec::new(),
        })
    }
}
#[cfg(target_os = "macos")]
impl Driver for NativeDriver {
    async fn running(&mut self) -> Result<bool> {
        self.closing = macos::capture(&self.app)?;
        Ok(!self.closing.is_empty())
    }
    async fn quit(&mut self) -> Result<()> {
        let app = self.app.clone();
        let closing = self.closing.clone();
        tokio::task::spawn_blocking(move || macos::terminate(&app, &closing)).await?
    }
    async fn wait_stopped(&mut self) -> Result<()> {
        for _ in 0..75 {
            if macos::capture(&self.app)?.is_empty() {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        bail!("搭子后台尚未完全退出，未重复打开。请手动退出后重新打开")
    }
    async fn open(&mut self) -> Result<()> {
        macos::validate_app(&self.app)?;
        if !macos::capture(&self.app)?.is_empty() {
            bail!("搭子仍在运行，未重复打开");
        }
        let app = self.app.clone();
        tokio::task::spawn_blocking(move || {
            command_output(
                "/usr/bin/open",
                &["-a".into(), app.to_string_lossy().into_owned()],
                Duration::from_secs(5),
            )
        })
        .await??;
        Ok(())
    }
}
#[cfg(target_os = "macos")]
fn command_output(program: &str, args: &[String], timeout: Duration) -> Result<String> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("无法执行搭子重启操作")?;
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            let output = child.wait_with_output()?;
            if !status.success() {
                bail!("系统未能执行搭子重启，请手动退出并重新打开")
            };
            return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
        }
        if start.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            bail!("重启请求超时，请手动退出并重新打开搭子")
        }
        std::thread::sleep(Duration::from_millis(40));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fake {
        running: Option<bool>,
        stop_ok: bool,
        calls: Vec<&'static str>,
    }
    impl Driver for Fake {
        async fn running(&mut self) -> Result<bool> {
            self.calls.push("identity");
            self.running.context(UNKNOWN)
        }
        async fn quit(&mut self) -> Result<()> {
            self.calls.push("force-stop");
            Ok(())
        }
        async fn wait_stopped(&mut self) -> Result<()> {
            self.calls.push("wait");
            if self.stop_ok {
                Ok(())
            } else {
                bail!("still running")
            }
        }
        async fn open(&mut self) -> Result<()> {
            self.calls.push("open");
            Ok(())
        }
    }
    fn fake(running: Option<bool>) -> Fake {
        Fake {
            running,
            stop_ok: true,
            calls: vec![],
        }
    }
    #[tokio::test]
    async fn restart_requires_confirmation_without_querying_task_state() {
        let mut driver = fake(None);
        let result = execute(&mut driver, false).await.unwrap();
        assert_eq!(result.state, "confirmation_required");
        assert_eq!(result.message, WARNING);
        assert!(driver.calls.is_empty());
    }
    #[tokio::test]
    async fn confirmed_restart_only_checks_process_identity_and_full_exit() {
        let mut driver = fake(Some(true));
        assert_eq!(execute(&mut driver, true).await.unwrap().state, "restarted");
        assert_eq!(driver.calls, ["identity", "force-stop", "wait", "open"]);
        let mut stopped = fake(Some(false));
        assert_eq!(execute(&mut stopped, true).await.unwrap().state, "opened");
        assert_eq!(stopped.calls, ["identity", "open"]);
        let mut unknown = fake(None);
        assert!(execute(&mut unknown, true).await.is_err());
        assert_eq!(unknown.calls, ["identity"]);
        let mut remaining = fake(Some(true));
        remaining.stop_ok = false;
        assert!(execute(&mut remaining, true).await.is_err());
        assert!(!remaining.calls.contains(&"open"));
    }
    #[cfg(target_os = "macos")]
    #[tokio::test]
    #[ignore = "read-only diagnostics of the currently running DuMate instance"]
    async fn current_native_restart_preflight_is_read_only() {
        let inventory = crate::inventory::ToolInventory::current();
        let app = inventory
            .application_path(crate::ToolId::DuMate)
            .unwrap()
            .canonicalize()
            .unwrap();
        let processes = macos::capture(&app).expect("read-only installed process identity capture");
        assert!(
            !processes.is_empty(),
            "DuMate must be running for this diagnostic"
        );
    }
}
