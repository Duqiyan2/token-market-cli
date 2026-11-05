use serde::Serialize;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
#[cfg(target_os = "macos")]
mod macos;
mod reminders;
#[cfg(windows)]
pub mod windows;
use reminders::Reminders;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tauri_plugin_opener::OpenerExt;
use tauri_plugin_updater::{Update, UpdaterExt};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientUpdateStatus {
    pub status: String,
    pub current_version: String,
    pub available_version: Option<String>,
    pub release_notes: Option<String>,
    pub error: Option<String>,
    pub automatic: bool,
    pub remind: bool,
    pub install_available: bool,
    pub recovery_available: bool,
    pub show_recovery_notice: bool,
}

/// Constructed only from a successfully verified native download. Never IPC data.
#[allow(dead_code)] // Read by validated platform installers in the following slices.
pub struct VerifiedClientUpdate {
    update: Update,
    bytes: Vec<u8>,
}

impl VerifiedClientUpdate {
    #[allow(dead_code)]
    pub fn into_parts(self) -> (Update, Vec<u8>) {
        (self.update, self.bytes)
    }
}

/// Platform boundary: prepare must obtain the full legacy lock set and retain it
/// in the returned object (or its already-running guardian) before returning.
/// Each registered platform implementation validates its guarded lifetime.
pub trait ClientUpdateInstaller: Send + Sync {
    fn prepare(&self, update: VerifiedClientUpdate)
        -> Result<Box<dyn ClientUpdateHandoff>, String>;
}

/// Dropping before commit must abort without installation. Commit must retain
/// protection until known installer termination, even if the desktop exits.
/// Success means handoff only; actual target-version startup is separate.
/// Err is allowed only when installation is known inactive (precommit abort,
/// launch failure, or known termination). An uncertain postcommit result stays
/// a pending handoff in the durable journal; it must not enable another attempt.
pub trait ClientUpdateHandoff: Send {
    fn commit(self: Box<Self>) -> Result<ClientUpdateCommit, String>;
}

/// A postcommit warning must never make the verified candidate retryable.
pub enum ClientUpdateCommit {
    Handoff,
    HandoffWithWarning(String),
}

pub enum ClientUpdateStartup {
    Clear,
    Pending(String),
    RetryableFailure(String),
}

struct UpdateConfig {
    endpoint: Option<String>,
    public_key: Option<String>,
    recovery_url: Option<String>,
    target: Option<String>,
    #[cfg(test)]
    executable_path: Option<std::path::PathBuf>,
    #[cfg(test)]
    allow_loopback_http: bool,
}

impl UpdateConfig {
    fn from_build() -> Self {
        Self {
            endpoint: option_env!("TOKEN_MARKET_UPDATER_ENDPOINT").map(str::to_owned),
            public_key: option_env!("TOKEN_MARKET_UPDATER_PUBLIC_KEY").map(str::to_owned),
            recovery_url: option_env!("TOKEN_MARKET_UPDATER_RECOVERY_URL").map(str::to_owned),
            target: match (std::env::consts::OS, std::env::consts::ARCH) {
                ("macos", "aarch64") => Some("darwin-aarch64".into()),
                ("windows", "x86_64") => Some("windows-x86_64-nsis".into()),
                _ => None,
            },
            #[cfg(test)]
            executable_path: None,
            #[cfg(test)]
            allow_loopback_http: false,
        }
    }

    fn validate_url(&self, url: &tauri::Url) -> Result<(), String> {
        if url.scheme() == "https" {
            return Ok(());
        }
        #[cfg(test)]
        if self.allow_loopback_http
            && url.scheme() == "http"
            && url
                .host_str()
                .and_then(|host| host.parse::<std::net::IpAddr>().ok())
                .is_some_and(|host| host.is_loopback())
        {
            return Ok(());
        }
        Err("客户端更新仅允许 HTTPS 地址，请联系发布方".into())
    }

    fn fixture_http_enabled(&self) -> bool {
        #[cfg(test)]
        {
            self.allow_loopback_http
        }
        #[cfg(not(test))]
        {
            false
        }
    }
}

struct UpdateState {
    status: ClientUpdateStatus,
    // Keep the native candidate here; the renderer never receives package URLs or signatures.
    candidate: Option<Update>,
    verified_bytes: Option<Vec<u8>>,
}

pub struct ClientUpdater {
    config: UpdateConfig,
    installer: Option<Arc<dyn ClientUpdateInstaller>>,
    reminders: Mutex<Reminders>,
    startup_started: AtomicBool,
    state: Mutex<UpdateState>,
    operation: tauri::async_runtime::Mutex<()>,
}

