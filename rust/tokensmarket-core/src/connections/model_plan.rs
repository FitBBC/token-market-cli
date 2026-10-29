use crate::ToolId;
use anyhow::{bail, Context, Result};
use serde_json::Value;

/// Model type metadata determines which key-visible models can be configured for agents.
pub(super) fn is_text_generation(row: &Value) -> bool {
    row["model_types"]
        .as_array()
        .is_some_and(|types| types.iter().any(|kind| kind == "TEXT_GENERATION"))
}
pub(super) fn select(tool: ToolId, catalogue: &Value) -> Result<Vec<Value>> {
    select_with_model(
        tool,
        catalogue,
        option_env!("TOKEN_MARKET_BUILD_DESKTOP_DEFAULT_MODEL").unwrap_or("glm-5.3"),
    )
}

fn select_with_model(tool: ToolId, catalogue: &Value, default_model: &str) -> Result<Vec<Value>> {
    let rows = catalogue["data"]
        .as_array()
        .context("模型目录无效，原配置已保留")?;
    if matches!(
        tool,
        ToolId::DeepSeekHarness | ToolId::WorkBuddy | ToolId::OpenClaw | ToolId::OpenCode
    ) {
        let mut models = vec![];
        let mut ids = std::collections::HashSet::new();
        for row in rows {
            if !is_text_generation(row) {
                continue;
            }
            let id = row["id"]
                .as_str()
                .filter(|id| !id.trim().is_empty())
                .context("模型目录缺少模型 ID，原配置已保留")?;
            if !ids.insert(id) {
                bail!("模型目录含重复模型 ID，原配置已保留")
            }
            models.push(row.clone());
        }
        if models.is_empty() {
            bail!("当前 Key 没有文本生成模型，原配置已保留")
        }
        return Ok(models);
    }
    Ok(vec![rows
        .iter()
        .find(|row| row["id"] == default_model && is_text_generation(row))
        .with_context(|| format!("当前 Key 无法使用 {default_model}，原配置已保留"))?
        .clone()])
}

/// Metadata shape shared by Harness and OpenClaw; adapter mutation stays local.
pub(super) fn project_metadata(model: &Value, target: &mut Value) {
    for (source, name) in [
        ("context_window_tokens", "contextWindow"),
        ("max_output_tokens", "maxTokens"),
    ] {
        if let Some(tokens) = model[source].as_u64().filter(|n| *n > 0) {
            target[name] = serde_json::json!(tokens);
        }
    }
    if let Some(input) = model["input_modalities"].as_array() {
        let known: Vec<_> = input
            .iter()
            .filter(|v| matches!(v.as_str(), Some("text" | "image")))
            .cloned()
            .collect();
        if !known.is_empty() {
            target["input"] = serde_json::json!(known);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn text_generation_model_types_are_the_selection_contract() {
        let catalog = json!({"data":[
            {"id":"glm-5.2", "model_types":["TEXT_GENERATION"]},
            {"id":"other", "model_types":["TEXT_GENERATION", "IMAGE_GENERATION"]},
            {"id":"image", "model_types":["IMAGE_GENERATION"]},
            {"id":"missing"}
        ]});
        for tool in [ToolId::ClaudeCode, ToolId::Codex] {
            assert_eq!(
                select_with_model(tool, &catalog, "glm-5.2").unwrap()[0]["id"],
                "glm-5.2"
            );
        }
        let selected = select_with_model(ToolId::OpenCode, &catalog, "glm-5.2").unwrap();
        assert_eq!(
            selected
                .iter()
                .map(|r| r["id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["glm-5.2", "other"]
        );
    }

    #[test]
    fn default_model_matches_build_policy() {
        let expected = option_env!("TOKEN_MARKET_BUILD_DESKTOP_DEFAULT_MODEL").unwrap_or("glm-5.3");
        let catalogue = json!({"data":[
            {"id":"glm-5.2","model_types":["TEXT_GENERATION"]},
            {"id":"glm-5.3","model_types":["TEXT_GENERATION"]}
        ]});
        assert_eq!(
            select(ToolId::Codex, &catalogue).unwrap()[0]["id"],
            expected
        );
    }

    #[test]
    fn endpoint_metadata_cannot_replace_text_generation_type() {
        let wrong = json!({"data":[{"id":"glm-5.2","endpoints":["CLAUDE_MESSAGES"]}]});
        assert!(select_with_model(ToolId::ClaudeCode, &wrong, "glm-5.2").is_err());
    }

    #[test]
    fn single_model_selection_respects_environment_policy_and_model_type() {
        let catalogue = json!({"data":[
            {"id":"glm-5.2","model_types":["TEXT_GENERATION"]},
            {"id":"glm-5.3","model_types":["TEXT_GENERATION"]}
        ]});
        for tool in [ToolId::ClaudeCode, ToolId::Codex] {
            assert_eq!(
                select_with_model(tool, &catalogue, "glm-5.2").unwrap()[0]["id"],
                "glm-5.2"
            );
            assert_eq!(
                select_with_model(tool, &catalogue, "glm-5.3").unwrap()[0]["id"],
                "glm-5.3"
            );
        }
    }
}
