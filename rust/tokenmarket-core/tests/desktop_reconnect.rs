mod support;
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tempfile::tempdir;
use tokenmarket_core::{
    connections::{ConnectContext, DisconnectRequest},
    inventory::{HostPlatform, InstallationEnvironment, ToolInventory},
    ToolId,
};

struct Platform {
    url: String,
    requests: Arc<Mutex<Vec<String>>>,
    reject_first: Arc<AtomicBool>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Platform {
    fn drop(&mut self) {
        self.task.abort();
    }
}
async fn platform(config: std::path::PathBuf, deny_old: bool) -> Platform {
    platform_with_failure(config, deny_old, None, None).await
}
async fn platform_with_failure(
    config: std::path::PathBuf,
    deny_old: bool,
    failure: Option<&'static str>,
    read_only: Option<std::path::PathBuf>,
) -> Platform {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let requests = Arc::new(Mutex::new(vec![]));
    let captured = requests.clone();
    let reject_first = Arc::new(AtomicBool::new(false));
    let rejected = reject_first.clone();
    let task = tokio::spawn(async move {
        let mut issued = 0;
        let mut catalogues = 0;
        let mut harness = false;
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = vec![];
            loop {
                let mut chunk = [0; 4096];
                let n = socket.read(&mut chunk).await.unwrap();
                if n == 0 {
                    break;
                }
                bytes.extend_from_slice(&chunk[..n]);
                if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    let h = String::from_utf8_lossy(&bytes[..end]);
                    let length: usize = h
                        .lines()
                        .find_map(|l| {
                            l.to_lowercase()
                                .strip_prefix("content-length:")
                                .map(|v| v.trim().parse().unwrap())
                        })
                        .unwrap_or(0);
                    if bytes.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            let request = String::from_utf8(bytes).unwrap();
            let first = request.lines().next().unwrap();
            let body = if first.starts_with("GET /v1/models ") {
                catalogues += 1;
                if request.contains("Bearer sk-key-2") {
                    #[cfg(unix)]
                    if let Some(path) = &read_only {
                        use std::os::unix::fs::PermissionsExt;
                        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o500))
                            .unwrap();
                    }
                }
                if failure == Some("catalogue") && catalogues > 1 {
                    json!({"data":[]})
                } else if failure == Some("protocol") && catalogues > 1 {
                    json!({"data":[{"id":"glm-5.3","model_types":["IMAGE_GENERATION"]}]})
                } else if harness {
                    if catalogues == 1 {
                        json!({"data":[{"id":"zeta","model_types":["TEXT_GENERATION"]},{"id":"excluded","model_types":["IMAGE_GENERATION"]},{"id":"alpha","model_types":["TEXT_GENERATION"]}]})
                    } else {
                        json!({"data":[{"id":"beta","model_types":["TEXT_GENERATION"]},{"id":"gamma","model_types":["TEXT_GENERATION"]}]})
                    }
                } else if catalogues == 1 {
                    json!({"data":[{"id":"glm-5.3","model_types":["TEXT_GENERATION"],"context_window_tokens":202752}]})
                } else {
                    json!({"data":[{"id":"glm-5.3","model_types":["TEXT_GENERATION"]}]})
                }
            } else if first.starts_with("POST /v1/") {
                json!({"type":"message","role":"assistant","stop_reason":"end_turn","content":[{"type":"text","text":"OK"}]})
            } else if first.contains("/disable ") {
                // Retiring old credentials is legal only after replacement files are visible.
                if first.contains("/1001/") {
                    let credentials = if harness {
                        config.with_file_name(".credentials.yaml")
                    } else {
                        config.clone()
                    };
                    assert!(std::fs::read_to_string(credentials)
                        .unwrap()
                        .contains(&format!("sk-key-{issued}")));
                }
                json!({"success":!deny_old,"data":!deny_old})
            } else {
                issued += 1;
                harness = request.contains("\"tool\":\"deepseek-harness\"");
                let (id, name) = if request.contains("Bearer account-b") {
                    ("b", "Bob")
                } else {
                    ("a", "Ada")
                };
                let payload: Value =
                    serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
                let space = payload["workspace_id"].as_str().unwrap();
                json!({"success":!(failure == Some("key") && issued==2),"data":{"api_key":format!("sk-key-{issued}"),"key_id":format!("{}",1000+issued),"owner":{"id":id,"name":name},"bound_workspace":{"id":space,"name":space}}})
            };
            let status = if rejected.load(Ordering::SeqCst) && request.contains("Bearer sk-key-1") {
                "401 Unauthorized"
            } else {
                "200 OK"
            };
            captured.lock().unwrap().push(request);
            let body = body.to_string();
            socket
                .write_all(
                    format!(
                        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
        }
    });
    Platform {
        url,
        requests,
        reject_first,
        task,
    }
}
fn inventory(root: &std::path::Path) -> ToolInventory {
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    for name in [
        "claude",
        "codex",
        "dsh",
        "workbuddy",
        "openclaw",
        "opencode",
    ] {
        let exe = bin.join(name);
        std::fs::write(&exe, "fixture").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(exe, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
    }
    ToolInventory::new(InstallationEnvironment {
        platform: HostPlatform::MacOs,
        home: root.into(),
        command_directories: vec![bin],
        application_directories: vec![],
        executable_extensions: vec![],
    })
}
fn context(account: &str) -> ConnectContext {
    ConnectContext {
        session_token: format!("account-{account}"),
        owner_id: account.into(),
        workspace_id: format!("space-{account}"),
    }
}
#[tokio::test]
async fn explicit_new_key_uses_current_account_keeps_old_key_and_restores_first_baseline() {
    let root = tempdir().unwrap();
    let inv = inventory(root.path());
    let path = root.path().join("settings.json");
    let original = json!({"model":"personal","env":{"ANTHROPIC_API_KEY":null,"ANTHROPIC_MODEL":"personal-override"},"permissions":{"allow":["Read"]}});
    std::fs::write(&path, original.to_string()).unwrap();
    let server = platform(path.clone(), false).await;
    let manager = support::manager(
        root.path().join("records"),
        server.url.clone(),
        server.url.clone(),
    );
    manager
        .connect(context("a"), ToolId::ClaudeCode, &inv, path.clone())
        .await
        .unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&std::fs::read(&path).unwrap()).unwrap()["env"]
            ["ANTHROPIC_MODEL"],
        "glm-5.3"
    );
    let replaced = manager
        .reconnect(context("b"), ToolId::ClaudeCode, "1001", &inv, path.clone())
        .await
        .unwrap();
    assert!(!replaced.key_reused);
    assert_eq!(replaced.connection.owner["id"], "b");
    assert_eq!(replaced.connection.bound_workspace["id"], "space-b");
    assert!(!serde_json::to_string(&replaced).unwrap().contains("sk-key"));
    assert_eq!(manager.list().unwrap().len(), 1);
    let requests = server.requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 4);
    assert!(!requests.iter().any(|r| r.contains("/disable ")));
    assert!(!requests
        .iter()
        .any(|r| r.contains("chat/completions") || r.contains("/messages")));
    manager
        .disconnect(
            ToolId::ClaudeCode,
            "1002",
            None,
            DisconnectRequest::default(),
        )
        .await
        .unwrap();
    let restored: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(restored, original);
}

