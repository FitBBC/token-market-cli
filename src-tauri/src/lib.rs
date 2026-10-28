mod connections;
mod workspace;
use serde::Serialize;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_deep_link::DeepLinkExt;
use tauri_plugin_opener::OpenerExt;
use tokensmarket_core::desktop::{
    DesktopClient, DesktopLogin, DesktopSession, LoginAttempt, NativeDesktopCredentials,
};

use tokensmarket_core::setup::{run_setup, SetupProgress};

#[derive(Clone, Serialize, Default)]
struct SetupStatus {
    running: bool,
    authorize_url: Option<String>,
    error: Option<String>,
    outcome: Option<serde_json::Value>,
}

#[derive(Default)]
struct SetupState {
    status: Mutex<SetupStatus>,
}

// Read-only catalogue and installation gate shared with subsequent connection commands.
#[tauri::command]
fn desktop_tool_inventory() -> Result<Vec<tokensmarket_core::inventory::ToolInventoryEntry>, String> {
    tokensmarket_core::inventory::ToolInventory::current().list().map_err(|error| error.to_string())
}

#[tauri::command]
fn prepare_tool_connect(tool: String) -> Result<(), String> {
    let tool = tokensmarket_core::tool_id(&tool).map_err(|error| error.to_string())?;
    tokensmarket_core::inventory::ToolInventory::current().require_installed(tool).map_err(|error| error.to_string())
}

#[tauri::command]
fn download_tool(app: AppHandle, tool: String) -> Result<(), String> {
    let tool = tokensmarket_core::tool_id(&tool).map_err(|error| error.to_string())?;
    app.opener().open_url(tokensmarket_core::inventory::official_download_url(tool), None::<&str>)
        .map_err(|_| "无法打开官网下载页面，请稍后重试".into())
}

#[tauri::command]
fn tool_status(tool: String) -> serde_json::Value {
    match tokensmarket_core::adapters::tool_id(&tool) {
        Ok(id) => {
            let adapter = tokensmarket_core::adapters::get_adapter(id);
            serde_json::json!({
                "installed": adapter.is_installed(),
                "configPath": adapter.config_path().display().to_string(),
            })
        }
        Err(error) => serde_json::json!({
            "error": error.to_string(),
        }),
    }
}

#[tauri::command]
async fn start_setup(app: AppHandle, tool: String) -> Result<serde_json::Value, String> {
    set_status(&app, |status| {
        *status = SetupStatus {
            running: true,
            authorize_url: None,
            error: None,
            outcome: None,
        };
    });

    let opener_app = app.clone();
    let result = tauri::async_runtime::spawn(async move {
        run_setup(&tool, None, move |progress| {
            if let SetupProgress::AuthorizeStarted { authorize_url } = progress {
                if let Err(error) = opener_app
                    .opener()
                    .open_url(authorize_url.as_str(), None::<&str>)
                {
                    eprintln!("failed to open authorization browser: {error}");
                }
            }
        })
        .await
    })
    .await
    .map_err(|error| error.to_string())?;

    match result {
        Ok(outcome) => {
            set_status(&app, |status| {
                status.running = false;
                status.outcome = Some(serde_json::to_value(&outcome).unwrap_or_default());
            });
            let _ = app.emit("setup-status", status_value(&app));
            Ok(status_value(&app))
        }
        Err(error) => {
            set_status(&app, |status| {
                status.running = false;
                status.error = Some(error.to_string());
            });
            let _ = app.emit("setup-status", status_value(&app));
            Err(error.to_string())
        }
    }
}

#[tauri::command]
fn setup_status(app: AppHandle) -> serde_json::Value {
    status_value(&app)
}

#[tauri::command]
async fn reopen_authorize(app: AppHandle, url: String) -> Result<(), String> {
    app.opener()
        .open_url(url.as_str(), None::<&str>)
        .map_err(|error| error.to_string())
}

fn set_status(app: &AppHandle, update: impl FnOnce(&mut SetupStatus)) {
    let state = app.state::<SetupState>();
    let mut status = state.status.lock().expect("setup status lock");
    update(&mut status);
}

