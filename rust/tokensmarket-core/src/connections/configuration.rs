use super::{harness::HarnessConfig, multi_model::MultiModelConfig, FieldReceipt};
use crate::ToolId;
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use toml_edit::DocumentMut;

pub(super) enum ConfigDocument {
    Claude(Value),
    Codex(DocumentMut),
    Harness(Option<HarnessConfig>),
    MultiModel(MultiModelConfig),
}

pub(super) struct PreparedFile {
    pub path: PathBuf,
    pub before: Option<Vec<u8>>,
    pub bytes: Vec<u8>,
    pub fields: Vec<FieldReceipt>,
    pub created_containers: Vec<String>,
}

pub(super) struct PreparedConfiguration {
    pub bytes: Vec<u8>,
    pub fields: Vec<FieldReceipt>,
    pub created_containers: Vec<String>,
    pub additional_files: Vec<PreparedFile>,
    pub protocol: &'static str,
}

impl ConfigDocument {
    pub fn parse(tool: ToolId, before: &Option<Vec<u8>>, path: &Path) -> Result<Self> {
        if tool == ToolId::OpenCode
            && matches!(
                path.extension().and_then(|v| v.to_str()),
                Some("json" | "jsonc")
            )
        {
            let sibling = path.with_extension(if path.extension().is_some_and(|v| v == "json") {
                "jsonc"
            } else {
                "json"
            });
            if sibling.exists() {
                bail!("OpenCode 同时存在 JSON 与 JSONC 配置，请先确认唯一生效配置")
            }
        }
        if matches!(
            tool,
            ToolId::WorkBuddy | ToolId::OpenClaw | ToolId::OpenCode
        ) {
            if !matches!(
                path.extension().and_then(|v| v.to_str()),
                Some("json" | "jsonc")
            ) {
                bail!("桌面配置需要可安全恢复的 JSON 文件路径")
            }
            return Ok(Self::MultiModel(MultiModelConfig::parse(tool, before)?));
        }
        if tool == ToolId::DeepSeekHarness {
            return Ok(Self::Harness(Some(HarnessConfig::read(path)?)));
        }
        if tool == ToolId::Codex {
            let document = match before {
                Some(bytes) => std::str::from_utf8(bytes)
                    .context("Codex CLI 配置不是有效 UTF-8")?
                    .parse::<DocumentMut>()
                    .context("Codex CLI 配置不是有效 TOML，请先修复")?,
                None => DocumentMut::new(),
            };
            for key in ["model", "model_provider"] {
                if document.get(key).is_some_and(|v| !v.is_str()) {
                    bail!("Codex CLI 配置结构无效，请先修复")
                }
            }
            if let Some(providers) = document.get("model_providers") {
                if !providers.is_table_like() {
                    bail!("Codex CLI model_providers 必须是 TOML 表")
                }
                if let Some(provider) = providers.get("tokensmarket") {
                    if !provider.is_table_like() {
                        bail!("Codex CLI Token Market provider 必须是 TOML 表")
                    }
                    for key in ["name", "base_url", "experimental_bearer_token", "wire_api"] {
                        if provider.get(key).is_some_and(|v| !v.is_str()) {
                            bail!("Codex CLI Token Market provider 配置结构无效")
                        }
                    }
                    for key in ["requires_openai_auth", "supports_websockets"] {
                        if provider.get(key).is_some_and(|v| !v.is_bool()) {
                            bail!("Codex CLI Token Market provider 配置结构无效")
                        }
                    }
                    // A second credential mechanism can override or invalidate our bearer token.
                    // Fail before issuance rather than silently changing unknown authentication rules.
                    if provider.get("auth").is_some() || provider.get("env_key").is_some() {
                        bail!("Codex CLI Token Market provider 已使用其他凭据方式，请先移除 auth/env_key 后重试")
                    }
                    for kind in ["http_headers", "env_http_headers"] {
                        if let Some(headers) = provider.get(kind).and_then(|v| v.as_table_like()) {
                            if headers
                                .iter()
                                .any(|(name, _)| name.eq_ignore_ascii_case("authorization"))
                            {
                                bail!("Codex CLI Token Market provider 已设置 Authorization 请求头，请先确认凭据方式")
                            }
                        }
                    }
                }
            }
            return Ok(Self::Codex(document));
        }
        let config: Value = match before {
            Some(bytes) => {
                serde_json::from_slice(bytes).context("Claude Code 配置不是有效 JSON，请先修复")?
            }
            None => json!({}),
        };
        if !config.is_object() || config.get("env").is_some_and(|v| !v.is_object()) {
            bail!("Claude Code 配置结构无效，请先修复")
        }
        Ok(Self::Claude(config))
    }

