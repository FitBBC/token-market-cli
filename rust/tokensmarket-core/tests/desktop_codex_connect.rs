use serde_json::json;
use tempfile::tempdir;
use tokensmarket_core::{
    connections::{ConnectContext, ConnectionManager},
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
                assert!(request.contains("\"tool\":\"codex\""));
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
    let executable = bin.join("codex");
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
async fn codex_connect_configures_responses_and_records_bound_workspace_without_a_model_request() {
    let root = tempdir().unwrap();
    let inventory = inventory(root.path());
    let config = root.path().join("config.toml");
    let original = "# personal settings\nmodel = \"old-model\" # chosen before Token Market\nmodel_provider = \"other\"\n\n[model_providers.other] # retain provider\nname = \"Other\"\nbase_url = \"https://other.example/v1\"\n\n[model_providers.tokensmarket]\nname = \"Previous\"\nexperimental_bearer_token = \"old-token\"\nrequest_max_retries = 8 # retain tuning\n\n[projects.\"/sample\"]\ntrust_level = \"trusted\"\n";
    std::fs::write(&config, original).unwrap();
    let (url, server) = platform(json!({"data":[{"id":"glm-5.3"}]}), false).await;
    let manager = ConnectionManager::new(
        root.path().join(".tokensmarket/desktop-connections"),
        url.clone(),
        url.clone(),
    );
    let summary = manager
        .connect(context(), ToolId::Codex, &inventory, config.clone())
        .await
        .unwrap();
    assert_eq!(summary.state, "configured_untested");
    assert_eq!(summary.owner["name"], "Ada");
    assert_eq!(summary.bound_workspace["name"], "Research");
    assert_eq!(summary.models, vec!["glm-5.3"]);
    assert!(!serde_json::to_string(&summary)
        .unwrap()
        .contains("sk-fixture"));
    let output = std::fs::read_to_string(&config).unwrap();
    let applied: toml_edit::DocumentMut = output.parse().unwrap();
    assert_eq!(applied["model"].as_str(), Some("glm-5.3"));
    assert_eq!(applied["model_provider"].as_str(), Some("tokensmarket"));
    let provider = &applied["model_providers"]["tokensmarket"];
    assert_eq!(provider["wire_api"].as_str(), Some("responses"));
    assert_eq!(
        provider["base_url"].as_str(),
        Some(format!("{url}/v1").as_str())
    );
    assert_eq!(
        provider["experimental_bearer_token"].as_str(),
        Some("sk-fixture")
    );
    assert_eq!(provider["requires_openai_auth"].as_bool(), Some(false));
    assert_eq!(provider["supports_websockets"].as_bool(), Some(false));
    assert!(output.contains("# chosen before Token Market"));
    assert!(output.contains("# retain provider"));
    assert!(output.contains("request_max_retries = 8 # retain tuning"));
    assert_eq!(
        applied["model_providers"]["other"]["base_url"].as_str(),
        Some("https://other.example/v1")
    );
    assert_eq!(
        applied["projects"]["/sample"]["trust_level"].as_str(),
        Some("trusted")
    );
    let records = manager.list().unwrap();
    assert_eq!(records[0].tool, ToolId::Codex);
    assert_eq!(records[0].protocol, "OPENAI_RESPONSES");
    assert_eq!(records[0].credential(), "sk-fixture");
    let receipt = &records[0].receipts[0];
    assert_eq!(
        std::fs::read_to_string(receipt.backup_path.as_ref().unwrap()).unwrap(),
        original
    );
    let field = receipt
        .fields
        .iter()
        .find(|f| f.pointer == "/model_providers/tokensmarket/experimental_bearer_token")
        .unwrap();
    assert_eq!(field.before, Some(json!("old-token")));
    assert_eq!(field.applied, json!("sk-fixture"));
    assert_eq!(
        server.await.unwrap(),
        vec![
            "POST /api/desktop/connections/keys HTTP/1.1",
            "GET /v1/models HTTP/1.1"
        ]
    );
}
