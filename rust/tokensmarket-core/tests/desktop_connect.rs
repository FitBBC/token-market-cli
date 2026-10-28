use serde_json::json;
use tempfile::tempdir;
use tokensmarket_core::{
    connections::{ConnectContext, ConnectionManager},
    inventory::{HostPlatform, InstallationEnvironment, ToolInventory},
    ToolId,
};

#[tokio::test]
async fn malformed_config_is_preserved_before_any_key_is_requested() {
    let root = tempdir().unwrap();
    let bin = root.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    let executable = bin.join("claude");
    std::fs::write(&executable, "fixture").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let config = root.path().join("settings.json");
    std::fs::write(&config, "{broken").unwrap();
    let inventory = ToolInventory::new(InstallationEnvironment {
        platform: HostPlatform::MacOs,
        home: root.path().into(),
        command_directories: vec![bin],
        application_directories: vec![],
        executable_extensions: vec![],
    });
    let manager = ConnectionManager::new(
        root.path().join(".tokensmarket/desktop-connections"),
        "http://127.0.0.1:1".into(),
        "http://127.0.0.1:1".into(),
    );
    let result = manager
        .connect(
            ConnectContext {
                session_token: "private".into(),
                workspace_id: "42".into(),
            },
            ToolId::ClaudeCode,
            &inventory,
            config.clone(),
        )
        .await;
    assert!(result.unwrap_err().to_string().contains("配置"));
    assert_eq!(std::fs::read_to_string(config).unwrap(), "{broken");
    assert!(manager.list().unwrap().is_empty());
}

async fn platform(
    models: serde_json::Value,
    fail_key: bool,
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
async fn connect_uses_desktop_key_and_visible_model_without_generation_and_records_restore_receipts(
) {
    let root = tempdir().unwrap();
    let inventory = inventory(root.path());
    let config = root.path().join("settings.json");
    let original = json!({"permissions":{"allow":["Read"]},"env":{"OTHER_PROVIDER_KEY":"keep","ANTHROPIC_API_KEY":"old-provider"},"model":"other-model"});
    std::fs::write(&config, original.to_string()).unwrap();
    let (url, server) = platform(
        json!({"data":[{"id":"glm-5.3","context_window_tokens":202752}]}),
        false,
    )
    .await;
    let manager = ConnectionManager::new(
        root.path().join(".tokensmarket/desktop-connections"),
        url.clone(),
        url.clone(),
    );
    let device = manager.device_id().unwrap();
    let summary = manager
        .connect(context(), ToolId::ClaudeCode, &inventory, config.clone())
        .await
        .unwrap();
    assert_eq!(summary.state, "configured_untested");
    assert_eq!(
        inventory.list().unwrap()[0]
            .connection
            .as_ref()
            .unwrap()
            .state,
        "configured_untested"
    );
    assert_eq!(summary.owner["name"], "Ada");
    assert_eq!(summary.bound_workspace["name"], "Research");
    assert!(!serde_json::to_string(&summary)
        .unwrap()
        .contains("sk-fixture"));
    let applied: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&config).unwrap()).unwrap();
    assert_eq!(applied["permissions"], original["permissions"]);
    assert_eq!(applied["env"]["OTHER_PROVIDER_KEY"], "keep");
    assert_eq!(applied["model"], "glm-5.3");
    assert_eq!(applied["env"]["ANTHROPIC_API_KEY"], "sk-fixture");
    assert_eq!(applied["env"]["CLAUDE_CODE_MAX_CONTEXT_TOKENS"], "202752");
    let reopened = ConnectionManager::new(
        root.path().join(".tokensmarket/desktop-connections"),
        url.clone(),
        url,
    );
    assert_eq!(reopened.device_id().unwrap(), device);
    let records = reopened.list().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].device_id, device);
    assert_eq!(records[0].protocol, "ANTHROPIC_MESSAGES");
    let receipt = &records[0].receipts[0];
    assert_eq!(
        std::fs::read_to_string(receipt.backup_path.as_ref().unwrap()).unwrap(),
        original.to_string()
    );
    assert_eq!(
        receipt
            .fields
            .iter()
            .find(|f| f.pointer == "/env/ANTHROPIC_API_KEY")
            .unwrap()
            .before,
        Some(json!("old-provider"))
    );
    assert_eq!(
        server.await.unwrap(),
        vec![
            "POST /api/desktop/connections/keys HTTP/1.1",
            "GET /v1/models HTTP/1.1"
        ]
    );
}
#[tokio::test]
async fn missing_required_model_preserves_config_and_cleans_only_unused_issued_key() {
    let root = tempdir().unwrap();
    let inventory = inventory(root.path());
    let config = root.path().join("settings.json");
    std::fs::write(&config, "{\"model\":\"keep\"}").unwrap();
    let (url, server) = platform(json!({"data":[{"id":"other-model"}]}), false).await;
    let manager = ConnectionManager::new(
        root.path().join(".tokensmarket/desktop-connections"),
        url.clone(),
        url,
    );
    assert!(manager
        .connect(context(), ToolId::ClaudeCode, &inventory, config.clone())
        .await
        .unwrap_err()
        .to_string()
        .contains("glm-5.3"));
    assert_eq!(
        std::fs::read_to_string(config).unwrap(),
        "{\"model\":\"keep\"}"
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

#[tokio::test]
async fn key_issuance_failure_and_missing_installation_leave_existing_files_and_records_untouched()
{
    let root = tempdir().unwrap();
    let inventory = inventory(root.path());
    let config = root.path().join("settings.json");
    std::fs::write(&config, "{\"model\":\"keep\"}").unwrap();
    let (url, server) = platform(json!({"data":[]}), true).await;
    let manager = ConnectionManager::new(
        root.path().join(".tokensmarket/desktop-connections"),
        url.clone(),
        url,
    );
    assert!(manager
        .connect(context(), ToolId::ClaudeCode, &inventory, config.clone())
        .await
        .is_err());
    assert_eq!(
        std::fs::read_to_string(&config).unwrap(),
        "{\"model\":\"keep\"}"
    );
    assert!(manager.list().unwrap().is_empty());
    assert_eq!(server.await.unwrap().len(), 1);
    std::fs::remove_file(root.path().join("bin/claude")).unwrap();
    assert!(manager
        .connect(context(), ToolId::ClaudeCode, &inventory, config.clone())
        .await
        .unwrap_err()
        .to_string()
        .contains("未安装"));
    assert_eq!(
        std::fs::read_to_string(&config).unwrap(),
        "{\"model\":\"keep\"}"
    );
}
#[cfg(unix)]
#[tokio::test]
async fn config_write_failure_keeps_prior_config_and_cleans_the_new_unused_key() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempdir().unwrap();
    let inventory = inventory(root.path());
    let directory = root.path().join("read-only");
    std::fs::create_dir(&directory).unwrap();
    let config = directory.join("settings.json");
    std::fs::write(&config, "{\"model\":\"keep\"}").unwrap();
    std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o500)).unwrap();
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
    let result = manager
        .connect(context(), ToolId::ClaudeCode, &inventory, config.clone())
        .await;
    std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(result.is_err());
    assert_eq!(
        std::fs::read_to_string(&config).unwrap(),
        "{\"model\":\"keep\"}"
    );
    assert!(manager.list().unwrap().is_empty());
    assert_eq!(
        server.await.unwrap().last().unwrap(),
        "POST /api/desktop/connections/keys/9001/disable HTTP/1.1"
    );
}
