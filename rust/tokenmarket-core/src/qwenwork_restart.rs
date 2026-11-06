//! User-requested restart delegates confirmation and task shutdown to QwenWork.
//! Never inject a task query, bypass its confirmation, or force-kill the app.
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestartOutcome {
    pub state: &'static str,
    pub message: &'static str,
}
const MANUAL: &str = "千问办公尚未完全退出，未强制关闭或重复打开。请在千问中确认退出，或手动完全退出后重新打开；已保存的配置不受影响。";
static OPERATION: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn operation_lock() -> Result<tokio::sync::MutexGuard<'static, ()>> {
    OPERATION
        .try_lock()
        .map_err(|_| anyhow::anyhow!("千问办公配置或重启操作正在进行，请稍后重试"))
}
pub(crate) fn configuration_lock(
    path: &Path,
) -> Result<Option<tokio::sync::MutexGuard<'static, ()>>> {
    let platform = if cfg!(target_os = "macos") {
        crate::inventory::HostPlatform::MacOs
    } else if cfg!(windows) {
        crate::inventory::HostPlatform::Windows
    } else {
        crate::inventory::HostPlatform::Unix
    };
    if path != crate::qwenwork::database_path(&crate::config::home_dir(), platform) {
        return Ok(None);
    }
    operation_lock().map(Some)
}
trait Driver {
    fn running(&mut self) -> impl std::future::Future<Output = Result<bool>> + Send;
    fn quit(&mut self) -> impl std::future::Future<Output = Result<()>> + Send;
    fn wait_stopped(&mut self) -> impl std::future::Future<Output = Result<()>> + Send;
    fn open(&mut self) -> impl std::future::Future<Output = Result<()>> + Send;
}
async fn execute(driver: &mut impl Driver) -> Result<RestartOutcome> {
    let running = driver.running().await?;
    if running {
        driver.quit().await?;
        driver.wait_stopped().await?;
    }
    driver
        .open()
        .await
        .context("未能打开千问办公，请手动打开；已保存的配置不受影响")?;
    Ok(RestartOutcome {
        state: if running { "restarted" } else { "opened" },
        message: "已打开千问办公，配置将在启动时加载。",
    })
}

pub async fn restart_current() -> Result<RestartOutcome> {
    let _guard = operation_lock()?;
    let app = installed_app()?;
    #[cfg(any(target_os = "macos", windows))]
    {
        return execute(&mut NativeDriver {
            app,
            identity: None,
        })
        .await;
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = app;
        bail!("当前平台请手动完全退出并重新打开千问办公")
    }
}

/// Post-save feedback only; never request termination or open the application.
pub async fn is_running() -> Result<bool> {
    let app = installed_app()?;
    #[cfg(any(target_os = "macos", windows))]
    {
        Ok(!NativeDriver {
            app,
            identity: None,
        }
        .fully_stopped()
        .await?)
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = app;
        bail!("当前平台不支持检测千问办公运行状态")
    }
}

fn installed_app() -> Result<PathBuf> {
    let inventory = crate::inventory::ToolInventory::current();
    inventory.require_installed(crate::ToolId::QwenWork)?;
    let app = inventory
        .application_path(crate::ToolId::QwenWork)
        .context("未找到千问办公应用")?
        .canonicalize()?;
    #[cfg(windows)]
    let app = PathBuf::from(
        app.to_string_lossy()
            .strip_prefix(r"\\?\")
            .unwrap_or(&app.to_string_lossy()),
    );
    Ok(app)
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct Identity {
    pid: u32,
    birth: String,
}
#[cfg(any(target_os = "macos", windows))]
struct NativeDriver {
    app: PathBuf,
    identity: Option<Identity>,
}

async fn output(mut command: Command, timeout: Duration) -> Result<String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // Keep process probes out of the user's console.
    }
    tokio::task::spawn_blocking(move || {
        let mut child = command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let started = Instant::now();
        loop {
            if let Some(status) = child.try_wait()? {
                let result = child.wait_with_output()?;
                if !status.success() {
                    bail!("系统未能执行千问办公重启，请手动重启")
                }
                if result.stdout.len() > 16384 {
                    bail!("千问办公进程状态无法确认")
                }
                return Ok(String::from_utf8(result.stdout)?);
            }
            if started.elapsed() > timeout {
                let _ = child.kill();
                let _ = child.wait(); // Only our short-lived OS helper.
                bail!("系统重启请求超时，请手动重启千问办公")
            }
            std::thread::sleep(Duration::from_millis(40));
        }
    })
    .await?
}

