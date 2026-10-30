//! Desktop account login. The legacy CLI tool/key authorization lives in `auth`.
use crate::auth::create_pkce;
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};
use url::Url;

pub const CALLBACK_URL: &str = "tokenmarket://auth/callback";

#[derive(Debug)]
struct SessionRejected;
impl std::fmt::Display for SessionRejected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "共享登录已失效，请重新登录")
    }
}
impl std::error::Error for SessionRejected {}

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
    #[serde(default)]
    pub web_origin: Option<String>,
}

impl DesktopSession {
    pub fn is_valid(&self) -> bool {
        !self.session_token.is_empty()
            && !self.user.id.is_empty()
            && !self.default_workspace.id.is_empty()
            && DateTime::parse_from_rfc3339(&self.expires_at)
                .is_ok_and(|expires| expires > Utc::now())
    }
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
    #[serde(default)]
    credential_fingerprint: Option<String>,
}

/// OS credential storage is the external persistence seam. Implementations must not log secrets.
pub trait DesktopCredentialStore: Send + Sync {
    fn read(&self) -> Result<Option<String>>;
    fn write(&self, value: &str) -> Result<()>;
    fn delete(&self) -> Result<()>;
}

/// Process-local cache around an OS credential store. Never persisted to an app file.
/// Cache entries are committed only after successful OS operations; failures are retried.
pub struct CachedDesktopCredentials {
    store: Arc<dyn DesktopCredentialStore>,
    cached: Mutex<Option<Option<String>>>,
}
impl CachedDesktopCredentials {
    pub fn new(store: Arc<dyn DesktopCredentialStore>) -> Self {
        Self {
            store,
            cached: Mutex::new(None),
        }
    }
}
impl DesktopCredentialStore for CachedDesktopCredentials {
    fn read(&self) -> Result<Option<String>> {
        let mut cached = self.cached.lock().expect("credential cache lock");
        if let Some(value) = cached.as_ref() {
            return Ok(value.clone());
        }
        let value = self.store.read()?;
        *cached = Some(value.clone());
        Ok(value)
    }
    fn write(&self, value: &str) -> Result<()> {
        let mut cached = self.cached.lock().expect("credential cache lock");
        if cached.as_ref().and_then(|value| value.as_deref()) == Some(value) {
            return Ok(());
        }
        // An OS error may leave persistence uncertain; force the next read to consult it.
        *cached = None;
        self.store.write(value)?;
        *cached = Some(Some(value.to_owned()));
        Ok(())
    }
    fn delete(&self) -> Result<()> {
        let mut cached = self.cached.lock().expect("credential cache lock");
        *cached = None;
        self.store.delete()?;
        *cached = Some(None);
        Ok(())
    }
}

/// Production persistence: macOS Keychain / Windows Credential Store, with no plaintext fallback.
/// Credentials are scoped to the configured platform, so test and production logins never overlap.
pub struct NativeDesktopCredentials {
    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    service: &'static str,
    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    account: String,
}
impl NativeDesktopCredentials {
    pub fn new(platform_url: &str) -> Self {
        Self::scoped("cn.tokenmarket.setup.desktop-session", platform_url)
    }
    pub fn scoped(service: &'static str, scope: &str) -> Self {
        #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
        {
            use sha2::{Digest, Sha256};
            Self {
                service,
                account: format!(
                    "platform-{:x}",
                    Sha256::digest(scope.trim_end_matches('/').as_bytes())
                ),
            }
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        {
            let _ = (service, scope);
            Self {}
        }
    }
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    fn entry(&self) -> Result<keyring::Entry> {
        keyring::Entry::new(self.service, &self.account)
            .map_err(|_| anyhow::anyhow!("无法访问系统私密存储"))
    }
}
impl DesktopCredentialStore for NativeDesktopCredentials {
    fn read(&self) -> Result<Option<String>> {
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            match self.entry()?.get_password() {
                Ok(value) => Ok(Some(value)),
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(_) => bail!("无法读取系统私密存储中的桌面会话"),
            }
        }
        #[cfg(target_os = "linux")]
        {
            crate::credential_vault::linux::read(self.service, &self.account)
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        {
            bail!("当前系统不支持桌面会话私密存储")
        }
    }
    fn write(&self, value: &str) -> Result<()> {
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            self.entry()?
                .set_password(value)
                .map_err(|_| anyhow::anyhow!("无法保存桌面会话到系统私密存储"))
        }
        #[cfg(target_os = "linux")]
        {
            crate::credential_vault::linux::write(self.service, &self.account, value)
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        {
            let _ = value;
            bail!("当前系统不支持桌面会话私密存储")
        }
    }
    fn delete(&self) -> Result<()> {
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            match self.entry()?.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                Err(_) => bail!("无法移除系统私密存储中的桌面会话"),
            }
        }
        #[cfg(target_os = "linux")]
        {
            crate::credential_vault::linux::delete(self.service, &self.account)
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        {
            bail!("当前系统不支持桌面会话私密存储")
        }
    }
}

