//! Desktop tool connections: public summaries exclude credentials and private receipts.
mod configuration;
mod issuance;
pub use issuance::CredentialLoss;
mod harness;
mod jsonc;
mod model_plan;
mod multi_model;
mod private_storage;
mod restore;
mod testing;
use configuration::{ConfigDocument, PreparedFile};
pub use testing::{ConnectionTestRequest, ConnectionTestResult};

use crate::{desktop::DesktopIdentity, inventory::ToolInventory, ToolId};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone)]
pub struct ConnectContext {
    pub session_token: String,
    pub owner_id: String,
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
    /// Retained FIRST baseline after ownership was released; not an active config requirement.
    #[serde(default)]
    pub retired: bool,
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
    #[serde(default)]
    pub connection_id: String,
    pub tool: ToolId,
    pub device_id: String,
    pub key_id: String,
    pub owner: DesktopIdentity,
    pub bound_workspace: DesktopIdentity,
    pub protocol: String,
    pub models: Vec<String>,
    pub model_snapshot: Vec<Value>,
    pub gateway_url: String,
    #[serde(default)]
    pub platform_url: String,
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
    pub protocol: String,
    pub test_result: Option<ConnectionTestResult>,
}
impl ConnectionRecord {
    pub fn summary(&self) -> ConnectionSummary {
        ConnectionSummary {
            state: "configured_untested",
            owner: json!(self.owner),
            bound_workspace: json!(self.bound_workspace),
            models: self.models.clone(),
            key_id: self.key_id.clone(),
            protocol: self.protocol.clone(),
            test_result: None,
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
    pub key_retained: bool,
}
/// Reconfiguration never revokes credentials retained for other workspaces.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconnectOutcome {
    pub connection: ConnectionSummary,
    pub key_reused: bool,
}
#[derive(Clone, Serialize, Deserialize)]
struct IssuedKey {
    #[serde(default)]
    issuance: Option<issuance::IssuanceReceipt>,
    api_key: String,
    key_id: String,
    owner: DesktopIdentity,
    bound_workspace: DesktopIdentity,
}
#[derive(Serialize, Deserialize)]
struct SavedKey {
    platform_url: String,
    owner_id: String,
    workspace_id: String,
    device_id: String,
    tool: ToolId,
    key: IssuedKey,
}
pub struct ConnectionManager {
    directory: PathBuf,
    platform_url: String,
    gateway_url: String,
    http: reqwest::Client,
    gate: tokio::sync::Mutex<()>,
    private: private_storage::PrivateStorage,
    client_login: Option<crate::desktop::DesktopLogin>,
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
        let private = private_storage::PrivateStorage::native(&directory);
        Self::with_storage(directory, platform_url, gateway_url, private)
            .with_shared_login(crate::client::login_manager())
    }
    pub fn with_shared_login(mut self, login: crate::desktop::DesktopLogin) -> Self {
        self.client_login = Some(login);
        self
    }
    /// Isolated persistence seam for tests; production uses the native OS vault.
    pub fn with_credentials(
        directory: PathBuf,
        platform_url: String,
        gateway_url: String,
        credentials: Arc<dyn crate::desktop::DesktopCredentialStore>,
    ) -> Self {
        Self::with_storage(
            directory,
            platform_url,
            gateway_url,
            private_storage::PrivateStorage::new(credentials),
        )
    }
    fn with_storage(
        directory: PathBuf,
        platform_url: String,
        gateway_url: String,
        private: private_storage::PrivateStorage,
    ) -> Self {
        Self {
            directory,
            platform_url,
            gateway_url,
            http: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("connection HTTP client"),
            gate: tokio::sync::Mutex::new(()),
            private,
            client_login: None,
        }
    }
    pub fn list(&self) -> Result<Vec<ConnectionRecord>> {
        let path = self.directory.join("connections.json");
        match fs::read(path) {
            Ok(bytes) => serde_json::from_slice(
                &self
                    .private
                    .decode(&self.directory.join("connections.json"), &bytes)?,
            )
            .context("无法读取连接记录"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
            Err(e) => Err(e).context("无法读取连接记录"),
        }
    }
    /// OS file lock also serializes distinct manager instances/processes on this device.
    fn operation_lock(&self) -> Result<fs::File> {
        fs::create_dir_all(&self.directory)?;
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.directory.join("operation.lock"))?;
        file.try_lock()
            .map_err(|_| anyhow::anyhow!("工具操作正在进行，请稍候"))?;
        Ok(file)
    }
    fn saved_keys(&self) -> Result<Vec<SavedKey>> {
        let path = self.directory.join("keys.json");
        match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&self.private.decode(&path, &bytes)?)
                .context("无法读取本机保存的连接信息"),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
            Err(_) => bail!("无法读取本机保存的连接信息"),
        }
    }
    fn save_keys(&self, keys: &[SavedKey]) -> Result<()> {
        self.private.write(
            &self.directory.join("keys.json"),
            &serde_json::to_vec(keys)?,
        )
    }
    fn remember_key(
        &self,
        context: &ConnectContext,
        tool: ToolId,
        device: &str,
        key: &IssuedKey,
    ) -> Result<()> {
        let mut keys = self.saved_keys()?;
        keys.retain(|saved| !self.matches_key(saved, context, tool, device));
        keys.push(SavedKey {
            platform_url: self.platform_url.trim_end_matches('/').into(),
            owner_id: context.owner_id.clone(),
            workspace_id: context.workspace_id.clone(),
            device_id: device.into(),
            tool,
            key: key.clone(),
        });
        self.save_keys(&keys)
    }
    fn matches_key(
        &self,
        key: &SavedKey,
        context: &ConnectContext,
        tool: ToolId,
        device: &str,
    ) -> bool {
        key.platform_url == self.platform_url.trim_end_matches('/')
            && key.owner_id == context.owner_id
            && key.workspace_id == context.workspace_id
            && key.device_id == device
            && key.tool == tool
    }
    fn migrate_records(&self, records: &[ConnectionRecord]) -> Result<()> {
        if records.is_empty() {
            return Ok(());
        }
        // Preserve original baselines while removing plaintext copies owned by the application.
        let backups = self.directory.join("backups");
        if backups.exists() {
            if fs::symlink_metadata(&backups)?.file_type().is_symlink() {
                bail!("连接备份目录是符号链接，请先检查本地配置")
            }
            // Include old unreferenced replacement backups, which may contain prior keys.
            for entry in fs::read_dir(backups)? {
                let entry = entry?;
                if entry.file_type()?.is_file() {
                    self.private.protect_legacy(&entry.path())?;
                }
            }
        }
        self.private
            .protect_legacy(&self.directory.join("connections.json"))
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
        Ok(self
            .establish(context, tool, inventory, path, None, None)
            .await?
            .connection)
    }
    pub async fn reconnect(
        &self,
        context: ConnectContext,
        tool: ToolId,
        expected_key_id: &str,
        inventory: &ToolInventory,
        path: PathBuf,
    ) -> Result<ReconnectOutcome> {
        self.establish(context, tool, inventory, path, Some(expected_key_id), None)
            .await
    }
    pub async fn recover_connection(
        &self,
        context: ConnectContext,
        tool: ToolId,
        expected_connection_key: Option<&str>,
        inventory: &ToolInventory,
        path: PathBuf,
        confirmed_lost_key: &str,
    ) -> Result<ReconnectOutcome> {
        self.establish(
            context,
            tool,
            inventory,
            path,
            expected_connection_key,
            Some(confirmed_lost_key),
        )
        .await
    }
    async fn establish(
        &self,
        context: ConnectContext,
        tool: ToolId,
        inventory: &ToolInventory,
        path: PathBuf,
        replacement: Option<&str>,
        confirmed_lost_key: Option<&str>,
    ) -> Result<ReconnectOutcome> {
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
        if context.session_token.is_empty()
            || context.workspace_id.is_empty()
            || context.owner_id.is_empty()
        {
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
        if fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
            bail!("工具配置是符号链接，请先确认实际配置路径");
        }
        let before = match fs::read(&path) {
            Ok(v) => Some(v),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(_) => bail!("无法读取工具配置"),
        };
        ConfigDocument::parse(tool, &before, &path)?;
        let _operation = self.operation_lock()?;
        let latest = match fs::read(&path) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(_) => bail!("无法读取工具配置"),
        };
        if latest != before {
            bail!("工具配置已改变，请重新接入")
        }
        let mut config = ConfigDocument::parse(tool, &latest, &path)?;
        let mut records = self.list()?;
        self.migrate_records(&records)?;
        let previous = records.iter().find(|r| r.tool == tool).cloned();
        match (replacement, &previous) {
            (None, Some(_)) => bail!("已有连接，请使用重新接入操作"),
            (Some(expected), Some(old)) if old.key_id == expected => {
                if old.receipts.first().map(|r| &r.path) != Some(&path) {
                    bail!("配置路径已改变，请先取消原连接后重新接入")
                }
                let preview = restore::prepare(old, &DisconnectRequest::default(), &self.private)?;
                if !preview.conflicts.is_empty() {
                    bail!("配置含接入后的人工修改，请先取消接入并明确选择冲突处理后重新接入")
                }
            }
            (Some(_), _) => bail!("连接已改变，请刷新后重试"),
            _ => {}
        }
        let device_id = self.device_id()?;
        // Import existing credentials before replacing the active connection. Old releases
        // did not record the console URL; only import those matching this gateway.
        let mut saved = self.saved_keys()?;
        for record in &records {
            if (record.platform_url == self.platform_url.trim_end_matches('/')
                || (record.platform_url.is_empty() && record.gateway_url == self.gateway_url))
                && !saved.iter().any(|key| {
                    key.owner_id == record.owner.id
                        && key.workspace_id == record.bound_workspace.id
                        && key.tool == record.tool
                        && key.device_id == record.device_id
                        && key.platform_url == self.platform_url.trim_end_matches('/')
                })
            {
                saved.push(SavedKey {
                    platform_url: self.platform_url.trim_end_matches('/').into(),
                    owner_id: record.owner.id.clone(),
                    workspace_id: record.bound_workspace.id.clone(),
                    device_id: record.device_id.clone(),
                    tool: record.tool,
                    key: IssuedKey {
                        issuance: None,
                        api_key: record.api_key.clone(),
                        key_id: record.key_id.clone(),
                        owner: record.owner.clone(),
                        bound_workspace: record.bound_workspace.clone(),
                    },
                });
            }
        }
        if !saved.is_empty() {
            self.save_keys(&saved)?;
        }
        let cached = saved
            .into_iter()
            .find(|key| self.matches_key(key, &context, tool, &device_id))
            .map(|saved| saved.key);
        let recovered = if cached.is_none() {
            config.credential(&self.gateway_url)?
        } else {
            None
        };
        let (key, key_reused) = self
            .obtain_key(
                &context,
                tool,
                &device_id,
                cached,
                recovered,
                confirmed_lost_key,
            )
            .await?;
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
                previous.as_ref(),
            )
            .await;
        let connection = result?;
        self.confirm_key(&context, tool, &device_id, &key, true)
            .await?;
        Ok(ReconnectOutcome {
            connection,
            key_reused,
        })
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
        previous: Option<&ConnectionRecord>,
    ) -> Result<ConnectionSummary> {
        if key.bound_workspace.id != context.workspace_id
            || key.owner.id != context.owner_id
            || !key.key_id.chars().all(|c| c.is_ascii_digit())
            || key.key_id.is_empty()
            || key.api_key.is_empty()
        {
            bail!("连接归属校验失败")
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
        if let Some(previous) = previous {
            restore::retire_removed_fields(previous, &mut files, &self.private)?;
        }
        // Detect external edits during network I/O before staging any configuration mutation.
        let _session_fence = self
            .client_login
            .as_ref()
            .map(|login| -> Result<fs::File> {
                let guard = login.process_lock()?;
                let session = login
                    .shared_session()?
                    .context("共享登录已退出，接入尚未写入工具配置")?;
                if session.session_token != context.session_token
                    || session.user.id != context.owner_id
                    || !session.is_valid()
                {
                    bail!("共享登录已改变，请刷新后重新接入")
                }
                Ok(guard)
            })
            .transpose()?;
        self.validate_for_apply(context, tool, device_id, key)
            .await?;
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
                self.private.write(&backup, bytes)?;
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
        if let Some(previous) = previous {
            let baseline = restore::baseline_receipts(previous, &self.private)?;
            for receipt in &mut receipts {
                if let Some(old) = baseline.iter().find(|old| old.path == receipt.path) {
                    receipt.existed = old.existed;
                    receipt.backup_path = old.backup_path.clone();
                    for field in &mut receipt.fields {
                        if let Some(original) =
                            old.fields.iter().find(|f| f.pointer == field.pointer)
                        {
                            if original.retired && field.before != original.before {
                                bail!("已退出管理的配置项存在人工修改，请先取消原连接并保留修改后重新接入")
                            }
                            field.before = original.before.clone();
                        }
                    }
                    for field in &old.fields {
                        if !receipt.fields.iter().any(|f| f.pointer == field.pointer) {
                            let mut historical = field.clone();
                            historical.retired =
                                !restore::bootstrap_field(tool, &receipt.path, &field.pointer);
                            receipt.fields.push(historical);
                        }
                    }
                    for container in &old.created_containers {
                        if !receipt.created_containers.contains(container) {
                            receipt.created_containers.push(container.clone());
                        }
                    }
                }
            }
        }
        let record = ConnectionRecord {
            version: 2,
            connection_id: crate::auth::create_pkce().code_verifier,
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
            platform_url: self.platform_url.trim_end_matches('/').into(),
            created_at: crate::config::now_iso8601(),
            receipts,
            api_key: key.api_key.clone(),
        };
        records.retain(|old| old.tool != tool);
        records.push(record.clone());
        let store = self.directory.join("connections.json");
        let serialized = self
            .private
            .encode(&store, &serde_json::to_vec_pretty(records)?)?;
        let mut applied = 0;
        let transaction = (|| -> Result<()> {
            for file in &files {
                atomic_write(&file.path, &file.bytes)?;
                applied += 1;
            }
            atomic_write(&store, &serialized)
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
    /// Disconnect changes only local configuration; reusable workspace keys remain saved.
    pub async fn disconnect(
        &self,
        tool: ToolId,
        expected_key_id: &str,
        _session: Option<&str>,
        request: DisconnectRequest,
    ) -> Result<DisconnectOutcome> {
        let _guard = self
            .gate
            .try_lock()
            .map_err(|_| anyhow::anyhow!("工具操作正在进行，请稍候"))?;
        let _operation = self.operation_lock()?;
        let mut records = self.list()?;
        self.migrate_records(&records)?;
        let store = self.directory.join("connections.json");
        let original_store = fs::read(&store).context("无法读取连接记录")?;
        let index = records
            .iter()
            .position(|record| record.tool == tool && record.key_id == expected_key_id)
            .context("连接已改变，请刷新后重试")?;
        let record = records[index].clone();
        if !matches!(record.version, 1 | 2) {
            bail!("连接记录版本不支持安全恢复")
        }
        let plan = restore::prepare(&record, &request, &self.private)?;
        if !plan.conflicts.is_empty() {
            return Ok(DisconnectOutcome {
                state: "conflicts",
                revision: Some(plan.revision),
                conflicts: plan.conflicts,
                key_retained: true,
            });
        }
        // Ordinary cancellation retains legacy credentials for future connections.
        let context = ConnectContext {
            session_token: String::new(),
            owner_id: record.owner.id.clone(),
            workspace_id: record.bound_workspace.id.clone(),
        };
        if (record.platform_url == self.platform_url.trim_end_matches('/')
            || (record.platform_url.is_empty() && record.gateway_url == self.gateway_url))
            && !self
                .saved_keys()?
                .iter()
                .any(|key| self.matches_key(key, &context, record.tool, &record.device_id))
        {
            self.remember_key(
                &context,
                record.tool,
                &record.device_id,
                &IssuedKey {
                    issuance: None,
                    api_key: record.api_key.clone(),
                    key_id: record.key_id.clone(),
                    owner: record.owner.clone(),
                    bound_workspace: record.bound_workspace.clone(),
                },
            )?;
        }
        records.remove(index);
        plan.commit(
            &store,
            &original_store,
            &self
                .private
                .encode(&store, &serde_json::to_vec_pretty(&records)?)?,
            &[],
        )?;
        Ok(DisconnectOutcome {
            state: "disconnected",
            revision: None,
            conflicts: vec![],
            key_retained: true,
        })
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
