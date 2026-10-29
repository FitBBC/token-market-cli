use crate::DesktopState;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};
use tauri_plugin_opener::OpenerExt;
use tokenmarket_core::desktop::{DesktopIdentity, DesktopSession};
use tokenmarket_core::desktop_workspace::{
    menu_allows, DesktopWorkspace, DesktopWorkspaceController, WorkspaceSnapshot,
};

pub struct WorkspaceState {
    owner: Mutex<Option<String>>,
    controller: DesktopWorkspaceController,
}
impl WorkspaceState {
    pub fn new() -> Self {
        Self {
            owner: Mutex::new(None),
            controller: DesktopWorkspaceController::new(DesktopWorkspace::new(
                tokenmarket_core::config::console_api_base_url(),
                String::new(),
            )),
        }
    }
    pub fn clear(&self) {
        self.controller.clear();
        *self.owner.lock().expect("workspace owner") = None;
    }
}
fn session(app: &AppHandle) -> Result<DesktopSession, String> {
    app.state::<DesktopState>()
        .session
        .lock()
        .expect("desktop session lock")
        .clone()
        .ok_or("请先登录".into())
}
/// Current Workspace captured by subsequent Connect; no connection binding is modified here.
pub fn current_context(app: &AppHandle) -> Result<DesktopIdentity, String> {
    let session = session(app)?;
    let state = app.state::<WorkspaceState>();
    if state.owner.lock().expect("workspace owner").as_deref() != Some(&session.session_token) {
        return Err("请先刷新当前空间".into());
    }
    let snapshot = state.controller.current().ok_or("请先刷新当前空间")?;
    if snapshot.owner_user_id != session.user.id
        || snapshot.current_workspace.id != session.default_workspace.id
    {
        return Err("当前空间已改变，请刷新后重试".into());
    }
    Ok(DesktopIdentity {
        id: snapshot.current_workspace.id,
        name: snapshot.current_workspace.name,
    })
}
async fn load(
    app: &AppHandle,
    target: Option<String>,
) -> Result<Option<WorkspaceSnapshot>, String> {
    let session = session(app)?;
    let state = app.state::<WorkspaceState>();
    {
        let mut owner = state.owner.lock().expect("workspace owner");
        if owner.as_deref() != Some(&session.session_token) {
            state.controller.clear();
            *owner = Some(session.session_token.clone());
        }
    }
    let result = if let Some(target) = target {
        state.controller.switch(&session, &target).await
    } else {
        state.controller.refresh(&session).await
    }
    .map_err(|e| e.to_string())?;
    let desktop = app.state::<DesktopState>();
    let mut current = desktop.session.lock().expect("desktop session lock");
    if current.as_ref().map(|s| &s.session_token) != Some(&session.session_token) {
        return Ok(None);
    }
    if let (Some(snapshot), Some(active)) = (&result, current.as_mut()) {
        active.default_workspace = DesktopIdentity {
            id: snapshot.current_workspace.id.clone(),
            name: snapshot.current_workspace.name.clone(),
        };
    }
    Ok(result)
}
#[tauri::command]
pub async fn desktop_workspace_context(
    app: AppHandle,
) -> Result<Option<WorkspaceSnapshot>, String> {
    load(&app, None).await
}
#[tauri::command]
pub async fn switch_desktop_workspace(
    app: AppHandle,
    workspace_id: String,
) -> Result<Option<WorkspaceSnapshot>, String> {
    load(&app, Some(workspace_id)).await
}
#[tauri::command]
pub async fn open_desktop_account_page(app: AppHandle, action: String) -> Result<(), String> {
    current_context(&app)?;
    let session = session(&app)?;
    let state = app.state::<WorkspaceState>();
    if state.owner.lock().expect("workspace owner").as_deref() != Some(&session.session_token) {
        return Err("请刷新空间上下文".into());
    }
    let snapshot = state.controller.current().ok_or("请刷新空间上下文")?;
    if snapshot.owner_user_id != session.user.id {
        return Err("请刷新空间上下文".into());
    }
    if (action == "recharge" && !snapshot.can_recharge)
        || (action != "recharge" && !menu_allows(&snapshot.menus, &action))
    {
        return Err("没有该菜单的访问权限".into());
    }
    let origin = std::env::var("TOKEN_MARKET_CONSOLE_WEB_BASE_URL")
        .ok()
        .or(session.web_origin)
        .ok_or("服务端缺少浏览器站点地址，请重新登录或联系管理员")?;
    let client = DesktopWorkspace::new(String::new(), origin);
    let url = client
        .browser_url(&session.user.id, &action)
        .map_err(|e| e.to_string())?;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|_| "无法打开浏览器，请重试".into())
}