#[tokio::test]
async fn claude_reconnect_fills_model_aliases_missing_from_old_receipts_and_restores_their_baseline(
) {
    let root = tempdir().unwrap();
    let inv = inventory(root.path());
    let path = root.path().join("settings.json");
    let added = [
        "ANTHROPIC_DEFAULT_FABLE_MODEL",
        "ANTHROPIC_DEFAULT_FABLE_MODEL_NAME",
        "ANTHROPIC_DEFAULT_HAIKU_MODEL_NAME",
        "ANTHROPIC_DEFAULT_OPUS_MODEL_NAME",
        "ANTHROPIC_DEFAULT_SONNET_MODEL_NAME",
        "CLAUDE_CODE_AUTO_MODE_SERVER",
    ];
    let mut original = json!({"env":{"OTHER":"keep"},"permissions":{"allow":["Read"]}});
    for name in added {
        original["env"][name] = json!(format!("original-{name}"));
    }
    std::fs::write(&path, original.to_string()).unwrap();
    let server = platform(path.clone(), false).await;
    let directory = root.path().join("records");
    let manager = support::manager(directory.clone(), server.url.clone(), server.url.clone());
    manager
        .connect(context("a"), ToolId::ClaudeCode, &inv, path.clone())
        .await
        .unwrap();
    // Simulate a connection created by the previous release: these fields were never owned.
    let store = directory.join("connections.json");
    let mut records: Value = serde_json::from_slice(&support::read_private(&store)).unwrap();
    records[0]["receipts"][0]["fields"]
        .as_array_mut()
        .unwrap()
        .retain(|field| {
            !added
                .iter()
                .any(|name| field["pointer"] == format!("/env/{name}"))
        });
    std::fs::write(&store, records.to_string()).unwrap();
    let mut current: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    for name in added {
        current["env"][name] = original["env"][name].clone();
    }
    std::fs::write(&path, current.to_string()).unwrap();
    manager
        .reconnect(context("a"), ToolId::ClaudeCode, "1001", &inv, path.clone())
        .await
        .unwrap();
    let applied: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    for name in added {
        let expected = if name == "CLAUDE_CODE_AUTO_MODE_SERVER" {
            "0"
        } else {
            "glm-5.3"
        };
        assert_eq!(applied["env"][name], expected, "{name}");
        let record = manager.list().unwrap().remove(0);
        let field = record.receipts[0]
            .fields
            .iter()
            .find(|field| field.pointer == format!("/env/{name}"))
            .unwrap();
        assert_eq!(field.before, Some(original["env"][name].clone()));
    }
    manager
        .disconnect(
            ToolId::ClaudeCode,
            "1001",
            None,
            DisconnectRequest::default(),
        )
        .await
        .unwrap();
    let restored: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(restored, original);
}

