//! Desktop tool connections: credentials never cross the native/public-summary boundary.
mod configuration;
mod harness;
mod model_plan;
mod multi_model;
mod restore;
use configuration::{ConfigDocument, PreparedFile};

use crate::{desktop::DesktopIdentity, inventory::ToolInventory, ToolId};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

pub struct ConnectContext {
    pub session_token: String,
    pub workspace_id: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct FieldReceipt {
    pub pointer: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_value"
    )]
    pub before: Option<Value>,
    pub applied: Value,
}
fn present_value<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<Value>, D::Error> {
    Value::deserialize(deserializer).map(Some)
}
#[derive(Clone, Serialize, Deserialize)]
pub struct FileReceipt {
    pub path: PathBuf,
    pub existed: bool,
    pub backup_path: Option<PathBuf>,
    pub fields: Vec<FieldReceipt>,
    pub created_containers: Vec<String>,
}
/// Private durable record; callers must return `summary()` rather than serialize this to UI.
#[derive(Clone, Serialize, Deserialize)]
pub struct ConnectionRecord {
    pub version: u32,
    pub tool: ToolId,
    pub device_id: String,
    pub key_id: String,
    pub owner: DesktopIdentity,
    pub bound_workspace: DesktopIdentity,
    pub protocol: String,
    pub models: Vec<String>,
    pub model_snapshot: Vec<Value>,
    pub gateway_url: String,
    pub created_at: String,
    pub receipts: Vec<FileReceipt>,
    api_key: String,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionSummary {
    pub state: &'static str,
    pub owner: Value,
    pub bound_workspace: Value,
    pub models: Vec<String>,
    pub key_id: String,
}
impl ConnectionRecord {
    pub fn summary(&self) -> ConnectionSummary {
        ConnectionSummary {
            state: "configured_untested",
            owner: json!(self.owner),
            bound_workspace: json!(self.bound_workspace),
            models: self.models.clone(),
            key_id: self.key_id.clone(),
        }
    }
    pub fn credential(&self) -> &str {
        &self.api_key
    }
}
/// Conflict identifiers are positional and deliberately exclude paths, values and credentials.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RestoreConflict {
    pub file: usize,
    pub field: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RestoreChoice {
    KeepCurrent,
    RestoreOriginal,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictResolution {
    pub file: usize,
    pub field: usize,
    pub choice: RestoreChoice,
}
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct DisconnectRequest {
    pub revision: Option<String>,
    #[serde(default)]
    pub resolutions: Vec<ConflictResolution>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisconnectOutcome {
    pub state: &'static str,
    pub revision: Option<String>,
    pub conflicts: Vec<RestoreConflict>,
    pub remote_disabled: bool,
}
#[derive(Deserialize)]
struct IssuedKey {
    api_key: String,
    key_id: String,
    owner: DesktopIdentity,
    bound_workspace: DesktopIdentity,
}
pub struct ConnectionManager {
    directory: PathBuf,
    platform_url: String,
    gateway_url: String,
    http: reqwest::Client,
    gate: tokio::sync::Mutex<()>,
}
impl ConnectionManager {
    pub fn current() -> Self {
        Self::new(
            crate::config::token_market_home().join("desktop-connections"),
            crate::config::console_api_base_url(),
            crate::config::gateway_api_base_url(),
        )
    }
    pub fn new(directory: PathBuf, platform_url: String, gateway_url: String) -> Self {
        Self {
            directory,
            platform_url,
            gateway_url,
            http: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("connection HTTP client"),
            gate: tokio::sync::Mutex::new(()),
        }
    }
    pub fn list(&self) -> Result<Vec<ConnectionRecord>> {
        let path = self.directory.join("connections.json");
        match fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).context("无法读取连接记录"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
            Err(e) => Err(e).context("无法读取连接记录"),
        }
    }
    pub fn device_id(&self) -> Result<String> {
        fs::create_dir_all(&self.directory)?;
        let path = self.directory.join("device-id");
        match fs::read_to_string(&path) {
            Ok(value) => {
                let value = value.trim();
                if (32..=128).contains(&value.len())
                    && value
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
                {
                    return Ok(value.into());
                }
                bail!("设备标识损坏，请修复后重试");
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => bail!("无法读取设备标识，请检查本地权限"),
        }
        let id = crate::auth::create_pkce().code_verifier;
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&path) {
            Ok(mut f) => {
                f.write_all(id.as_bytes())?;
                f.sync_all()?;
                Ok(id)
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => self.device_id(),
            Err(e) => Err(e.into()),
        }
    }
    pub async fn connect(
        &self,
        context: ConnectContext,
        tool: ToolId,
        inventory: &ToolInventory,
        path: PathBuf,
    ) -> Result<ConnectionSummary> {
        let _guard = self
            .gate
            .try_lock()
            .map_err(|_| anyhow::anyhow!("接入正在进行，请稍候"))?;
        inventory.require_installed(tool)?;
        if !matches!(
            tool,
            ToolId::ClaudeCode
                | ToolId::Codex
                | ToolId::DeepSeekHarness
                | ToolId::WorkBuddy
                | ToolId::OpenClaw
                | ToolId::OpenCode
        ) {
            bail!("此工具的桌面接入尚未开放")
        }
        if context.session_token.is_empty() || context.workspace_id.is_empty() {
            bail!("请先登录并选择空间")
        }
        for base in [&self.platform_url, &self.gateway_url] {
            let url = url::Url::parse(base).context("接入服务地址无效")?;
            if !url.username().is_empty()
                || url.password().is_some()
                || url.query().is_some()
                || url.fragment().is_some()
                || !(url.scheme() == "https"
                    || (url.scheme() == "http"
                        && matches!(url.host_str(), Some("127.0.0.1" | "localhost"))))
            {
                bail!("接入服务地址必须使用安全连接");
            }
        }
        let mut records = self.list()?;
        if records.iter().any(|r| r.tool == tool) {
            bail!("已有连接，请使用重新接入操作")
        }
        if fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
            bail!("工具配置是符号链接，请先确认实际配置路径");
        }
        let before = match fs::read(&path) {
            Ok(v) => Some(v),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(_) => bail!("无法读取工具配置"),
        };
        let mut config = ConfigDocument::parse(tool, &before, &path)?;
        let device_id = self.device_id()?;
        let response = self
            .http
            .post(format!(
                "{}/api/desktop/connections/keys",
                self.platform_url.trim_end_matches('/')
            ))
            .bearer_auth(&context.session_token)
            .header("X-Workspace-Id", &context.workspace_id)
            .json(&json!({"workspace_id":context.workspace_id,"device_id":device_id,"tool":tool}))
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await
            .context("无法取得接入 Key")?;
        let status = response.status();
        let body: Value = response.json().await.context("接入服务响应无效")?;
        if !status.is_success() || body["success"] == false {
            bail!("无法取得接入 Key，请确认登录和空间权限")
        }
        let key: IssuedKey = serde_json::from_value(body.get("data").cloned().unwrap_or(body))
            .context("接入服务缺少 Key 或归属信息")?;
        let result = self
            .apply(
                &context,
                tool,
                &path,
                &before,
                &mut config,
                &device_id,
                &key,
                &mut records,
            )
            .await;
        if let Err(error) = result {
            let cleanup = self
                .disable_key(
                    &context.session_token,
                    &context.workspace_id,
                    &device_id,
                    tool,
                    &key.key_id,
                )
                .await;
            if cleanup.is_err() {
                bail!("{}；新 Key 未确认停用，请在平台 Key 管理中检查", error)
            }
            return Err(error);
        }
        result
    }
    async fn apply(
        &self,
        context: &ConnectContext,
        tool: ToolId,
        path: &Path,
        before: &Option<Vec<u8>>,
        config: &mut ConfigDocument,
        device_id: &str,
        key: &IssuedKey,
        records: &mut Vec<ConnectionRecord>,
    ) -> Result<ConnectionSummary> {
        if key.bound_workspace.id != context.workspace_id
            || key.owner.id.is_empty()
            || !key.key_id.chars().all(|c| c.is_ascii_digit())
            || key.key_id.is_empty()
            || key.api_key.is_empty()
        {
            bail!("接入 Key 归属无效")
        }
        let response = self
            .http
            .get(format!(
                "{}/v1/models",
                self.gateway_url.trim_end_matches('/')
            ))
            .bearer_auth(&key.api_key)
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await
            .context("无法获取模型目录，原配置已保留")?;
        if !response.status().is_success() {
            bail!("无法获取模型目录，原配置已保留")
        }
        let catalogue: Value = response
            .json()
            .await
            .context("模型目录无效，原配置已保留")?;
        let models = model_plan::select(tool, &catalogue)?;
        let prepared = config.apply(&self.gateway_url, &key.api_key, &models)?;
        let mut files = vec![PreparedFile {
            path: path.into(),
            before: before.clone(),
            bytes: prepared.bytes,
            fields: prepared.fields,
            created_containers: prepared.created_containers,
        }];
        files.extend(prepared.additional_files);
        // Detect external edits during network I/O before staging any configuration mutation.
        let mut receipts = vec![];
        for file in &files {
            let current = match fs::read(&file.path) {
                Ok(bytes) => Some(bytes),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(e) => return Err(e.into()),
            };
            if current != file.before {
                bail!("配置在接入期间已变化，请重试")
            }
            let backup_path = if let Some(bytes) = &file.before {
                let backup = self.directory.join("backups").join(format!(
                    "{}-{}",
                    key.key_id,
                    crate::auth::create_pkce().code_verifier
                ));
                atomic_write(&backup, bytes)?;
                Some(backup)
            } else {
                None
            };
            receipts.push(FileReceipt {
                path: file.path.clone(),
                existed: file.before.is_some(),
                backup_path,
                fields: file.fields.clone(),
                created_containers: file.created_containers.clone(),
            });
        }
        let record = ConnectionRecord {
            version: 2,
            tool,
            device_id: device_id.into(),
            key_id: key.key_id.clone(),
            owner: key.owner.clone(),
            bound_workspace: key.bound_workspace.clone(),
            protocol: prepared.protocol.into(),
            models: models
                .iter()
                .map(|model| model["id"].as_str().unwrap().into())
                .collect(),
            model_snapshot: models,
            gateway_url: self.gateway_url.clone(),
            created_at: crate::config::now_iso8601(),
            receipts,
            api_key: key.api_key.clone(),
        };
        records.push(record.clone());
        let serialized = serde_json::to_vec_pretty(records)?;
        let mut applied = 0;
        let transaction = (|| -> Result<()> {
            for file in &files {
                atomic_write(&file.path, &file.bytes)?;
                applied += 1;
            }
            atomic_write(&self.directory.join("connections.json"), &serialized)
        })();
        if transaction.is_err() {
            for file in files[..applied].iter().rev() {
                let rollback = if let Some(bytes) = &file.before {
                    atomic_write(&file.path, bytes)
                } else {
                    fs::remove_file(&file.path).map_err(Into::into)
                };
                rollback.context("接入保存失败，且配置回滚失败；请使用备份恢复")?;
            }
            bail!("接入保存失败，原配置已恢复")
        }
        Ok(record.summary())
    }
    /// Restore the saved connection's local ownership before one optional exact-key disable.
    /// Current account/workspace never substitute for the saved connection's provenance.
    pub async fn disconnect(
        &self,
        tool: ToolId,
        expected_key_id: &str,
        session: Option<&str>,
        request: DisconnectRequest,
    ) -> Result<DisconnectOutcome> {
        let _guard = self
            .gate
            .try_lock()
            .map_err(|_| anyhow::anyhow!("工具操作正在进行，请稍候"))?;
        let store = self.directory.join("connections.json");
        let original_store = fs::read(&store).context("无法读取连接记录")?;
        let mut records: Vec<ConnectionRecord> =
            serde_json::from_slice(&original_store).context("无法读取连接记录")?;
        let index = records
            .iter()
            .position(|record| record.tool == tool && record.key_id == expected_key_id)
            .context("连接已改变，请刷新后重试")?;
        let record = records[index].clone();
        if !matches!(record.version, 1 | 2) {
            bail!("连接记录版本不支持安全恢复")
        }
        let plan = restore::prepare(&record, &request)?;
        if !plan.conflicts.is_empty() {
            return Ok(DisconnectOutcome {
                state: "conflicts",
                revision: Some(plan.revision),
                conflicts: plan.conflicts,
                remote_disabled: false,
            });
        }
        records.remove(index);
        plan.commit(
            &store,
            &original_store,
            &serde_json::to_vec_pretty(&records)?,
        )?;
        let remote_disabled = if let Some(session) = session.filter(|s| !s.is_empty()) {
            self.disable_key(
                session,
                &record.bound_workspace.id,
                &record.device_id,
                record.tool,
                &record.key_id,
            )
            .await
            .is_ok()
        } else {
            false
        };
        Ok(DisconnectOutcome {
            state: "disconnected",
            revision: None,
            conflicts: vec![],
            remote_disabled,
        })
    }
    pub async fn disable_key(
        &self,
        session: &str,
        workspace: &str,
        device: &str,
        tool: ToolId,
        key_id: &str,
    ) -> Result<()> {
        if key_id.is_empty() || !key_id.chars().all(|c| c.is_ascii_digit()) {
            bail!("Key 标识无效，未执行停用")
        }
        let response = self
            .http
            .post(format!(
                "{}/api/desktop/connections/keys/{}/disable",
                self.platform_url.trim_end_matches('/'),
                key_id
            ))
            .bearer_auth(session)
            .header("X-Workspace-Id", workspace)
            .json(&json!({"workspace_id":workspace,"device_id":device,"tool":tool}))
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await
            .context("无法停用 Key")?;
        if !response.status().is_success() {
            bail!("Key 未确认停用")
        }
        let body: Value = response.json().await.context("Key 未确认停用")?;
        if body["success"] != true || body["data"] != true {
            bail!("Key 未确认停用")
        }
        Ok(())
    }
}
/// Atomic same-directory replacement, private staging file, synced before publication.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().context("无效配置路径")?;
    fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(
        ".tokenmarket-{}",
        crate::auth::create_pkce().code_verifier
    ));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| -> Result<()> {
        let mut file = options.open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}