impl ClientUpdater {
    pub fn new(current_version: String) -> Self {
        #[cfg(not(test))]
        let path = Some(
            tokenmarket_core::config::token_market_home().join("client-update-preferences.json"),
        );
        #[cfg(test)]
        let path = None;
        Self::with_preferences(current_version, path)
    }

    #[cfg(test)]
    fn with_reminder_path(current_version: String, path: std::path::PathBuf) -> Self {
        Self::with_preferences(current_version, Some(path))
    }

    fn with_preferences(current_version: String, path: Option<std::path::PathBuf>) -> Self {
        Self {
            config: UpdateConfig::from_build(),
            installer: None,
            reminders: Mutex::new(Reminders::new(path)),
            startup_started: AtomicBool::new(false),
            state: Mutex::new(UpdateState {
                status: ClientUpdateStatus {
                    status: "idle".into(),
                    current_version,
                    available_version: None,
                    release_notes: None,
                    error: None,
                    automatic: false,
                    remind: false,
                    install_available: false,
                    recovery_available: false,
                    show_recovery_notice: false,
                },
                candidate: None,
                verified_bytes: None,
            }),
            operation: tauri::async_runtime::Mutex::new(()),
        }
    }

    /// Native platform registration only; renderer cannot select an executor.
    pub fn with_installer(mut self, installer: Arc<dyn ClientUpdateInstaller>) -> Self {
        self.installer = Some(installer);
        self.state
            .lock()
            .expect("client update state")
            .status
            .install_available = true;
        self
    }

