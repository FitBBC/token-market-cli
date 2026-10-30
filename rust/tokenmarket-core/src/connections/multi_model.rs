//! Desktop multi-model imports. Legacy terminal adapters retain their existing behavior.
use super::{configuration::PreparedConfiguration, FieldReceipt};
use crate::ToolId;
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

pub(super) struct MultiModelConfig {
    tool: ToolId,
    value: Value,
    original: Option<Vec<u8>>,
}
fn escaped(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}
impl MultiModelConfig {
    pub fn credential(&self, gateway: &str) -> Result<Option<String>> {
        let route = if self.tool == ToolId::OpenCode {
            self.value.pointer("/provider/tokenmarket/options")
        } else if self.tool == ToolId::OpenClaw {
            self.value.pointer("/models/providers/tokenmarket")
        } else {
            None
        };
        if let Some(route) = route {
            let base = route
                .get("baseURL")
                .or_else(|| route.get("baseUrl"))
                .and_then(Value::as_str);
            if base.is_some_and(|base| {
                base.trim_end_matches('/') == format!("{}/v1", gateway.trim_end_matches('/'))
            }) {
                return Ok(route
                    .get("apiKey")
                    .and_then(Value::as_str)
                    .filter(|key| !key.is_empty())
                    .map(str::to_owned));
            }
        }
        if self.tool == ToolId::WorkBuddy {
            let rows = self
                .value
                .as_array()
                .or_else(|| self.value.get("models").and_then(Value::as_array));
            let mut found = None;
            for row in rows.into_iter().flatten() {
                if row.get("url").and_then(Value::as_str).is_some_and(|base| {
                    base.trim_end_matches('/')
                        == format!("{}/v1/chat/completions", gateway.trim_end_matches('/'))
                }) {
                    if let Some(key) = row
                        .get("apiKey")
                        .and_then(Value::as_str)
                        .filter(|key| !key.is_empty())
                    {
                        if found.as_deref().is_some_and(|old| old != key) {
                            bail!("现有配置包含多份连接凭证，请先确认实际连接")
                        }
                        found = Some(key.to_owned());
                    }
                }
            }
            return Ok(found);
        }
        Ok(None)
    }
    pub fn parse(tool: ToolId, before: &Option<Vec<u8>>) -> Result<Self> {
        // OpenCode supports lossless JSONC; the other adapters retain strict JSON validation.
        let value: Value = match before {
            Some(bytes) if tool == ToolId::OpenCode => super::jsonc::parse(bytes)?,
            Some(bytes) => {
                serde_json::from_slice(bytes).context("工具配置不是有效 JSON，请先修复")?
            }
            None if tool == ToolId::WorkBuddy => json!([]),
            None => json!({}),
        };
        if tool == ToolId::OpenCode {
            validate_objects(
                &value,
                &[
                    "",
                    "/provider",
                    "/provider/tokenmarket",
                    "/provider/tokenmarket/options",
                    "/provider/tokenmarket/models",
                ],
            )?;
            if let Some(models) = value
                .pointer("/provider/tokenmarket/models")
                .and_then(Value::as_object)
            {
                for model in models.values() {
                    if !model.is_object() || model.get("limit").is_some_and(|v| !v.is_object()) {
                        bail!("OpenCode 模型和 limit 配置必须是对象")
                    }
                }
            }
            return Ok(Self {
                tool,
                value,
                original: before.clone(),
            });
        }
        if tool == ToolId::OpenClaw {
            validate_objects(
                &value,
                &[
                    "",
                    "/models",
                    "/models/providers",
                    "/models/providers/tokenmarket",
                ],
            )?;
            if let Some(models) = value.pointer("/models/providers/tokenmarket/models") {
                validate_rows(models)?;
            }
            return Ok(Self {
                tool,
                value,
                original: before.clone(),
            });
        }
        if tool != ToolId::WorkBuddy {
            bail!("此工具的多模型配置尚未开放")
        }
        let rows = if value.is_array() {
            value.as_array().unwrap()
        } else {
            let object = value
                .as_object()
                .context("WorkBuddy 配置必须是数组或 models 对象")?;
            if let Some(available) = object.get("availableModels") {
                let mut seen = std::collections::HashSet::new();
                for id in available
                    .as_array()
                    .context("WorkBuddy availableModels 必须是字符串数组")?
                {
                    let id = id
                        .as_str()
                        .context("WorkBuddy availableModels 必须是字符串数组")?;
                    if !seen.insert(id) {
                        bail!("WorkBuddy availableModels 存在重复标识")
                    }
                }
            }
            object
                .get("models")
                .context("WorkBuddy 配置缺少 models 数组")?
                .as_array()
                .context("WorkBuddy models 必须是数组")?
        };
        let mut seen = std::collections::HashSet::new();
        for row in rows {
            let id = row
                .get("id")
                .and_then(Value::as_str)
                .filter(|id| !id.trim().is_empty())
                .context("WorkBuddy 模型必须包含非空 id")?;
            for name in ["name", "vendor", "url", "apiKey"] {
                if row.get(name).is_some_and(|v| !v.is_string()) {
                    bail!("WorkBuddy 模型字符串字段类型无效")
                }
            }
            for name in [
                "supportsToolCall",
                "supportsImages",
                "supportsReasoning",
                "onlyReasoning",
                "useCustomProtocol",
            ] {
                if row.get(name).is_some_and(|v| !v.is_boolean()) {
                    bail!("WorkBuddy 模型能力字段类型无效")
                }
            }
            for name in ["maxInputTokens", "maxOutputTokens", "temperature"] {
                if row.get(name).is_some_and(|v| {
                    v.as_f64()
                        .or_else(|| v.as_str().and_then(|s| s.parse::<f64>().ok()))
                        .is_none_or(|v| !v.is_finite())
                }) {
                    bail!("WorkBuddy 模型数值字段类型无效")
                }
            }
            if row.get("reasoning").is_some_and(|v| !v.is_object()) {
                bail!("WorkBuddy reasoning 必须是对象")
            }
            if !row.is_object() || !seen.insert(id) {
                bail!("WorkBuddy 模型结构或标识重复，请先修复")
            }
        }
        Ok(Self {
            tool,
            value,
            original: before.clone(),
        })
    }
    pub fn apply(
        &mut self,
        gateway: &str,
        key: &str,
        models: &[Value],
    ) -> Result<PreparedConfiguration> {
        let mut fields = vec![];
        let mut created_containers = vec![];
        if self.tool == ToolId::WorkBuddy {
            let prefix = if self.value.is_array() { "" } else { "/models" };
            let rows = if prefix.is_empty() {
                self.value.as_array_mut().unwrap()
            } else {
                self.value["models"].as_array_mut().unwrap()
            };
            for model in models {
                let id = model["id"].as_str().context("模型 ID 无效")?;
                let selector = format!("{prefix}/@id={}", escaped(id));
                let index = if let Some(i) = rows.iter().position(|row| row["id"] == id) {
                    i
                } else {
                    rows.push(json!({"id":id}));
                    created_containers.push(selector.clone());
                    rows.len() - 1
                };
                let row = rows[index].as_object_mut().unwrap();
                let mut values = vec![
                    (
                        "name",
                        json!(model["display_name"]
                            .as_str()
                            .filter(|name| !name.is_empty())
                            .unwrap_or(id)),
                    ),
                    (
                        "url",
                        json!(format!(
                            "{}/v1/chat/completions",
                            gateway.trim_end_matches('/')
                        )),
                    ),
                    ("apiKey", json!(key)),
                    ("vendor", json!("Token Market")),
                    ("useCustomProtocol", json!(true)),
                ];
                if let Some(tokens) = model["max_output_tokens"].as_u64().filter(|n| *n > 0) {
                    values.push(("maxOutputTokens", json!(tokens)));
                }
                for (name, applied) in values {
                    fields.push(FieldReceipt {
                        retired: false,
                        pointer: format!("{selector}/{name}"),
                        before: row.insert(name.into(), applied.clone()),
                        applied,
                    });
                }
            }
            if let Some(available) = self
                .value
                .get_mut("availableModels")
                .and_then(Value::as_array_mut)
            {
                for model in models {
                    let id = model["id"].as_str().unwrap();
                    let before = available.iter().find(|v| v.as_str() == Some(id)).cloned();
                    if before.is_none() {
                        available.push(json!(id));
                    }
                    fields.push(FieldReceipt {
                        retired: false,
                        pointer: format!("/availableModels/@value={}", escaped(id)),
                        before,
                        applied: json!(id),
                    });
                }
            }
        }
        if self.tool == ToolId::OpenClaw {
            let prefix = "/models/providers/tokenmarket";
            ensure_objects(
                &mut self.value,
                &["/models", "/models/providers", prefix],
                &mut created_containers,
            );
            for (name, applied) in [
                (
                    "baseUrl",
                    json!(format!("{}/v1", gateway.trim_end_matches('/'))),
                ),
                ("apiKey", json!(key)),
                ("api", json!("openai-completions")),
            ] {
                put(
                    &mut self.value,
                    &format!("{prefix}/{name}"),
                    applied,
                    &mut fields,
                );
            }
            if self.value.pointer(&format!("{prefix}/models")).is_none() {
                self.value.pointer_mut(prefix).unwrap()["models"] = json!([]);
                created_containers.push(format!("{prefix}/models"));
            }
            let rows = self
                .value
                .pointer_mut(&format!("{prefix}/models"))
                .unwrap()
                .as_array_mut()
                .unwrap();
            for model in models {
                let id = model["id"].as_str().unwrap();
                let selector = format!("{prefix}/models/@id={}", escaped(id));
                let index = if let Some(i) = rows.iter().position(|row| row["id"] == id) {
                    i
                } else {
                    rows.push(json!({"id":id}));
                    created_containers.push(selector.clone());
                    rows.len() - 1
                };
                let row = rows[index].as_object_mut().unwrap();
                let mut metadata = json!({"name":model["display_name"].as_str().filter(|s|!s.is_empty()).unwrap_or(id)});
                super::model_plan::project_metadata(model, &mut metadata);
                for (name, applied) in metadata.as_object().unwrap() {
                    fields.push(FieldReceipt {
                        retired: false,
                        pointer: format!("{selector}/{name}"),
                        before: row.insert(name.clone(), applied.clone()),
                        applied: applied.clone(),
                    });
                }
            }
        }
        if self.tool == ToolId::OpenCode {
            let prefix = "/provider/tokenmarket";
            ensure_objects(
                &mut self.value,
                &[
                    "/provider",
                    prefix,
                    "/provider/tokenmarket/options",
                    "/provider/tokenmarket/models",
                ],
                &mut created_containers,
            );
            for (suffix, applied) in [
                ("npm", json!("@ai-sdk/openai-compatible")),
                ("name", json!("Token Market")),
                (
                    "options/baseURL",
                    json!(format!("{}/v1", gateway.trim_end_matches('/'))),
                ),
                ("options/apiKey", json!(key)),
            ] {
                put(
                    &mut self.value,
                    &format!("{prefix}/{suffix}"),
                    applied,
                    &mut fields,
                );
            }
            for model in models {
                let id = model["id"].as_str().unwrap();
                let pointer = format!("{prefix}/models/{}", escaped(id));
                // Object keys retain the original model ID; RFC6901 escaping is receipt-only.
                if self.value.pointer(&pointer).is_none() {
                    self.value.pointer_mut(&format!("{prefix}/models")).unwrap()[id] = json!({});
                    created_containers.push(pointer.clone());
                }
                put(
                    &mut self.value,
                    &format!("{pointer}/name"),
                    json!(model["display_name"]
                        .as_str()
                        .filter(|name| !name.is_empty())
                        .unwrap_or(id)),
                    &mut fields,
                );
                if let (Some(context), Some(output)) = (
                    model["context_window_tokens"].as_u64().filter(|n| *n > 0),
                    model["max_output_tokens"].as_u64().filter(|n| *n > 0),
                ) {
                    validate_objects(&self.value, &[&format!("{pointer}/limit")])?;
                    ensure_objects(
                        &mut self.value,
                        &[&format!("{pointer}/limit")],
                        &mut created_containers,
                    );
                    put(
                        &mut self.value,
                        &format!("{pointer}/limit/context"),
                        json!(context),
                        &mut fields,
                    );
                    put(
                        &mut self.value,
                        &format!("{pointer}/limit/output"),
                        json!(output),
                        &mut fields,
                    );
                }
            }
        }
        Ok(PreparedConfiguration {
            bytes: if self.tool == ToolId::OpenCode {
                super::jsonc::render(self.original.as_deref(), &self.value)?
            } else {
                serde_json::to_vec_pretty(&self.value)?
            },
            fields,
            created_containers,
            additional_files: vec![],
            protocol: "OPENAI_CHAT_COMPLETIONS",
        })
    }
}

