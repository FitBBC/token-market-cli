use serde_json::{json, Value};
use tempfile::tempdir;
use tokensmarket_core::{
    connections::{ConnectContext, ConnectionManager, DisconnectRequest},
    inventory::{HostPlatform, InstallationEnvironment, ToolInventory},
    ToolId,
};
async fn platform(
    models: serde_json::Value,
    fail_key: bool,
) -> (String, tokio::task::JoinHandle<Vec<String>>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let mut requests = vec![];
        let mut issued_scope = Value::Null;
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut data = vec![];
            loop {
                let mut chunk = [0; 4096];
                let n = socket.read(&mut chunk).await.unwrap();
                if n == 0 {
                    break;
                }
                data.extend_from_slice(&chunk[..n]);
                if let Some(end) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                    let header = String::from_utf8_lossy(&data[..end]);
                    let length: usize = header
                        .lines()
                        .find_map(|line| {
                            line.to_lowercase()
                                .strip_prefix("content-length:")
                                .map(|s| s.trim().parse().unwrap())
                        })
                        .unwrap_or(0);
                    if data.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            let request = String::from_utf8(data).unwrap();
            let first = request.lines().next().unwrap().to_string();
            requests.push(first.clone());
            let response = if first.starts_with("GET /v1/models ") {
                assert!(request.contains("Bearer sk-fixture"));
                models.clone()
            } else if first.contains("/disable ") {
                assert_eq!(
                    first,
                    "POST /api/desktop/connections/keys/9001/disable HTTP/1.1"
                );
                assert!(request.contains("Bearer new-account-session"));
                let payload: Value =
                    serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
                assert_eq!(
                    payload, issued_scope,
                    "disable must retain exact old workspace/device/tool scope"
                );
                json!({"success":models["disable_success"] == true,"data":models["disable_result"] != false})
            } else {
                assert!(request.contains("Bearer desktop-secret"));
                assert!(request.contains("\"workspace_id\":\"42\""));
                issued_scope =
                    serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
                json!({"success":!fail_key,"data":{"api_key":"sk-fixture","key_id":"9001","owner":{"id":"u1","name":"Ada"},"bound_workspace":{"id":"42","name":"Research"}}})
            };
            let body = response.to_string();
            socket
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
            if fail_key
                || first.contains("/disable ")
                || (first.starts_with("GET ")
                    && models["data"][0]["id"] == "glm-5.3"
                    && models["wait_for_cleanup"] != true)
            {
                break;
            }
        }
        requests
    });
    (address, task)
}
fn inventory(root: &std::path::Path) -> ToolInventory {
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let executable = bin.join("claude");
    std::fs::write(&executable, "fixture").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    ToolInventory::new(InstallationEnvironment {
        platform: HostPlatform::MacOs,
        home: root.into(),
        command_directories: vec![bin],
        application_directories: vec![],
        executable_extensions: vec![],
    })
}
fn context() -> ConnectContext {
    ConnectContext {
        session_token: "desktop-secret".into(),
        workspace_id: "42".into(),
    }
}

#[tokio::test]
async fn disconnect_restores_owned_fields_preserves_later_edits_and_finishes_when_old_key_disable_fails(
) {
    let root = tempdir().unwrap();
    let config = root.path().join("settings.json");
    std::fs::write(
        &config,
        json!({"model":"old","env":{"ANTHROPIC_API_KEY":"old-secret","OTHER":"keep"}}).to_string(),
    )
    .unwrap();
    let (url, server) = platform(
        json!({"wait_for_cleanup":true,"data":[{"id":"glm-5.3"}]}),
        false,
    )
    .await;
    let manager = ConnectionManager::new(
        root.path().join(".tokensmarket/desktop-connections"),
        url.clone(),
        url,
    );
    manager
        .connect(
            context(),
            ToolId::ClaudeCode,
            &inventory(root.path()),
            config.clone(),
        )
        .await
        .unwrap();
    let mut edited: Value = serde_json::from_slice(&std::fs::read(&config).unwrap()).unwrap();
    edited["permissions"] = json!({"allow":["Read"]});
    std::fs::write(&config, edited.to_string()).unwrap();
    let outcome = manager
        .disconnect(
            ToolId::ClaudeCode,
            "9001",
            Some("new-account-session"),
            DisconnectRequest::default(),
        )
        .await
        .unwrap();
    assert_eq!(outcome.state, "disconnected");
    assert!(!outcome.remote_disabled);
    assert!(manager.list().unwrap().is_empty());
    let restored: Value = serde_json::from_slice(&std::fs::read(&config).unwrap()).unwrap();
    assert_eq!(
        restored,
        json!({"model":"old","env":{"ANTHROPIC_API_KEY":"old-secret","OTHER":"keep"},"permissions":{"allow":["Read"]}})
    );
    assert!(manager
        .disconnect(
            ToolId::ClaudeCode,
            "9001",
            Some("new-account-session"),
            DisconnectRequest::default()
        )
        .await
        .is_err());
    assert_eq!(
        server.await.unwrap(),
        vec![
            "POST /api/desktop/connections/keys HTTP/1.1",
            "GET /v1/models HTTP/1.1",
            "POST /api/desktop/connections/keys/9001/disable HTTP/1.1"
        ]
    );
}

