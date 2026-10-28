use tauri::{AppHandle, Manager};
use tokensmarket_core::{
    connections::{ConnectContext, ConnectionManager, ConnectionSummary},
    inventory::ToolInventory,
    ToolId,
};

#[tauri::command]
pub async fn connect_tool(app: AppHandle, tool: String) -> Result<ConnectionSummary, String> {
    let tool = tokensmarket_core::tool_id(&tool).map_err(|e| e.to_string())?;
    // Snapshot both identities before awaiting key issuance. Later header changes cannot rebind this action.
    let workspace = crate::workspace::current_context(&app)?;
    let session = app
        .state::<crate::DesktopState>()
        .session
        .lock()
        .expect("desktop session lock")
        .clone()
        .ok_or("请先登录")?;
    if !session.is_valid() {
        return Err("登录已过期，请重新登录".into());
    }
    let inventory = ToolInventory::current();
    inventory
        .require_installed(tool)
        .map_err(|e| e.to_string())?;
    let path = tokensmarket_core::get_adapter(tool).config_path();
    app.state::<ConnectionManager>()
        .connect(
            ConnectContext {
                session_token: session.session_token,
                workspace_id: workspace.id,
            },
            tool,
            &inventory,
            path,
        )
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn launch_connected_tool(app: AppHandle, tool: String) -> Result<(), String> {
    let id = tokensmarket_core::tool_id(&tool).map_err(|e| e.to_string())?;
    if id != ToolId::ClaudeCode {
        return Err("此工具的启动入口尚未开放".into());
    }
    ToolInventory::current()
        .require_installed(id)
        .map_err(|e| e.to_string())?;
    if !app
        .state::<ConnectionManager>()
        .list()
        .map_err(|e| e.to_string())?
        .iter()
        .any(|r| r.tool == id)
    {
        return Err("请先完成接入".into());
    }
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("osascript")
        .args([
            "-e",
            "tell application \"Terminal\" to activate",
            "-e",
            "tell application \"Terminal\" to do script \"claude\"",
        ])
        .status();
    #[cfg(windows)]
    let result = std::process::Command::new("cmd")
        .args(["/c", "start", "", "cmd", "/k", "claude"])
        .status();
    #[cfg(not(any(target_os = "macos", windows)))]
    return Err("请打开终端并运行 claude".into());
    #[cfg(any(target_os = "macos", windows))]
    match result {
        Ok(status) if status.success() => Ok(()),
        _ => Err("终端启动失败，请手动打开新终端运行 claude".into()),
    }
}
