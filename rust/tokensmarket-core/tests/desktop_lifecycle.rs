use anyhow::Result;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use tempfile::tempdir;
use tokensmarket_core::desktop::{DesktopClient, DesktopCredentialStore, DesktopLogin};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Default)]
struct PrivateStore(Mutex<Option<String>>);
impl DesktopCredentialStore for PrivateStore {
    fn read(&self) -> Result<Option<String>> {
        Ok(self.0.lock().unwrap().clone())
    }
    fn write(&self, value: &str) -> Result<()> {
        *self.0.lock().unwrap() = Some(value.into());
        Ok(())
    }
    fn delete(&self) -> Result<()> {
        *self.0.lock().unwrap() = None;
        Ok(())
    }
}

async fn platform(replies: Vec<(&'static str, Value)>) -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        for (path, response) in replies {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = vec![];
            loop {
                let mut chunk = [0; 4096];
                let count = stream.read(&mut chunk).await.unwrap();
                request.extend_from_slice(&chunk[..count]);
                if let Some(end) = request.windows(4).position(|x| x == b"\r\n\r\n") {
                    let header = String::from_utf8_lossy(&request[..end]);
                    let length = header
                        .lines()
                        .find_map(|line| {
                            line.to_lowercase()
                                .strip_prefix("content-length:")
                                .map(|v| v.trim().parse::<usize>().unwrap())
                        })
                        .unwrap_or(0);
                    if request.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            let text = String::from_utf8_lossy(&request).to_lowercase();
            assert!(text.starts_with(&format!("post {path} ")));
            assert!(text.contains("authorization: bearer dedicated-desktop-secret"));
            let body = response.to_string();
            stream
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
        }
    });
    (base, task)
}

fn saved() -> String {
    json!({"session_token":"dedicated-desktop-secret", "expires_at":"2098-01-01T00:00:00Z", "user":{"id":"old-user","name":"Old account"},"default_workspace":{"id":"old-space","name":"Old workspace"},"web_origin":"https://test.tokensapi.cn"}).to_string()
}
fn context() -> Value {
    json!({"success":true,"data":{"expires_at":"2099-01-01T00:00:00Z","user":{"id":"old-user","name":"Old account"},"default_workspace":{"id":"new-default","name":"Updated default"}}})
}

#[tokio::test]
async fn restart_renews_private_desktop_credential_and_leaves_existing_connections_untouched() {
    let root = tempdir().unwrap();
    let connection = root.path().join("connections.json");
    let agent = root.path().join("settings.json");
    let record = br#"{"user":"owner-1","bound_workspace":"billing-1","key_id":"key-1","api_key":"agent-key"}"#;
    std::fs::write(&connection, record).unwrap();
    std::fs::write(&agent, b"old agent config").unwrap();
    let store = Arc::new(PrivateStore::default());
    store.write(&saved()).unwrap();
    let (base, server) = platform(vec![("/api/desktop/auth/renew", context())]).await;
    let app = DesktopLogin::with_credentials(
        root.path().join("desktop-login"),
        DesktopClient::new(base.clone()),
        store.clone(),
    );
    drop(app);
    let restarted = DesktopLogin::with_credentials(
        root.path().join("desktop-login"),
        DesktopClient::new(base),
        store.clone(),
    );
    let session = restarted.restore().await.unwrap().unwrap();
    assert_eq!(session.default_workspace.id, "new-default");
    assert_eq!(
        session.web_origin.as_deref(),
        Some("https://test.tokensapi.cn")
    );
    assert_eq!(session.expires_at, "2099-01-01T00:00:00Z");
    assert_eq!(std::fs::read(connection).unwrap(), record);
    assert_eq!(std::fs::read(agent).unwrap(), b"old agent config");
    assert!(!root.path().join("desktop-login/session.json").exists());
    assert!(!format!("{session:?}").contains("dedicated-desktop-secret"));
    server.await.unwrap();
}