#[cfg(target_os = "macos")]
async fn mac_command(app: &Path, identity: Option<&Identity>) -> Result<String> {
    let info = plist::Value::from_file(app.join("Contents/Info.plist"))?;
    if info
        .as_dictionary()
        .and_then(|d| d.get("CFBundleIdentifier"))
        .and_then(plist::Value::as_string)
        != Some("cn.qwenwork.desktop.mac")
    {
        bail!("千问办公应用身份无法确认，请手动重启")
    }
    let script = r#"ObjC.import('AppKit');function run(argv){var apps=$.NSRunningApplication.runningApplicationsWithBundleIdentifier('cn.qwenwork.desktop.mac'),found=[];for(var i=0;i<Number(apps.count);i++){var app=apps.objectAtIndex(i);if(ObjC.unwrap(app.bundleURL.path)!==argv[0])throw Error('installation changed');found.push({pid:Number(app.processIdentifier),birth:String(Number(app.launchDate.timeIntervalSince1970))});if(argv.length>1&&String(Number(app.processIdentifier))===argv[1]&&String(Number(app.launchDate.timeIntervalSince1970))===argv[2])return app.terminate?'requested':'refused';}return argv.length>1?'changed':JSON.stringify(found);}"#;
    let mut cmd = Command::new("/usr/bin/osascript");
    cmd.args(["-l", "JavaScript", "-e", script, "--"]).arg(app);
    if let Some(id) = identity {
        cmd.arg(id.pid.to_string()).arg(&id.birth);
    }
    output(cmd, Duration::from_secs(5)).await
}

#[cfg(windows)]
async fn windows_command(app: &Path, identity: Option<&Identity>) -> Result<String> {
    // Values cross the PowerShell boundary through environment variables, not
    // source interpolation. CloseMainWindow invokes the product's native UI.
    let script = r#"$ErrorActionPreference='Stop';$me=[System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value;$path=[IO.Path]::GetFullPath($env:TOKENMARKET_QWEN_RESTART_APP);$all=@(Get-CimInstance Win32_Process -Filter "Name='QwenWorkCN.exe'" | Where-Object {$_.ExecutablePath -and [IO.Path]::GetFullPath($_.ExecutablePath) -ieq $path});$ids=@($all|ForEach-Object {$_.ProcessId});$found=@();foreach($p in $all){if((Invoke-CimMethod -InputObject $p -MethodName GetOwnerSid).Sid -ne $me){throw 'owner changed'};if($ids -notcontains $p.ParentProcessId){$live=Get-Process -Id $p.ProcessId;$found+=@{pid=[int]$p.ProcessId;birth=[string]$live.StartTime.ToUniversalTime().Ticks};if($env:TOKENMARKET_QWEN_RESTART_PID -eq [string]$p.ProcessId){if([string]$live.StartTime.ToUniversalTime().Ticks -ne $env:TOKENMARKET_QWEN_RESTART_BIRTH){throw 'process changed'};if(!$live.CloseMainWindow()){throw 'quit refused'};Write-Output 'requested';exit 0}}};if($env:TOKENMARKET_QWEN_RESTART_PID){throw 'process changed'};ConvertTo-Json -InputObject @($found) -Compress"#;
    let mut cmd = Command::new("powershell.exe");
    cmd.args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("TOKENMARKET_QWEN_RESTART_APP", app);
    if let Some(id) = identity {
        cmd.env("TOKENMARKET_QWEN_RESTART_PID", id.pid.to_string())
            .env("TOKENMARKET_QWEN_RESTART_BIRTH", &id.birth);
    } else {
        cmd.env_remove("TOKENMARKET_QWEN_RESTART_PID")
            .env_remove("TOKENMARKET_QWEN_RESTART_BIRTH");
    }
    output(cmd, Duration::from_secs(8)).await
}

