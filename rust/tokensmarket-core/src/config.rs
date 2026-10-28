use anyhow::{bail, Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::adapters::{ToolId, ToolStatus};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManagedKeyRecord {
    pub tool: ToolId,
    pub key_name: String,
    pub key_id: String,
    pub setup_id: String,
    pub config_path: String,
    pub backup_path: Option<String>,
    pub model: String,
    pub created_at: String,
    pub revocable: bool,
}

pub fn console_api_base_url() -> String {
    std::env::var("TOKEN_MARKET_CONSOLE_API_BASE_URL")
        .unwrap_or_else(|_| "https://console-api-test.tokensapi.cn".into())
}

pub fn gateway_api_base_url() -> String {
    std::env::var("TOKEN_MARKET_GATEWAY_API_BASE_URL")
        .unwrap_or_else(|_| "https://api-test.tokensapi.cn".into())
}

pub fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

pub fn token_market_home() -> PathBuf {
    home_dir().join(".tokensmarket")
}

pub fn keys_file() -> PathBuf {
    token_market_home().join("keys.json")
}

pub fn backups_root() -> PathBuf {
    token_market_home().join("backups")
}

pub fn tool_statuses() -> Vec<ToolStatus> {
    crate::adapters::adapters()
        .into_iter()
        .map(|adapter| ToolStatus {
            id: adapter.id,
            name: adapter.name,
            installed: adapter.is_installed(),
            config_path: adapter.config_path().display().to_string(),
        })
        .collect()
}

pub fn list_managed_keys() -> Result<Vec<ManagedKeyRecord>> {
    let file = keys_file();
    if !file.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(&file).with_context(|| format!("read {}", file.display()))?;
    if content.trim().is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str(&content).with_context(|| format!("parse {}", file.display()))
}

pub fn save_managed_key(record: ManagedKeyRecord) -> Result<()> {
    let mut current = list_managed_keys()?;
    current.retain(|item| item.tool != record.tool);
    current.push(record);

    let file = keys_file();
    write_private_json(&file, &current)?;
    Ok(())
}

pub fn find_managed_key(tool: ToolId) -> Result<Option<ManagedKeyRecord>> {
    Ok(list_managed_keys()?
        .into_iter()
        .find(|record| record.tool == tool))
}

pub fn now_iso8601() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub fn ensure_parent_directory(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
        }
    }
    Ok(())
}

pub fn write_private_file(path: &Path, contents: &str) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    ensure_parent_directory(path)?;
    fs::write(path, contents).with_context(|| format!("write {}", path.display()))?;
    let metadata =
        fs::metadata(path).with_context(|| format!("read metadata for {}", path.display()))?;
    let mut permissions = metadata.permissions();
    permissions.set_mode(0o600);
    fs::set_permissions(path, permissions)
        .with_context(|| format!("set permissions for {}", path.display()))?;
    Ok(())
}

pub fn write_private_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut contents = serde_json::to_string_pretty(value)?;
    contents.push('\n');
    write_private_file(path, &contents)
}

pub fn read_json_if_exists(path: &Path) -> Result<serde_json::Value> {
    if !path.exists() {
        return Ok(serde_json::Value::Null);
    }
    let content = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    if content.trim().is_empty() {
        return Ok(serde_json::Value::Null);
    }
    serde_json::from_str(&content).with_context(|| format!("parse {}", path.display()))
}

pub fn create_backup(config_path: &Path) -> Result<Option<PathBuf>> {
    if !config_path.exists() {
        return Ok(None);
    }
    let extension = config_path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("json");
    let stem = config_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("config");
    let file_name = format!(
        "{}.{}",
        Utc::now().format("%Y-%m-%dT%H-%M-%S-%z"),
        extension
    );
    let backup_path = backups_root().join(stem).join(file_name);
    ensure_parent_directory(&backup_path)?;
    fs::copy(config_path, &backup_path).with_context(|| {
        format!(
            "backup {} to {}",
            config_path.display(),
            backup_path.display()
        )
    })?;
    Ok(Some(backup_path))
}

pub fn command_exists(command: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;

    let home = home_dir();
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(path_env) = std::env::var_os("PATH") {
        candidates.extend(std::env::split_paths(&path_env));
    }
    candidates.extend([
        home.join(".local/bin"),
        home.join(".bun/bin"),
        home.join(".npm-global/bin"),
        home.join(".cargo/bin"),
        home.join(".nvm/versions"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/bin"),
        PathBuf::from("/bin"),
        PathBuf::from("/usr/sbin"),
        PathBuf::from("/sbin"),
    ]);

    candidates.iter().any(|directory| {
        let candidate = directory.join(command);
        fs::metadata(&candidate)
            .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    })
}

pub fn parse_tool_id(value: &str) -> Result<ToolId> {
    match value {
        "claude-code" => Ok(ToolId::ClaudeCode),
        "codex" => Ok(ToolId::Codex),
        "workbuddy" => Ok(ToolId::WorkBuddy),
        "deepseek-harness" => Ok(ToolId::DeepSeekHarness),
        "openclaw" => Ok(ToolId::OpenClaw),
        "opencode" => Ok(ToolId::OpenCode),
        other => bail!("unsupported tool: {other}"),
    }
}
