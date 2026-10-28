//! Desktop account login. The legacy CLI tool/key authorization lives in `auth`.
use crate::auth::create_pkce;
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use url::Url;

pub const CALLBACK_URL: &str = "tokenmarket://auth/callback";

#[derive(Clone, Serialize, Deserialize)]
pub struct DesktopIdentity {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct DesktopSession {
    pub session_token: String,
    pub expires_at: String,
    pub user: DesktopIdentity,
    pub default_workspace: DesktopIdentity,
}

impl std::fmt::Debug for DesktopSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DesktopSession")
            .field("user_id", &self.user.id)
            .field("workspace_id", &self.default_workspace.id)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct LoginAttempt {
    pub session_id: String,
    pub authorize_url: String,
    pub expires_at: String,
}

#[derive(Serialize, Deserialize)]
struct PendingLogin {
    attempt: LoginAttempt,
    state: String,
    verifier: String,
}

pub struct DesktopClient {
    base_url: String,
    http: reqwest::Client,
}

impl DesktopClient {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            http: reqwest::Client::new(),
        }
    }

    async fn post<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        body: serde_json::Value,
    ) -> Result<T> {
        let response = self
            .http
            .post(format!("{}{}", self.base_url.trim_end_matches('/'), path))
            .timeout(std::time::Duration::from_secs(30))
            .json(&body)
            .send()
            .await
            .context("无法连接登录服务，请重试")?;
        let status = response.status();
        // Auth payloads and response bodies must never enter errors or logs.
        let payload: serde_json::Value = response.json().await.context("登录服务返回无效响应")?;
        if !status.is_success() || payload.get("success").and_then(|v| v.as_bool()) == Some(false) {
            bail!("登录授权无效、已过期或已使用，请重新登录");
        }
        serde_json::from_value(payload.get("data").cloned().unwrap_or(payload))
            .context("登录服务响应缺少必要字段")
    }
}

/// Public business entry used by desktop commands and integration tests.
/// Only a pending PKCE request is stored here; reusable credential storage is separate.
pub struct DesktopLogin {
    directory: PathBuf,
    client: DesktopClient,
    generation: AtomicU64,
    gate: std::sync::Mutex<()>,
}

impl DesktopLogin {
    pub fn new(directory: PathBuf, client: DesktopClient) -> Self {
        Self {
            directory,
            client,
            generation: AtomicU64::new(0),
            gate: std::sync::Mutex::new(()),
        }
    }

    fn pending_path(&self) -> PathBuf {
        self.directory.join("pending-login.json")
    }

    pub fn pending(&self) -> Result<Option<LoginAttempt>> {
        let _gate = self.gate.lock().expect("desktop login lock");
        if !self.pending_path().exists() {
            return Ok(None);
        }
        let pending: PendingLogin = serde_json::from_slice(&fs::read(self.pending_path())?)
            .context("无法读取登录请求，请重新登录")?;
        if DateTime::parse_from_rfc3339(&pending.attempt.expires_at)? <= Utc::now() {
            self.cancel_locked()?;
            return Ok(None);
        }
        Ok(Some(pending.attempt))
    }

    pub async fn begin(&self) -> Result<LoginAttempt> {
        let generation = {
            let _gate = self.gate.lock().expect("desktop login lock");
            self.cancel_locked()?;
            self.generation.load(Ordering::SeqCst)
        };
        let pkce = create_pkce();
        let state = create_pkce().code_verifier;
        let attempt: LoginAttempt = self.client.post("/api/desktop/auth/sessions", serde_json::json!({
            "state": state, "code_challenge": pkce.code_challenge, "code_challenge_method":"S256"
        })).await?;
        let url = Url::parse(&attempt.authorize_url).context("登录链接无效")?;
        if url.scheme() != "https"
            && !(url.scheme() == "http"
                && matches!(url.host_str(), Some("localhost" | "127.0.0.1")))
        {
            bail!("登录链接必须使用安全连接");
        }
        let _gate = self.gate.lock().expect("desktop login lock");
        if generation != self.generation.load(Ordering::SeqCst) {
            bail!("登录已取消");
        }
        fs::create_dir_all(&self.directory)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
            fs::set_permissions(&self.directory, fs::Permissions::from_mode(0o700))?;
            let file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(self.pending_path())?;
            serde_json::to_writer(
                file,
                &PendingLogin {
                    attempt: attempt.clone(),
                    state,
                    verifier: pkce.code_verifier,
                },
            )?;
        }
        #[cfg(not(unix))]
        {
            let file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(self.pending_path())?;
            serde_json::to_writer(
                file,
                &PendingLogin {
                    attempt: attempt.clone(),
                    state,
                    verifier: pkce.code_verifier,
                },
            )?;
        }
        Ok(attempt)
    }

    pub fn cancel(&self) -> Result<()> {
        let _gate = self.gate.lock().expect("desktop login lock");
        self.cancel_locked()
    }

    fn cancel_locked(&self) -> Result<()> {
        self.generation.fetch_add(1, Ordering::SeqCst);
        match fs::remove_file(self.pending_path()) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    pub async fn complete(&self, callback: &str) -> Result<DesktopSession> {
        let url = Url::parse(callback).context("应用回调无效")?;
        if url.scheme() != "tokenmarket"
            || url.host_str() != Some("auth")
            || url.path() != "/callback"
            || url.port().is_some()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
        {
            bail!("应用回调无效");
        }
        let mut values = std::collections::HashMap::new();
        for (key, value) in url.query_pairs() {
            if !matches!(key.as_ref(), "session_id" | "state" | "code" | "error")
                || values
                    .insert(key.into_owned(), value.into_owned())
                    .is_some()
            {
                bail!("应用回调参数无效");
            }
        }
        let (pending, code, generation) = {
            let _gate = self.gate.lock().expect("desktop login lock");
            let generation = self.generation.load(Ordering::SeqCst);
            let pending: PendingLogin = serde_json::from_slice(
                &fs::read(self.pending_path()).context("没有等待中的登录请求，请重新登录")?,
            )?;
            if values.get("session_id") != Some(&pending.attempt.session_id)
                || values.get("state") != Some(&pending.state)
            {
                bail!("登录请求不匹配，请返回发起登录的应用");
            }
            if DateTime::parse_from_rfc3339(&pending.attempt.expires_at)? <= Utc::now() {
                self.cancel_locked()?;
                bail!("登录请求已过期，请重新登录");
            }
            if values.contains_key("error") {
                self.cancel_locked()?;
                bail!("登录授权未完成，请重试");
            }
            let code = values
                .get("code")
                .filter(|v| !v.is_empty())
                .context("回调缺少授权码")?
                .clone();
            // Consume before network I/O: duplicated OS callbacks cannot exchange twice.
            fs::remove_file(self.pending_path()).context("登录请求已处理")?;
            (pending, code, generation)
        };
        let session: DesktopSession = self
            .client
            .post(
                "/api/desktop/auth/exchange",
                serde_json::json!({
                    "session_id": pending.attempt.session_id, "state": pending.state,
                    "code": code, "code_verifier": pending.verifier
                }),
            )
            .await?;
        if generation != self.generation.load(Ordering::SeqCst) {
            bail!("登录已取消");
        }
        if session.session_token.is_empty()
            || session.user.id.is_empty()
            || session.default_workspace.id.is_empty()
            || DateTime::parse_from_rfc3339(&session.expires_at)? <= Utc::now()
        {
            bail!("登录会话无效，请重新登录");
        }
        Ok(session)
    }
}
