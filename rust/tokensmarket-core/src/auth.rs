use anyhow::{Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::adapters::ToolId;
use crate::config::console_api_base_url;

#[derive(Debug, Clone)]
pub struct Pkce {
    pub code_verifier: String,
    pub code_challenge: String,
    pub code_challenge_method: &'static str,
}

pub fn create_pkce() -> Pkce {
    let mut bytes = [0u8; 48];
    rand::thread_rng().fill_bytes(&mut bytes);
    let code_verifier = URL_SAFE_NO_PAD.encode(bytes);
    let digest = Sha256::digest(code_verifier.as_bytes());
    Pkce {
        code_verifier,
        code_challenge: URL_SAFE_NO_PAD.encode(digest),
        code_challenge_method: "S256",
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupSession {
    pub setup_id: String,
    pub authorize_url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRequest<'a> {
    pub tool: ToolId,
    pub redirect_port: u16,
    pub code_challenge: &'a str,
    pub code_challenge_method: &'a str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExchangeRequest<'a> {
    pub setup_id: &'a str,
    pub code: &'a str,
    pub code_verifier: &'a str,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExchangeResult {
    #[serde(rename = "apiKey")]
    pub api_key: String,
    #[serde(rename = "keyId")]
    pub key_id: String,
    pub name: String,
    #[serde(rename = "expiresAt", default)]
    pub expires_at: Option<String>,
}

pub struct TokenMarketClient {
    base_url: String,
    http: reqwest::Client,
}

impl TokenMarketClient {
    pub fn new() -> Self {
        Self::with_base_url(console_api_base_url())
    }

    pub fn with_base_url(base_url: String) -> Self {
        Self {
            base_url,
            http: reqwest::Client::new(),
        }
    }

    pub async fn create_setup_session(&self, request: &SessionRequest<'_>) -> Result<SetupSession> {
        self.request("/api/cli/sessions", request).await
    }

    pub async fn exchange(&self, request: &ExchangeRequest<'_>) -> Result<ExchangeResult> {
        self.request("/api/cli/exchange", request).await
    }

    async fn request<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        body: &impl Serialize,
    ) -> Result<T> {
        let url = format!(
            "{}/{}",
            self.base_url.trim_end_matches('/'),
            path.trim_start_matches('/')
        );
        let response = self
            .http
            .post(url)
            .json(body)
            .send()
            .await
            .context("Token Market request failed")?;
        let status = response.status();
        let text = response.text().await?;
        let json: serde_json::Value =
            serde_json::from_str(&text).unwrap_or(serde_json::Value::Null);
        if !status.is_success() {
            let message = json
                .get("error_description")
                .or_else(|| json.get("message"))
                .or_else(|| json.get("error"))
                .and_then(|value| value.as_str())
                .unwrap_or("Token Market request failed");
            anyhow::bail!("{message} (HTTP {status})");
        }
        if json.get("success").and_then(|value| value.as_bool()) == Some(false) {
            anyhow::bail!(
                "{}",
                json.get("message")
                    .and_then(|value| value.as_str())
                    .unwrap_or("Token Market request failed")
            );
        }
        let data = json.get("data").cloned().unwrap_or(json);
        serde_json::from_value(data).with_context(|| "parse Token Market response")
    }
}

impl Default for TokenMarketClient {
    fn default() -> Self {
        Self::new()
    }
}

pub struct CallbackServer {
    listener: tokio::net::TcpListener,
}

#[derive(Debug, Clone)]
pub struct CallbackResult {
    pub code: String,
    pub state: Option<String>,
}

impl CallbackServer {
    pub async fn bind() -> Result<Self> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .context("bind local callback server")?;
        Ok(Self { listener })
    }

    pub fn port(&self) -> u16 {
        self.listener.local_addr().expect("listener address").port()
    }

    pub async fn wait_for_result(self) -> Result<CallbackResult> {
        let (mut socket, _) = self
            .listener
            .accept()
            .await
            .context("accept authorization callback")?;
        let mut buffer = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            let read = socket.read(&mut chunk).await?;
            if read == 0 {
                break;
            }
            buffer.extend_from_slice(&chunk[..read]);
            if buffer.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
        }
        let request = String::from_utf8_lossy(&buffer).to_string();
        let path = request
            .lines()
            .next()
            .and_then(|line| line.split(' ').nth(1))
            .unwrap_or_default();
        let query = path
            .split_once('?')
            .map(|(_, query)| query)
            .unwrap_or_default();
        let mut code = None;
        let mut state = None;
        let mut error = None;
        let mut error_description = None;
        for pair in query.split('&').filter(|pair| !pair.is_empty()) {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            match key {
                "code" => code = Some(url_decode(value)),
                "state" => state = Some(url_decode(value)),
                "error" => error = Some(url_decode(value)),
                "error_description" => error_description = Some(url_decode(value)),
                _ => {}
            }
        }

        if let Some(error) = error {
            let message = error_description.as_deref().unwrap_or(error.as_str());
            write_callback_response(&mut socket, false, &format!("授权失败：{message}")).await?;
            anyhow::bail!("authorization failed: {message}");
        }

        write_callback_response(&mut socket, true, "授权完成。").await?;
        let code = code.context("authorization callback did not contain code")?;
        Ok(CallbackResult { code, state })
    }
}

async fn write_callback_response(
    socket: &mut tokio::net::TcpStream,
    success: bool,
    message: &str,
) -> Result<()> {
    let status_text = if success {
        "Token Market CLI"
    } else {
        "Token Market CLI — 授权失败"
    };
    let status_class = if success { "is-success" } else { "is-failed" };
    let body = format!(
        r#"<!doctype html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<style>
:root {{ color-scheme: light; }}
* {{ box-sizing: border-box; }}
body {{ margin:0; min-height:100vh; display:grid; place-items:center; padding:24px; font:14px/1.5 -apple-system,BlinkMacSystemFont,"Segoe UI","PingFang SC","Microsoft YaHei",sans-serif; color:#0f172a; background:radial-gradient(circle at 12% 8%,rgba(59,130,246,.16),transparent 34%),radial-gradient(circle at 88% 12%,rgba(16,185,129,.14),transparent 32%),#f4f6fb; }}
.card {{ width:min(420px,100%); overflow:hidden; border:1px solid #dbe1ea; border-radius:18px; background:rgba(255,255,255,.92); box-shadow:0 24px 80px rgba(15,23,42,.12); backdrop-filter:blur(18px); }}
.header {{ display:flex; align-items:center; justify-content:space-between; padding:18px 22px; border-bottom:1px solid #e8edf4; background:linear-gradient(90deg,rgba(239,246,255,.95),rgba(255,255,255,0)); }}
.brand {{ display:flex; align-items:center; gap:12px; }}
.brand h1 {{ margin:0; font-size:17px; font-weight:650; letter-spacing:-.01em; }}
.brand p {{ margin:3px 0 0; color:#64748b; font-size:13px; }}
.badge {{ display:inline-flex; align-items:center; justify-content:center; width:38px; height:38px; border-radius:12px; }}
.badge.is-success {{ color:#059669; background:#ecfdf5; border:1px solid #a7f3d0; }}
.badge.is-failed {{ color:#dc2626; background:#fef2f2; border:1px solid #fecaca; }}
.body {{ padding:22px; }}
.status {{ display:flex; align-items:center; gap:10px; padding:14px 16px; border-radius:12px; background:#f8fafc; border:1px solid #e2e8f0; color:#334155; font-weight:500; }}
.status strong {{ color:#0f172a; }}
.hint {{ margin:16px 0 0; color:#64748b; font-size:12px; line-height:1.7; }}
.steps {{ margin:14px 0 0; padding:0; list-style:none; color:#64748b; font-size:12px; }}
.steps li {{ display:flex; align-items:center; gap:8px; margin-top:8px; }}
.steps span {{ width:16px; height:16px; border-radius:50%; background:#2563eb; color:white; display:inline-flex; align-items:center; justify-content:center; font-size:10px; }}
</style>
</head>
<body>
<main class="card">
  <header class="header">
    <div class="brand">
      <span class="badge {status_class}" aria-hidden="true">{icon}</span>
      <div>
        <h1>{title}</h1>
        <p>本机授权回调</p>
      </div>
    </div>
  </header>
  <section class="body">
    <div class="status">{status}</div>
    {steps}
    <p class="hint">如果页面没有自动关闭，你可以手动关闭这个标签页。</p>
  </section>
</main>
<script>
(function closeTab() {{
  function requestClose() {{
    try {{ window.close(); }} catch (_) {{}}
    try {{ window.open('', '_self').close(); }} catch (_) {{}}
  }}
  requestClose();
  setTimeout(requestClose, 200);
  setTimeout(requestClose, 800);
}})();
</script>
</body>
</html>"#,
        title = escape_html(status_text),
        status_class = status_class,
        icon = if success { "✓" } else { "!" },
        status = if success {
            "<strong>授权完成。</strong>".to_string()
        } else {
            format!("<strong>{}</strong>", escape_html(message))
        },
        steps = if success {
            r#"<p class="hint">CLI 正在继续完成后续步骤：</p><ul class="steps"><li><span>1</span>创建 API Key</li><li><span>2</span>写入工具配置</li><li><span>3</span>发送测试请求</li></ul>"#.to_string()
        } else {
            String::new()
        },
    );

    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Security-Policy: default-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline'\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
        body.len(),
        body
    );
    socket.write_all(response.as_bytes()).await?;
    socket.flush().await?;
    Ok(())
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn url_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or("");
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    output.push(byte);
                    index += 3;
                } else {
                    output.push(b'%');
                    index += 1;
                }
            }
            b'+' => {
                output.push(b' ');
                index += 1;
            }
            byte => {
                output.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&output).to_string()
}

pub async fn authorize_and_exchange(
    tool: ToolId,
    client: &TokenMarketClient,
    mut open_url: impl FnMut(&str) -> Result<()> + Send,
) -> Result<(SetupSession, ExchangeResult)> {
    let pkce = create_pkce();
    let server = CallbackServer::bind().await?;
    let session = client
        .create_setup_session(&SessionRequest {
            tool,
            redirect_port: server.port(),
            code_challenge: &pkce.code_challenge,
            code_challenge_method: pkce.code_challenge_method,
        })
        .await?;
    open_url(&session.authorize_url)?;
    let callback = server.wait_for_result().await?;
    let exchange = client
        .exchange(&ExchangeRequest {
            setup_id: &session.setup_id,
            code: &callback.code,
            code_verifier: &pkce.code_verifier,
        })
        .await?;
    Ok((session, exchange))
}

pub fn open_browser(url: &str) -> Result<()> {
    std::process::Command::new("open")
        .arg(url)
        .spawn()
        .context("open browser")?;
    Ok(())
}
