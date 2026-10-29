use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const DEFAULT_CONTEXT_WINDOW_TOKENS: u64 = 200_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelPreset {
    Recommended,
    Fast,
    Cheap,
    Powerful,
}

impl ModelPreset {
    pub fn parse(value: Option<&str>) -> Result<Self> {
        match value.map(str::trim).filter(|value| !value.is_empty()) {
            None | Some("recommended") => Ok(Self::Recommended),
            Some("fast") => Ok(Self::Fast),
            Some("cheap") => Ok(Self::Cheap),
            Some("powerful") => Ok(Self::Powerful),
            Some(other) => bail!("unsupported model preset: {other}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ModelCapabilities {
    pub default_effort: &'static str,
    pub supported_efforts: &'static [&'static str],
    pub can_disable_thinking: bool,
}

pub fn resolve_model(preset: Option<&str>) -> Result<String> {
    Ok(match ModelPreset::parse(preset)? {
        ModelPreset::Recommended | ModelPreset::Powerful | ModelPreset::Cheap => "glm-5.3".into(),
        ModelPreset::Fast => "deepseek-v4.1-flash".into(),
    })
}

pub fn resolve_model_capabilities(model: &str) -> Result<ModelCapabilities> {
    match model {
        "glm-5.3" => Ok(ModelCapabilities {
            default_effort: "max",
            supported_efforts: &["low", "high", "max"],
            can_disable_thinking: false,
        }),
        other => bail!("unsupported Token Market model: {other}"),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_window_tokens: Option<u64>,
}

impl ToolConfig {
    pub fn validate(&self) -> Result<()> {
        if !self.base_url.starts_with("https://") {
            bail!("base URL must use HTTPS");
        }
        if !self.api_key.starts_with("sk-") {
            bail!("invalid Token Market API key");
        }
        if !["glm-5.3", "deepseek-v4.1-flash"].contains(&self.model.as_str()) {
            bail!("unsupported Token Market model");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
struct ModelsResponse {
    #[serde(default)]
    data: Vec<ModelMetadata>,
}

#[derive(Debug, Clone, Deserialize)]
struct ModelMetadata {
    id: Option<String>,
    #[serde(default)]
    context_window_tokens: Option<serde_json::Number>,
}

pub async fn resolve_context_window_tokens(config: &ToolConfig) -> Result<u64> {
    let url = format!("{}/v1/models", config.base_url.trim_end_matches('/'));
    let response = reqwest::Client::new()
        .get(url)
        .bearer_auth(&config.api_key)
        .send()
        .await;
    let response = match response {
        Ok(response) if response.status().is_success() => response,
        _ => return Ok(DEFAULT_CONTEXT_WINDOW_TOKENS),
    };

    let payload: ModelsResponse = match response.json().await {
        Ok(payload) => payload,
        Err(_) => return Ok(DEFAULT_CONTEXT_WINDOW_TOKENS),
    };

    let value = payload
        .data
        .into_iter()
        .find(|item| item.id.as_deref() == Some(config.model.as_str()))
        .and_then(|item| item.context_window_tokens)
        .and_then(|value| value.as_u64())
        .filter(|value| *value > 0);

    match value {
        Some(value) => Ok(value),
        None => Ok(DEFAULT_CONTEXT_WINDOW_TOKENS),
    }
}

pub fn model_names() -> BTreeMap<&'static str, &'static str> {
    BTreeMap::from([
        ("glm-5.3", "GLM 5.3"),
        ("deepseek-v4.1-flash", "DeepSeek V4.1 Flash"),
    ])
}

pub async fn verify_config(config: &ToolConfig) -> Result<()> {
    config.validate().with_context(|| "invalid configuration")
}