    pub async fn install<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        expected_version: &str,
    ) -> Result<ClientUpdateStatus, String> {
        let _operation = self.operation.lock().await;
        let installer = self
            .installer
            .as_ref()
            .ok_or("安装并重启暂不可用，请使用官方安装包更新")?;
        let verified = {
            let mut state = self.state.lock().expect("client update state");
            if state.status.status != "ready"
                || state.status.available_version.as_deref() != Some(expected_version)
            {
                return Err("请先下载并验证当前客户端更新，再确认安装".into());
            }
            let update = state.candidate.clone().ok_or("请重新检查客户端更新")?;
            let bytes = state.verified_bytes.clone().ok_or("请先验证客户端更新包")?;
            state.status.status = "installing".into();
            state.status.error = None;
            state.status.automatic = false;
            state.status.remind = false;
            VerifiedClientUpdate { update, bytes }
        };
        let _ = app.emit("desktop-client-update-status", self.status());
        let result = installer
            .prepare(verified)
            .and_then(|handoff| handoff.commit());
        let status = {
            let mut state = self.state.lock().expect("client update state");
            match result {
                Ok(outcome) => {
                    state.status.status = "handoff".into();
                    if let ClientUpdateCommit::HandoffWithWarning(error) = outcome {
                        state.status.error = Some(error);
                    }
                }
                Err(error) => {
                    state.status.status = "ready".into();
                    state.status.error = Some(error);
                }
            }
            state.status.clone()
        };
        let _ = app.emit("desktop-client-update-status", &status);
        Ok(status)
    }

    /// Preserve unresolved native installation facts before any network/config check.
    fn reconcile_startup_result(&self, recovery: Result<ClientUpdateStartup, String>) {
        let outcome = recovery.unwrap_or_else(ClientUpdateStartup::Pending);
        let (status, error, show_notice) = match outcome {
            ClientUpdateStartup::Clear => return,
            ClientUpdateStartup::Pending(error) => ("handoff", error, false),
            ClientUpdateStartup::RetryableFailure(error) => {
                // Preserve this known native outcome until a manual check. The
                // automatic network/config check must not erase recovery help.
                self.startup_started.store(true, Ordering::SeqCst);
                ("install_failed", error, true)
            }
        };
        let mut state = self.state.lock().expect("client update state");
        state.status.status = status.into();
        state.status.show_recovery_notice = show_notice;
        state.status.error = Some(error);
        state.status.automatic = false;
        state.status.remind = false;
    }

    pub fn status(&self) -> ClientUpdateStatus {
        let mut state = self.state.lock().expect("client update state");
        state.status.recovery_available = self.recovery_url().is_ok();
        state.status.clone()
    }

    pub fn acknowledge_failure_notice(&self) -> ClientUpdateStatus {
        let mut state = self.state.lock().expect("client update state");
        state.status.show_recovery_notice = false;
        state.status.clone()
    }

    pub fn open_recovery<R: Runtime>(&self, app: &AppHandle<R>) -> Result<(), String> {
        let url = self.recovery_url()?;
        app.opener()
            .open_url(url.as_str(), None::<&str>)
            .map_err(|_| "无法打开客户端恢复页面，请联系发布方获取官方安装包".into())
    }

    fn recovery_url(&self) -> Result<tauri::Url, String> {
        self.config
            .recovery_url
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .and_then(|value| value.parse::<tauri::Url>().ok())
            .filter(|url| {
                url.scheme() == "https"
                    && url.host_str().is_some()
                    && url.username().is_empty()
                    && url.password().is_none()
            })
            .ok_or_else(|| "客户端恢复地址暂不可用，请联系发布方获取官方安装包和恢复说明".into())
    }

    pub fn later(&self) -> ClientUpdateStatus {
        let mut state = self.state.lock().expect("client update state");
        self.reminders.lock().expect("client reminders").later();
        state.status.remind = false;
        state.status.clone()
    }

    pub fn ignore_version(&self, version: &str) -> Result<ClientUpdateStatus, String> {
        let mut state = self.state.lock().expect("client update state");
        if state.status.available_version.as_deref() != Some(version) {
            return Err("可用版本已改变，请重新检查客户端更新".into());
        }
        self.reminders
            .lock()
            .expect("client reminders")
            .ignore(version)?;
        state.status.remind = false;
        Ok(state.status.clone())
    }

    pub async fn startup_check<R: Runtime>(&self, app: &AppHandle<R>) -> ClientUpdateStatus {
        if self.startup_started.swap(true, Ordering::SeqCst) {
            return self.status();
        }
        self.check_with_origin(app, true).await
    }

    pub async fn check<R: Runtime>(&self, app: &AppHandle<R>) -> ClientUpdateStatus {
        self.check_with_origin(app, false).await
    }

    async fn check_with_origin<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        automatic: bool,
    ) -> ClientUpdateStatus {
        if matches!(
            self.status().status.as_str(),
            "downloading" | "verifying" | "ready" | "installing" | "handoff"
        ) {
            return self.status();
        }
        let _operation = self.operation.lock().await;
        if matches!(
            self.status().status.as_str(),
            "ready" | "installing" | "handoff"
        ) {
            return self.status();
        }
        {
            let mut state = self.state.lock().expect("client update state");
            state.status.automatic = automatic;
            state.status.remind = false;
            state.candidate = None;
            state.verified_bytes = None;
            state.status.status = "checking".into();
            state.status.show_recovery_notice = false;
            state.status.available_version = None;
            state.status.release_notes = None;
            state.status.error = None;
        }
        let result = self.check_release(app).await;
        let mut state = self.state.lock().expect("client update state");
        match result {
            Ok(Some(update)) => {
                state.status.remind = automatic
                    && self
                        .reminders
                        .lock()
                        .expect("client reminders")
                        .eligible(&update.version);
                state.status.status = "available".into();
                state.status.available_version = Some(update.version.clone());
                state.status.release_notes = update.body.clone();
                state.candidate = Some(update);
            }
            Ok(None) => state.status.status = "current".into(),
            Err(error) => {
                state.status.status = "failed".into();
                state.status.error = Some(error);
            }
        }
        state.status.clone()
    }

    // Version is only a consent context; URLs, signatures and bytes never cross IPC.
    pub async fn download<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        expected_version: &str,
    ) -> Result<ClientUpdateStatus, String> {
        let _operation = self.operation.lock().await;
        let mut update = {
            let mut state = self.state.lock().expect("client update state");
            let update = state
                .candidate
                .as_ref()
                .filter(|candidate| candidate.version == expected_version)
                .ok_or("更新版本已改变，请重新检查并确认下载")?
                .clone();
            if matches!(
                state.status.status.as_str(),
                "ready" | "installing" | "handoff"
            ) {
                return Ok(state.status.clone());
            }
            state.verified_bytes = None;
            state.status.status = "downloading".into();
            state.status.automatic = false;
            state.status.remind = false;
            state.status.error = None;
            update
        };
        // Upstream's check timeout is not copied onto Update.
        update.timeout = Some(Duration::from_secs(120));
        let _ = app.emit("desktop-client-update-status", self.status());
        let result = update
            .download(
                |_, _| {},
                || {
                    self.state
                        .lock()
                        .expect("client update state")
                        .status
                        .status = "verifying".into();
                    let _ = app.emit("desktop-client-update-status", self.status());
                },
            )
            .await;
        let status = {
            let mut state = self.state.lock().expect("client update state");
            match result {
                Ok(bytes) => {
                    state.verified_bytes = Some(bytes);
                    state.status.status = "ready".into();
                }
                Err(error) => {
                    state.verified_bytes = None;
                    state.status.status = "failed".into();
                    state.status.error =
                        Some(format!("更新包下载或签名验证失败，请重试下载：{error}"));
                }
            }
            state.status.clone()
        };
        let _ = app.emit("desktop-client-update-status", &status);
        Ok(status)
    }

    async fn check_release<R: Runtime>(
        &self,
        app: &AppHandle<R>,
    ) -> Result<Option<Update>, String> {
        let target = self
            .config
            .target
            .as_deref()
            .ok_or("当前系统暂不支持客户端更新；仅支持 macOS Apple Silicon 和 Windows x64")?;
        let endpoint = self
            .config
            .endpoint
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .ok_or("客户端更新暂不可用：尚未配置稳定版清单地址，请联系发布方")?;
        let public_key = self
            .config
            .public_key
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .ok_or("客户端更新暂不可用：尚未配置签名公钥，请联系发布方")?;
        let endpoint = endpoint
            .parse::<tauri::Url>()
            .map_err(|_| "客户端更新清单地址无效")?;
        self.config.validate_url(&endpoint)?;
        let fixture_http = self.config.fixture_http_enabled();
        let builder = app
            .updater_builder()
            .endpoints(vec![endpoint])
            .map_err(|error| error.to_string())?
            .pubkey(public_key)
            .target(target)
            .version_comparator(|current, remote| {
                remote.version.pre.is_empty() && remote.version > current
            })
            .configure_client(move |client| client.https_only(!fixture_http))
            .timeout(Duration::from_secs(15));
        #[cfg(test)]
        let builder = match &self.config.executable_path {
            Some(path) => builder.executable_path(path).no_proxy(),
            None => builder,
        };
        let update = builder
            .build()
            .map_err(|error| error.to_string())?
            .check()
            .await
            .map_err(|error| format!("无法读取稳定版清单：{error}"))?;
        if let Some(update) = &update {
            self.config.validate_url(&update.download_url)?;
            if update.raw_json.get("url").is_some()
                || update.raw_json.get("signature").is_some()
                || update
                    .raw_json
                    .get("platforms")
                    .and_then(|platforms| platforms.get(target))
                    .is_none()
            {
                return Err("稳定版清单缺少匹配当前系统的静态更新包".into());
            }
            if update.signature.trim().is_empty() {
                return Err("稳定版更新包缺少签名，请联系发布方".into());
            }
        }
        Ok(update)
    }
}

