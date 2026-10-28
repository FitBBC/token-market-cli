use tokensmarket_core::adapters::{adapters, tool_id};

#[test]
fn registers_all_supported_tools() {
    let ids: Vec<_> = adapters()
        .into_iter()
        .map(|adapter| adapter.id.to_string())
        .collect();
    assert_eq!(
        ids,
        vec![
            "claude-code",
            "codex",
            "workbuddy",
            "deepseek-harness",
            "openclaw",
            "opencode"
        ]
    );
}

#[test]
fn rejects_unsupported_tools() {
    assert!(tool_id("unknown").is_err());
}

#[test]
fn resolves_model_presets() {
    assert_eq!(
        tokensmarket_core::models::resolve_model(None).unwrap(),
        "glm-5.3"
    );
    assert_eq!(
        tokensmarket_core::models::resolve_model(Some("fast")).unwrap(),
        "deepseek-v4.1-flash"
    );
    assert!(tokensmarket_core::models::resolve_model(Some("unknown")).is_err());
}

#[test]
fn serializes_every_tool_with_the_server_contract_id() {
    for (input, expected) in [
        ("claude-code", "claude-code"),
        ("codex", "codex"),
        ("workbuddy", "workbuddy"),
        ("deepseek-harness", "deepseek-harness"),
        ("openclaw", "openclaw"),
        ("opencode", "opencode"),
    ] {
        let tool = tool_id(input).unwrap();
        let value = serde_json::to_value(&tool).unwrap();
        assert_eq!(value, serde_json::json!(expected));
        assert_eq!(tool.to_string(), expected);
    }
}

#[test]
fn serializes_authorization_requests_with_legacy_camel_case_fields() {
    let tool = tool_id("claude-code").unwrap();
    let session = tokensmarket_core::auth::SessionRequest {
        tool,
        redirect_port: 12345,
        code_challenge: "challenge",
        code_challenge_method: "S256",
    };
    let value = serde_json::to_value(&session).unwrap();
    assert_eq!(value["tool"], "claude-code");
    assert_eq!(value["redirectPort"], 12345);
    assert_eq!(value["codeChallenge"], "challenge");
    assert_eq!(value["codeChallengeMethod"], "S256");
}

#[tokio::test]
async fn callback_server_renders_success_page_and_returns_code() {
    use std::time::Duration;

    let server = tokensmarket_core::auth::CallbackServer::bind()
        .await
        .unwrap();
    let port = server.port();
    let handle = tokio::spawn(async move { server.wait_for_result().await });

    let response = reqwest::get(format!(
        "http://127.0.0.1:{port}/callback?code=test-code&state=test-state"
    ))
    .await
    .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    assert!(response
        .headers()
        .get("content-security-policy")
        .unwrap()
        .to_str()
        .unwrap()
        .contains("default-src 'none'"));
    let body = response.text().await.unwrap();
    assert!(body.contains("<strong>授权完成。</strong>"));
    assert!(body.contains("创建 API Key"));
    assert!(body.contains("写入工具配置"));
    assert!(body.contains("发送测试请求"));

    let result = tokio::time::timeout(Duration::from_secs(2), handle)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(result.code, "test-code");
    assert_eq!(result.state.as_deref(), Some("test-state"));
}