#[tokio::test]
async fn logout_revokes_only_desktop_login_and_account_change_preserves_original_tool_ownership() {
    let root = tempdir().unwrap();
    let connection = root.path().join("connections.json");
    let config = root.path().join("agent.json");
    let record = br#"{"owner_account":"original-owner","bound_workspace":"original-billing","key":"original-agent-key"}"#;
    std::fs::write(&connection, record).unwrap();
    std::fs::write(&config, b"original agent config").unwrap();
    let store = Arc::new(PrivateStore::default());
    store.write(&saved()).unwrap();
    let (base, server) = platform(vec![(
        "/api/desktop/auth/logout",
        json!({"success":true,"data":true}),
    )])
    .await;
    let app = DesktopLogin::with_credentials(
        root.path().join("login"),
        DesktopClient::new(base),
        store.clone(),
    );
    assert!(app.logout().await.unwrap());
    assert!(app.restore().await.unwrap().is_none());
    assert!(store.read().unwrap().is_none());
    server.await.unwrap();

    // An independent browser login establishes account B, never migrating the retained connection.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let login_server = tokio::spawn(async move {
        for mut payload in [
            json!({"session_id":"second-login","authorize_url":"https://tokensmarket.ai/desktop/authorize","expires_at":"2099-01-01T00:00:00Z"}),
            json!({"session_token":"account-b-desktop-secret","expires_at":"2099-01-01T00:00:00Z","user":{"id":"account-b","name":"Second account"},"default_workspace":{"id":"space-b","name":"Second workspace"}}),
        ] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buffer = [0; 4096];
            let count = stream.read(&mut buffer).await.unwrap();
            if payload.get("session_id").is_some() {
                let request = String::from_utf8_lossy(&buffer[..count]);
                let body: Value =
                    serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap()).unwrap();
                payload["authorize_url"] = json!(format!(
                    "https://tokensmarket.ai/desktop/authorize?state={}",
                    body["state"].as_str().unwrap()
                ));
            }
            let body = json!({"success":true,"data":payload}).to_string();
            stream
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
        }
    });
    let second = DesktopLogin::with_credentials(
        root.path().join("login"),
        DesktopClient::new(base),
        store.clone(),
    );
    let attempt = second.begin().await.unwrap();
    let state = url::Url::parse(&attempt.authorize_url)
        .unwrap()
        .query_pairs()
        .find(|(key, _)| key == "state")
        .unwrap()
        .1
        .to_string();
    let account_b = second
        .complete(&format!(
            "tokenmarket://auth/callback?session_id=second-login&state={state}&code=one-use"
        ))
        .await
        .unwrap();
    assert_eq!(account_b.user.id, "account-b");
    assert_eq!(std::fs::read(connection).unwrap(), record);
    assert_eq!(std::fs::read(config).unwrap(), b"original agent config");
    assert!(store
        .read()
        .unwrap()
        .unwrap()
        .contains("account-b-desktop-secret"));
    login_server.await.unwrap();
}

#[tokio::test]
async fn revoked_or_unrenewable_credentials_return_to_login_and_do_not_reappear_after_restart() {
    let root = tempdir().unwrap();
    let config = root.path().join("agent.json");
    std::fs::write(&config, b"retained agent-key and bound workspace").unwrap();
    let store = Arc::new(PrivateStore::default());
    store.write(&saved()).unwrap();
    let (base, server) = platform(vec![(
        "/api/desktop/auth/renew",
        json!({"success":false,"message":"revoked"}),
    )])
    .await;
    let app = DesktopLogin::with_credentials(
        root.path().join("login"),
        DesktopClient::new(base.clone()),
        store.clone(),
    );
    assert!(app.restore().await.is_err());
    drop(app);
    let restart = DesktopLogin::with_credentials(
        root.path().join("login"),
        DesktopClient::new(base),
        store.clone(),
    );
    assert!(restart.restore().await.unwrap().is_none());
    assert!(store.read().unwrap().is_none());
    assert_eq!(
        std::fs::read(config).unwrap(),
        b"retained agent-key and bound workspace"
    );
    server.await.unwrap();
}

#[tokio::test]
async fn expired_credential_cannot_be_shown_as_signed_in_or_sent_for_renewal() {
    let root = tempdir().unwrap();
    let store = Arc::new(PrivateStore::default());
    store
        .write(&saved().replace("2098-01-01", "2000-01-01"))
        .unwrap();
    let app = DesktopLogin::with_credentials(
        root.path().join("login"),
        DesktopClient::new("http://127.0.0.1:1".into()),
        store.clone(),
    );
    assert!(app
        .restore()
        .await
        .unwrap_err()
        .to_string()
        .contains("已过期"));
    assert!(store.read().unwrap().is_none());
}

#[tokio::test]
async fn offline_logout_finishes_locally_and_does_not_claim_remote_revocation() {
    let root = tempdir().unwrap();
    let store = Arc::new(PrivateStore::default());
    store.write(&saved()).unwrap();
    let (base, server) =
        platform(vec![("/api/desktop/auth/logout", json!({"success":false}))]).await;
    let app = DesktopLogin::with_credentials(
        root.path().join("login"),
        DesktopClient::new(base),
        store.clone(),
    );
    assert!(!app.logout().await.unwrap());
    assert!(app.restore().await.unwrap().is_none());
    server.await.unwrap();
}

#[tokio::test]
async fn discarded_callback_cannot_restore_login_or_remove_a_newer_account_credential() {
    let root = tempdir().unwrap();
    let store = Arc::new(PrivateStore::default());
    store.write(&saved()).unwrap();
    let (base, server) = platform(vec![("/api/desktop/auth/renew", context())]).await;
    let app = DesktopLogin::with_credentials(
        root.path().join("login"),
        DesktopClient::new(base),
        store.clone(),
    );
    let discarded = app.restore().await.unwrap().unwrap();
    app.discard_unaccepted_session(&discarded).unwrap();
    assert!(app.restore().await.unwrap().is_none());
    // A later account's credential is outside the stale callback's authority.
    let next = saved().replace("dedicated-desktop-secret", "newer-account-secret");
    store.write(&next).unwrap();
    app.discard_unaccepted_session(&discarded).unwrap();
    assert_eq!(store.read().unwrap().unwrap(), next);
    server.await.unwrap();
}