#[tokio::test]
async fn reconnect_retires_metadata_no_longer_advertised_without_losing_original_restore_value() {
    let root = tempdir().unwrap();
    let inv = inventory(root.path());
    let path = root.path().join("settings.json");
    let original = json!({"env":{"CLAUDE_CODE_MAX_CONTEXT_TOKENS":"original-limit"}});
    std::fs::write(&path, original.to_string()).unwrap();
    let server = platform(path.clone(), false).await;
    let manager = support::manager(
        root.path().join("records"),
        server.url.clone(),
        server.url.clone(),
    );
    manager
        .connect(context("a"), ToolId::ClaudeCode, &inv, path.clone())
        .await
        .unwrap();
    manager
        .reconnect(context("a"), ToolId::ClaudeCode, "1001", &inv, path.clone())
        .await
        .unwrap();
    let current: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(
        current["env"]["CLAUDE_CODE_MAX_CONTEXT_TOKENS"], "original-limit",
        "new catalogue does not authorize retaining an old model context override"
    );
    manager
        .disconnect(
            ToolId::ClaudeCode,
            "1001",
            None,
            DisconnectRequest::default(),
        )
        .await
        .unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&std::fs::read(path).unwrap()).unwrap(),
        original
    );
}

#[tokio::test]
async fn cross_account_reconnect_never_attempts_to_disable_the_old_accounts_key() {
    let root = tempdir().unwrap();
    let inv = inventory(root.path());
    let path = root.path().join("settings.json");
    let server = platform(path.clone(), true).await;
    let manager = support::manager(
        root.path().join("records"),
        server.url.clone(),
        server.url.clone(),
    );
    manager
        .connect(context("a"), ToolId::ClaudeCode, &inv, path.clone())
        .await
        .unwrap();
    let result = manager
        .reconnect(context("b"), ToolId::ClaudeCode, "1001", &inv, path.clone())
        .await
        .unwrap();
    assert!(!result.key_reused);
    assert_eq!(result.connection.key_id, "1002");
    assert!(std::fs::read_to_string(&path).unwrap().contains("sk-key-2"));
    assert!(manager
        .reconnect(context("b"), ToolId::ClaudeCode, "1001", &inv, path)
        .await
        .is_err());
    assert_eq!(
        server
            .requests
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.contains("/1001/disable "))
            .count(),
        0
    );
}
#[tokio::test]
async fn new_key_or_catalogue_failure_keeps_old_connection_and_retains_issued_keys() {
    for failure in ["key", "catalogue"] {
        let root = tempdir().unwrap();
        let inv = inventory(root.path());
        let path = root.path().join("settings.json");
        let server = platform_with_failure(path.clone(), false, Some(failure), None).await;
        let manager = support::manager(
            root.path().join("records"),
            server.url.clone(),
            server.url.clone(),
        );
        manager
            .connect(context("a"), ToolId::ClaudeCode, &inv, path.clone())
            .await
            .unwrap();
        let old_bytes = std::fs::read(&path).unwrap();
        let old_records = std::fs::read(root.path().join("records/connections.json")).unwrap();
        let error = manager
            .reconnect(context("b"), ToolId::ClaudeCode, "1001", &inv, path.clone())
            .await
            .unwrap_err()
            .to_string();
        assert!(
            error.contains(if failure == "key" {
                "连接"
            } else {
                "glm-5.3"
            }),
            "{error}"
        );
        assert_eq!(std::fs::read(&path).unwrap(), old_bytes);
        assert_eq!(
            std::fs::read(root.path().join("records/connections.json")).unwrap(),
            old_records
        );
        let requests = server.requests.lock().unwrap();
        assert!(!requests.iter().any(|r| r.contains("/1001/disable ")));
        assert_eq!(
            requests
                .iter()
                .filter(|r| r.contains("/1002/disable "))
                .count(),
            0
        );
    }
}
#[cfg(unix)]
#[tokio::test]
async fn config_or_record_publication_failure_rolls_back_and_never_retires_the_working_key() {
    use std::os::unix::fs::PermissionsExt;
    for fail_record in [false, true] {
        let root = tempdir().unwrap();
        let inv = inventory(root.path());
        let config_dir = root.path().join("config");
        std::fs::create_dir(&config_dir).unwrap();
        let path = config_dir.join("settings.json");
        let directory = root.path().join("records");
        let blocked = if fail_record {
            directory.clone()
        } else {
            config_dir
        };
        let server = platform_with_failure(path.clone(), false, None, Some(blocked.clone())).await;
        let manager = support::manager(directory.clone(), server.url.clone(), server.url.clone());
        manager
            .connect(context("a"), ToolId::ClaudeCode, &inv, path.clone())
            .await
            .unwrap();
        std::fs::create_dir_all(directory.join("backups")).unwrap();
        let old_bytes = std::fs::read(&path).unwrap();
        let old_records = std::fs::read(directory.join("connections.json")).unwrap();
        let result = manager
            .reconnect(context("b"), ToolId::ClaudeCode, "1001", &inv, path.clone())
            .await;
        std::fs::set_permissions(blocked, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(result.is_err());
        assert_eq!(std::fs::read(path).unwrap(), old_bytes);
        assert_eq!(
            std::fs::read(directory.join("connections.json")).unwrap(),
            old_records
        );
        let requests = server.requests.lock().unwrap();
        assert!(!requests.iter().any(|r| r.contains("/1001/disable ")));
        assert_eq!(
            requests
                .iter()
                .filter(|r| r.contains("/1002/disable "))
                .count(),
            0
        );
    }
}
#[tokio::test]
async fn repeated_reconnect_preserves_unrelated_edits_and_refuses_owned_conflicts_before_issuance()
{
    let root = tempdir().unwrap();
    let inv = inventory(root.path());
    let path = root.path().join("settings.json");
    let original = json!({"model":"before-first","env":{"ANTHROPIC_API_KEY":null,"ANTHROPIC_MODEL":"personal-override"}});
    std::fs::write(&path, original.to_string()).unwrap();
    let server = platform(path.clone(), false).await;
    let manager = support::manager(
        root.path().join("records"),
        server.url.clone(),
        server.url.clone(),
    );
    manager
        .connect(context("a"), ToolId::ClaudeCode, &inv, path.clone())
        .await
        .unwrap();
    manager
        .reconnect(context("b"), ToolId::ClaudeCode, "1001", &inv, path.clone())
        .await
        .unwrap();
    let mut current: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    current["personal"] = json!("later edit");
    std::fs::write(&path, current.to_string()).unwrap();
    manager
        .reconnect(context("a"), ToolId::ClaudeCode, "1002", &inv, path.clone())
        .await
        .unwrap();
    let mut current: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    current["model"] = json!("manual-model");
    std::fs::write(&path, current.to_string()).unwrap();
    let count = server.requests.lock().unwrap().len();
    assert!(manager
        .reconnect(context("b"), ToolId::ClaudeCode, "1001", &inv, path.clone())
        .await
        .unwrap_err()
        .to_string()
        .contains("人工修改"));
    assert_eq!(server.requests.lock().unwrap().len(), count);
    let conflict = manager
        .disconnect(
            ToolId::ClaudeCode,
            "1001",
            None,
            DisconnectRequest::default(),
        )
        .await
        .unwrap();
    assert_eq!(conflict.state, "conflicts");
    assert_eq!(
        serde_json::from_slice::<Value>(&std::fs::read(&path).unwrap()).unwrap(),
        current
    );
    use tokenmarket_core::connections::{ConflictResolution, RestoreChoice};
    let outcome = manager
        .disconnect(
            ToolId::ClaudeCode,
            "1001",
            None,
            DisconnectRequest {
                revision: conflict.revision,
                resolutions: conflict
                    .conflicts
                    .iter()
                    .map(|c| ConflictResolution {
                        file: c.file,
                        field: c.field,
                        choice: RestoreChoice::KeepCurrent,
                    })
                    .collect(),
            },
        )
        .await
        .unwrap();
    assert_eq!(outcome.state, "disconnected");
    let restored: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(
        restored,
        json!({"model":"manual-model","env":{"ANTHROPIC_API_KEY":null,"ANTHROPIC_MODEL":"personal-override"},"personal":"later edit"})
    );
}

#[tokio::test]
async fn reconnect_on_one_logical_device_does_not_retire_another_devices_connection() {
    let root = tempdir().unwrap();
    let second = tempdir().unwrap();
    let inv = inventory(root.path());
    let inv2 = inventory(second.path());
    let path = root.path().join("settings.json");
    let path2 = second.path().join("settings.json");
    let server = platform(path.clone(), false).await;
    let manager = support::manager(
        root.path().join("records"),
        server.url.clone(),
        server.url.clone(),
    );
    let other = support::manager(
        second.path().join("records"),
        server.url.clone(),
        server.url.clone(),
    );
    manager
        .connect(context("a"), ToolId::ClaudeCode, &inv, path.clone())
        .await
        .unwrap();
    other
        .connect(context("a"), ToolId::ClaudeCode, &inv2, path2.clone())
        .await
        .unwrap();
    assert_ne!(manager.device_id().unwrap(), other.device_id().unwrap());
    let other_bytes = std::fs::read(&path2).unwrap();
    manager
        .reconnect(context("b"), ToolId::ClaudeCode, "1001", &inv, path)
        .await
        .unwrap();
    assert_eq!(other.list().unwrap()[0].key_id, "1002");
    assert_eq!(std::fs::read(path2).unwrap(), other_bytes);
    let requests = server.requests.lock().unwrap();
    assert_eq!(
        requests.iter().filter(|r| r.contains("/disable ")).count(),
        0
    );
}
#[tokio::test]
async fn reconnect_upgrades_legacy_v1_null_ambiguity_using_original_private_baseline() {
    let root = tempdir().unwrap();
    let inv = inventory(root.path());
    let path = root.path().join("settings.json");
    let original = json!({"env":{"ANTHROPIC_API_KEY":null,"ANTHROPIC_MODEL":"personal-override"},"model":"personal"});
    std::fs::write(&path, original.to_string()).unwrap();
    let server = platform(path.clone(), false).await;
    let directory = root.path().join("records");
    let manager = support::manager(directory.clone(), server.url.clone(), server.url.clone());
    manager
        .connect(context("a"), ToolId::ClaudeCode, &inv, path.clone())
        .await
        .unwrap();
    let store = directory.join("connections.json");
    let mut legacy: Value = serde_json::from_slice(&support::read_private(&store)).unwrap();
    legacy[0]["version"] = json!(1);
    for field in legacy[0]["receipts"][0]["fields"].as_array_mut().unwrap() {
        field["before"] = Value::Null;
    }
    std::fs::write(store, legacy.to_string()).unwrap();
    manager
        .reconnect(context("b"), ToolId::ClaudeCode, "1001", &inv, path.clone())
        .await
        .unwrap();
    assert_eq!(manager.list().unwrap()[0].version, 2);
    manager
        .disconnect(
            ToolId::ClaudeCode,
            "1002",
            None,
            DisconnectRequest::default(),
        )
        .await
        .unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&std::fs::read(path).unwrap()).unwrap(),
        original
    );
}
#[tokio::test]
async fn harness_reconnect_reloads_all_chat_models_and_restores_the_first_two_file_baseline() {
    let root = tempdir().unwrap();
    let inv = inventory(root.path());
    let path = root.path().join("cordis.patch.yml");
    let credentials = root.path().join(".credentials.yaml");
    let original = json!([{"id":"other","config":{"enabled":true}},{"id":"llm-pi-ai","config":{"providers":{"official":{"keep":true},"tokenmarket":{"custom":"keep","models":[{"id":"personal"}]}}}}]);
    let original_credentials = json!({"version":1,"refs":{"TOKEN_MARKET_API_KEY":null,"OTHER_KEY":"personal"},"records":{"personal":{"keep":true}}});
    std::fs::write(&path, serde_yaml::to_string(&original).unwrap()).unwrap();
    std::fs::write(
        &credentials,
        serde_yaml::to_string(&original_credentials).unwrap(),
    )
    .unwrap();
    let server = platform(path.clone(), false).await;
    let manager = support::manager(
        root.path().join("records"),
        server.url.clone(),
        server.url.clone(),
    );
    let first = manager
        .connect(context("a"), ToolId::DeepSeekHarness, &inv, path.clone())
        .await
        .unwrap();
    assert_eq!(first.models, vec!["zeta", "alpha"]);
    let mut current: Value = serde_yaml::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    current
        .as_array_mut()
        .unwrap()
        .insert(0, json!({"id":"later","personal":true}));
    current.as_array_mut().unwrap().swap(1, 2);
    std::fs::write(&path, serde_yaml::to_string(&current).unwrap()).unwrap();
    let second = manager
        .reconnect(
            context("b"),
            ToolId::DeepSeekHarness,
            "1001",
            &inv,
            path.clone(),
        )
        .await
        .unwrap();
    assert_eq!(second.connection.models, vec!["beta", "gamma"]);
    assert!(!second.key_reused);
    let current: Value = serde_yaml::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let route = current
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == "llm-pi-ai")
        .unwrap();
    assert_eq!(
        route["config"]["providers"]["tokenmarket"]["models"],
        json!([{"id":"beta"},{"id":"gamma"}])
    );
    manager
        .disconnect(
            ToolId::DeepSeekHarness,
            "1002",
            None,
            DisconnectRequest::default(),
        )
        .await
        .unwrap();
    let restored: Value = serde_yaml::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let route = restored
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == "llm-pi-ai")
        .unwrap();
    assert_eq!(route, &original[1]);
    assert!(restored
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v["id"] == "later"));
    assert_eq!(
        serde_yaml::from_slice::<Value>(&std::fs::read(credentials).unwrap()).unwrap(),
        original_credentials
    );
}