#[derive(Default)]
struct MemoryCredentials(Mutex<Option<String>>);
impl DesktopCredentialStore for MemoryCredentials {
    fn read(&self) -> Result<Option<String>> {
        Ok(self.0.lock().expect("credential lock").clone())
    }
    fn write(&self, value: &str) -> Result<()> {
        *self.0.lock().expect("credential lock") = Some(value.into());
        Ok(())
    }
    fn delete(&self) -> Result<()> {
        *self.0.lock().expect("credential lock") = None;
        Ok(())
    }
}

#[derive(Deserialize)]
struct SessionContext {
    expires_at: String,
    user: DesktopIdentity,
    default_workspace: DesktopIdentity,
    #[serde(default)]
    web_origin: Option<String>,
}

pub struct DesktopClient {
    base_url: String,
    http: reqwest::Client,
}

impl DesktopClient {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            http: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("client HTTP configuration"),
        }
    }

    async fn authenticated<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        token: &str,
    ) -> Result<T> {
        let response = self
            .http
            .post(format!("{}{}", self.base_url.trim_end_matches('/'), path))
            .bearer_auth(token)
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await
            .context("无法验证桌面会话，请重新登录")?;
        let status = response.status();
        if matches!(status.as_u16(), 401 | 403) {
            return Err(SessionRejected.into());
        }
        let payload: serde_json::Value = response.json().await.context("会话服务返回无效响应")?;
        if !status.is_success() || payload.get("success").and_then(|v| v.as_bool()) == Some(false) {
            if status.is_success()
                && matches!(
                    payload["message"].as_str(),
                    Some(
                        "revoked"
                            | "token invalid"
                            | "Token无效"
                            | "desktop session expired or revoked"
                    )
                )
            {
                return Err(SessionRejected.into());
            }
            bail!("桌面会话失效或无法续期，请重新登录");
        }
        serde_json::from_value(payload.get("data").cloned().unwrap_or(payload))
            .context("会话服务响应缺少必要字段")
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
        // Never surface authentication response bodies, even on failures.
        if !status.is_success() {
            if status.as_u16() == 404 {
                bail!("登录服务接口不可用（HTTP 404），请检查服务部署或地址");
            }
            if status.is_server_error() {
                bail!("登录服务暂时不可用（HTTP {}），请稍后重试", status.as_u16());
            }
            bail!("登录请求失败（HTTP {}），请重新登录", status.as_u16());
        }
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
    credentials: Arc<dyn DesktopCredentialStore>,
    generation: AtomicU64,
    gate: std::sync::Mutex<()>,
}

impl DesktopLogin {
    pub fn new(directory: PathBuf, client: DesktopClient) -> Self {
        Self::with_credentials(directory, client, Arc::new(MemoryCredentials::default()))
    }

    pub fn with_credentials(
        directory: PathBuf,
        client: DesktopClient,
        credentials: Arc<dyn DesktopCredentialStore>,
    ) -> Self {
        Self {
            directory,
            client,
            credentials,
            generation: AtomicU64::new(0),
            gate: std::sync::Mutex::new(()),
        }
    }