#[cfg(any(target_os = "macos", windows))]
impl NativeDriver {
    async fn probe(&self) -> Result<Vec<Identity>> {
        #[cfg(target_os = "macos")]
        let text = mac_command(&self.app, None).await?;
        #[cfg(windows)]
        let text = windows_command(&self.app, None).await?;
        let ids: Vec<Identity> = serde_json::from_str(&text).context("千问办公进程状态无法确认")?;
        if ids.len() > 1 {
            bail!("发现多个千问办公实例，请手动重启")
        }
        Ok(ids)
    }
    async fn fully_stopped(&self) -> Result<bool> {
        if !self.probe().await?.is_empty() {
            return Ok(false);
        }
        // Include Chromium helpers/background processes before opening again.
        #[cfg(target_os = "macos")]
        {
            let pattern = self
                .app
                .join("Contents/")
                .to_string_lossy()
                .chars()
                .map(|c| {
                    if ".[](){}*+?^$|\\".contains(c) {
                        format!("\\{c}")
                    } else {
                        c.to_string()
                    }
                })
                .collect::<String>();
            let status = Command::new("/usr/bin/pgrep")
                .arg("-f")
                .arg(pattern)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()?;
            match status.code() {
                Some(1) => Ok(true),
                Some(0) => Ok(false),
                _ => bail!("千问办公后台状态无法确认"),
            }
        }
        #[cfg(windows)]
        {
            let script = r#"$p=@(Get-Process -Name QwenWorkCN -ErrorAction SilentlyContinue | Where-Object {$_.Path -and [IO.Path]::GetFullPath($_.Path) -ieq [IO.Path]::GetFullPath($env:TOKENMARKET_QWEN_RESTART_APP)});if($p.Count){Write-Output 'running'}else{Write-Output 'stopped'}"#;
            let mut cmd = Command::new("powershell.exe");
            cmd.args(["-NoProfile", "-NonInteractive", "-Command", script])
                .env("TOKENMARKET_QWEN_RESTART_APP", &self.app);
            Ok(output(cmd, Duration::from_secs(5)).await?.trim() == "stopped")
        }
    }
}
#[cfg(any(target_os = "macos", windows))]
impl Driver for NativeDriver {
    async fn running(&mut self) -> Result<bool> {
        let ids = self.probe().await?;
        self.identity = ids.into_iter().next();
        if self.identity.is_none() && !self.fully_stopped().await? {
            bail!(MANUAL)
        }
        Ok(self.identity.is_some())
    }
    async fn quit(&mut self) -> Result<()> {
        if self.probe().await?.first() != self.identity.as_ref() {
            bail!("千问办公实例已改变，请重新点击重启")
        }
        #[cfg(target_os = "macos")]
        let result = mac_command(&self.app, self.identity.as_ref()).await?;
        #[cfg(windows)]
        let result = windows_command(&self.app, self.identity.as_ref()).await?;
        if result.trim() != "requested" {
            bail!(MANUAL)
        }
        Ok(())
    }
    async fn wait_stopped(&mut self) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(90);
        while Instant::now() < deadline {
            if self.fully_stopped().await? {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(350)).await;
        }
        bail!(MANUAL)
    }
    async fn open(&mut self) -> Result<()> {
        if !self.fully_stopped().await? {
            bail!(MANUAL)
        }
        #[cfg(target_os = "macos")]
        {
            let mut cmd = Command::new("/usr/bin/open");
            cmd.arg("-a").arg(&self.app);
            output(cmd, Duration::from_secs(5)).await?;
        }
        #[cfg(windows)]
        {
            Command::new(&self.app)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fake {
        running: bool,
        quit_ok: bool,
        stopped: bool,
        events: Vec<&'static str>,
    }
    impl Driver for Fake {
        async fn running(&mut self) -> Result<bool> {
            self.events.push("probe");
            Ok(self.running)
        }
        async fn quit(&mut self) -> Result<()> {
            self.events.push("native-confirm");
            if !self.quit_ok {
                bail!("cancelled")
            }
            Ok(())
        }
        async fn wait_stopped(&mut self) -> Result<()> {
            self.events.push("wait");
            if !self.stopped {
                bail!(MANUAL)
            }
            Ok(())
        }
        async fn open(&mut self) -> Result<()> {
            self.events.push("open");
            Ok(())
        }
    }
    #[tokio::test]
    async fn native_confirmation_and_full_exit_are_required_before_reopening() {
        for (running, quit_ok, stopped, expected) in [
            (false, true, true, vec!["probe", "open"]),
            (
                true,
                true,
                true,
                vec!["probe", "native-confirm", "wait", "open"],
            ),
            (true, false, true, vec!["probe", "native-confirm"]),
            (true, true, false, vec!["probe", "native-confirm", "wait"]),
        ] {
            let mut driver = Fake {
                running,
                quit_ok,
                stopped,
                events: vec![],
            };
            let result = execute(&mut driver).await;
            assert_eq!(result.is_ok(), !running || quit_ok && stopped);
            assert_eq!(driver.events, expected);
        }
    }
    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn native_app_probe_reads_identity_without_requesting_exit() {
        let app = PathBuf::from("/Applications/QwenWorkCN.app");
        if !app.is_dir() {
            return;
        }
        let driver = NativeDriver {
            app,
            identity: None,
        };
        let observed = driver.probe().await.unwrap();
        assert!(observed.len() <= 1);
        if let Some(id) = observed.first() {
            assert!(id.pid > 1);
            assert!(!id.birth.is_empty());
        }
    }
}