#[tokio::test]
async fn harness_reconnect_keeps_required_credential_bootstrap_then_disconnect_removes_both_new_files(
) {
    let root = tempdir().unwrap();
    let inv = inventory(root.path());
    let path = root.path().join("cordis.patch.yml");
    let credentials = root.path().join(".credentials.yaml");
    let server = platform(path.clone(), false).await;
    let manager = support::manager(
        root.path().join("records"),
        server.url.clone(),
        server.url.clone(),
    );
    manager
        .connect(context("a"), ToolId::DeepSeekHarness, &inv, path.clone())
        .await
        .unwrap();
    manager
        .reconnect(
            context("b"),
            ToolId::DeepSeekHarness,
            "1001",
            &inv,
            path.clone(),
        )
        .await
        .unwrap();
    let current: Value = serde_yaml::from_slice(&std::fs::read(&credentials).unwrap()).unwrap();
    assert_eq!(current["version"], 1);
    assert_eq!(current["records"], json!({}));
    assert_eq!(current["refs"]["TOKEN_MARKET_API_KEY"], "sk-key-2");
    manager
        .reconnect(
            context("a"),
            ToolId::DeepSeekHarness,
            "1002",
            &inv,
            path.clone(),
        )
        .await
        .unwrap();
    manager
        .disconnect(
            ToolId::DeepSeekHarness,
            "1001",
            None,
            DisconnectRequest::default(),
        )
        .await
        .unwrap();
    assert!(!path.exists());
    assert!(!credentials.exists());
}

