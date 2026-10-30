use tauri::{AppHandle, Manager};
use tokenmarket_core::{
    connections::{
        ConnectContext, ConnectionManager, ConnectionSummary, ConnectionTestRequest,
        ConnectionTestResult, CredentialLoss, DisconnectOutcome, DisconnectRequest,
        ReconnectOutcome,
    },
    inventory::ToolInventory,
};

fn current_context(
    app: &AppHandle,
) -> Result<
    (
        tokenmarket_core::desktop::DesktopSession,
        tokenmarket_core::desktop::DesktopIdentity,
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
    let shared = app
        .state::<crate::DesktopState>()
        .login
        .shared_session()
        .map_err(|error| error.to_string())?;
    if shared.as_ref().map(|saved| &saved.session_token) != Some(&session.session_token) {
        return Err("共享登录已改变，请刷新后重新操作".into());
    }
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
#[derive(serde::Serialize, serde::Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryTarget {
    key_id: String,
    owner_id: String,
    workspace_id: String,
    #[serde(default)]
    owner_name: String,
    #[serde(default)]
    workspace_name: String,
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectFailure {
    message: String,
    recovery: Option<RecoveryTarget>,
}
impl From<String> for ConnectFailure {
    fn from(message: String) -> Self {
        Self {
            message,
            recovery: None,
        }
    }
}
fn connection_error(
    error: anyhow::Error,
    owner: &tokenmarket_core::desktop::DesktopIdentity,
    workspace: &tokenmarket_core::desktop::DesktopIdentity,
) -> ConnectFailure {
    ConnectFailure {
        message: error.to_string(),
        recovery: error
            .downcast_ref::<CredentialLoss>()
            .map(|lost| RecoveryTarget {
                key_id: lost.key_id.clone(),
                owner_id: owner.id.clone(),
                workspace_id: workspace.id.clone(),
                owner_name: owner.name.clone(),
                workspace_name: workspace.name.clone(),
            }),
    }
}
/// Only safe identities cross the UI boundary; receipts and credentials remain private.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconnectPreview {
    old_owner: tokenmarket_core::desktop::DesktopIdentity,
    old_workspace: tokenmarket_core::desktop::DesktopIdentity,
    current_owner: tokenmarket_core::desktop::DesktopIdentity,
    current_workspace: tokenmarket_core::desktop::DesktopIdentity,
}
#[tauri::command]
pub fn prepare_tool_reconnect(
    app: AppHandle,
    tool: String,
    expected_key_id: String,
) -> Result<ReconnectPreview, String> {
    let tool = tokenmarket_core::tool_id(&tool).map_err(|e| e.to_string())?;
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
    recovery: Option<RecoveryTarget>,
) -> Result<ReconnectOutcome, ConnectFailure> {
    let tool = tokenmarket_core::tool_id(&tool).map_err(|e| e.to_string())?;
    let (session, workspace) = current_context(&app)?;
    if session.user.id != expected_owner_id || workspace.id != expected_workspace_id {
        return Err("账号或当前空间已改变，请重新确认接入归属"
            .to_string()
            .into());
    }
    let inventory = ToolInventory::current();
    let path = inventory.config_path(tool);
    if let Some(target) = recovery {
        if target.owner_id != session.user.id || target.workspace_id != workspace.id {
            return Err("账号或空间已改变，请重新确认".to_string().into());
        }
        return app
            .state::<ConnectionManager>()
            .recover_connection(
                ConnectContext {
                    session_token: session.session_token,
                    owner_id: session.user.id.clone(),
                    workspace_id: workspace.id.clone(),
                },
                tool,
                Some(&expected_key_id),
                &inventory,
                path,
                &target.key_id,
            )
            .await
            .map_err(|error| connection_error(error, &session.user, &workspace));
    }
    let owner = session.user.clone();
    let space = workspace.clone();
    app.state::<ConnectionManager>()
        .reconnect(
            ConnectContext {
                session_token: session.session_token,
                owner_id: session.user.id,
                workspace_id: workspace.id,
            },
            tool,
            &expected_key_id,
            &inventory,
            path,
        )
        .await
        .map_err(|error| connection_error(error, &owner, &space))
}

#[tauri::command]
pub async fn connect_tool(
    app: AppHandle,
    tool: String,
    recovery: Option<RecoveryTarget>,
) -> Result<ConnectionSummary, ConnectFailure> {
    let tool = tokenmarket_core::tool_id(&tool).map_err(|e| e.to_string())?;
    // Capture one operation context; later header changes never rebind this action.
    let (session, workspace) = current_context(&app)?;
    let inventory = ToolInventory::current();
    inventory
        .require_installed(tool)
        .map_err(|e| e.to_string())?;
    let path = inventory.config_path(tool);
    if let Some(target) = recovery {
        if target.owner_id != session.user.id || target.workspace_id != workspace.id {
            return Err("账号或空间已改变，请重新确认".to_string().into());
        }
        return app
            .state::<ConnectionManager>()
            .recover_connection(
                ConnectContext {
                    session_token: session.session_token,
                    owner_id: session.user.id.clone(),
                    workspace_id: workspace.id.clone(),
                },
                tool,
                None,
                &inventory,
                path,
                &target.key_id,
            )
            .await
            .map(|result| result.connection)
            .map_err(|error| connection_error(error, &session.user, &workspace));
    }
    let owner = session.user.clone();
    let space = workspace.clone();
    app.state::<ConnectionManager>()
        .connect(
            ConnectContext {
                session_token: session.session_token,
                owner_id: session.user.id,
                workspace_id: workspace.id,
            },
            tool,
            &inventory,
            path,
        )
        .await
        .map_err(|error| connection_error(error, &owner, &space))
}

/// Paid testing uses only the saved connection, independent of the desktop session.
#[tauri::command]
pub async fn test_tool_connection(
    app: AppHandle,
    tool: String,
    expected_key_id: String,
    request: ConnectionTestRequest,
) -> Result<ConnectionTestResult, String> {
    let tool = tokenmarket_core::tool_id(&tool).map_err(|e| e.to_string())?;
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
    let tool = tokenmarket_core::tool_id(&tool).map_err(|e| e.to_string())?;
    // Local cancellation remains available after account changes or session expiration.
    app.state::<ConnectionManager>()
        .disconnect(tool, &expected_key_id, None, request)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn launch_connected_tool(app: AppHandle, tool: String) -> Result<(), String> {
    let id = tokenmarket_core::tool_id(&tool).map_err(|e| e.to_string())?;
    let inventory = ToolInventory::current();
    let plan = inventory.launch_plan(id).map_err(|e| e.to_string())?;
    if !app
        .state::<ConnectionManager>()
        .list()
        .map_err(|e| e.to_string())?
        .iter()
        .any(|r| r.tool == id)
    {
        return Err("请先完成接入".into());
    }
    let mut command = std::process::Command::new(&plan.program);
    command.args(&plan.args).envs(plan.environment);
    if plan.wait_for_exit {
        match command.status() {
            Ok(status) if status.success() => Ok(()),
            _ => Err("工具启动失败，请从系统应用入口或新终端打开".into()),
        }
    } else {
        command
            .spawn()
            .map(|_| ())
            .map_err(|_| "工具启动失败，请从系统应用入口或新终端打开".into())
    }
}