    /// Restore only after the platform has accepted and renewed the private credential.
    pub fn shared_session(&self) -> Result<Option<DesktopSession>> {
        self.credentials
            .read()?
            .map(|saved| {
                serde_json::from_str::<DesktopSession>(&saved).context("保存的客户端会话无效")
            })
            .transpose()
    }

    pub async fn restore(&self) -> Result<Option<DesktopSession>> {
        let (saved, generation) = {
            let _gate = self.gate.lock().expect("desktop login lock");
            let _process = self.process_lock()?;
            (
                self.credentials.read()?,
                self.generation.load(Ordering::SeqCst),
            )
        };
        let Some(saved) = saved else {
            return Ok(None);
        };
        let original_saved = saved.clone();
        let renewal = async {
            let mut session: DesktopSession = serde_json::from_str(&saved)
                .map_err(|_| anyhow::anyhow!("保存的桌面会话无效，请重新登录"))?;
            if session.session_token.is_empty()
                || DateTime::parse_from_rfc3339(&session.expires_at)
                    .map_or(true, |expires| expires <= Utc::now())
            {
                bail!("桌面会话已过期，请重新登录");
            }
            let context: SessionContext = self
                .client
                .authenticated("/api/desktop/auth/renew", &session.session_token)
                .await?;
            if context.user.id != session.user.id
                || context.default_workspace.id.is_empty()
                || DateTime::parse_from_rfc3339(&context.expires_at)
                    .map_or(true, |expires| expires <= Utc::now())
            {
                bail!("桌面会话无效，请重新登录");
            }
            session.expires_at = context.expires_at;
            session.user = context.user;
            session.default_workspace = context.default_workspace;
            if context.web_origin.is_some() {
                session.web_origin = context.web_origin;
            }
            Ok(session)
        }
        .await;
        let _gate = self.gate.lock().expect("desktop login lock");
        let _process = self.process_lock()?;
        if generation != self.generation.load(Ordering::SeqCst) {
            bail!("桌面会话已变更");
        }
        if self.credentials.read()?.as_deref() != Some(original_saved.as_str()) {
            bail!("其他客户端已改变登录，请刷新后重试")
        }
        match renewal {
            Ok(session) => {
                self.credentials.write(&serde_json::to_string(&session)?)?;
                Ok(Some(session))
            }
            Err(error) => {
                if error.downcast_ref::<SessionRejected>().is_some()
                    || serde_json::from_str::<DesktopSession>(&original_saved)
                        .map_or(true, |session| !session.is_valid())
                {
                    self.credentials.delete()?;
                }
                Err(error)
            }
        }
    }

    /// Ends local desktop login even when the server is offline. False means remote revocation is unconfirmed.
    /// Existing tool credentials and connection metadata are never read or changed here.
    pub async fn logout(&self) -> Result<bool> {
        let saved = {
            let _gate = self.gate.lock().expect("desktop login lock");
            let _process = self.process_lock()?;
            self.cancel_locked()?;
            let saved = self.credentials.read()?;
            self.credentials
                .delete()
                .context("无法移除私密存储中的桌面会话，请重试退出")?;
            saved
        };
        let Some(saved) = saved else {
            return Ok(true);
        };
        let Ok(session) = serde_json::from_str::<DesktopSession>(&saved) else {
            return Ok(false);
        };
        Ok(self
            .client
            .authenticated::<serde_json::Value>("/api/desktop/auth/logout", &session.session_token)
            .await
            .map(|value| {
                value
                    .as_bool()
                    .or_else(|| value["logged_out"].as_bool())
                    .unwrap_or(false)
            })
            .unwrap_or(false))
    }

    /// Drop a callback result cancelled before the native command accepted it.
    /// A stale callback must never erase a newer account's credential.
    pub fn discard_unaccepted_session(&self, session: &DesktopSession) -> Result<()> {
        let _gate = self.gate.lock().expect("desktop login lock");
        let _process = self.process_lock()?;
        if let Some(saved) = self.credentials.read()? {
            let saved: DesktopSession =
                serde_json::from_str(&saved).map_err(|_| anyhow::anyhow!("保存的桌面会话无效"))?;
            if saved.session_token == session.session_token {
                self.credentials.delete()?;
            }
        }
        Ok(())
    }

