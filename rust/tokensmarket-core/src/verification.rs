use anyhow::Result;
use serde::Serialize;

use crate::adapters::Protocol;
use crate::models::ToolConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum VerificationTest {
    Models,
    Chat,
    Stream,
    ToolCall,
}

#[derive(Debug, Clone, Serialize)]
pub struct VerificationResult {
    pub name: String,
    pub passed: bool,
    pub detail: Option<String>,
}

pub async fn verify_token_market(
    config: &ToolConfig,
    protocol: Protocol,
    tests: &[VerificationTest],
) -> Result<Vec<VerificationResult>> {
    let mut results = Vec::new();
    if tests.contains(&VerificationTest::Models) {
        results.push(check_models(config, protocol).await);
    }
    if tests.contains(&VerificationTest::Chat) {
        results.push(check_chat(config, protocol, false).await);
    }
    if tests.contains(&VerificationTest::Stream) {
        results.push(check_chat(config, protocol, true).await);
    }
    if tests.contains(&VerificationTest::ToolCall) {
        results.push(check_tool_call(config, protocol).await);
    }
    Ok(results)
}

fn auth_headers(config: &ToolConfig, protocol: Protocol) -> Vec<(&'static str, String)> {
    if protocol == Protocol::AnthropicCompatible {
        vec![
            ("x-api-key", config.api_key.clone()),
            ("anthropic-version", "2023-06-01".into()),
        ]
    } else {
        vec![("Authorization", format!("Bearer {}", config.api_key))]
    }
}

async fn check_models(config: &ToolConfig, protocol: Protocol) -> VerificationResult {
    let url = format!("{}/v1/models", config.base_url.trim_end_matches('/'));
    let request = reqwest::Client::new().get(url);
    let request = auth_headers(config, protocol)
        .into_iter()
        .fold(request, |request, (key, value)| request.header(key, value));
    match request.send().await {
        Ok(response) => result(
            "Model list",
            response.status().is_success(),
            if response.status().is_success() {
                None
            } else {
                Some(format!("HTTP {}", response.status()))
            },
        ),
        Err(error) => result("Model list", false, Some(error.to_string())),
    }
}

async fn check_chat(config: &ToolConfig, protocol: Protocol, stream: bool) -> VerificationResult {
    let name = if stream {
        "Streaming chat"
    } else {
        "Chat request"
    };
    let body = serde_json::json!({
        "model": config.model,
        "max_tokens": 8,
        "stream": stream,
        "messages": [{"role": "user", "content": "Reply with OK only."}]
    });
    let url = chat_url(config, protocol);
    let request = reqwest::Client::new().post(url).json(&body);
    let request = auth_headers(config, protocol)
        .into_iter()
        .fold(request, |request, (key, value)| request.header(key, value));
    let response = match request.send().await {
        Ok(response) => response,
        Err(error) => return result(name, false, Some(error.to_string())),
    };
    if !response.status().is_success() {
        return result(name, false, Some(format!("HTTP {}", response.status())));
    }
    if !stream {
        return result(name, true, None);
    }
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    if !content_type.contains("text/event-stream") {
        return result(
            name,
            false,
            Some(format!(
                "expected text/event-stream but received {}",
                if content_type.is_empty() {
                    "unknown content-type".to_string()
                } else {
                    content_type
                }
            )),
        );
    }
    match response.text().await {
        Ok(text) => result(
            "Streaming chat",
            text.lines().any(|line| line.starts_with("data:")) && !text.contains("\"error\""),
            if text.lines().any(|line| line.starts_with("data:")) && !text.contains("\"error\"") {
                None
            } else {
                Some("response was not a valid SSE stream".into())
            },
        ),
        Err(error) => result("Streaming chat", false, Some(error.to_string())),
    }
}

async fn check_tool_call(config: &ToolConfig, protocol: Protocol) -> VerificationResult {
    let name = "Tool call";
    let body = if protocol == Protocol::AnthropicCompatible {
        serde_json::json!({"model": config.model, "max_tokens": 64, "tools": [{"name": "get_weather", "description": "Get weather", "input_schema": {"type": "object", "properties": {}}}], "messages": [{"role": "user", "content": "What is the weather? Reply with OK only."}]})
    } else {
        serde_json::json!({"model": config.model, "max_tokens": 64, "tools": [{"type": "function", "function": {"name": "get_weather", "description": "Get weather", "parameters": {"type": "object", "properties": {}}}}], "messages": [{"role": "user", "content": "What is the weather? Reply with OK only."}]})
    };
    let url = chat_url(config, protocol);
    let request = reqwest::Client::new().post(url).json(&body);
    let request = auth_headers(config, protocol)
        .into_iter()
        .fold(request, |request, (key, value)| request.header(key, value));
    match request.send().await {
        Ok(response) => result(
            name,
            response.status().is_success(),
            if response.status().is_success() {
                None
            } else {
                Some(format!("HTTP {}", response.status()))
            },
        ),
        Err(error) => result(name, false, Some(error.to_string())),
    }
}

fn chat_url(config: &ToolConfig, protocol: Protocol) -> String {
    let base = config.base_url.trim_end_matches('/');
    if protocol == Protocol::AnthropicCompatible {
        format!("{base}/v1/messages")
    } else {
        format!("{base}/v1/chat/completions")
    }
}

fn result(name: &str, passed: bool, detail: Option<String>) -> VerificationResult {
    VerificationResult {
        name: name.into(),
        passed,
        detail,
    }
}
