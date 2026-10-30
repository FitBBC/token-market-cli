use tokenmarket_core::adapters::{adapters, tool_id};

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
fn generates_independent_pkce_verifiers_with_matching_challenges() {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    use sha2::{Digest, Sha256};
    let first = tokenmarket_core::auth::create_pkce();
    let second = tokenmarket_core::auth::create_pkce();
    assert_ne!(first.code_verifier, second.code_verifier);
    assert_eq!(
        first.code_challenge,
        URL_SAFE_NO_PAD.encode(Sha256::digest(first.code_verifier.as_bytes()))
    );
}
