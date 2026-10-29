use tempfile::tempdir;
use tokensmarket_core::desktop::{DesktopClient, DesktopLogin};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn login_distinguishes_http_failures_from_invalid_json_without_exposing_response() {
    for (status, body, expected) in [
        ("404 Not Found", "secret route detail", "HTTP 404"),
        (
            "503 Service Unavailable",
            "secret upstream detail",
            "HTTP 503",
        ),
        ("200 OK", "secret invalid json", "登录服务返回无效响应"),
        (
            "200 OK",
            "{\"success\":false}",
            "登录授权无效、已过期或已使用",
        ),
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut input = [0u8; 4096];
            let _ = socket.read(&mut input).await.unwrap();
            socket.write_all(format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
        });
        let dir = tempdir().unwrap();
        let login = DesktopLogin::new(
            dir.path().into(),
            DesktopClient::new(format!("http://{address}")),
        );
        let error = match login.begin().await {
            Ok(_) => panic!("unexpected login success"),
            Err(error) => error.to_string(),
        };
        assert!(error.contains(expected), "{status}: {error}");
        assert!(!error.contains("secret"));
        task.await.unwrap();
    }
}
