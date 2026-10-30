use serde_json::json;
use tokenmarket_core::desktop::DesktopSession;
use tokenmarket_core::desktop_workspace::DesktopWorkspace;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn switching_saves_shared_preference_and_keeps_decimal_parent_account_and_agent_files() {
    let root = tempfile::tempdir().unwrap();
    let agent = root.path().join("config.toml");
    let original_config = "model_provider = \"tokenmarket\"\nmodel = \"existing-model\"\n[model_providers.tokenmarket]\nbase_url = \"https://bound-a.example/v1\"\nexperimental_bearer_token = \"fixture-unchanged-key\"\n";
    std::fs::write(&agent, original_config).unwrap();
    let record = root.path().join("keys.json");
    let records = json!([{"tool":"codex","keyId":"existing-key","boundWorkspace":"old-space"}]);
    std::fs::write(&record, serde_json::to_vec(&records).unwrap()).unwrap();
    let original_record = std::fs::read(&record).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let mut saved = false;
        for _ in 0..8 {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = [0; 8192];
            let n = socket.read(&mut bytes).await.unwrap();
            let request = String::from_utf8_lossy(&bytes[..n]);
            assert!(request
                .to_lowercase()
                .contains("authorization: bearer private-session"));
            let value = if request.starts_with("POST /api/client/workspaces/current") {
                assert!(request.contains("\"workspaceId\":\"22\""));
                assert!(request.to_lowercase().contains("x-workspace-id: 22"));
                saved = true;
                json!(true)
            } else if request.starts_with("GET /api/client/workspaces/context") {
                let id = if saved { "22" } else { "11" };
                json!({"currentWorkspace":{"id":id,"name":"Current","status":"ACTIVE"},"switchableWorkspaces":[{"id":"11","name":"Personal","status":"ACTIVE"},{"id":"22","name":"Child","parentId":"21","status":"ACTIVE"}],"billingWorkspaces":[{"id":"11","name":"Personal"},{"id":"22","name":"Child"}],"menus":[{"pageKey":"billing-account-summary","visible":true}]})
            } else if request.starts_with("GET /api/client/profile") {
                json!({"display_name":"Ada","email":"ada@example.test","avatar_url":"avatar_2","role":1})
            } else if request.starts_with("GET /api/client/workspaces/11/balance") {
                json!({"workspaceId":"11","balanceAmount":1.123456,"currency":"CNY"})
            } else {
                assert!(request.starts_with("GET /api/client/workspaces/22/balance"));
                serde_json::from_str(
                    r#"{"workspaceId":"21","balanceAmount":123456789.123456,"currency":"CNY"}"#,
                )
                .unwrap()
            };
            let body = json!({"success":true,"data":value}).to_string();
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
        }
    });
    let session: DesktopSession = serde_json::from_value(json!({"session_token":"private-session","expires_at":"2099-01-01T00:00:00Z","user":{"id":"1","name":"Ada"},"default_workspace":{"id":"11","name":"Personal"}})).unwrap();
    let workspace = DesktopWorkspace::new(base, "https://console.example.test".into());
    let initial = workspace.load(&session).await.unwrap();
    assert_eq!(initial.current_workspace.id, "11");
    assert_eq!(initial.balance.unwrap().amount, "1.123456");
    let next = workspace.switch(&session, "22").await.unwrap();
    assert_eq!(next.current_workspace.id, "22");
    let balance = next.balance.unwrap();
    assert_eq!(balance.account_workspace_id, "21");
    assert_eq!(balance.amount, "123456789.123456");
    assert_eq!(balance.currency, "CNY");
    assert_eq!(
        workspace.browser_url("1", "recharge").unwrap(),
        "https://console.example.test/desktop/recharge?user_id=1"
    );
    assert_eq!(std::fs::read_to_string(agent).unwrap(), original_config);
    assert_eq!(std::fs::read(record).unwrap(), original_record);
    server.await.unwrap();
}

