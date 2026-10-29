use serde_json::Value;
use tokensmarket_core::{
    adapters::{get_adapter, AdapterContext},
    models::ToolConfig,
    ToolId,
};

#[test]
fn terminal_claude_setup_writes_all_aliases_and_names_to_the_selected_model() {
    // This test has its own process and redirects the adapter away from real Agent files.
    let root = tempfile::tempdir().unwrap();
    std::env::set_var("CLAUDE_CONFIG_DIR", root.path());
    let path = root.path().join("settings.json");
    let fields = [
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
    for model in ["glm-5.3", "deepseek-v4.1-flash"] {
        let result = get_adapter(ToolId::ClaudeCode)
            .write_config(
                &ToolConfig {
                    base_url: "https://example.test".into(),
                    api_key: "sk-test-fixture".into(),
                    model: model.into(),
                    context_window_tokens: Some(202752),
                },
                &AdapterContext {
                    setup_id: "fixture-setup".into(),
                    key_id: "fixture-key".into(),
                },
            )
            .unwrap();
        let current: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        for name in fields {
            assert_eq!(
                current["env"][name], model,
                "{name} must use selected model"
            );
        }
        assert_eq!(current["env"]["CLAUDE_CODE_AUTO_MODE_SERVER"], "0");
        assert!(result.backup_path.is_none());
        // Keep backup storage out of this isolated writer test.
        std::fs::remove_file(&path).unwrap();
    }
}