fn validate_objects(value: &Value, pointers: &[&str]) -> Result<()> {
    for pointer in pointers {
        if value.pointer(pointer).is_some_and(|v| !v.is_object()) {
            bail!("工具 provider 配置必须是对象，请先修复")
        }
    }
    Ok(())
}
fn validate_rows(models: &Value) -> Result<()> {
    let mut seen = std::collections::HashSet::new();
    for row in models.as_array().context("工具 models 必须是数组")? {
        let id = row
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.trim().is_empty())
            .context("工具模型必须包含非空 id")?;
        if row
            .get("name")
            .and_then(Value::as_str)
            .filter(|name| !name.trim().is_empty())
            .is_none()
        {
            bail!("OpenClaw 模型必须包含非空 name")
        }
        if !seen.insert(id) {
            bail!("工具模型存在重复 id")
        }
    }
    Ok(())
}
fn ensure_objects(value: &mut Value, pointers: &[&str], created: &mut Vec<String>) {
    for pointer in pointers {
        if value.pointer(pointer).is_none() {
            let (parent, leaf) = pointer.rsplit_once('/').unwrap();
            value.pointer_mut(parent).unwrap()[leaf] = json!({});
            created.push((*pointer).into());
        }
    }
}
fn put(value: &mut Value, pointer: &str, applied: Value, fields: &mut Vec<FieldReceipt>) {
    let (parent, leaf) = pointer.rsplit_once('/').unwrap();
    let before = value
        .pointer_mut(parent)
        .unwrap()
        .as_object_mut()
        .unwrap()
        .insert(leaf.into(), applied.clone());
    fields.push(FieldReceipt {
        retired: false,
        pointer: pointer.into(),
        before,
        applied,
    });
}
