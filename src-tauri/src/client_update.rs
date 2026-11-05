use serde::Serialize;
use std::{sync::Mutex, time::Duration};
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
    verified_bytes: Option<Vec<u8>>,
}

pub struct ClientUpdater {
    config: UpdateConfig,
    state: Mutex<UpdateState>,
    operation: tauri::async_runtime::Mutex<()>,
}

impl ClientUpdater {
    pub fn new(current_version: String) -> Self {
        Self {
            config: UpdateConfig::from_build(),
            state: Mutex::new(UpdateState {
                status: ClientUpdateStatus {
                    status: "idle".into(),
                    current_version,
                    available_version: None,
                    release_notes: None,
                    error: None,
                },
                candidate: None,
                verified_bytes: None,
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

    pub async fn check<R: Runtime>(&self, app: &AppHandle<R>) -> ClientUpdateStatus {
        if matches!(
            self.status().status.as_str(),
            "downloading" | "verifying" | "ready"
        ) {
            return self.status();
        }
        let _operation = self.operation.lock().await;
        if self.status().status == "ready" {
            return self.status();
        }
        {
            let mut state = self.state.lock().expect("client update state");
            state.candidate = None;
            state.verified_bytes = None;
            state.status.status = "checking".into();
            state.status.available_version = None;
            state.status.release_notes = None;
            state.status.error = None;
        }
        let result = self.check_release(app).await;
        let mut state = self.state.lock().expect("client update state");
        match result {
            Ok(Some(update)) => {
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
            if state.status.status == "ready" {
                return Ok(state.status.clone());
            }
            state.verified_bytes = None;
            state.status.status = "downloading".into();
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
