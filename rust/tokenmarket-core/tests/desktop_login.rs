use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tempfile::tempdir;
use tokenmarket_core::desktop::{DesktopClient, DesktopLogin};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn platform() -> (String, tokio::task::JoinHandle<Vec<Value>>) {
    platform_with("2099-01-01T00:00:00Z", true, 0).await
}

async fn platform_with(
    expiry: &'static str,
    exchange_success: bool,
    delay_ms: u64,
) -> (String, tokio::task::JoinHandle<Vec<Value>>) {
    let server = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("http://{}", server.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let mut requests: Vec<Value> = vec![];
        for index in 0..2 {
            let (mut socket, _) = server.accept().await.unwrap();
            let mut data = vec![];
            let body;
            loop {
                let mut chunk = [0u8; 4096];
                let n = socket.read(&mut chunk).await.unwrap();
                data.extend_from_slice(&chunk[..n]);
                if let Some(end) = data.windows(4).position(|x| x == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&data[..end]);
                    let length: usize = headers
                        .lines()
                        .find_map(|line| {
                            line.to_lowercase()
                                .strip_prefix("content-length:")
                                .map(|v| v.trim().parse().unwrap())
                        })
                        .unwrap_or(0);
                    if data.len() >= end + 4 + length {
                        assert!(headers.starts_with(if index == 0 {
                            "POST /api/desktop/auth/sessions "
                        } else {
                            "POST /api/desktop/auth/exchange "
                        }));
                        body = serde_json::from_slice::<Value>(&data[end + 4..end + 4 + length])
                            .unwrap();
                        break;
                    }
                }
            }
            let response = if index == 0 {
                json!({"session_id":"attempt-1","authorize_url":format!("https://tokenmarket.ai/desktop/authorize?session_id=attempt-1&state={}", body["state"].as_str().unwrap()),"expires_at":expiry})
            } else {
                assert_eq!(body["session_id"], "attempt-1");
                assert_eq!(body["code"], "one-use");
                let verifier = body["code_verifier"].as_str().unwrap();
                assert!(verifier.len() >= 43);
                assert_eq!(
                    requests[0]["code_challenge"],
                    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
                );
                assert_eq!(body["state"], requests[0]["state"]);
                json!({"session_token":"desktop-secret","expires_at":"2099-01-01T00:00:00Z","user":{"id":"u1","name":"Ada"},"default_workspace":{"id":"w2","name":"Team"}})
            };
            requests.push(body);
            if index == 1 {
                tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
            }
            let text =
                json!({"success":index == 0 || exchange_success,"data":response}).to_string();
            socket
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        text.len(),
                        text
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
        }
        requests
    });
    (address, task)
}

#[tokio::test]
async fn login_survives_a_cold_start_and_exchanges_once_for_default_workspace() {
    let root = tempdir().unwrap();
    let (base, platform) = platform().await;
    let login = DesktopLogin::new(root.path().into(), DesktopClient::new(base.clone()));
    let attempt = login.begin().await.unwrap();
    let url = url::Url::parse(&attempt.authorize_url).unwrap();
    let state = url
        .query_pairs()
        .find(|(key, _)| key == "state")
        .unwrap()
        .1
        .to_string();
    drop(login);
    let login = DesktopLogin::new(root.path().into(), DesktopClient::new(base));
    let callback =
        format!("tokenmarket://auth/callback?session_id=attempt-1&state={state}&code=one-use");
    let session = login.complete(&callback).await.unwrap();
    assert_eq!(session.user.name, "Ada");
    assert_eq!(session.default_workspace.id, "w2");
    assert_eq!(session.session_token, "desktop-secret");
    assert!(login.complete(&callback).await.is_err());
    let requests = platform.await.unwrap();
    assert_eq!(requests[0]["code_challenge_method"], "S256");
    assert!(requests[0].get("tool").is_none());
    assert!(requests[0].get("workspace_id").is_none());
    assert!(!format!("{session:?}").contains("desktop-secret"));
}