pub fn setup(app: &AppHandle) {
    let updater = ClientUpdater::new(app.package_info().version.to_string());
    #[cfg(target_os = "macos")]
    let updater = {
        let updater = match macos::installer(app) {
            Some(installer) => updater.with_installer(installer),
            None => updater,
        };
        updater.reconcile_startup_result(macos::reconcile_startup());
        updater
    };
    app.manage(updater);
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let status = handle.state::<ClientUpdater>().startup_check(&handle).await;
        let _ = handle.emit("desktop-client-update-status", status);
    });
}

#[tauri::command]
pub fn defer_client_update(app: AppHandle) -> ClientUpdateStatus {
    let status = app.state::<ClientUpdater>().later();
    let _ = app.emit("desktop-client-update-status", &status);
    status
}

#[tauri::command]
pub fn ignore_client_update_version(
    app: AppHandle,
    version: String,
) -> Result<ClientUpdateStatus, String> {
    let status = app.state::<ClientUpdater>().ignore_version(&version)?;
    let _ = app.emit("desktop-client-update-status", &status);
    Ok(status)
}

#[tauri::command]
pub fn desktop_client_update_status(app: AppHandle) -> ClientUpdateStatus {
    app.state::<ClientUpdater>().status()
}

#[tauri::command]
pub async fn check_client_update(app: AppHandle) -> ClientUpdateStatus {
    app.state::<ClientUpdater>().check(&app).await
}

#[tauri::command]
pub async fn download_client_update(
    app: AppHandle,
    version: String,
) -> Result<ClientUpdateStatus, String> {
    app.state::<ClientUpdater>().download(&app, &version).await
}

#[cfg(test)]
mod tests;

#[tauri::command]
pub async fn install_client_update(
    app: AppHandle,
    version: String,
) -> Result<ClientUpdateStatus, String> {
    app.state::<ClientUpdater>().install(&app, &version).await
}

#[tauri::command]
pub fn open_client_update_recovery(app: AppHandle) -> Result<(), String> {
    app.state::<ClientUpdater>().open_recovery(&app)
}

#[tauri::command]
pub fn acknowledge_client_update_failure(app: AppHandle) -> ClientUpdateStatus {
    app.state::<ClientUpdater>().acknowledge_failure_notice()
}