fn status_value(app: &AppHandle) -> serde_json::Value {
    let state = app.state::<SetupState>();
    let status = state.status.lock().expect("setup status lock").clone();
    serde_json::json!({
        "running": status.running,
        "authorizeUrl": status.authorize_url,
        "error": status.error,
        "outcome": status.outcome,
    })
}

// Desktop login state contains the management credential only on the Rust side.
struct DesktopState {
    login: DesktopLogin,
    session: Mutex<Option<DesktopSession>>,
    error: Mutex<Option<String>>,
    generation: Mutex<u64>,
    exchanging: AtomicBool,
    refreshing: AtomicBool,
}

fn desktop_value(app: &AppHandle) -> serde_json::Value {
    let state = app.state::<DesktopState>();
    let mut session = state.session.lock().expect("desktop session lock");
    if session.as_ref().is_some_and(|s| !s.is_valid()) {
        *session = None;
    }
    let pending = state.login.pending().ok().flatten();
    serde_json::json!({
        "pending": pending,
        "exchanging": state.exchanging.load(Ordering::SeqCst),
        "restoring": state.refreshing.load(Ordering::SeqCst),
        "context": session.as_ref().map(|s| serde_json::json!({
            "user": s.user,
            "default_workspace": s.default_workspace,
            "expires_at": s.expires_at,
        })),
        "error": state.error.lock().expect("desktop error lock").clone(),
    })
}

#[tauri::command]
fn desktop_login_status(app: AppHandle) -> serde_json::Value {
    desktop_value(&app)
}

#[tauri::command]
async fn refresh_desktop_session(app: AppHandle) -> serde_json::Value {
    let state = app.state::<DesktopState>();
    if state.refreshing.swap(true, Ordering::SeqCst) {
        return desktop_value(&app);
    }
    let generation = *state.generation.lock().expect("desktop generation lock");
    let result = state.login.restore().await;
    {
        let current = state.generation.lock().expect("desktop generation lock");
        if *current == generation {
            match result {
                Ok(session) => {
                    if session.is_none() {
                        app.state::<workspace::WorkspaceState>().clear();
                    }
                    *state.session.lock().expect("desktop session lock") = session;
                    *state.error.lock().expect("desktop error lock") = None;
                }
                Err(error) => {
                    *state.session.lock().expect("desktop session lock") = None;
                    app.state::<workspace::WorkspaceState>().clear();
                    *state.error.lock().expect("desktop error lock") = Some(error.to_string());
                }
            }
        }
        state.refreshing.store(false, Ordering::SeqCst);
    }
    let value = desktop_value(&app);
    let _ = app.emit("desktop-login-status", &value);
    value
}

#[tauri::command]
async fn logout_desktop_session(app: AppHandle) -> Result<serde_json::Value, String> {
    let state = app.state::<DesktopState>();
    *state.generation.lock().expect("desktop generation lock") += 1;
    *state.session.lock().expect("desktop session lock") = None;
    app.state::<workspace::WorkspaceState>().clear();
    state.exchanging.store(false, Ordering::SeqCst);
    let result = state.login.logout().await;
    let (remote_revoked, warning) = match result {
        Ok(true) => (true, None),
        Ok(false) => (
            false,
            Some("已退出本机登录；服务端会话撤销未确认。已有 Agent 配置保留。".to_string()),
        ),
        Err(error) => {
            *state.error.lock().expect("desktop error lock") = Some(error.to_string());
            let _ = app.emit("desktop-login-status", desktop_value(&app));
            return Err(error.to_string());
        }
    };
    *state.error.lock().expect("desktop error lock") = warning.clone();
    let _ = app.emit("desktop-login-status", desktop_value(&app));
    Ok(serde_json::json!({"remote_revoked":remote_revoked,"warning":warning}))
}

#[tauri::command]
async fn begin_desktop_login(app: AppHandle) -> Result<LoginAttempt, String> {
    let state = app.state::<DesktopState>();
    *state.generation.lock().expect("desktop generation lock") += 1;
    *state.error.lock().expect("desktop error lock") = None;
    let attempt = state.login.begin().await.map_err(|e| e.to_string())?;
    // Browser failure leaves a copyable login link available.
    let _ = app.opener().open_url(&attempt.authorize_url, None::<&str>);
    let _ = app.emit("desktop-login-status", desktop_value(&app));
    Ok(attempt)
}