#[tokio::test]
async fn mismatched_callbacks_do_not_consume_the_attempt_and_cancelled_callbacks_cannot_login() {
    let root = tempdir().unwrap();
    let (base, platform) = platform().await;
    let login = DesktopLogin::new(root.path().into(), DesktopClient::new(base));
    let attempt = login.begin().await.unwrap();
    let state = url::Url::parse(&attempt.authorize_url)
        .unwrap()
        .query_pairs()
        .find(|(k, _)| k == "state")
        .unwrap()
        .1
        .to_string();
    for callback in [
        "tokenmarket://auth/callback?session_id=other&state=wrong&code=one-use".into(),
        format!("https://auth/callback?session_id=attempt-1&state={state}&code=one-use"),
        format!("tokenmarket://auth/callback?session_id=attempt-1&state={state}&state={state}&code=one-use"),
    ] {
        assert!(login.complete(&callback).await.is_err());
        assert!(login.pending().unwrap().is_some());
    }
    login.cancel().unwrap();
    assert!(login.pending().unwrap().is_none());
    assert!(login
        .complete(&format!(
            "tokenmarket://auth/callback?session_id=attempt-1&state={state}&code=one-use"
        ))
        .await
        .is_err());
    platform.abort();
}

#[tokio::test]
async fn authorization_error_clears_the_request_without_creating_a_session() {
    let root = tempdir().unwrap();
    let (base, platform) = platform().await;
    let login = DesktopLogin::new(root.path().into(), DesktopClient::new(base));
    let attempt = login.begin().await.unwrap();
    let state = url::Url::parse(&attempt.authorize_url)
        .unwrap()
        .query_pairs()
        .find(|(k, _)| k == "state")
        .unwrap()
        .1
        .to_string();
    assert!(login
        .complete(&format!(
            "tokenmarket://auth/callback?session_id=attempt-1&state={state}&error=denied"
        ))
        .await
        .is_err());
    assert!(login.pending().unwrap().is_none());
    platform.abort();
}

#[tokio::test]
async fn expired_attempt_and_server_rejected_code_do_not_establish_a_session() {
    let root = tempdir().unwrap();
    let (base, platform) = platform_with("2000-01-01T00:00:00Z", true, 0).await;
    let login = DesktopLogin::new(root.path().into(), DesktopClient::new(base));
    let attempt = login.begin().await.unwrap();
    let state = url::Url::parse(&attempt.authorize_url)
        .unwrap()
        .query_pairs()
        .find(|(k, _)| k == "state")
        .unwrap()
        .1
        .to_string();
    assert!(login
        .complete(&format!(
            "tokenmarket://auth/callback?session_id=attempt-1&state={state}&code=one-use"
        ))
        .await
        .is_err());
    assert!(login.pending().unwrap().is_none());
    platform.abort();

    let (base, platform) = platform_with("2099-01-01T00:00:00Z", false, 0).await;
    let login = DesktopLogin::new(root.path().into(), DesktopClient::new(base));
    let attempt = login.begin().await.unwrap();
    let state = url::Url::parse(&attempt.authorize_url)
        .unwrap()
        .query_pairs()
        .find(|(k, _)| k == "state")
        .unwrap()
        .1
        .to_string();
    assert!(login
        .complete(&format!(
            "tokenmarket://auth/callback?session_id=attempt-1&state={state}&code=one-use"
        ))
        .await
        .is_err());
    assert!(login.pending().unwrap().is_none());
    platform.await.unwrap();
}

#[tokio::test]
async fn cancellation_during_exchange_does_not_accept_a_late_session() {
    let root = tempdir().unwrap();
    let (base, platform) = platform_with("2099-01-01T00:00:00Z", true, 50).await;
    let login = std::sync::Arc::new(DesktopLogin::new(
        root.path().into(),
        DesktopClient::new(base),
    ));
    let attempt = login.begin().await.unwrap();
    let state = url::Url::parse(&attempt.authorize_url)
        .unwrap()
        .query_pairs()
        .find(|(k, _)| k == "state")
        .unwrap()
        .1
        .to_string();
    let cloned = login.clone();
    let exchange = tokio::spawn(async move {
        cloned
            .complete(&format!(
                "tokenmarket://auth/callback?session_id=attempt-1&state={state}&code=one-use"
            ))
            .await
    });
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while login.pending().unwrap().is_some() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    login.cancel().unwrap();
    assert!(exchange.await.unwrap().is_err());
    platform.await.unwrap();
}