    pub fn apply(
        &mut self,
        gateway: &str,
        key: &str,
        models: &[Value],
    ) -> Result<PreparedConfiguration> {
        let model = models.first().context("当前 Key 没有兼容模型")?;
        let model_id = model["id"]
            .as_str()
            .filter(|id| !id.is_empty())
            .context("模型目录缺少模型 ID")?;
        match self {
            Self::MultiModel(config) => config.apply(gateway, key, models),
            Self::Harness(config) => {
                let mut files = config
                    .take()
                    .context("Harness 配置已应用")?
                    .apply(gateway, key, models)?
                    .into_iter();
                let primary = files.next().context("Harness 配置缺失")?;
                Ok(PreparedConfiguration {
                    bytes: primary.bytes,
                    fields: primary.fields,
                    created_containers: primary.created_containers,
                    additional_files: files.collect(),
                    protocol: "OPENAI_CHAT_COMPLETIONS",
                })
            }
            Self::Codex(document) => {
                let mut created_containers = vec![];
                if document.get("model_providers").is_none() {
                    created_containers.push("/model_providers".into());
                }
                if document
                    .get("model_providers")
                    .and_then(|v| v.get("tokensmarket"))
                    .is_none()
                {
                    created_containers.push("/model_providers/tokensmarket".into());
                }
                let mut fields = vec![];
                for (path, applied) in [
                    (vec!["model"], json!(model_id)),
                    (vec!["model_provider"], json!("tokensmarket")),
                    (
                        vec!["model_providers", "tokensmarket", "name"],
                        json!("Token Market"),
                    ),
                    (
                        vec!["model_providers", "tokensmarket", "base_url"],
                        json!(format!("{}/v1", gateway.trim_end_matches('/'))),
                    ),
                    (
                        vec![
                            "model_providers",
                            "tokensmarket",
                            "experimental_bearer_token",
                        ],
                        json!(key),
                    ),
                    (
                        vec!["model_providers", "tokensmarket", "wire_api"],
                        json!("responses"),
                    ),
                    (
                        vec!["model_providers", "tokensmarket", "requires_openai_auth"],
                        json!(false),
                    ),
                    (
                        vec!["model_providers", "tokensmarket", "supports_websockets"],
                        json!(false),
                    ),
                ] {
                    let mut item = document.as_item_mut();
                    for name in &path {
                        item = &mut item[name];
                    }
                    let before = item
                        .as_str()
                        .map(|v| json!(v))
                        .or_else(|| item.as_bool().map(|v| json!(v)));
                    let mut replacement = if let Some(value) = applied.as_str() {
                        toml_edit::value(value)
                    } else {
                        toml_edit::value(applied.as_bool().expect("owned TOML boolean"))
                    };
                    if let (Some(old), Some(new)) = (item.as_value(), replacement.as_value_mut()) {
                        *new.decor_mut() = old.decor().clone();
                    }
                    *item = replacement;
                    fields.push(FieldReceipt {
                        retired: false,
                        pointer: format!("/{}", path.join("/")),
                        before,
                        applied,
                    });
                }
                Ok(PreparedConfiguration {
                    bytes: document.to_string().into_bytes(),
                    fields,
                    created_containers,
                    additional_files: vec![],
                    protocol: "OPENAI_RESPONSES",
                })
            }
            Self::Claude(config) => {
                let created_containers = if config.get("env").is_none() {
                    vec!["/env".into()]
                } else {
                    vec![]
                };
                if config.get("env").is_none() {
                    config["env"] = json!({});
                }
                let mut values: Vec<(String, Value)> = vec![
                    ("/model", json!(model_id)),
                    ("/env/ANTHROPIC_BASE_URL", json!(gateway)),
                    ("/env/ANTHROPIC_API_KEY", json!(key)),
                    ("/env/ANTHROPIC_AUTH_TOKEN", json!(key)),
                ]
                .into_iter()
                .map(|(pointer, value)| (pointer.into(), value))
                .collect();
                values.extend(
                    crate::adapters::claude_model_environment(model_id)
                        .into_iter()
                        .map(|(name, value)| (format!("/env/{name}"), json!(value))),
                );
                if let Some(tokens) = model["context_window_tokens"].as_u64().filter(|v| *v > 0) {
                    values.push((
                        "/env/CLAUDE_CODE_MAX_CONTEXT_TOKENS".into(),
                        json!(tokens.to_string()),
                    ));
                }
                let mut fields = vec![];
                for (pointer, applied) in values {
                    let before = config.pointer(&pointer).cloned();
                    if pointer == "/model" {
                        config["model"] = applied.clone();
                    } else {
                        config["env"][pointer.trim_start_matches("/env/")] = applied.clone();
                    }
                    fields.push(FieldReceipt {
                        retired: false,
                        pointer,
                        before,
                        applied,
                    });
                }
                Ok(PreparedConfiguration {
                    bytes: serde_json::to_vec_pretty(config)?,
                    fields,
                    created_containers,
                    additional_files: vec![],
                    protocol: "ANTHROPIC_MESSAGES",
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_overwrites_every_model_alias_and_name_and_records_auto_mode_for_restore() {
        let names = [
            "ANTHROPIC_MODEL",
            "ANTHROPIC_DEFAULT_FABLE_MODEL",
            "ANTHROPIC_DEFAULT_FABLE_MODEL_NAME",
            "ANTHROPIC_DEFAULT_HAIKU_MODEL",
            "ANTHROPIC_DEFAULT_HAIKU_MODEL_NAME",
            "ANTHROPIC_DEFAULT_OPUS_MODEL",
            "ANTHROPIC_DEFAULT_OPUS_MODEL_NAME",
            "ANTHROPIC_DEFAULT_SONNET_MODEL",
            "ANTHROPIC_DEFAULT_SONNET_MODEL_NAME",
        ];
        for model_id in ["glm-5.2", "glm-5.3"] {
            let mut original = json!({"env":{"CLAUDE_CODE_AUTO_MODE_SERVER":"1","OTHER":"keep"},"permissions":{"allow":["Read"]}});
            for name in names {
                original["env"][name] = json!(format!("old-{name}"));
            }
            let before = Some(serde_json::to_vec(&original).unwrap());
            let mut document =
                ConfigDocument::parse(ToolId::ClaudeCode, &before, Path::new("settings.json"))
                    .unwrap();
            let applied = document
                .apply(
                    "https://example.test",
                    "fixture-key",
                    &[json!({"id":model_id})],
                )
                .unwrap();
            let current: Value = serde_json::from_slice(&applied.bytes).unwrap();
            for name in names {
                assert_eq!(
                    current["env"][name], model_id,
                    "{name} must use selected model"
                );
                let pointer = format!("/env/{name}");
                let receipt = applied
                    .fields
                    .iter()
                    .find(|field| field.pointer == pointer)
                    .unwrap();
                assert_eq!(receipt.before, Some(original["env"][name].clone()));
                assert_eq!(receipt.applied, json!(model_id));
            }
            assert_eq!(current["env"]["CLAUDE_CODE_AUTO_MODE_SERVER"], "0");
            let mode = applied
                .fields
                .iter()
                .find(|field| field.pointer == "/env/CLAUDE_CODE_AUTO_MODE_SERVER")
                .unwrap();
            assert_eq!(mode.before, Some(json!("1")));
            assert_eq!(mode.applied, json!("0"));
            assert_eq!(current["env"]["OTHER"], "keep");
            assert_eq!(current["permissions"], original["permissions"]);
        }
    }

    #[test]
    fn single_model_configs_use_the_selected_catalogue_model() {
        let models = vec![json!({"id":"glm-5.2"})];
        let mut claude =
            ConfigDocument::parse(ToolId::ClaudeCode, &None, Path::new("settings.json")).unwrap();
        let result = claude
            .apply("https://example.test", "test-key", &models)
            .unwrap();
        let config: Value = serde_json::from_slice(&result.bytes).unwrap();
        assert_eq!(config["model"], "glm-5.2");
        assert_eq!(config["env"]["ANTHROPIC_MODEL"], "glm-5.2");
        assert_eq!(config["env"]["ANTHROPIC_DEFAULT_SONNET_MODEL"], "glm-5.2");
        let mut codex =
            ConfigDocument::parse(ToolId::Codex, &None, Path::new("config.toml")).unwrap();
        let result = codex
            .apply("https://example.test", "test-key", &models)
            .unwrap();
        let document = std::str::from_utf8(&result.bytes)
            .unwrap()
            .parse::<DocumentMut>()
            .unwrap();
        assert_eq!(document["model"].as_str(), Some("glm-5.2"));
    }
}