#[tokio::test]
async fn codex_reconnect_preserves_first_provider_and_later_toml_comments_and_unrelated_settings() {
    let root = tempdir().unwrap();
    let inv = inventory(root.path());
    let path = root.path().join("config.toml");
    std::fs::write(&path,"# personal\nmodel='before'\nmodel_provider='personal'\n[model_providers.personal]\nname='Personal'\n[model_providers.tokenmarket]\nname='Previous'\nexperimental_bearer_token='original-secret'\n").unwrap();
    let server = platform(path.clone(), false).await;
    let manager = support::manager(
        root.path().join("records"),
        server.url.clone(),
        server.url.clone(),
    );
    manager
        .connect(context("a"), ToolId::Codex, &inv, path.clone())
        .await
        .unwrap();
    manager
        .reconnect(context("b"), ToolId::Codex, "1001", &inv, path.clone())
        .await
        .unwrap();
    let mut current = std::fs::read_to_string(&path).unwrap();
    current.push_str("\n# later comment\n[projects.personal]\ntrust_level='trusted'\n");
    std::fs::write(&path, current).unwrap();
    manager
        .reconnect(context("a"), ToolId::Codex, "1002", &inv, path.clone())
        .await
        .unwrap();
    manager
        .disconnect(ToolId::Codex, "1001", None, DisconnectRequest::default())
        .await
        .unwrap();
    let restored = std::fs::read_to_string(path).unwrap();
    assert!(restored.contains("# personal"));
    assert!(restored.contains("# later comment"));
    let doc: toml_edit::DocumentMut = restored.parse().unwrap();
    assert_eq!(doc["model"].as_str(), Some("before"));
    assert_eq!(doc["model_provider"].as_str(), Some("personal"));
    assert_eq!(
        doc["model_providers"]["tokenmarket"]["experimental_bearer_token"].as_str(),
        Some("original-secret")
    );
    assert_eq!(
        doc["projects"]["personal"]["trust_level"].as_str(),
        Some("trusted")
    );
    assert!(doc["model_providers"]["tokenmarket"]
        .get("wire_api")
        .is_none());
}