#[tokio::test]
async fn delayed_old_balance_cannot_replace_new_current_workspace() {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    use tokenmarket_core::desktop_workspace::DesktopWorkspaceController;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let old_started = Arc::new(AtomicBool::new(false));
    let old_flag = old_started.clone();
    let server = tokio::spawn(async move {
        for _ in 0..8 {
            let (mut socket, _) = listener.accept().await.unwrap();
            let flag = old_flag.clone();
            tokio::spawn(async move {
                let mut bytes = [0; 8192];
                let n = socket.read(&mut bytes).await.unwrap();
                let request = String::from_utf8_lossy(&bytes[..n]);
                let value = if request.starts_with("POST") {
                    json!(true)
                } else if request.contains("/api/client/workspaces/context") {
                    let id = if request.contains("workspaceId=22") {
                        "22"
                    } else {
                        "11"
                    };
                    json!({"currentWorkspace":{"id":id,"name":id},"switchableWorkspaces":[{"id":"11","name":"A"},{"id":"22","name":"B"}],"billingWorkspaces":[],"menus":[]})
                } else if request.contains("/api/client/profile") {
                    json!({"display_name":"Ada"})
                } else if request.contains("/api/client/workspaces/11/balance") {
                    flag.store(true, Ordering::SeqCst);
                    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                    json!({"workspaceId":"11","balanceAmount":10,"currency":"USD"})
                } else {
                    json!({"workspaceId":"22","balanceAmount":20,"currency":"USD"})
                };
                let body = json!({"success":true,"data":value}).to_string();
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
            });
        }
    });
    let session: DesktopSession = serde_json::from_value(json!({"session_token":"private-session","expires_at":"2099-01-01T00:00:00Z","user":{"id":"1","name":"Ada"},"default_workspace":{"id":"11","name":"A"}})).unwrap();
    let controller = Arc::new(DesktopWorkspaceController::new(DesktopWorkspace::new(
        base,
        "https://console.example.test".into(),
    )));
    let first = controller.clone();
    let original = session.clone();
    let delayed = tokio::spawn(async move { first.refresh(&original).await.unwrap() });
    while !old_started.load(Ordering::SeqCst) {
        tokio::task::yield_now().await;
    }
    let switched = controller.switch(&session, "22").await.unwrap().unwrap();
    assert_eq!(switched.balance.unwrap().amount, "20");
    assert!(delayed.await.unwrap().is_none());
    assert_eq!(controller.current().unwrap().current_workspace.id, "22");
    server.await.unwrap();
}

#[tokio::test]
async fn forbidden_switch_never_saves_and_missing_currency_never_gets_a_guessed_label() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        for _ in 0..2 {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = [0; 4096];
            let n = socket.read(&mut bytes).await.unwrap();
            let request = String::from_utf8_lossy(&bytes[..n]);
            assert!(!request.starts_with("POST"));
            let value = if request.contains("/api/client/workspaces/context") {
                json!({"currentWorkspace":{"id":"11","name":"A"},"switchableWorkspaces":[{"id":"11","name":"A"},{"id":"22","name":"disabled","switchable":false}],"billingWorkspaces":[],"menus":[]})
            } else {
                json!({"workspaceId":"11","balanceAmount":"9007199254740993.123456"})
            };
            let body = json!({"success":true,"data":value}).to_string();
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
        }
    });
    let session: DesktopSession = serde_json::from_value(json!({"session_token":"private-session","expires_at":"2099-01-01T00:00:00Z","user":{"id":"1","name":"Ada"},"default_workspace":{"id":"11","name":"A"}})).unwrap();
    let client = DesktopWorkspace::new(base, "https://console.example.test".into());
    assert!(client
        .switch(&session, "22")
        .await
        .err()
        .unwrap()
        .to_string()
        .contains("不可切换"));
    assert!(client
        .balance(&session, "11")
        .await
        .err()
        .unwrap()
        .to_string()
        .contains("币种"));
    assert!(client.browser_url("11?token=secret", "recharge").is_err());
    assert!(client.browser_url("11", "https://evil.example").is_err());
    server.await.unwrap();
}
