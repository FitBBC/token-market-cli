use serde_json::{json, Value};
use tempfile::tempdir;
use tokensmarket_core::{
    connections::{ConnectContext, ConnectionManager},
    inventory::{HostPlatform, InstallationEnvironment, ToolInventory},
    ToolId,
};
async fn platform(
    models: serde_json::Value,
    expected_requests: usize,
) -> (String, tokio::task::JoinHandle<Vec<String>>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let mut requests = vec![];
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
                json!({"success":true,"data":true})
            } else {
                assert!(request.contains("Bearer desktop-secret"));
                assert!(request.contains("\"workspace_id\":\"42\""));
                json!({"success":true,"data":{"api_key":"sk-fixture","key_id":"9001","owner":{"id":"u1","name":"Ada"},"bound_workspace":{"id":"42","name":"Research"}}})
            };
            let body = response.to_string();
            let status = if first.starts_with("GET ") && models["fail_http"] == true {
                "503 Service Unavailable"
            } else {
                "200 OK"
            };
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
            if requests.len() == expected_requests {
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
    let executable = bin.join("dsh");
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
fn yaml(path: &std::path::Path) -> Value {
    serde_yaml::from_slice(&std::fs::read(path).unwrap()).unwrap()
}
#[tokio::test]
async fn harness_connect_imports_every_chat_model_in_catalogue_order_and_preserves_both_documents()
{
    let root = tempdir().unwrap();
    let inventory = inventory(root.path());
    let profile = root.path().join("profiles/desktop/cordis.patch.yml");
    std::fs::create_dir_all(profile.parent().unwrap()).unwrap();
    let credentials = root.path().join(".credentials.yaml");
    let old_profile = json!([{"id":"plugin-other","config":{"keep":true}},{"id":"llm-pi-ai","unknown":"keep","config":{"providers":{"official":{"apiKeyEnv":"OFFICIAL_KEY"},"tokensmarket":{"custom":"keep"}}}}]);
    let old_credentials = json!({"version":1,"refs":{"OFFICIAL_KEY":"private-other"},"records":{"official":{"secret":"keep"}},"custom":"keep"});
    std::fs::write(&profile, serde_yaml::to_string(&old_profile).unwrap()).unwrap();
    std::fs::write(
        &credentials,
        serde_yaml::to_string(&old_credentials).unwrap(),
    )
    .unwrap();
    let catalogue = json!({"data":[{"id":"zeta","model_types":["TEXT_GENERATION"],"context_window_tokens":65536,"max_output_tokens":8192,"input_modalities":["text","image"]},{"id":"responses-only","model_types":["IMAGE_GENERATION"]},{"id":"alpha","display_name":"Alpha","model_types":["TEXT_GENERATION"]},{"id":"missing-types"}]});
    let (url, server) = platform(catalogue.clone(), 2).await;
    let manager = ConnectionManager::new(
        root.path().join(".tokensmarket/desktop-connections"),
        url.clone(),
        url.clone(),
    );
    let summary = manager
        .connect(
            context(),
            ToolId::DeepSeekHarness,
            &inventory,
            profile.clone(),
        )
        .await
        .unwrap();
    assert_eq!(summary.models, vec!["zeta", "alpha"]);
    let applied = yaml(&profile);
    assert_eq!(applied[0], old_profile[0]);
    assert_eq!(applied[1]["unknown"], "keep");
    assert_eq!(
        applied[1]["config"]["providers"]["official"],
        old_profile[1]["config"]["providers"]["official"]
    );
    let route = &applied[1]["config"]["providers"]["tokensmarket"];
    assert_eq!(route["custom"], "keep");
    assert_eq!(route["api"], "openai-completions");
    assert_eq!(route["baseURL"], format!("{url}/v1"));
    assert_eq!(
        route["models"],
        json!([{"id":"zeta","contextWindow":65536,"maxTokens":8192,"input":["text","image"]},{"id":"alpha","name":"Alpha"}])
    );
    assert!(applied[1]["config"].get("model").is_none());
    let mut expected = old_credentials.clone();
    expected["refs"]["TOKEN_MARKET_API_KEY"] = json!("sk-fixture");
    assert_eq!(yaml(&credentials), expected);
    let record = manager.list().unwrap().remove(0);
    assert_eq!(record.protocol, "OPENAI_CHAT_COMPLETIONS");
    assert_eq!(
        record.model_snapshot,
        catalogue["data"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|m| m["id"] == "zeta" || m["id"] == "alpha")
            .cloned()
            .collect::<Vec<_>>()
    );
    assert_eq!(record.receipts.len(), 2);
    assert!(record.receipts.iter().all(|r| r.backup_path.is_some()));
    assert_eq!(
        server.await.unwrap(),
        vec![
            "POST /api/desktop/connections/keys HTTP/1.1",
            "GET /v1/models HTTP/1.1"
        ]
    );
}

#[tokio::test]
async fn harness_preserves_legacy_credentials_shape_and_declares_only_real_metadata() {
    let root = tempdir().unwrap();
    let inventory = inventory(root.path());
    let profile = root.path().join("cordis.patch.yml");
    let credentials = root.path().join(".credentials.yaml");
    std::fs::write(
        &credentials,
        "OTHER_KEY: keep-secret\nTOKEN_MARKET_API_KEY: old-secret\n",
    )
    .unwrap();
    let (url,server)=platform(json!({"data":[{"id":"custom-one","model_types":["TEXT_GENERATION"]},{"id":"glm-5.3","model_types":["TEXT_GENERATION"],"context_window_tokens":0,"max_output_tokens":0}]}),2).await;
    let manager = ConnectionManager::new(root.path().join("state"), url.clone(), url);
    let summary = manager
        .connect(
            context(),
            ToolId::DeepSeekHarness,
            &inventory,
            profile.clone(),
        )
        .await
        .unwrap();
    assert_eq!(summary.models, vec!["custom-one", "glm-5.3"]);
    assert_eq!(
        yaml(&credentials),
        json!({"OTHER_KEY":"keep-secret","TOKEN_MARKET_API_KEY":"sk-fixture"})
    );
    assert_eq!(
        yaml(&profile)[0]["config"]["providers"]["tokensmarket"]["models"],
        json!([{"id":"custom-one"},{"id":"glm-5.3"}])
    );
    assert!(manager.list().unwrap()[0].receipts[1]
        .fields
        .iter()
        .any(|f| f.pointer == "/TOKEN_MARKET_API_KEY" && f.before == Some(json!("old-secret"))));
    assert_eq!(server.await.unwrap().len(), 2);
}

#[tokio::test]
async fn harness_catalogue_failure_keeps_both_files_and_cleans_only_new_key_once() {
    for catalogue in [
        json!({"data":[]}),
        json!({"data":[{"id":"glm-5.3","model_types":["IMAGE_GENERATION"]},{"id":"unknown"}]}),
        json!({"error":{"message":"unavailable"}}),
        json!({"fail_http":true}),
    ] {
        let root = tempdir().unwrap();
        let inventory = inventory(root.path());
        let profile = root.path().join("cordis.patch.yml");
        let credentials = root.path().join(".credentials.yaml");
        let original_profile = "- id: other\n  config: {keep: true}\n";
        let original_credentials = "version: 1\nrefs: {OTHER_KEY: secret}\nrecords: {}\n";
        std::fs::write(&profile, original_profile).unwrap();
        std::fs::write(&credentials, original_credentials).unwrap();
        let (url, server) = platform(catalogue, 3).await;
        let manager = ConnectionManager::new(root.path().join("state"), url.clone(), url);
        assert!(manager
            .connect(
                context(),
                ToolId::DeepSeekHarness,
                &inventory,
                profile.clone()
            )
            .await
            .is_err());
        assert_eq!(std::fs::read_to_string(profile).unwrap(), original_profile);
        assert_eq!(
            std::fs::read_to_string(credentials).unwrap(),
            original_credentials
        );
        assert!(manager.list().unwrap().is_empty());
        assert_eq!(
            server.await.unwrap(),
            vec![
                "POST /api/desktop/connections/keys HTTP/1.1",
                "GET /v1/models HTTP/1.1",
                "POST /api/desktop/connections/keys/9001/disable HTTP/1.1"
            ]
        );
    }
}

#[tokio::test]
async fn malformed_profile_or_credentials_fails_before_issuance() {
    for (profile_text, credentials_text) in [
        ("- [broken", "{}"),
        ("[]", "version: 2\nrefs: {}\nrecords: {}"),
        ("[]", "[broken"),
        ("- id: llm-pi-ai\n- id: llm-pi-ai", "{}"),
    ] {
        let root = tempdir().unwrap();
        let inventory = inventory(root.path());
        let profile = root.path().join("cordis.patch.yml");
        let credentials = root.path().join(".credentials.yaml");
        std::fs::write(&profile, profile_text).unwrap();
        std::fs::write(&credentials, credentials_text).unwrap();
        let manager = ConnectionManager::new(
            root.path().join("state"),
            "http://127.0.0.1:1".into(),
            "http://127.0.0.1:1".into(),
        );
        let error = manager
            .connect(
                context(),
                ToolId::DeepSeekHarness,
                &inventory,
                profile.clone(),
            )
            .await
            .unwrap_err()
            .to_string();
        assert!(!error.contains("取得接入 Key"));
        assert_eq!(std::fs::read_to_string(profile).unwrap(), profile_text);
        assert_eq!(
            std::fs::read_to_string(credentials).unwrap(),
            credentials_text
        );
        assert!(manager.list().unwrap().is_empty());
    }
}

#[cfg(unix)]
#[tokio::test]
async fn second_file_write_failure_rolls_back_profile_and_retains_credentials() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempdir().unwrap();
    let inventory = inventory(root.path());
    let home = root.path().join("home");
    let profile = home.join("profiles/desktop/cordis.patch.yml");
    std::fs::create_dir_all(profile.parent().unwrap()).unwrap();
    let credentials = home.join(".credentials.yaml");
    let original_profile = "- id: other\n  config: {keep: true}\n";
    let original_credentials = "OTHER_KEY: keep-secret\n";
    std::fs::write(&profile, original_profile).unwrap();
    std::fs::write(&credentials, original_credentials).unwrap();
    std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o500)).unwrap();
    let (url, server) = platform(
        json!({"data":[{"id":"available","model_types":["TEXT_GENERATION"]}]}),
        3,
    )
    .await;
    let manager = ConnectionManager::new(root.path().join("state"), url.clone(), url);
    let result = manager
        .connect(
            context(),
            ToolId::DeepSeekHarness,
            &inventory,
            profile.clone(),
        )
        .await;
    std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(result.is_err());
    assert_eq!(std::fs::read_to_string(profile).unwrap(), original_profile);
    assert_eq!(
        std::fs::read_to_string(credentials).unwrap(),
        original_credentials
    );
    assert!(manager.list().unwrap().is_empty());
    assert_eq!(server.await.unwrap().len(), 3);
}

