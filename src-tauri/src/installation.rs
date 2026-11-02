use tauri::{AppHandle, Manager};
use tokenmarket_core::installation::{InstallStatus, InstallationManager};

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
