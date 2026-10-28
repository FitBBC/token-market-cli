use serde::Serialize;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_opener::OpenerExt;

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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(SetupState::default())
        .invoke_handler(tauri::generate_handler![
            tool_status,
            start_setup,
            setup_status,
            reopen_authorize
        ])
        .run(tauri::generate_context!())
        .expect("error while building tauri application");
}