#[tokio::test]
async fn harness_disconnect_restores_both_owned_scopes_and_preserves_later_provider_plugin_and_credential_edits(
) {
    use tokensmarket_core::connections::DisconnectRequest;
    let root = tempdir().unwrap();
    let inventory = inventory(root.path());
    let profile = root.path().join("cordis.patch.yml");
    let credentials = root.path().join(".credentials.yaml");
    let original = json!([{"id":"llm-pi-ai","custom":"keep","config":{"providers":{"official":{"key":"other"},"tokensmarket":{"custom":"before"}}}},{"id":"existing-plugin"}]);
    let original_credentials = json!({"version":1,"refs":{"TOKEN_MARKET_API_KEY":null,"OTHER":"secret"},"records":{"official":{"key":"keep"}}});
    std::fs::write(&profile, serde_yaml::to_string(&original).unwrap()).unwrap();
    std::fs::write(
        &credentials,
        serde_yaml::to_string(&original_credentials).unwrap(),
    )
    .unwrap();
    let (url, server) = platform(
        json!({"data":[{"id":"available","model_types":["TEXT_GENERATION"]}]}),
        2,
    )
    .await;
    let manager = ConnectionManager::new(root.path().join("state"), url.clone(), url);
    manager
        .connect(
            context(),
            ToolId::DeepSeekHarness,
            &inventory,
            profile.clone(),
        )
        .await
        .unwrap();
    server.await.unwrap();
    let mut changed = yaml(&profile);
    changed
        .as_array_mut()
        .unwrap()
        .insert(0, json!({"id":"new-plugin","config":{"manual":true}}));
    changed[1]["config"]["providers"]["tokensmarket"]["manual"] = json!(true);
    changed[1]["config"]["providers"]["new-provider"] = json!({"key":"new-secret"});
    std::fs::write(&profile, serde_yaml::to_string(&changed).unwrap()).unwrap();
    let mut changed_credentials = yaml(&credentials);
    changed_credentials["refs"]["NEW"] = json!("new-secret");
    changed_credentials["records"]["new-provider"] = json!({"key":"new-secret"});
    std::fs::write(
        &credentials,
        serde_yaml::to_string(&changed_credentials).unwrap(),
    )
    .unwrap();
    // Reopen from disk so receipts must preserve null and selector identity through serialization.
    let outcome = manager
        .disconnect(
            ToolId::DeepSeekHarness,
            "9001",
            None,
            DisconnectRequest::default(),
        )
        .await
        .unwrap();
    assert_eq!(outcome.state, "disconnected");
    assert!(manager.list().unwrap().is_empty());
    let mut expected = original.clone();
    expected
        .as_array_mut()
        .unwrap()
        .insert(0, json!({"id":"new-plugin","config":{"manual":true}}));
    expected[1]["config"]["providers"]["tokensmarket"]["manual"] = json!(true);
    expected[1]["config"]["providers"]["new-provider"] = json!({"key":"new-secret"});
    assert_eq!(yaml(&profile), expected);
    let mut expected_credentials = original_credentials;
    expected_credentials["refs"]["NEW"] = json!("new-secret");
    expected_credentials["records"]["new-provider"] = json!({"key":"new-secret"});
    assert_eq!(yaml(&credentials), expected_credentials);
}

