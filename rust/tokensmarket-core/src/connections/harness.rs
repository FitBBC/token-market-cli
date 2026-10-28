//! Harness profile and credential mutations share one connection transaction.
use super::{configuration::PreparedFile, FieldReceipt};
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub(super) struct HarnessConfig {
    profile: Document,
    credentials: Document,
}
struct Document {
    path: PathBuf,
    before: Option<Vec<u8>>,
    value: Value,
    fields: Vec<FieldReceipt>,
    containers: Vec<String>,
}
impl Document {
    fn read(path: PathBuf, default: Value) -> Result<Self> {
        if fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
            bail!("Harness 配置是符号链接，请确认实际配置路径");
        }
        let before = match fs::read(&path) {
            Ok(bytes) => Some(bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(_) => bail!("无法读取 Harness 配置"),
        };
        let value = match &before {
            Some(bytes) => {
                serde_yaml::from_slice(bytes).context("Harness 配置不是有效 YAML，请先修复")?
            }
            None => default,
        };
        Ok(Self {
            path,
            before,
            value,
            fields: vec![],
            containers: vec![],
        })
    }
    fn prepared(self) -> Result<PreparedFile> {
        Ok(PreparedFile {
            path: self.path,
            before: self.before,
            bytes: serde_yaml::to_string(&self.value)?.into_bytes(),
            fields: self.fields,
            created_containers: self.containers,
        })
    }
}
impl HarnessConfig {
    pub fn read(path: &Path) -> Result<Self> {
        let home = if path
            .parent()
            .and_then(Path::parent)
            .is_some_and(|p| p.file_name().is_some_and(|n| n == "profiles"))
        {
            path.parent()
                .and_then(Path::parent)
                .and_then(Path::parent)
                .context("Harness 配置路径无效")?
        } else {
            path.parent().context("Harness 配置路径无效")?
        };
        let profile = Document::read(path.into(), json!([]))?;
        let rows = profile
            .value
            .as_array()
            .context("Harness profile 必须是配置条目数组")?;
        let matches: Vec<_> = rows.iter().filter(|row| row["id"] == "llm-pi-ai").collect();
        if matches.len() > 1 {
            bail!("Harness profile 存在重复 llm-pi-ai 条目，请先修复")
        }
        if let Some(row) = matches.first() {
            for pointer in [
                "/config",
                "/config/providers",
                "/config/providers/tokensmarket",
            ] {
                if row.pointer(pointer).is_some_and(|v| !v.is_object()) {
                    bail!("Harness provider 配置结构无效")
                }
            }
            // Explicit model lists and modelOverrides are mutually exclusive in Harness.
            if row
                .pointer("/config/providers/tokensmarket/modelOverrides")
                .is_some()
            {
                bail!("Harness Token Market 路由已有 modelOverrides，请先确认路由配置")
            }
        }
        let credentials = Document::read(home.join(".credentials.yaml"), json!({}))?;
        if !credentials.value.is_object() {
            bail!("Harness 凭据结构无效")
        }
        if let Some(version) = credentials.value.get("version") {
            if version != 1
                || !credentials.value.get("refs").is_some_and(Value::is_object)
                || !credentials
                    .value
                    .get("records")
                    .is_some_and(Value::is_object)
            {
                bail!("Harness 凭据版本或结构无效")
            }
        } else if credentials.value.get("refs").is_some()
            || credentials.value.get("records").is_some()
        {
            bail!("Harness 凭据缺少版本，请先确认格式")
        }
        Ok(Self {
            profile,
            credentials,
        })
    }
    pub fn apply(
        mut self,
        gateway: &str,
        key: &str,
        models: &[Value],
    ) -> Result<Vec<PreparedFile>> {
        let rows = self.profile.value.as_array_mut().unwrap();
        let index = if let Some(index) = rows.iter().position(|row| row["id"] == "llm-pi-ai") {
            index
        } else {
            rows.push(json!({"id":"llm-pi-ai"}));
            self.profile.containers.push("/@id=llm-pi-ai".into());
            rows.len() - 1
        };
        let entry = &mut rows[index];
        let prefix = "/@id=llm-pi-ai";
        for pointer in [
            "/config",
            "/config/providers",
            "/config/providers/tokensmarket",
        ] {
            if entry.pointer(pointer).is_none() {
                let (parent, leaf) = pointer.rsplit_once('/').unwrap();
                entry
                    .pointer_mut(parent)
                    .unwrap()
                    .as_object_mut()
                    .unwrap()
                    .insert(leaf.into(), json!({}));
                self.profile.containers.push(format!("{prefix}{pointer}"));
            }
        }
        let route = entry
            .pointer_mut("/config/providers/tokensmarket")
            .unwrap()
            .as_object_mut()
            .unwrap();
        let model_rows: Vec<Value> = models
            .iter()
            .map(|model| {
                let mut row = json!({"id":model["id"]});
                if let Some(name) = model["display_name"]
                    .as_str()
                    .filter(|name| !name.is_empty())
                {
                    row["name"] = json!(name)
                }
                super::model_plan::project_metadata(model, &mut row);
                row
            })
            .collect();
        for (name, applied) in [
            ("api", json!("openai-completions")),
            (
                "baseURL",
                json!(format!("{}/v1", gateway.trim_end_matches('/'))),
            ),
            ("apiKeyEnv", json!("TOKEN_MARKET_API_KEY")),
            ("models", json!(model_rows)),
        ] {
            self.profile.fields.push(FieldReceipt {
                pointer: format!("{prefix}/config/providers/tokensmarket/{name}"),
                before: route.insert(name.into(), applied.clone()),
                applied,
            });
        }
        if self.credentials.before.is_none() {
            for (name, applied) in [("version", json!(1)), ("records", json!({}))] {
                self.credentials.value[name] = applied.clone();
                self.credentials.fields.push(FieldReceipt {
                    pointer: format!("/{name}"),
                    before: None,
                    applied,
                });
            }
            self.credentials.value["refs"] = json!({});
            self.credentials.containers.push("/refs".into());
        }
        let pointer = if self.credentials.value.get("version").is_some() {
            "/refs/TOKEN_MARKET_API_KEY"
        } else {
            "/TOKEN_MARKET_API_KEY"
        };
        let before = self.credentials.value.pointer(pointer).cloned();
        if pointer.starts_with("/refs/") {
            self.credentials.value["refs"]["TOKEN_MARKET_API_KEY"] = json!(key)
        } else {
            self.credentials.value["TOKEN_MARKET_API_KEY"] = json!(key)
        }
        self.credentials.fields.push(FieldReceipt {
            pointer: pointer.into(),
            before,
            applied: json!(key),
        });
        Ok(vec![self.profile.prepared()?, self.credentials.prepared()?])
    }
}
