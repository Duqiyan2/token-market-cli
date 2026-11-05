use crate::DesktopState;
use tauri::{AppHandle, Manager};
use tokenmarket_core::usage::{UsageClient, UsageKind, UsageQuery, UsageResponse};

async fn query(
    app: AppHandle,
    request: UsageQuery,
    kind: UsageKind,
) -> Result<UsageResponse, String> {
    let session = app
        .state::<DesktopState>()
        .session
        .lock()
        .expect("desktop session lock")
        .clone()
        .ok_or("请先登录")?;
    let shared = app
        .state::<DesktopState>()
        .login
        .shared_session()
        .map_err(|e| e.to_string())?;
    if shared.as_ref().map(|s| &s.session_token) != Some(&session.session_token) {
        return Err("共享登录已改变，请刷新用量".into());
    }
    let workspace = crate::workspace::current_context(&app)?;
    if workspace.id != request.workspace_id {
        return Err("当前空间已改变，请刷新用量".into());
    }
    request.validate(&session).map_err(|e| e.to_string())?;
    let device = app
        .state::<tokenmarket_core::connections::ConnectionManager>()
        .device_id()
        .map_err(|e| e.to_string())?;
    let result = UsageClient::new(tokenmarket_core::config::console_api_base_url())
        .query(&session, &device, &request, kind)
        .await
        .map_err(|e| e.to_string())?;
    let shared_after = app
        .state::<DesktopState>()
        .login
        .shared_session()
        .map_err(|e| e.to_string())?;
    if shared_after.as_ref().map(|s| &s.session_token) != Some(&session.session_token) {
        return Err("共享登录已改变，已丢弃旧用量结果".into());
    }
    let current = app
        .state::<DesktopState>()
        .session
        .lock()
        .expect("desktop session lock")
        .clone();
    if current.as_ref().map(|s| &s.session_token) != Some(&session.session_token)
        || crate::workspace::current_context(&app)?.id != workspace.id
    {
        return Err("账号或当前空间已改变，已丢弃旧用量结果".into());
    }
    Ok(result)
}
#[tauri::command]
pub async fn desktop_usage_logs(
    app: AppHandle,
    request: UsageQuery,
) -> Result<UsageResponse, String> {
    query(app, request, UsageKind::Logs).await
}
#[tauri::command]
pub async fn desktop_usage_buckets(
    app: AppHandle,
    request: UsageQuery,
) -> Result<UsageResponse, String> {
    query(app, request, UsageKind::Buckets).await
}