#[tokio::test]
async fn harness_disconnect_removes_new_owned_profile_and_versioned_credential_bootstrap() {
    use tokensmarket_core::connections::DisconnectRequest;
    let root = tempdir().unwrap();
    let inventory = inventory(root.path());
    let profile = root.path().join("profiles/desktop/cordis.patch.yml");
    let credentials = root.path().join(".credentials.yaml");
    let (url, server) = platform(
        json!({"data":[{"id":"available","model_types":["TEXT_GENERATION"]}]}),
        2,
    )
    .await;
    let manager = ConnectionManager::new(root.path().join("state"), url.clone(), url);
    manager
        .connect(
            context(),
            ToolId::DeepSeekHarness,
            &inventory,
            profile.clone(),
        )
        .await
        .unwrap();
    server.await.unwrap();
    assert_eq!(
        yaml(&credentials),
        json!({"version":1,"refs":{"TOKEN_MARKET_API_KEY":"sk-fixture"},"records":{}})
    );
    let outcome = manager
        .disconnect(
            ToolId::DeepSeekHarness,
            "9001",
            None,
            DisconnectRequest::default(),
        )
        .await
        .unwrap();
    assert_eq!(outcome.state, "disconnected");
    assert!(!profile.exists());
    assert!(!credentials.exists());
    assert!(manager.list().unwrap().is_empty());
}
