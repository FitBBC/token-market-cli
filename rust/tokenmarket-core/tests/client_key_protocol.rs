//! Exercise the new HTTP contract directly, independently of legacy adapter fixtures.
mod support;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tempfile::tempdir;
use tokenmarket_core::{
    adapters::ToolId,
    connections::{ConnectContext, ConnectionManager, CredentialLoss, DisconnectRequest},
    inventory::ToolInventory,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Default)]
struct State {
    id: u64,
    secret: String,
    request: String,
    generation: u64,
    stage: String,
    status: String,
    issues: usize,
    rotations: usize,
    retired: usize,
    predecessors: usize,
    lose_issue: bool,
    lose_confirm: bool,
    fail_catalogue: bool,
    fail_inspect: bool,
    inspect_business_failure: bool,
    block_cache: Option<PathBuf>,
    logout_on_catalogue: Option<Arc<support::Credentials>>,
}
struct Platform {
    url: String,
    state: Arc<Mutex<State>>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Platform {
    fn drop(&mut self) {
        self.task.abort();
    }
}
fn hash(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
impl Platform {
    async fn start() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let state = Arc::new(Mutex::new(State::default()));
        let shared = state.clone();
        let task = tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut bytes = Vec::new();
                let end = loop {
                    let mut chunk = [0; 8192];
                    let n = socket.read(&mut chunk).await.unwrap();
                    if n == 0 {
                        break None;
                    }
                    bytes.extend_from_slice(&chunk[..n]);
                    if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                        let header = String::from_utf8_lossy(&bytes[..end]);
                        let length = header
                            .lines()
                            .find_map(|line| {
                                line.to_ascii_lowercase()
                                    .strip_prefix("content-length:")
                                    .map(|s| s.trim().parse::<usize>().unwrap())
                            })
                            .unwrap_or(0);
                        if bytes.len() >= end + 4 + length {
                            break Some(end);
                        }
                    }
                };
                let Some(end) = end else { continue };
                let header = String::from_utf8_lossy(&bytes[..end]);
                let route = header
                    .lines()
                    .next()
                    .unwrap()
                    .split_whitespace()
                    .nth(1)
                    .unwrap();
                let body: Value = serde_json::from_slice(&bytes[end + 4..]).unwrap_or(Value::Null);
                let response = {
                    let mut s = shared.lock().unwrap();
                    let mut failure = false;
                    let mut drop_response = false;
                    let data = if route == "/v1/models" {
                        if let Some(credentials) = s.logout_on_catalogue.take() {
                            *credentials.0.lock().unwrap() = None;
                        }
                        failure = s.fail_catalogue;
                        json!({"data":[{"id":"glm-5.3","model_types":["TEXT_GENERATION"]},{"id":"glm-5.2","model_types":["TEXT_GENERATION"]}]})
                    } else {
                        assert!(route.starts_with("/api/client/keys/"));
                        assert!(header.contains("Bearer management-fixture"));
                        let action = route.rsplit('/').next().unwrap();
                        let fingerprint = body["fingerprint"].as_str();
                        let valid = fingerprint == Some(hash(&s.secret).as_str());
                        let mut status = match action {
                            "inspect" => {
                                failure = s.fail_inspect;
                                if s.id == 0 {
                                    "NOT_FOUND".to_string()
                                } else if s.status != "ACTIVE" {
                                    s.status.clone()
                                } else if fingerprint.is_none() {
                                    if s.stage == "APPLIED" {
                                        "LOCAL_COPY_REQUIRED".into()
                                    } else {
                                        s.stage.clone()
                                    }
                                } else if !valid {
                                    "SECRET_CHANGED".into()
                                } else if s.stage == "APPLIED" {
                                    "VALID".into()
                                } else {
                                    s.stage.clone()
                                }
                            }
                            "issue" => {
                                if s.id > 0 && s.status == "ACTIVE" && s.stage == "PENDING" {
                                    "PENDING".into()
                                } else if s.id > 0
                                    && s.status == "ACTIVE"
                                    && !body["replace_lost"].as_bool().unwrap_or(false)
                                    && (valid || fingerprint.is_none())
                                {
                                    "LOCAL_COPY_REQUIRED".into()
                                } else {
                                    if body["replace_lost"] == true {
                                        assert_eq!(body["key_id"], s.id.to_string());
                                        s.predecessors += 1;
                                    }
                                    s.issues += 1;
                                    s.id += 1;
                                    s.generation += 1;
                                    s.secret = format!("fixture-secret-{}-{}", s.id, s.generation);
                                    s.request = body["request_id"].as_str().unwrap().into();
                                    s.status = "ACTIVE".into();
                                    s.stage = "PENDING".into();
                                    drop_response = s.lose_issue;
                                    s.lose_issue = false;
                                    if let Some(path) = s.block_cache.take() {
                                        std::fs::create_dir(path).unwrap();
                                    }
                                    "ISSUED".into()
                                }
                            }
                            "recover" => {
                                assert_eq!(body["generation"], s.generation);
                                assert_eq!(body["request_id"], s.request);
                                s.rotations += 1;
                                s.generation += 1;
                                s.secret = format!("recovered-{}-{}", s.id, s.generation);
                                "ISSUED".into()
                            }
                            "confirm" | "applied" => {
                                assert!(valid);
                                assert_eq!(body["generation"], s.generation);
                                assert_eq!(body["request_id"], s.request);
                                if action == "applied" {
                                    s.stage = "APPLIED".into();
                                    s.retired += s.predecessors;
                                    s.predecessors = 0;
                                } else if s.stage != "APPLIED" {
                                    s.stage = "SAVED".into();
                                    failure = s.lose_confirm;
                                    s.lose_confirm = false;
                                }
                                s.stage.clone()
                            }
                            _ => panic!("unexpected protocol action"),
                        };
                        if failure {
                            status = "UNAVAILABLE".into();
                        }
                        let mut reply = json!({"status":status,"key_id":if s.id==0 {None}else{Some(s.id.to_string())},"request_id":s.request,"generation":s.generation,"owner":{"id":"u1","name":"Fixture"},"bound_workspace":{"id":"42","name":"Scope"}});
                        if status == "ISSUED" {
                            reply["api_key"] = json!(s.secret);
                        }
                        if action == "inspect" && s.inspect_business_failure {
                            json!({"success":false,"message":"connection operation unavailable; retry without changing credentials"})
                        } else {
                            json!({"success":!failure,"data":reply})
                        }
                    };
                    if drop_response {
                        None
                    } else {
                        Some((
                            if failure {
                                "503 Service Unavailable"
                            } else {
                                "200 OK"
                            },
                            data.to_string(),
                        ))
                    }
                };
                if let Some((status, body)) = response {
                    socket.write_all(format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
                }
            }
        });
        Self { url, state, task }
    }
    fn manager(&self, root: &Path) -> ConnectionManager {
        let directory = root.join("state");
        ConnectionManager::with_credentials(
            directory.clone(),
            self.url.clone(),
            self.url.clone(),
            support::credentials(&directory),
        )
    }
}
fn context() -> ConnectContext {
    ConnectContext {
        session_token: "management-fixture".into(),
        owner_id: "u1".into(),
        workspace_id: "42".into(),
    }
}
fn inventory(root: &Path) -> ToolInventory {
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let executable = bin.join("claude");
    std::fs::write(&executable, "fixture").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    ToolInventory::new(tokenmarket_core::inventory::InstallationEnvironment {
        platform: tokenmarket_core::inventory::HostPlatform::MacOs,
        home: root.into(),
        command_directories: vec![bin],
        application_directories: vec![],
        executable_extensions: vec![],
    })
}

