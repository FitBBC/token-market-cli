use crate::ToolId;
use anyhow::{bail, Context, Result};
use serde_json::Value;

/// The catalogue belongs to the newly issued actual key; endpoint metadata alone chooses transport compatibility.
pub(super) fn select(tool: ToolId, catalogue: &Value) -> Result<Vec<Value>> {
    let rows = catalogue["data"]
        .as_array()
        .context("模型目录无效，原配置已保留")?;
    if tool == ToolId::DeepSeekHarness {
        let mut models = vec![];
        let mut ids = std::collections::HashSet::new();
        for row in rows {
            let compatible = row["supported_endpoints"]
                .as_array()
                .is_some_and(|endpoints| {
                    endpoints
                        .iter()
                        .any(|endpoint| endpoint == "OPENAI_CHAT_COMPLETIONS")
                });
            if !compatible {
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
            bail!("当前 Key 没有支持 OPENAI_CHAT_COMPLETIONS 的模型，原配置已保留")
        }
        return Ok(models);
    }
    Ok(vec![rows
        .iter()
        .find(|row| row["id"] == "glm-5.3")
        .context("当前 Key 无法使用 glm-5.3，原配置已保留")?
        .clone()])
}
