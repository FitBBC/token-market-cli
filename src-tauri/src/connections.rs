use tauri::{AppHandle, Manager};
use tokensmarket_core::{
    connections::{
        ConnectContext, ConnectionManager, ConnectionSummary, DisconnectOutcome, DisconnectRequest,
    },
    inventory::ToolInventory,
    ToolId,
};

#[tauri::command]
pub async fn connect_tool(app: AppHandle, tool: String) -> Result<ConnectionSummary, String> {
    let tool = tokensmarket_core::tool_id(&tool).map_err(|e| e.to_string())?;
    // Snapshot both identities before awaiting key issuance. Later header changes cannot rebind this action.
    let session = app
        .state::<crate::DesktopState>()
        .session
        .lock()
        .expect("desktop session lock")
        .clone()
        .ok_or("请先登录")?;
    let workspace = crate::workspace::current_context(&app)?;
    {
        let state = app.state::<crate::DesktopState>();
        let current = state.session.lock().expect("desktop session lock");
        if current.as_ref().map(|value| &value.session_token) != Some(&session.session_token)
            || workspace.id != session.default_workspace.id
        {
            return Err("账号或当前空间已改变，请刷新后重试".into());
        }
    }
    if !session.is_valid() {
        return Err("登录已过期，请重新登录".into());
    }
    let inventory = ToolInventory::current();
    inventory
        .require_installed(tool)
        .map_err(|e| e.to_string())?;
    let path = inventory.config_path(tool);
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
pub async fn disconnect_tool(
    app: AppHandle,
    tool: String,
    expected_key_id: String,
    request: DisconnectRequest,
) -> Result<DisconnectOutcome, String> {
    let tool = tokensmarket_core::tool_id(&tool).map_err(|e| e.to_string())?;
    // Local cancellation remains available after account changes or session expiration.
    // A current session only authorizes the optional disable of the old record's exact key.
    let session = app
        .state::<crate::DesktopState>()
        .session
        .lock()
        .expect("desktop session lock")
        .clone();
    let token = session
        .as_ref()
        .filter(|s| s.is_valid())
        .map(|s| s.session_token.as_str());
    app.state::<ConnectionManager>()
        .disconnect(tool, &expected_key_id, token, request)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn launch_connected_tool(app: AppHandle, tool: String) -> Result<(), String> {
    let id = tokensmarket_core::tool_id(&tool).map_err(|e| e.to_string())?;
    let command = match id {
        ToolId::ClaudeCode => "claude",
        ToolId::Codex => "codex",
        ToolId::DeepSeekHarness => "dsh",
        ToolId::WorkBuddy => "workbuddy",
        ToolId::OpenClaw => "openclaw tui",
        ToolId::OpenCode => "opencode",
    };
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
    if let Some(path) = ToolInventory::current().application_path(id) {
        #[cfg(target_os = "macos")]
        let result = std::process::Command::new("open")
            .arg("-a")
            .arg(path)
            .status();
        #[cfg(windows)]
        let result = std::process::Command::new(path).spawn();
        #[cfg(target_os = "macos")]
        return match result {
            Ok(status) if status.success() => Ok(()),
            _ => Err("工具启动失败，请从系统应用入口重新打开".into()),
        };
        #[cfg(windows)]
        return result
            .map(|_| ())
            .map_err(|_| "工具启动失败，请从系统应用入口重新打开".into());
    }
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("osascript")
        .args([
            "-e",
            "tell application \"Terminal\" to activate",
            "-e",
            &format!("tell application \"Terminal\" to do script \"{command}\""),
        ])
        .status();
    #[cfg(windows)]
    let result = std::process::Command::new("cmd")
        .args(["/c", "start", "", "cmd", "/k", command])
        .status();
    #[cfg(not(any(target_os = "macos", windows)))]
    return Err(format!("请打开终端并运行 {command}"));
    #[cfg(any(target_os = "macos", windows))]
    match result {
        Ok(status) if status.success() => Ok(()),
        _ => Err(format!("终端启动失败，请手动打开新终端运行 {command}")),
    }
}
