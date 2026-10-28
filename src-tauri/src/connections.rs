use tauri::{AppHandle, Manager};
use tokensmarket_core::{
    connections::{
        ConnectContext, ConnectionManager, ConnectionSummary, ConnectionTestRequest,
        ConnectionTestResult, DisconnectOutcome, DisconnectRequest, ReconnectOutcome,
    },
    inventory::ToolInventory,
    ToolId,
};

fn current_context(
    app: &AppHandle,
) -> Result<
    (
        tokensmarket_core::desktop::DesktopSession,
        tokensmarket_core::desktop::DesktopIdentity,
    ),
    String,
> {
    let session = app
        .state::<crate::DesktopState>()
        .session
        .lock()
        .expect("desktop session lock")
        .clone()
        .ok_or("请先登录")?;
    let workspace = crate::workspace::current_context(app)?;
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
    Ok((session, workspace))
}
/// Only safe identities cross the UI boundary; receipts and credentials remain private.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconnectPreview {
    old_owner: tokensmarket_core::desktop::DesktopIdentity,
    old_workspace: tokensmarket_core::desktop::DesktopIdentity,
    current_owner: tokensmarket_core::desktop::DesktopIdentity,
    current_workspace: tokensmarket_core::desktop::DesktopIdentity,
}
#[tauri::command]
pub fn prepare_tool_reconnect(
    app: AppHandle,
    tool: String,
    expected_key_id: String,
) -> Result<ReconnectPreview, String> {
    let tool = tokensmarket_core::tool_id(&tool).map_err(|e| e.to_string())?;
    let (session, workspace) = current_context(&app)?;
    ToolInventory::current()
        .require_installed(tool)
        .map_err(|e| e.to_string())?;
    let record = app
        .state::<ConnectionManager>()
        .list()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|r| r.tool == tool && r.key_id == expected_key_id)
        .ok_or("连接已改变，请刷新后重试")?;
    Ok(ReconnectPreview {
        old_owner: record.owner,
        old_workspace: record.bound_workspace,
        current_owner: session.user,
        current_workspace: workspace,
    })
}
#[tauri::command]
pub async fn reconnect_tool(
    app: AppHandle,
    tool: String,
    expected_key_id: String,
    expected_owner_id: String,
    expected_workspace_id: String,
) -> Result<ReconnectOutcome, String> {
    let tool = tokensmarket_core::tool_id(&tool).map_err(|e| e.to_string())?;
    let (session, workspace) = current_context(&app)?;
    if session.user.id != expected_owner_id || workspace.id != expected_workspace_id {
        return Err("账号或当前空间已改变，请重新确认接入归属".into());
    }
    let inventory = ToolInventory::current();
    let path = tokensmarket_core::get_adapter(tool).config_path();
    app.state::<ConnectionManager>()
        .reconnect(
            ConnectContext {
                session_token: session.session_token,
                workspace_id: workspace.id,
            },
            tool,
            &expected_key_id,
            &inventory,
            path,
        )
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn connect_tool(app: AppHandle, tool: String) -> Result<ConnectionSummary, String> {
    let tool = tokensmarket_core::tool_id(&tool).map_err(|e| e.to_string())?;
    // Capture one operation context; later header changes never rebind this action.
    let (session, workspace) = current_context(&app)?;
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

/// Paid testing uses only the saved connection, independent of the desktop session.
#[tauri::command]
pub async fn test_tool_connection(
    app: AppHandle,
    tool: String,
    expected_key_id: String,
    request: ConnectionTestRequest,
) -> Result<ConnectionTestResult, String> {
    let tool = tokensmarket_core::tool_id(&tool).map_err(|e| e.to_string())?;
    app.state::<ConnectionManager>()
        .test_connection(tool, &expected_key_id, request)
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
        _ => return Err("此工具的启动入口尚未开放".into()),
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
    if id == ToolId::DeepSeekHarness {
        if let Some(path) = ToolInventory::current().harness_application_path() {
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
                _ => Err("Harness 启动失败，请从系统应用入口重新打开".into()),
            };
            #[cfg(windows)]
            return result
                .map(|_| ())
                .map_err(|_| "Harness 启动失败，请从系统应用入口重新打开".into());
        }
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