#[tokio::test]
async fn explicit_conflict_choice_preserves_user_replaced_container_without_exposing_secrets() {
    use tokensmarket_core::connections::{ConflictResolution, RestoreChoice};
    let root = tempdir().unwrap();
    let config = root.path().join("settings.json");
    let (url, server) = platform(json!({"data":[{"id":"glm-5.3"}]}), false).await;
    let manager = ConnectionManager::new(
        root.path().join(".tokensmarket/desktop-connections"),
        url.clone(),
        url,
    );
    manager
        .connect(
            context(),
            ToolId::ClaudeCode,
            &inventory(root.path()),
            config.clone(),
        )
        .await
        .unwrap();
    server.await.unwrap();
    let manual = json!({"model":"my-private-model","env":"my-private-container","other":true});
    std::fs::write(&config, manual.to_string()).unwrap();
    let review = manager
        .disconnect(
            ToolId::ClaudeCode,
            "9001",
            None,
            DisconnectRequest::default(),
        )
        .await
        .unwrap();
    assert_eq!(review.state, "conflicts");
    assert!(!review.conflicts.is_empty());
    let wire = serde_json::to_string(&review).unwrap();
    assert!(!wire.contains("private"));
    assert!(!wire.contains("ANTHROPIC"));
    assert!(!wire.contains("settings.json"));
    assert_eq!(
        std::fs::read_to_string(&config).unwrap(),
        manual.to_string()
    );
    let result = manager
        .disconnect(
            ToolId::ClaudeCode,
            "9001",
            None,
            DisconnectRequest {
                revision: review.revision,
                resolutions: review
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
    assert_eq!(result.state, "disconnected");
    assert_eq!(
        serde_json::from_slice::<Value>(&std::fs::read(config).unwrap()).unwrap(),
        manual
    );
}

#[tokio::test]
async fn stale_conflict_approval_cannot_overwrite_newer_edits_and_fresh_restore_uses_first_baseline(
) {
    use tokensmarket_core::connections::{ConflictResolution, RestoreChoice};
    let root = tempdir().unwrap();
    let config = root.path().join("settings.json");
    std::fs::write(
        &config,
        json!({"model":"original-provider","env":{"ANTHROPIC_AUTH_TOKEN":"original-secret"}})
            .to_string(),
    )
    .unwrap();
    let (url, server) = platform(json!({"data":[{"id":"glm-5.3"}]}), false).await;
    let manager = ConnectionManager::new(
        root.path().join(".tokensmarket/desktop-connections"),
        url.clone(),
        url,
    );
    manager
        .connect(
            context(),
            ToolId::ClaudeCode,
            &inventory(root.path()),
            config.clone(),
        )
        .await
        .unwrap();
    server.await.unwrap();
    let mut manual: Value = serde_json::from_slice(&std::fs::read(&config).unwrap()).unwrap();
    manual["model"] = json!("first-manual-model");
    std::fs::write(&config, manual.to_string()).unwrap();
    let first = manager
        .disconnect(
            ToolId::ClaudeCode,
            "9001",
            None,
            DisconnectRequest::default(),
        )
        .await
        .unwrap();
    manual["model"] = json!("newer-manual-model");
    std::fs::write(&config, manual.to_string()).unwrap();
    let stale = manager
        .disconnect(
            ToolId::ClaudeCode,
            "9001",
            None,
            DisconnectRequest {
                revision: first.revision.clone(),
                resolutions: vec![ConflictResolution {
                    file: 0,
                    field: 0,
                    choice: RestoreChoice::RestoreOriginal,
                }],
            },
        )
        .await
        .unwrap();
    assert_eq!(stale.state, "conflicts");
    assert_ne!(stale.revision, first.revision);
    assert_eq!(
        std::fs::read_to_string(&config).unwrap(),
        manual.to_string()
    );
    let done = manager
        .disconnect(
            ToolId::ClaudeCode,
            "9001",
            None,
            DisconnectRequest {
                revision: stale.revision,
                resolutions: vec![ConflictResolution {
                    file: 0,
                    field: 0,
                    choice: RestoreChoice::RestoreOriginal,
                }],
            },
        )
        .await
        .unwrap();
    assert_eq!(done.state, "disconnected");
    assert_eq!(
        serde_json::from_slice::<Value>(&std::fs::read(config).unwrap()).unwrap(),
        json!({"model":"original-provider","env":{"ANTHROPIC_AUTH_TOKEN":"original-secret"}})
    );
}
#[tokio::test]
async fn malformed_later_config_and_stale_record_are_preserved_without_remote_requests() {
    let root = tempdir().unwrap();
    let config = root.path().join("settings.json");
    let (url, server) = platform(json!({"data":[{"id":"glm-5.3"}]}), false).await;
    let manager = ConnectionManager::new(
        root.path().join(".tokensmarket/desktop-connections"),
        url.clone(),
        url,
    );
    manager
        .connect(
            context(),
            ToolId::ClaudeCode,
            &inventory(root.path()),
            config.clone(),
        )
        .await
        .unwrap();
    server.await.unwrap();
    std::fs::write(&config, "{later-malformed").unwrap();
    assert!(manager
        .disconnect(
            ToolId::ClaudeCode,
            "9001",
            Some("new-account"),
            DisconnectRequest::default()
        )
        .await
        .is_err());
    assert!(manager
        .disconnect(
            ToolId::ClaudeCode,
            "different-key",
            Some("new-account"),
            DisconnectRequest::default()
        )
        .await
        .is_err());
    assert_eq!(std::fs::read_to_string(config).unwrap(), "{later-malformed");
    assert_eq!(manager.list().unwrap().len(), 1);
}
#[tokio::test]
async fn newly_created_config_is_removed_only_when_empty_after_owned_fields_are_restored() {
    for unrelated in [false, true] {
        let root = tempdir().unwrap();
        let config = root.path().join("settings.json");
        let (url, server) = platform(json!({"data":[{"id":"glm-5.3"}]}), false).await;
        let manager = ConnectionManager::new(
            root.path().join(".tokensmarket/desktop-connections"),
            url.clone(),
            url,
        );
        manager
            .connect(
                context(),
                ToolId::ClaudeCode,
                &inventory(root.path()),
                config.clone(),
            )
            .await
            .unwrap();
        server.await.unwrap();
        if unrelated {
            let mut value: Value =
                serde_json::from_slice(&std::fs::read(&config).unwrap()).unwrap();
            value["env"]["USER_ADDED"] = json!("keep");
            std::fs::write(&config, value.to_string()).unwrap();
        }
        assert_eq!(
            manager
                .disconnect(
                    ToolId::ClaudeCode,
                    "9001",
                    None,
                    DisconnectRequest::default()
                )
                .await
                .unwrap()
                .state,
            "disconnected"
        );
        if unrelated {
            assert_eq!(
                serde_json::from_slice::<Value>(&std::fs::read(config).unwrap()).unwrap(),
                json!({"env":{"USER_ADDED":"keep"}})
            );
        } else {
            assert!(!config.exists());
        }
    }
}
#[tokio::test]
async fn exact_old_key_disable_success_is_truthful_and_bound_to_saved_device_and_workspace() {
    let root = tempdir().unwrap();
    let config = root.path().join("settings.json");
    let (url, server) = platform(
        json!({"disable_success":true,"wait_for_cleanup":true,"data":[{"id":"glm-5.3"}]}),
        false,
    )
    .await;
    let manager = ConnectionManager::new(
        root.path().join(".tokensmarket/desktop-connections"),
        url.clone(),
        url,
    );
    manager
        .connect(
            context(),
            ToolId::ClaudeCode,
            &inventory(root.path()),
            config,
        )
        .await
        .unwrap();
    let result = manager
        .disconnect(
            ToolId::ClaudeCode,
            "9001",
            Some("new-account-session"),
            DisconnectRequest::default(),
        )
        .await
        .unwrap();
    assert!(result.remote_disabled);
    assert_eq!(server.await.unwrap().len(), 3);
}
#[tokio::test]
async fn successful_envelope_with_false_disable_result_does_not_claim_remote_success() {
    let root = tempdir().unwrap();
    let config = root.path().join("settings.json");
    let (url,server)=platform(json!({"disable_success":true,"disable_result":false,"wait_for_cleanup":true,"data":[{"id":"glm-5.3"}]}),false).await;
    let manager = ConnectionManager::new(
        root.path().join(".tokensmarket/desktop-connections"),
        url.clone(),
        url,
    );
    manager
        .connect(
            context(),
            ToolId::ClaudeCode,
            &inventory(root.path()),
            config,
        )
        .await
        .unwrap();
    let result = manager
        .disconnect(
            ToolId::ClaudeCode,
            "9001",
            Some("new-account-session"),
            DisconnectRequest::default(),
        )
        .await
        .unwrap();
    assert_eq!(result.state, "disconnected");
    assert!(!result.remote_disabled);
    assert!(manager.list().unwrap().is_empty());
    assert_eq!(server.await.unwrap().len(), 3);
}
#[tokio::test]
async fn first_baseline_distinguishes_explicit_null_from_an_absent_field_after_reopening_store() {
    let root = tempdir().unwrap();
    let config = root.path().join("settings.json");
    let (url, server) = platform(json!({"data":[{"id":"glm-5.3"}]}), false).await;
    let directory = root.path().join(".tokensmarket/desktop-connections");
    let manager = ConnectionManager::new(directory.clone(), url.clone(), url.clone());
    let original = json!({"model":null,"env":{"ANTHROPIC_API_KEY":null}});
    std::fs::write(&config, original.to_string()).unwrap();
    manager
        .connect(
            context(),
            ToolId::ClaudeCode,
            &inventory(root.path()),
            config.clone(),
        )
        .await
        .unwrap();
    server.await.unwrap();
    let reopened = ConnectionManager::new(directory, url.clone(), url);
    reopened
        .disconnect(
            ToolId::ClaudeCode,
            "9001",
            None,
            DisconnectRequest::default(),
        )
        .await
        .unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&std::fs::read(config).unwrap()).unwrap(),
        original
    );
}
#[tokio::test]
async fn version_one_fixture_recovers_only_owned_baseline_values_and_fails_closed_without_backup() {
    for missing_backup in [false, true] {
        let root = tempdir().unwrap();
        let config = root.path().join("settings.json");
        let (url, server) = platform(json!({"data":[{"id":"glm-5.3"}]}), false).await;
        let directory = root.path().join(".tokensmarket/desktop-connections");
        let manager = ConnectionManager::new(directory.clone(), url.clone(), url);
        std::fs::write(
            &config,
            json!({"model":null,"env":{"OTHER":"original"}}).to_string(),
        )
        .unwrap();
        manager
            .connect(
                context(),
                ToolId::ClaudeCode,
                &inventory(root.path()),
                config.clone(),
            )
            .await
            .unwrap();
        server.await.unwrap();
        // Fixture reproduces the persisted v1 wire format, where absent `before` serialized null.
        let store = directory.join("connections.json");
        let mut v1: Value = serde_json::from_slice(&std::fs::read(&store).unwrap()).unwrap();
        v1[0]["version"] = json!(1);
        for field in v1[0]["receipts"][0]["fields"].as_array_mut().unwrap() {
            if field.get("before").is_none() {
                field["before"] = Value::Null;
            }
        }
        if missing_backup {
            std::fs::remove_file(v1[0]["receipts"][0]["backup_path"].as_str().unwrap()).unwrap();
        }
        std::fs::write(&store, v1.to_string()).unwrap();
        let mut manual: Value = serde_json::from_slice(&std::fs::read(&config).unwrap()).unwrap();
        manual["env"]["OTHER"] = json!("later-user-change");
        std::fs::write(&config, manual.to_string()).unwrap();
        let result = manager
            .disconnect(
                ToolId::ClaudeCode,
                "9001",
                None,
                DisconnectRequest::default(),
            )
            .await;
        if missing_backup {
            assert!(result.is_err());
            assert_eq!(
                std::fs::read_to_string(&config).unwrap(),
                manual.to_string()
            );
            assert_eq!(manager.list().unwrap().len(), 1);
        } else {
            assert_eq!(result.unwrap().state, "disconnected");
            assert_eq!(
                serde_json::from_slice::<Value>(&std::fs::read(config).unwrap()).unwrap(),
                json!({"model":null,"env":{"OTHER":"later-user-change"}})
            );
        }
    }
}
#[tokio::test]
async fn codex_disconnect_preserves_toml_comments_other_providers_auth_file_and_later_provider_fields(
) {
    let root = tempdir().unwrap();
    let config = root.path().join("config.toml");
    let inventory = inventory(root.path());
    std::fs::copy(
        root.path().join("bin/claude"),
        root.path().join("bin/codex"),
    )
    .unwrap();
    let original="# personal comment\nmodel_provider = \"other\" # keep inline\nmodel = \"personal-model\"\n[model_providers.other]\nbase_url = \"https://other.example/v1\"\n[model_providers.tokensmarket]\nexperimental_bearer_token = \"previous-secret\"\n";
    std::fs::write(&config, original).unwrap();
    let auth = root.path().join("auth.json");
    std::fs::write(&auth, "auth-file-untouched").unwrap();
    let (url, server) = platform(json!({"data":[{"id":"glm-5.3"}]}), false).await;
    let manager = ConnectionManager::new(
        root.path().join(".tokensmarket/desktop-connections"),
        url.clone(),
        url,
    );
    manager
        .connect(context(), ToolId::Codex, &inventory, config.clone())
        .await
        .unwrap();
    server.await.unwrap();
    let mut edited = std::fs::read_to_string(&config)
        .unwrap()
        .parse::<toml_edit::DocumentMut>()
        .unwrap();
    edited["model_providers"]["tokensmarket"]["user_added"] = toml_edit::value(true);
    std::fs::write(&config, edited.to_string()).unwrap();
    let done = manager
        .disconnect(ToolId::Codex, "9001", None, DisconnectRequest::default())
        .await
        .unwrap();
    assert_eq!(done.state, "disconnected");
    let restored = std::fs::read_to_string(&config).unwrap();
    assert!(restored.contains("# personal comment"));
    assert!(restored.contains("# keep inline"));
    let value: Value = toml_edit::de::from_str(&restored).unwrap();
    assert_eq!(
        value,
        json!({"model_provider":"other","model":"personal-model","model_providers":{"other":{"base_url":"https://other.example/v1"},"tokensmarket":{"experimental_bearer_token":"previous-secret","user_added":true}}})
    );
    assert_eq!(
        std::fs::read_to_string(auth).unwrap(),
        "auth-file-untouched"
    );
}
#[tokio::test]
async fn newly_created_codex_config_keeps_later_comment_when_managed_fields_are_removed() {
    let root = tempdir().unwrap();
    let config = root.path().join("config.toml");
    let inventory = inventory(root.path());
    std::fs::copy(
        root.path().join("bin/claude"),
        root.path().join("bin/codex"),
    )
    .unwrap();
    let (url, server) = platform(json!({"data":[{"id":"glm-5.3"}]}), false).await;
    let manager = ConnectionManager::new(
        root.path().join(".tokensmarket/desktop-connections"),
        url.clone(),
        url,
    );
    manager
        .connect(context(), ToolId::Codex, &inventory, config.clone())
        .await
        .unwrap();
    server.await.unwrap();
    let configured = std::fs::read_to_string(&config).unwrap();
    std::fs::write(&config, format!("# later personal note\n{configured}")).unwrap();
    manager
        .disconnect(ToolId::Codex, "9001", None, DisconnectRequest::default())
        .await
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(config).unwrap().trim(),
        "# later personal note"
    );
}
