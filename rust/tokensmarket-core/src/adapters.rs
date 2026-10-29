use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::config::{
    create_backup, home_dir, read_json_if_exists, write_private_file, write_private_json,
};
use crate::models::{resolve_model_capabilities, ToolConfig};

/// Keep desktop connections and terminal setup aligned on every Claude model selector.
pub(crate) fn claude_model_environment(model: &str) -> [(&'static str, &str); 10] {
    [
        ("ANTHROPIC_MODEL", model),
        ("ANTHROPIC_DEFAULT_FABLE_MODEL", model),
        ("ANTHROPIC_DEFAULT_FABLE_MODEL_NAME", model),
        ("ANTHROPIC_DEFAULT_HAIKU_MODEL", model),
        ("ANTHROPIC_DEFAULT_HAIKU_MODEL_NAME", model),
        ("ANTHROPIC_DEFAULT_OPUS_MODEL", model),
        ("ANTHROPIC_DEFAULT_OPUS_MODEL_NAME", model),
        ("ANTHROPIC_DEFAULT_SONNET_MODEL", model),
        ("ANTHROPIC_DEFAULT_SONNET_MODEL_NAME", model),
        ("CLAUDE_CODE_AUTO_MODE_SERVER", "0"),
    ]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ToolId {
    ClaudeCode,
    Codex,
    #[serde(rename = "workbuddy")]
    WorkBuddy,
    #[serde(rename = "deepseek-harness")]
    DeepSeekHarness,
    #[serde(rename = "openclaw")]
    OpenClaw,
    #[serde(rename = "opencode")]
    OpenCode,
}

impl std::fmt::Display for ToolId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            ToolId::ClaudeCode => "claude-code",
            ToolId::Codex => "codex",
            ToolId::WorkBuddy => "workbuddy",
            ToolId::DeepSeekHarness => "deepseek-harness",
            ToolId::OpenClaw => "openclaw",
            ToolId::OpenCode => "opencode",
        };
        f.write_str(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Protocol {
    AnthropicCompatible,
    OpenAiCompatible,
    Custom,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolStatus {
    pub id: ToolId,
    pub name: &'static str,
    pub installed: bool,
    pub config_path: String,
}

#[derive(Debug, Clone)]
pub struct AdapterContext {
    pub setup_id: String,
    pub key_id: String,
}

#[derive(Debug, Clone)]
pub struct AdapterWriteResult {
    pub config_path: PathBuf,
    pub backup_path: Option<PathBuf>,
    pub requires_restart: bool,
}

#[derive(Debug, Clone)]
pub struct Adapter {
    pub id: ToolId,
    pub name: &'static str,
    pub protocol: Protocol,
    pub requires_restart: bool,
    path_kind: PathKind,
}

#[derive(Debug, Clone)]
enum PathKind {
    ClaudeSettings,
    CodexConfig,
    WorkBuddy,
    DeepSeekHarness { fallback_root: bool },
    OpenClaw,
    OpenCode,
}

impl Adapter {
    pub fn config_path(&self) -> PathBuf {
        let home = home_dir();
        match self.path_kind {
            PathKind::ClaudeSettings => std::env::var_os("CLAUDE_CONFIG_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".claude"))
                .join("settings.json"),
            PathKind::CodexConfig => std::env::var_os("CODEX_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".codex"))
                .join("config.toml"),
            PathKind::WorkBuddy => env_path("WORKBUDDY_CONFIG_PATH", || {
                home.join(".workbuddy/models.json")
            }),
            PathKind::DeepSeekHarness { fallback_root } => {
                let dsh_home = env_path("DSH_HOME", || home.join(".dsh"));
                let profile =
                    std::env::var("DSH_ACTIVE_PROFILE").unwrap_or_else(|_| "desktop".into());
                let profile_path = dsh_home
                    .join("profiles")
                    .join(profile)
                    .join("cordis.patch.yml");
                if fallback_root || profile_path.parent().map(Path::exists).unwrap_or(false) {
                    profile_path
                } else {
                    dsh_home.join("cordis.patch.yml")
                }
            }
            PathKind::OpenClaw => env_path("OPENCLAW_CONFIG_PATH", || {
                home.join(".openclaw/openclaw.json")
            }),
            PathKind::OpenCode => env_path("OPENCODE_CONFIG", || {
                home.join(".config/opencode/opencode.json")
            }),
        }
    }

    pub fn is_installed(&self) -> bool {
        crate::inventory::ToolInventory::current().is_installed(self.id)
    }

    pub fn read_config(&self) -> Result<Option<ToolConfig>> {
        match self.id {
            ToolId::ClaudeCode => {
                let value = read_json_if_exists(&self.config_path())?;
                let base_url = value
                    .pointer("/env/ANTHROPIC_BASE_URL")
                    .and_then(Value::as_str);
                let api_key = value
                    .pointer("/env/ANTHROPIC_API_KEY")
                    .and_then(Value::as_str);
                if let (Some(base_url), Some(api_key)) = (base_url, api_key) {
                    return Ok(Some(ToolConfig {
                        base_url: base_url.into(),
                        api_key: api_key.into(),
                        model: value
                            .get("model")
                            .and_then(Value::as_str)
                            .unwrap_or("glm-5.3")
                            .into(),
                        context_window_tokens: value
                            .pointer("/env/CLAUDE_CODE_MAX_CONTEXT_TOKENS")
                            .and_then(Value::as_str)
                            .and_then(|v| v.parse().ok()),
                    }));
                }
                Ok(None)
            }
            ToolId::Codex => {
                let content = read_file_if_exists(&self.config_path())?;
                let Some(content) = content else {
                    return Ok(None);
                };
                let value: toml_edit::DocumentMut =
                    content.parse().context("parse Codex config")?;
                let base_url = value
                    .get("model_providers")
                    .and_then(|value| value.get("tokensmarket"))
                    .and_then(|value| value.get("base_url"))
                    .and_then(|value| value.as_str());
                if let Some(base_url) = base_url {
                    return Ok(Some(ToolConfig {
                        base_url: base_url.into(),
                        api_key: value
                            .get("model_providers")
                            .and_then(|value| value.get("tokensmarket"))
                            .and_then(|value| value.get("experimental_bearer_token"))
                            .and_then(|value| value.as_str())
                            .unwrap_or("")
                            .into(),
                        model: value
                            .get("model")
                            .and_then(|v| v.as_str())
                            .unwrap_or("glm-5.3")
                            .into(),
                        context_window_tokens: None,
                    }));
                }
                Ok(None)
            }
            ToolId::WorkBuddy => {
                let value = read_json_if_exists(&self.config_path())?;
                let entry = value.as_array().and_then(|items| {
                    items.iter().find(|item| {
                        item.get("vendor").and_then(Value::as_str) == Some("Token Market")
                            || item
                                .get("id")
                                .and_then(Value::as_str)
                                .is_some_and(|id| id.starts_with("tokensmarket-"))
                    })
                });
                if let Some(entry) = entry {
                    if let Some(url) = entry.get("url").and_then(Value::as_str) {
                        if let Ok(origin) = url_origin(url) {
                            return Ok(Some(ToolConfig {
                                base_url: origin,
                                api_key: entry
                                    .get("apiKey")
                                    .and_then(Value::as_str)
                                    .unwrap_or("")
                                    .into(),
                                model: entry
                                    .get("model")
                                    .and_then(Value::as_str)
                                    .or_else(|| entry.get("id").and_then(Value::as_str))
                                    .unwrap_or("glm-5.3")
                                    .into(),
                                context_window_tokens: entry
                                    .get("maxInputTokens")
                                    .and_then(Value::as_u64),
                            }));
                        }
                    }
                }
                Ok(None)
            }
            ToolId::DeepSeekHarness => {
                let patch_path = self.config_path();
                let Some(content) = read_file_if_exists(&patch_path)? else {
                    return Ok(None);
                };
                let patch: Vec<Value> = serde_yaml::from_str(&content).unwrap_or_default();
                let provider = patch
                    .iter()
                    .find(|item| item.get("id").and_then(Value::as_str) == Some("llm-pi-ai"))
                    .and_then(|item| item.pointer("/config/providers/tokensmarket"));
                let Some(provider) = provider else {
                    return Ok(None);
                };
                let Some(base_url) = provider.get("baseURL").and_then(Value::as_str) else {
                    return Ok(None);
                };
                let credentials_path = dsh_home().join(".credentials.yaml");
                let credentials: Map<String, Value> = read_file_if_exists(&credentials_path)?
                    .and_then(|content| serde_yaml::from_str(&content).ok())
                    .unwrap_or_default();
                Ok(Some(ToolConfig {
                    base_url: base_url.into(),
                    api_key: credentials
                        .get("TOKEN_MARKET_API_KEY")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .into(),
                    model: provider
                        .pointer("/models/0/id")
                        .and_then(Value::as_str)
                        .unwrap_or("glm-5.3")
                        .into(),
                    context_window_tokens: None,
                }))
            }
            ToolId::OpenClaw => {
                let value = read_json_if_exists(&self.config_path())?;
                let provider = value.pointer("/models/providers/tokensmarket");
                if let Some(provider) = provider {
                    if let (Some(base_url), Some(api_key)) = (
                        provider.get("baseUrl").and_then(Value::as_str),
                        provider.get("apiKey").and_then(Value::as_str),
                    ) {
                        return Ok(Some(ToolConfig {
                            base_url: base_url.into(),
                            api_key: api_key.into(),
                            model: "glm-5.3".into(),
                            context_window_tokens: None,
                        }));
                    }
                }
                Ok(None)
            }
            ToolId::OpenCode => {
                let value = read_json_if_exists(&self.config_path())?;
                let provider = value.pointer("/provider/tokensmarket");
                if let Some(provider) = provider {
                    if let (Some(base_url), Some(api_key)) = (
                        provider.pointer("/options/baseURL").and_then(Value::as_str),
                        provider.pointer("/options/apiKey").and_then(Value::as_str),
                    ) {
                        return Ok(Some(ToolConfig {
                            base_url: base_url.into(),
                            api_key: api_key.into(),
                            model: "glm-5.3".into(),
                            context_window_tokens: None,
                        }));
                    }
                }
                Ok(None)
            }
        }
    }

    pub fn write_config(
        &self,
        config: &ToolConfig,
        context: &AdapterContext,
    ) -> Result<AdapterWriteResult> {
        config.validate()?;
        let config_path = self.config_path();
        let backup_path = create_backup(&config_path)?;

        match self.id {
            ToolId::ClaudeCode => {
                let mut current = read_json_if_exists(&config_path)?;
                if current.is_null() {
                    current = json!({});
                }
                current["model"] = json!(config.model);
                current["env"]["ANTHROPIC_BASE_URL"] = json!(config.base_url);
                current["env"]["ANTHROPIC_API_KEY"] = json!(config.api_key);
                for (name, value) in claude_model_environment(&config.model) {
                    current["env"][name] = json!(value);
                }
                current["env"]["CLAUDE_CODE_MAX_CONTEXT_TOKENS"] =
                    json!(config.context_window_tokens.unwrap_or(200_000).to_string());
                write_private_json(&config_path, &current)?;
            }
            ToolId::Codex => {
                let content = read_file_if_exists(&config_path)?.unwrap_or_default();
                let mut document: toml_edit::DocumentMut = content.parse().unwrap_or_default();
                document["model_provider"] = toml_edit::value("tokensmarket");
                document["model"] = toml_edit::value(config.model.as_str());
                document["model_providers"]["tokensmarket"]["name"] =
                    toml_edit::value("Token Market");
                document["model_providers"]["tokensmarket"]["base_url"] =
                    toml_edit::value(format!("{}/v1", config.base_url.trim_end_matches('/')));
                document["model_providers"]["tokensmarket"]["experimental_bearer_token"] =
                    toml_edit::value(config.api_key.as_str());
                document["model_providers"]["tokensmarket"]["wire_api"] =
                    toml_edit::value("responses");
                write_private_file(&config_path, &format!("{document}\n"))?;
            }
            ToolId::WorkBuddy => {
                let current = read_json_if_exists(&config_path)?;
                let mut entries = current.as_array().cloned().unwrap_or_default();
                entries.retain(|item| {
                    !(item.get("vendor").and_then(Value::as_str) == Some("Token Market")
                        || item
                            .get("id")
                            .and_then(Value::as_str)
                            .is_some_and(|id| id.starts_with("tokensmarket-")))
                });
                let capabilities = resolve_model_capabilities(&config.model)?;
                let mut entry = json!({
                    "id": config.model,
                    "name": "Token Market",
                    "model": config.model,
                    "vendor": "Token Market",
                    "url": format!("{}/v1/chat/completions", config.base_url.trim_end_matches('/')),
                    "apiKey": config.api_key,
                    "supportsToolCall": true,
                    "supportsImages": false,
                    "supportsReasoning": true,
                    "useCustomProtocol": true,
                    "reasoning": {
                        "defaultEffort": capabilities.default_effort,
                        "supportedEfforts": capabilities.supported_efforts,
                        "canDisableThinking": capabilities.can_disable_thinking,
                    }
                });
                if let Some(tokens) = config.context_window_tokens {
                    entry["maxInputTokens"] = json!(tokens);
                }
                entries.push(entry);
                write_private_json(&config_path, &Value::Array(entries))?;
            }
            ToolId::DeepSeekHarness => {
                write_deepseek_harness(config_path.clone(), config, context)?;
            }
            ToolId::OpenClaw => {
                let mut current = read_json_if_exists(&config_path)?;
                if current.is_null() {
                    current = json!({});
                }
                current["models"]["mode"] = json!(current
                    .pointer("/models/mode")
                    .cloned()
                    .unwrap_or_else(|| json!("merge")));
                let provider = &mut current["models"]["providers"]["tokensmarket"];
                *provider = json!({
                    "baseUrl": format!("{}/v1", config.base_url.trim_end_matches('/')),
                    "apiKey": config.api_key,
                    "api": "openai-completions",
                    "models": [
                        {"id": "glm-5.3", "name": "GLM 5.3"},
                        {"id": "deepseek-v4.1-flash", "name": "DeepSeek V4.1 Flash"}
                    ]
                });
                if current.pointer("/agents/defaults/model").is_none() {
                    current["agents"]["defaults"]["model"] =
                        json!({"primary": format!("tokensmarket/{}", config.model)});
                }
                write_private_json(&config_path, &current)?;
            }
            ToolId::OpenCode => {
                let mut current = read_json_if_exists(&config_path)?;
                if current.is_null() {
                    current = json!({});
                }
                if current.get("$schema").is_none() {
                    current["$schema"] = json!("https://opencode.ai/config.json");
                }
                current["model"] = json!(format!("tokensmarket/{}", config.model));
                current["small_model"] = json!(format!("tokensmarket/{}", config.model));
                current["provider"]["tokensmarket"] = json!({
                    "npm": "@ai-sdk/openai-compatible",
                    "name": "Token Market",
                    "options": {
                        "baseURL": format!("{}/v1", config.base_url.trim_end_matches('/')),
                        "apiKey": config.api_key
                    },
                    "models": {
                        "glm-5.3": {"name": "GLM 5.3"},
                        "deepseek-v4.1-flash": {"name": "DeepSeek V4.1 Flash"}
                    }
                });
                write_private_json(&config_path, &current)?;
            }
        }

        Ok(AdapterWriteResult {
            config_path,
            backup_path,
            requires_restart: self.requires_restart,
        })
    }
}

fn write_deepseek_harness(
    config_path: PathBuf,
    config: &ToolConfig,
    context: &AdapterContext,
) -> Result<()> {
    let credentials_path = dsh_home().join(".credentials.yaml");
    let credentials = format!("TOKEN_MARKET_API_KEY: {}\n", config.api_key);
    write_private_file(&credentials_path, &credentials)?;

    let current: Vec<Value> = read_file_if_exists(&config_path)?
        .and_then(|content| serde_yaml::from_str(&content).ok())
        .unwrap_or_default();
    let mut next: Vec<Value> = current
        .into_iter()
        .filter(|item| item.get("id").and_then(Value::as_str) != Some("llm-pi-ai"))
        .collect();
    next.push(json!({
        "id": "llm-pi-ai",
        "config": {
            "providers": {
                "tokensmarket": {
                    "apiKeyEnv": "TOKEN_MARKET_API_KEY",
                    "api": "openai-completions",
                    "baseURL": format!("{}/v1", config.base_url.trim_end_matches('/')),
                    "models": [{"id": "glm-5.3"}, {"id": "deepseek-v4.1-flash"}]
                }
            }
        },
        "tokensMarketSetup": {"setupId": context.setup_id, "keyId": context.key_id}
    }));
    let yaml = serde_yaml::to_string(&next)?;
    write_private_file(&config_path, &yaml)
}

fn dsh_home() -> PathBuf {
    env_path("DSH_HOME", || home_dir().join(".dsh"))
}

fn env_path(name: &str, default: impl FnOnce() -> PathBuf) -> PathBuf {
    std::env::var_os(name)
        .map(PathBuf::from)
        .unwrap_or_else(default)
}

fn read_file_if_exists(path: &Path) -> Result<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }
    fs::read_to_string(path)
        .with_context(|| format!("read {}", path.display()))
        .map(Some)
}

fn url_origin(value: &str) -> Result<String> {
    let url = url::Url::parse(value)?;
    Ok(format!(
        "{}://{}",
        url.scheme(),
        url.host_str().context("URL has no host")?
    ))
}

pub fn adapters() -> Vec<Adapter> {
    vec![
        Adapter {
            id: ToolId::ClaudeCode,
            name: "Claude Code",
            protocol: Protocol::AnthropicCompatible,
            requires_restart: false,
            path_kind: PathKind::ClaudeSettings,
        },
        Adapter {
            id: ToolId::Codex,
            name: "Codex CLI",
            protocol: Protocol::OpenAiCompatible,
            requires_restart: true,
            path_kind: PathKind::CodexConfig,
        },
        Adapter {
            id: ToolId::WorkBuddy,
            name: "WorkBuddy",
            protocol: Protocol::OpenAiCompatible,
            requires_restart: false,
            path_kind: PathKind::WorkBuddy,
        },
        Adapter {
            id: ToolId::DeepSeekHarness,
            name: "DeepSeek Harness",
            protocol: Protocol::OpenAiCompatible,
            requires_restart: true,
            path_kind: PathKind::DeepSeekHarness {
                fallback_root: false,
            },
        },
        Adapter {
            id: ToolId::OpenClaw,
            name: "OpenClaw",
            protocol: Protocol::OpenAiCompatible,
            requires_restart: false,
            path_kind: PathKind::OpenClaw,
        },
        Adapter {
            id: ToolId::OpenCode,
            name: "OpenCode",
            protocol: Protocol::OpenAiCompatible,
            requires_restart: false,
            path_kind: PathKind::OpenCode,
        },
    ]
}

pub fn get_adapter(id: ToolId) -> Adapter {
    adapters()
        .into_iter()
        .find(|adapter| adapter.id == id)
        .expect("supported adapter")
}

pub fn tool_id(value: &str) -> Result<ToolId> {
    crate::config::parse_tool_id(value)
}
