use serde::Serialize;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::Duration,
};
mod reminders;
use reminders::Reminders;
use tauri::{AppHandle, Emitter, Manager, Runtime};
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
}

struct UpdateConfig {
    endpoint: Option<String>,
    public_key: Option<String>,
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
            target: match (std::env::consts::OS, std::env::consts::ARCH) {
                ("macos", "aarch64") => Some("darwin-aarch64".into()),
                ("windows", "x86_64") => Some("windows-x86_64".into()),
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
}

pub struct ClientUpdater {
    config: UpdateConfig,
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
                },
                candidate: None,
            }),
            operation: tauri::async_runtime::Mutex::new(()),
        }
    }

    pub fn status(&self) -> ClientUpdateStatus {
        self.state
            .lock()
            .expect("client update state")
            .status
            .clone()
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
        let _operation = self.operation.lock().await;
        {
            let mut state = self.state.lock().expect("client update state");
            state.status.automatic = automatic;
            state.status.remind = false;
            state.candidate = None;
            state.status.status = "checking".into();
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
    app.manage(ClientUpdater::new(app.package_info().version.to_string()));
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

#[cfg(test)]
mod tests;