#[tokio::test]
async fn server_inspection_failure_does_not_misdiagnose_login_or_issue_a_key() {
    let root = tempdir().unwrap();
    let api = Platform::start().await;
    api.state.lock().unwrap().inspect_business_failure = true;
    let path = root.path().join("settings.json");
    let error = api
        .manager(root.path())
        .connect(
            context(),
            ToolId::ClaudeCode,
            &inventory(root.path()),
            path.clone(),
        )
        .await
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("连接服务未能完成凭证校验"),
        "unexpected error: {error}"
    );
    assert!(!error.contains("刷新登录"));
    assert_eq!(api.state.lock().unwrap().issues, 0);
    assert!(!path.exists());
}

#[tokio::test]
async fn lost_issue_response_rotates_only_pending_row_after_restart() {
    let root = tempdir().unwrap();
    let api = Platform::start().await;
    api.state.lock().unwrap().lose_issue = true;
    let path = root.path().join("settings.json");
    let inv = inventory(root.path());
    assert!(api
        .manager(root.path())
        .connect(context(), ToolId::ClaudeCode, &inv, path.clone())
        .await
        .is_err());
    assert!(!path.exists());
    api.manager(root.path())
        .connect(context(), ToolId::ClaudeCode, &inv, path.clone())
        .await
        .unwrap();
    let s = api.state.lock().unwrap();
    assert_eq!(s.issues, 1);
    assert_eq!(s.rotations, 1);
    assert_eq!(s.stage, "APPLIED");
    assert!(std::fs::read_to_string(path).unwrap().contains(&s.secret));
}
#[tokio::test]
async fn lost_confirmation_and_catalogue_failure_reuse_saved_secret() {
    for confirmation in [true, false] {
        let root = tempdir().unwrap();
        let api = Platform::start().await;
        {
            let mut s = api.state.lock().unwrap();
            s.lose_confirm = confirmation;
            s.fail_catalogue = !confirmation;
        }
        let manager = api.manager(root.path());
        let path = root.path().join("settings.json");
        let inv = inventory(root.path());
        assert!(manager
            .connect(context(), ToolId::ClaudeCode, &inv, path.clone())
            .await
            .is_err());
        assert!(!path.exists());
        api.state.lock().unwrap().fail_catalogue = false;
        manager
            .connect(context(), ToolId::ClaudeCode, &inv, path)
            .await
            .unwrap();
        let s = api.state.lock().unwrap();
        assert_eq!(s.issues, 1);
        assert_eq!(s.rotations, 0);
        assert_eq!(s.stage, "APPLIED");
    }
}
#[tokio::test]
async fn cache_write_failure_never_confirms_and_recovers_same_pending_row() {
    let root = tempdir().unwrap();
    let api = Platform::start().await;
    let manager = api.manager(root.path());
    let cache = root.path().join("state/keys.json");
    api.state.lock().unwrap().block_cache = Some(cache.clone());
    let path = root.path().join("settings.json");
    let inv = inventory(root.path());
    assert!(manager
        .connect(context(), ToolId::ClaudeCode, &inv, path.clone())
        .await
        .is_err());
    assert!(!path.exists());
    assert_eq!(api.state.lock().unwrap().stage, "PENDING");
    std::fs::remove_dir(cache).unwrap();
    manager
        .connect(context(), ToolId::ClaudeCode, &inv, path)
        .await
        .unwrap();
    let s = api.state.lock().unwrap();
    assert_eq!(s.issues, 1);
    assert_eq!(s.rotations, 1);
}
#[tokio::test]
async fn disabled_expired_or_network_errors_preserve_existing_connection() {
    let root = tempdir().unwrap();
    let api = Platform::start().await;
    let manager = api.manager(root.path());
    let inv = inventory(root.path());
    let path = root.path().join("settings.json");
    manager
        .connect(context(), ToolId::ClaudeCode, &inv, path.clone())
        .await
        .unwrap();
    let before = std::fs::read(&path).unwrap();
    for status in ["DISABLED", "EXPIRED", "ACTIVE"] {
        {
            let mut s = api.state.lock().unwrap();
            s.status = status.into();
            s.fail_inspect = status == "ACTIVE";
        }
        assert!(manager
            .reconnect(context(), ToolId::ClaudeCode, "1", &inv, path.clone())
            .await
            .is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(api.state.lock().unwrap().issues, 1);
    }
}
#[tokio::test]
async fn confirmed_server_deletion_or_reset_creates_replacement_without_retiring_user_key() {
    for deleted in [true, false] {
        let root = tempdir().unwrap();
        let api = Platform::start().await;
        let manager = api.manager(root.path());
        let inv = inventory(root.path());
        let path = root.path().join("settings.json");
        manager
            .connect(context(), ToolId::ClaudeCode, &inv, path.clone())
            .await
            .unwrap();
        {
            let mut s = api.state.lock().unwrap();
            if deleted {
                s.status = "DELETED".into()
            } else {
                s.secret = "owner-reset".into()
            }
        }
        manager
            .reconnect(context(), ToolId::ClaudeCode, "1", &inv, path)
            .await
            .unwrap();
        let s = api.state.lock().unwrap();
        assert_eq!(s.issues, 2);
        assert_eq!(s.retired, 0);
    }
}
#[tokio::test]
async fn missing_local_copy_requires_scoped_confirmation_and_retires_only_after_config_success() {
    let root = tempdir().unwrap();
    let api = Platform::start().await;
    let manager = api.manager(root.path());
    let inv = inventory(root.path());
    let path = root.path().join("settings.json");
    manager
        .connect(context(), ToolId::ClaudeCode, &inv, path.clone())
        .await
        .unwrap();
    manager
        .disconnect(ToolId::ClaudeCode, "1", None, DisconnectRequest::default())
        .await
        .unwrap();
    std::fs::remove_file(root.path().join("state/keys.json")).unwrap();
    let error = manager
        .connect(context(), ToolId::ClaudeCode, &inv, path.clone())
        .await
        .unwrap_err();
    assert_eq!(error.downcast_ref::<CredentialLoss>().unwrap().key_id, "1");
    assert_eq!(api.state.lock().unwrap().issues, 1);
    api.state.lock().unwrap().fail_catalogue = true;
    assert!(manager
        .recover_connection(context(), ToolId::ClaudeCode, None, &inv, path.clone(), "1")
        .await
        .is_err());
    {
        let s = api.state.lock().unwrap();
        assert_eq!(s.issues, 2);
        assert_eq!(s.retired, 0);
    }
    api.state.lock().unwrap().fail_catalogue = false;
    manager
        .connect(context(), ToolId::ClaudeCode, &inv, path)
        .await
        .unwrap();
    assert_eq!(api.state.lock().unwrap().retired, 1);
}

#[tokio::test]
async fn logout_from_other_entry_during_catalogue_load_prevents_configuration_commit() {
    let root = tempdir().unwrap();
    let api = Platform::start().await;
    let credentials = Arc::new(support::Credentials::default());
    *credentials.0.lock().unwrap()=Some(json!({"session_token":"management-fixture","expires_at":"2999-01-01T00:00:00Z","user":{"id":"u1","name":"Fixture"},"default_workspace":{"id":"42","name":"Scope"}}).to_string());
    let login = tokenmarket_core::desktop::DesktopLogin::with_credentials(
        root.path().join("login"),
        tokenmarket_core::desktop::DesktopClient::new(api.url.clone()),
        credentials.clone(),
    );
    let manager = api.manager(root.path()).with_shared_login(login);
    let inv = inventory(root.path());
    let path = root.path().join("settings.json");
    api.state.lock().unwrap().logout_on_catalogue = Some(credentials);
    assert!(manager
        .connect(context(), ToolId::ClaudeCode, &inv, path.clone())
        .await
        .is_err());
    assert!(!path.exists());
    assert!(manager.list().unwrap().is_empty());
    let s = api.state.lock().unwrap();
    assert_eq!(s.issues, 1);
    assert_eq!(s.stage, "SAVED");
    assert_eq!(s.retired, 0);
}
