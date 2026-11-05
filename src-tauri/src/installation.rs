use tauri::{AppHandle, Manager};
use tokenmarket_core::installation::{InstallStatus, InstallationManager};

#[tauri::command]
pub async fn tool_uninstall_candidates(
    app: AppHandle,
    tool: String,
) -> Result<Vec<tokenmarket_core::installation::uninstall::UninstallCandidate>, String> {
    let tool = tokenmarket_core::tool_id(&tool).map_err(|e| e.to_string())?;
    let manager = app.state::<InstallationManager>().inner().clone();
    tauri::async_runtime::spawn_blocking(move || manager.uninstall_candidates(tool))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn start_tool_uninstall(
    app: AppHandle,
    tool: String,
    plan: tokenmarket_core::installation::uninstall::UninstallPlan,
) -> Result<InstallStatus, String> {
    let tool = tokenmarket_core::tool_id(&tool).map_err(|e| e.to_string())?;
    let manager = app.state::<InstallationManager>().inner().clone();
    tauri::async_runtime::spawn_blocking(move || manager.start_uninstall(tool, plan))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn check_tool_updates(
    app: AppHandle,
) -> Result<Vec<tokenmarket_core::installation::updates::ToolUpdate>, String> {
    let manager = app.state::<InstallationManager>().inner().clone();
    Ok(manager.check_updates().await)
}

#[tauri::command]
pub async fn start_tool_upgrade(
    app: AppHandle,
    tool: String,
    target_version: String,
    plan_revision: String,
) -> Result<InstallStatus, String> {
    let tool = tokenmarket_core::tool_id(&tool).map_err(|error| error.to_string())?;
    let manager = app.state::<InstallationManager>().inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        manager.start_upgrade_confirmed(tool, &target_version, &plan_revision)
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn start_tool_installation(app: AppHandle, tool: String) -> Result<InstallStatus, String> {
    let tool = tokenmarket_core::tool_id(&tool).map_err(|error| error.to_string())?;
    app.state::<InstallationManager>()
        .start(tool)
        .map_err(|error| error.to_string())
}
#[tauri::command]
pub fn tool_installation_status(
    app: AppHandle,
    tool: String,
) -> Result<Option<InstallStatus>, String> {
    let tool = tokenmarket_core::tool_id(&tool).map_err(|error| error.to_string())?;
    Ok(app.state::<InstallationManager>().status(tool))
}
#[tauri::command]
pub fn cancel_tool_installation(app: AppHandle, tool: String) -> Result<(), String> {
    let tool = tokenmarket_core::tool_id(&tool).map_err(|error| error.to_string())?;
    app.state::<InstallationManager>()
        .cancel(tool)
        .map_err(|error| error.to_string())
}