#[tauri::command]
fn cancel_desktop_login(app: AppHandle) -> Result<(), String> {
    let state = app.state::<DesktopState>();
    {
        let mut generation = state.generation.lock().expect("desktop generation lock");
        *generation += 1;
        state.login.cancel().map_err(|e| e.to_string())?;
        state.exchanging.store(false, Ordering::SeqCst);
    }
    *state.error.lock().expect("desktop error lock") = None;
    let _ = app.emit("desktop-login-status", desktop_value(&app));
    Ok(())
}

#[tauri::command]
fn reopen_desktop_login(app: AppHandle) -> Result<(), String> {
    let pending = app
        .state::<DesktopState>()
        .login
        .pending()
        .map_err(|e| e.to_string())?
        .ok_or("没有等待中的登录请求")?;
    app.opener()
        .open_url(pending.authorize_url, None::<&str>)
        .map_err(|_| "无法打开浏览器，请复制登录链接".into())
}

fn accept_desktop_callback(app: AppHandle, callback: String) {
    tauri::async_runtime::spawn(async move {
        let state = app.state::<DesktopState>();
        if state.login.pending().ok().flatten().is_none() {
            return;
        }
        let generation = {
            let mut generation = state.generation.lock().expect("desktop generation lock");
            if state.exchanging.swap(true, Ordering::SeqCst) {
                return;
            }
            *generation += 1;
            *generation
        };
        let result = state.login.complete(&callback).await;
        {
            let current = state.generation.lock().expect("desktop generation lock");
            if *current != generation {
                if let Ok(session) = result {
                    // complete() may have saved just before the user cancelled the native command.
                    if let Err(error) = state.login.discard_unaccepted_session(&session) {
                        *state.error.lock().expect("desktop error lock") = Some(error.to_string());
                    }
                }
                return;
            }
            state.exchanging.store(false, Ordering::SeqCst);
            match result {
                Ok(session) => {
                    *state.session.lock().expect("desktop session lock") = Some(session);
                    *state.error.lock().expect("desktop error lock") = None;
                }
                Err(error) => {
                    *state.error.lock().expect("desktop error lock") = Some(error.to_string())
                }
            }
        }
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.unminimize();
            let _ = window.show();
            let _ = window.set_focus();
        }
        let _ = app.emit("desktop-login-status", desktop_value(&app));
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_opener::init())
        .manage(SetupState::default())
        .manage(tokensmarket_core::connections::ConnectionManager::current())
        .manage(workspace::WorkspaceState::new())
        .setup(|app| {
            let api_base = tokensmarket_core::config::console_api_base_url();
            app.manage(DesktopState {
                login: DesktopLogin::with_credentials(
                    app.path().app_local_data_dir()?.join("desktop-login"),
                    DesktopClient::new(api_base.clone()),
                    Arc::new(NativeDesktopCredentials::new(&api_base)),
                ),
                session: Mutex::new(None),
                error: Mutex::new(None),
                generation: Mutex::new(0),
                exchanging: AtomicBool::new(false),
                refreshing: AtomicBool::new(false),
            });
            let restore_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                refresh_desktop_session(restore_handle).await;
            });
            let handle = app.handle().clone();
            app.deep_link().on_open_url(move |event| {
                for url in event.urls() {
                    accept_desktop_callback(handle.clone(), url.to_string());
                }
            });
            if let Some(urls) = app.deep_link().get_current()? {
                for url in urls {
                    accept_desktop_callback(app.handle().clone(), url.to_string());
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            workspace::desktop_workspace_context,
            workspace::switch_desktop_workspace,
            workspace::open_desktop_account_page,
            tool_status,
            connections::connect_tool,
            connections::launch_connected_tool,
            desktop_tool_inventory,
            prepare_tool_connect,
            download_tool,
            start_setup,
            setup_status,
            reopen_authorize,
            desktop_login_status,
            refresh_desktop_session,
            logout_desktop_session,
            begin_desktop_login,
            cancel_desktop_login,
            reopen_desktop_login,
        ])
        .run(tauri::generate_context!())
        .expect("error while building tauri application");
}