#[tokio::test]
async fn reconnect_rejects_non_text_model_and_keeps_old_key_and_exact_config() {
    for tool in [ToolId::ClaudeCode, ToolId::Codex] {
        let root = tempdir().unwrap();
        let inv = inventory(root.path());
        let path = root.path().join(if tool == ToolId::Codex {
            "config.toml"
        } else {
            "settings.json"
        });
        let server = platform_with_failure(path.clone(), false, Some("protocol"), None).await;
        let manager = support::manager(
            root.path().join("state"),
            server.url.clone(),
            server.url.clone(),
        );
        manager
            .connect(context("a"), tool, &inv, path.clone())
            .await
            .unwrap();
        let before = std::fs::read(&path).unwrap();
        assert!(manager
            .reconnect(context("a"), tool, "1001", &inv, path.clone())
            .await
            .is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(manager.list().unwrap()[0].key_id, "1001");
        let requests = server.requests.lock().unwrap();
        assert_eq!(requests.len(), 3);
        assert!(!requests
            .iter()
            .any(|r| r.starts_with("POST /api/desktop/connections/keys/1001/disable ")));
    }
}

#[tokio::test]
async fn workspace_keys_survive_reconnect_switch_back_disconnect_and_cold_start() {
    let root = tempdir().unwrap();
    let inv = inventory(root.path());
    let path = root.path().join("settings.json");
    let server = platform(path.clone(), false).await;
    let directory = root.path().join("state");
    let manager = support::manager(directory.clone(), server.url.clone(), server.url.clone());
    manager
        .connect(context("a"), ToolId::ClaudeCode, &inv, path.clone())
        .await
        .unwrap();
    let same = manager
        .reconnect(context("a"), ToolId::ClaudeCode, "1001", &inv, path.clone())
        .await
        .unwrap();
    assert!(same.key_reused);
    assert_eq!(same.connection.key_id, "1001");
    let mut team = context("a");
    team.workspace_id = "team".into();
    let changed = manager
        .reconnect(team, ToolId::ClaudeCode, "1001", &inv, path.clone())
        .await
        .unwrap();
    assert!(!changed.key_reused);
    assert_eq!(changed.connection.bound_workspace["id"], "team");
    let reopened = support::manager(directory.clone(), server.url.clone(), server.url.clone());
    let back = reopened
        .reconnect(context("a"), ToolId::ClaudeCode, "1002", &inv, path.clone())
        .await
        .unwrap();
    assert!(back.key_reused);
    assert_eq!(back.connection.key_id, "1001");
    assert!(std::fs::read_to_string(&path).unwrap().contains("sk-key-1"));
    let outcome = reopened
        .disconnect(
            ToolId::ClaudeCode,
            "1001",
            None,
            DisconnectRequest::default(),
        )
        .await
        .unwrap();
    assert!(outcome.key_retained);
    assert!(!path.exists());
    let again = reopened
        .connect(context("a"), ToolId::ClaudeCode, &inv, path.clone())
        .await
        .unwrap();
    assert_eq!(again.key_id, "1001");
    let requests = server.requests.lock().unwrap();
    assert_eq!(
        requests
            .iter()
            .filter(|r| r.starts_with("POST /api/desktop/connections/keys "))
            .count(),
        2
    );
    assert!(!requests.iter().any(|r| r.contains("/disable ")));
    for file in [
        directory.join("keys.json"),
        directory.join("connections.json"),
    ] {
        let bytes = std::fs::read(file).unwrap();
        assert!(bytes.starts_with(b"TOKENMARKET-PRIVATE-1\n"));
        assert!(!bytes.windows(6).any(|chunk| chunk == b"sk-key"));
    }
    for entry in std::fs::read_dir(directory.join("backups")).unwrap() {
        assert!(std::fs::read(entry.unwrap().path())
            .unwrap()
            .starts_with(b"TOKENMARKET-PRIVATE-1\n"));
    }
}

#[tokio::test]
async fn account_and_tool_credentials_remain_independent_and_each_can_be_reused() {
    let root = tempdir().unwrap();
    let inv = inventory(root.path());
    let path = root.path().join("settings.json");
    let server = platform(path.clone(), false).await;
    let manager = support::manager(
        root.path().join("state"),
        server.url.clone(),
        server.url.clone(),
    );
    manager
        .connect(context("a"), ToolId::ClaudeCode, &inv, path.clone())
        .await
        .unwrap();
    let mut account_b = context("b");
    account_b.workspace_id = "space-a".into();
    manager
        .reconnect(account_b, ToolId::ClaudeCode, "1001", &inv, path.clone())
        .await
        .unwrap();
    let back = manager
        .reconnect(context("a"), ToolId::ClaudeCode, "1002", &inv, path.clone())
        .await
        .unwrap();
    assert!(back.key_reused);
    assert_eq!(back.connection.key_id, "1001");
    let codex_path = root.path().join("config.toml");
    manager
        .connect(context("a"), ToolId::Codex, &inv, codex_path.clone())
        .await
        .unwrap();
    let codex = manager
        .reconnect(context("a"), ToolId::Codex, "1003", &inv, codex_path)
        .await
        .unwrap();
    assert!(codex.key_reused);
    assert_eq!(codex.connection.key_id, "1003");
    let requests = server.requests.lock().unwrap();
    assert_eq!(
        requests
            .iter()
            .filter(|r| r.starts_with("POST /api/desktop/connections/keys "))
            .count(),
        3
    );
    assert!(!requests.iter().any(|r| r.contains("/disable ")));
}

#[tokio::test]
async fn catalogue_rejection_does_not_prove_deletion_and_never_reissues_a_key() {
    let root = tempdir().unwrap();
    let inv = inventory(root.path());
    let path = root.path().join("settings.json");
    let server = platform(path.clone(), false).await;
    let manager = support::manager(
        root.path().join("state"),
        server.url.clone(),
        server.url.clone(),
    );
    manager
        .connect(context("a"), ToolId::ClaudeCode, &inv, path.clone())
        .await
        .unwrap();
    server.reject_first.store(true, Ordering::SeqCst);
    // Discovery does not probe credentials or recreate anything.
    manager.summaries().unwrap();
    manager.list().unwrap();
    assert_eq!(server.requests.lock().unwrap().len(), 2);
    let before = std::fs::read(&path).unwrap();
    assert!(manager
        .reconnect(context("a"), ToolId::ClaudeCode, "1001", &inv, path.clone())
        .await
        .is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert!(manager
        .reconnect(context("a"), ToolId::ClaudeCode, "1001", &inv, path.clone())
        .await
        .is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(manager.list().unwrap()[0].key_id, "1001");
    let requests = server.requests.lock().unwrap();
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.starts_with("POST /api/desktop/connections/keys "))
            .count(),
        1
    );
    assert!(!requests.iter().any(|request| request.contains("/disable ")));
}

#[tokio::test]
async fn all_six_tools_reuse_a_saved_key_when_reconfiguring() {
    for tool in [
        ToolId::ClaudeCode,
        ToolId::Codex,
        ToolId::DeepSeekHarness,
        ToolId::WorkBuddy,
        ToolId::OpenClaw,
        ToolId::OpenCode,
    ] {
        let root = tempdir().unwrap();
        let inv = inventory(root.path());
        let path = root.path().join(match tool {
            ToolId::Codex => "config.toml",
            ToolId::DeepSeekHarness => "cordis.patch.yml",
            _ => "config.json",
        });
        let server = platform(path.clone(), false).await;
        let manager = support::manager(
            root.path().join("state"),
            server.url.clone(),
            server.url.clone(),
        );
        manager
            .connect(context("a"), tool, &inv, path.clone())
            .await
            .unwrap();
        let first_id = manager.list().unwrap()[0].connection_id.clone();
        if tool == ToolId::ClaudeCode {
            let tested = manager
                .test_connection(
                    tool,
                    "1001",
                    tokenmarket_core::connections::ConnectionTestRequest {
                        confirmed: true,
                        model: None,
                    },
                )
                .await
                .unwrap();
            assert!(tested.passed);
            assert_eq!(manager.summaries().unwrap()[0].1.state, "test_passed");
        }
        let outcome = manager
            .reconnect(context("a"), tool, "1001", &inv, path)
            .await
            .unwrap();
        assert!(outcome.key_reused);
        assert_eq!(outcome.connection.key_id, "1001");
        assert_ne!(first_id, manager.list().unwrap()[0].connection_id);
        assert_eq!(
            manager.summaries().unwrap()[0].1.state,
            "configured_untested"
        );
        let requests = server.requests.lock().unwrap();
        assert_eq!(
            requests
                .iter()
                .filter(|r| r.starts_with("POST /api/desktop/connections/keys "))
                .count(),
            1
        );
        assert!(!requests.iter().any(|r| r.contains("/disable ")));
    }
}

#[tokio::test]
async fn legacy_plaintext_connection_is_imported_and_encrypted_without_issuing_another_key() {
    let root = tempdir().unwrap();
    let inv = inventory(root.path());
    let path = root.path().join("settings.json");
    std::fs::write(&path, r#"{"env":{"ANTHROPIC_API_KEY":"original"}}"#).unwrap();
    let server = platform(path.clone(), false).await;
    let directory = root.path().join("state");
    let manager = support::manager(directory.clone(), server.url.clone(), server.url.clone());
    manager
        .connect(context("a"), ToolId::ClaudeCode, &inv, path.clone())
        .await
        .unwrap();
    let store = directory.join("connections.json");
    let mut records: Value = serde_json::from_slice(&support::read_private(&store)).unwrap();
    records[0].as_object_mut().unwrap().remove("platform_url");
    records[0].as_object_mut().unwrap().remove("connection_id");
    std::fs::write(&store, records.to_string()).unwrap();
    std::fs::remove_file(directory.join("keys.json")).unwrap();
    for entry in std::fs::read_dir(directory.join("backups")).unwrap() {
        let backup = entry.unwrap().path();
        let plain = support::read_private(&backup);
        std::fs::write(backup, plain).unwrap();
    }
    let reopened = support::manager(directory.clone(), server.url.clone(), server.url.clone());
    let outcome = reopened
        .reconnect(context("a"), ToolId::ClaudeCode, "1001", &inv, path)
        .await
        .unwrap();
    assert!(outcome.key_reused);
    assert_eq!(outcome.connection.key_id, "1001");
    assert!(std::fs::read(&store)
        .unwrap()
        .starts_with(b"TOKENMARKET-PRIVATE-1\n"));
    for entry in std::fs::read_dir(directory.join("backups")).unwrap() {
        assert!(std::fs::read(entry.unwrap().path())
            .unwrap()
            .starts_with(b"TOKENMARKET-PRIVATE-1\n"));
    }
    let requests = server.requests.lock().unwrap();
    assert_eq!(
        requests
            .iter()
            .filter(|r| r.starts_with("POST /api/desktop/connections/keys "))
            .count(),
        1
    );
}

#[tokio::test]
async fn a_second_process_cannot_issue_a_key_while_an_operation_holds_the_device_lock() {
    let root = tempdir().unwrap();
    let inv = inventory(root.path());
    let path = root.path().join("settings.json");
    let server = platform(path.clone(), false).await;
    let directory = root.path().join("state");
    std::fs::create_dir(&directory).unwrap();
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(directory.join("operation.lock"))
        .unwrap();
    lock.lock().unwrap();
    let other = support::manager(directory.clone(), server.url.clone(), server.url.clone());
    assert!(other
        .connect(context("a"), ToolId::ClaudeCode, &inv, path.clone())
        .await
        .is_err());
    assert!(server.requests.lock().unwrap().is_empty());
    assert!(!path.exists());
    drop(lock);
    other
        .connect(context("a"), ToolId::ClaudeCode, &inv, path.clone())
        .await
        .unwrap();
    let reopened = support::manager(directory, server.url.clone(), server.url.clone());
    assert!(
        reopened
            .reconnect(context("a"), ToolId::ClaudeCode, "1001", &inv, path)
            .await
            .unwrap()
            .key_reused
    );
    assert_eq!(
        server
            .requests
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.starts_with("POST /api/desktop/connections/keys "))
            .count(),
        1
    );
}

#[tokio::test]
async fn test_and_production_platforms_never_reuse_each_others_saved_keys() {
    let root = tempdir().unwrap();
    let inv = inventory(root.path());
    let path = root.path().join("settings.json");
    let production = platform(path.clone(), false).await;
    let test = platform(path.clone(), false).await;
    let directory = root.path().join("state");
    let prod_manager = support::manager(
        directory.clone(),
        production.url.clone(),
        production.url.clone(),
    );
    let test_manager = support::manager(directory, test.url.clone(), test.url.clone());
    prod_manager
        .connect(context("a"), ToolId::ClaudeCode, &inv, path.clone())
        .await
        .unwrap();
    assert!(
        !test_manager
            .reconnect(context("a"), ToolId::ClaudeCode, "1001", &inv, path.clone())
            .await
            .unwrap()
            .key_reused
    );
    assert!(
        prod_manager
            .reconnect(context("a"), ToolId::ClaudeCode, "1001", &inv, path)
            .await
            .unwrap()
            .key_reused
    );
    for server in [&production, &test] {
        assert_eq!(
            server
                .requests
                .lock()
                .unwrap()
                .iter()
                .filter(|r| r.starts_with("POST /api/desktop/connections/keys "))
                .count(),
            1
        );
    }
}
