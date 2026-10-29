use anyhow::Result;
use serde::Serialize;

use crate::adapters::{get_adapter, AdapterContext};
use crate::auth::{authorize_and_exchange, TokenMarketClient};
use crate::config::{gateway_api_base_url, now_iso8601, save_managed_key, ManagedKeyRecord};
use crate::models::{resolve_context_window_tokens, resolve_model, ToolConfig};
use crate::verification::{verify_token_market, VerificationResult, VerificationTest};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "kebab-case", tag = "type")]
pub enum SetupProgress {
    AuthorizeStarted { authorize_url: String },
    Authorizing,
    ResolvingModel,
    WritingConfig,
    Testing,
    Completed,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupOutcome {
    pub tool: String,
    pub tool_name: String,
    pub api_key_name: String,
    pub api_key_id: String,
    pub model: String,
    pub config_path: String,
    pub backup_path: Option<String>,
    pub requires_restart: bool,
    pub tests: Vec<VerificationResult>,
}

pub async fn run_setup(
    tool: &str,
    model_preset: Option<&str>,
    mut on_progress: impl FnMut(SetupProgress) + Send,
) -> Result<SetupOutcome> {
    let tool = crate::adapters::tool_id(tool)?;
    let adapter = get_adapter(tool);
    if !adapter.is_installed() {
        anyhow::bail!(
            "{} is not installed or not available on PATH.",
            adapter.name
        );
    }

    let client = TokenMarketClient::new();
    let (session, exchange) = authorize_and_exchange(tool, &client, |url| {
        on_progress(SetupProgress::AuthorizeStarted {
            authorize_url: url.to_string(),
        });
        crate::auth::open_browser(url)
    })
    .await?;
    on_progress(SetupProgress::Authorizing);
    on_progress(SetupProgress::ResolvingModel);

    let model = resolve_model(model_preset)?;
    let config = ToolConfig {
        base_url: gateway_api_base_url(),
        api_key: exchange.api_key.clone(),
        model: model.clone(),
        context_window_tokens: Some(
            resolve_context_window_tokens(&ToolConfig {
                base_url: gateway_api_base_url(),
                api_key: exchange.api_key.clone(),
                model: model.clone(),
                context_window_tokens: None,
            })
            .await?,
        ),
    };

    on_progress(SetupProgress::WritingConfig);
    let write_result = adapter.write_config(
        &config,
        &AdapterContext {
            setup_id: session.setup_id.clone(),
            key_id: exchange.key_id.clone(),
        },
    )?;
    save_managed_key(ManagedKeyRecord {
        tool,
        key_name: exchange.name.clone(),
        key_id: exchange.key_id.clone(),
        setup_id: session.setup_id,
        config_path: write_result.config_path.display().to_string(),
        backup_path: write_result
            .backup_path
            .as_ref()
            .map(|path| path.display().to_string()),
        model: model.clone(),
        created_at: now_iso8601(),
        revocable: true,
    })?;

    on_progress(SetupProgress::Testing);
    let tests: &[VerificationTest] = if model == "glm-5.3" {
        &[
            VerificationTest::Models,
            VerificationTest::Chat,
            VerificationTest::Stream,
            VerificationTest::ToolCall,
        ]
    } else {
        &[
            VerificationTest::Models,
            VerificationTest::Chat,
            VerificationTest::Stream,
        ]
    };
    let results = verify_token_market(&config, adapter.protocol, tests).await?;
    on_progress(SetupProgress::Completed);

    Ok(SetupOutcome {
        tool: tool.to_string(),
        tool_name: adapter.name.into(),
        api_key_name: exchange.name,
        api_key_id: exchange.key_id,
        model,
        config_path: write_result.config_path.display().to_string(),
        backup_path: write_result
            .backup_path
            .as_ref()
            .map(|path| path.display().to_string()),
        requires_restart: write_result.requires_restart,
        tests: results,
    })
}