    pub(crate) fn process_lock(&self) -> Result<fs::File> {
        fs::create_dir_all(&self.directory)?;
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.directory.join("operation.lock"))?;
        file.try_lock()
            .map_err(|_| anyhow::anyhow!("另一个入口正在更新共享登录，请稍后重试"))?;
        Ok(file)
    }

    fn pending_path(&self) -> PathBuf {
        self.directory.join("pending-login.json")
    }

    pub fn pending(&self) -> Result<Option<LoginAttempt>> {
        let _gate = self.gate.lock().expect("desktop login lock");
        let _process = self.process_lock()?;
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
        self.begin_request(false).await
    }

    pub async fn begin_remote(&self) -> Result<LoginAttempt> {
        self.begin_request(true).await
    }

    async fn begin_request(&self, remote: bool) -> Result<LoginAttempt> {
        let generation = {
            let _gate = self.gate.lock().expect("desktop login lock");
            let _process = self.process_lock()?;
            self.cancel_locked()?;
            self.generation.load(Ordering::SeqCst)
        };
        let credential_fingerprint = self.credentials.read()?.as_deref().map(session_fingerprint);
        let pkce = create_pkce();
        let state = create_pkce().code_verifier;
        let attempt: LoginAttempt = self.client.post("/api/desktop/auth/sessions", serde_json::json!({
            "state": state, "code_challenge": pkce.code_challenge, "code_challenge_method":"S256", "remote": remote
        })).await?;
        let url = Url::parse(&attempt.authorize_url).context("登录链接无效")?;
        if url.scheme() != "https"
            && !(url.scheme() == "http"
                && matches!(url.host_str(), Some("localhost" | "127.0.0.1")))
        {
            bail!("登录链接必须使用安全连接");
        }
        let _gate = self.gate.lock().expect("desktop login lock");
        let _process = self.process_lock()?;
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
                    credential_fingerprint,
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
                    credential_fingerprint,
                },
            )?;
        }
        Ok(attempt)
    }

    pub async fn poll_remote(&self) -> Result<Option<DesktopSession>> {
        let (pending, generation) = {
            let _gate = self.gate.lock().expect("client login lock");
            let _process = self.process_lock()?;
            let pending: PendingLogin =
                serde_json::from_slice(&fs::read(self.pending_path())?).context("登录请求无效")?;
            if DateTime::parse_from_rfc3339(&pending.attempt.expires_at)? <= Utc::now() {
                bail!("登录请求已过期，请重新登录")
            }
            (pending, self.generation.load(Ordering::SeqCst))
        };
        let value: serde_json::Value = self.client.post("/api/desktop/auth/remote-claim", serde_json::json!({
            "session_id":pending.attempt.session_id,"state":pending.state,"code_verifier":pending.verifier
        })).await?;
        if value["status"] == "pending" {
            return Ok(None);
        }
        let session: DesktopSession = serde_json::from_value(value).context("登录响应不完整")?;
        if !session.is_valid() {
            bail!("登录会话无效")
        }
        let _gate = self.gate.lock().expect("client login lock");
        let _process = self.process_lock()?;
        if generation != self.generation.load(Ordering::SeqCst)
            || self.credentials.read()?.as_deref().map(session_fingerprint)
                != pending.credential_fingerprint
        {
            bail!("登录已取消或被其他客户端更改")
        }
        self.credentials.write(&serde_json::to_string(&session)?)?;
        fs::remove_file(self.pending_path())?;
        Ok(Some(session))
    }

    pub fn cancel(&self) -> Result<()> {
        let _gate = self.gate.lock().expect("desktop login lock");
        let _process = self.process_lock()?;
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
            let _process = self.process_lock()?;
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
        let _gate = self.gate.lock().expect("desktop login lock");
        let _process = self.process_lock()?;
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
        if self.credentials.read()?.as_deref().map(session_fingerprint)
            != pending.credential_fingerprint
        {
            bail!("其他客户端已改变登录，请重新操作");
        }
        self.credentials.write(&serde_json::to_string(&session)?)?;
        Ok(session)
    }
}

fn session_fingerprint(value: &str) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
