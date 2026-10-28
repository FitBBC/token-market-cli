use super::{atomic_write, ConnectionRecord, DisconnectRequest, RestoreChoice, RestoreConflict};
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};

struct Change {
    path: PathBuf,
    before: Option<Vec<u8>>,
    after: Option<Vec<u8>>,
}
pub(super) struct RestorePlan {
    changes: Vec<Change>,
    pub revision: String,
    pub conflicts: Vec<RestoreConflict>,
}
fn read(path: &Path) -> Result<Option<Vec<u8>>> {
    if fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
        bail!("配置路径是符号链接，请先确认实际路径")
    }
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => bail!("无法读取配置，未执行恢复"),
    }
}
pub(super) fn prepare(
    record: &ConnectionRecord,
    request: &DisconnectRequest,
) -> Result<RestorePlan> {
    let mut changes = vec![];
    let mut hash = Sha256::new();
    hash.update(serde_json::to_vec(record)?);
    for receipt in &record.receipts {
        if changes.iter().any(|c: &Change| c.path == receipt.path) {
            bail!("连接恢复记录存在重复文件")
        }
        let before = read(&receipt.path)?;
        hash.update([u8::from(before.is_some())]);
        if let Some(bytes) = &before {
            hash.update((bytes.len() as u64).to_le_bytes());
            hash.update(bytes);
        }
        changes.push(Change {
            path: receipt.path.clone(),
            before,
            after: None,
        });
    }
    let revision = format!("{:x}", hash.finalize());
    let approved = request.revision.as_ref() == Some(&revision);
    let mut conflicts = vec![];
    for (file, (receipt, change)) in record.receipts.iter().zip(changes.iter_mut()).enumerate() {
        let format = Format::for_path(&receipt.path)?;
        let mut value = format.parse(change.before.as_deref())?;
        let original = value.clone();
        // Version 1 serialized missing and explicit null identically. Recover ONLY the owned
        // baseline fields from the original backup; never publish that backup over current files.
        let legacy_baseline = if record.version == 1 && receipt.existed {
            let backup = receipt
                .backup_path
                .as_ref()
                .context("旧连接缺少原始恢复基线，未执行恢复")?;
            Some(format.parse(Some(
                &read(backup)?.context("旧连接恢复基线不可读，未执行恢复")?,
            ))?)
        } else {
            None
        };
        let mut touched = vec![];
        for (field, receipt) in receipt.fields.iter().enumerate() {
            let path = tokens(&receipt.pointer)?;
            let baseline = if record.version == 1 {
                legacy_baseline
                    .as_ref()
                    .map(|v| lookup(v, &path))
                    .transpose()?
                    .flatten()
                    .cloned()
            } else {
                receipt.before.clone()
            };
            let current = lookup(&value, &path)?;
            if current == baseline.as_ref() {
                continue;
            }
            let collision = current != Some(&receipt.applied);
            let choice = approved
                .then(|| {
                    request
                        .resolutions
                        .iter()
                        .find(|r| r.file == file && r.field == field)
                })
                .flatten();
            if collision {
                match choice.map(|r| &r.choice) {
                    Some(RestoreChoice::KeepCurrent) => continue,
                    Some(RestoreChoice::RestoreOriginal) => {}
                    None => {
                        conflicts.push(RestoreConflict { file, field });
                        continue;
                    }
                }
            }
            set(&mut value, &receipt.pointer, baseline)?;
            touched.push(receipt.pointer.clone());
        }
        let mut containers = receipt.created_containers.clone();
        containers.sort_by_key(|pointer| std::cmp::Reverse(pointer.matches('/').count()));
        for pointer in containers {
            let path = tokens(&pointer)?;
            if lookup(&value, &path)?
                .is_some_and(|v| empty_container(v, path.last().map(String::as_str)))
            {
                set(&mut value, &pointer, None)?;
                touched.push(pointer);
            }
        }
        change.after = if original == value {
            change.before.clone()
        } else if !receipt.existed && empty_container(&value, None) {
            None
        } else {
            Some(format.render(change.before.as_deref(), &value, &touched)?)
        };
    }
    Ok(RestorePlan {
        changes,
        revision,
        conflicts,
    })
}
fn tokens(pointer: &str) -> Result<Vec<String>> {
    if !pointer.starts_with('/') {
        bail!("无效恢复字段")
    }
    Ok(pointer[1..]
        .split('/')
        .map(|s| s.replace("~1", "/").replace("~0", "~"))
        .collect())
}
fn selector_index(array: &[Value], token: &str) -> Result<Option<usize>> {
    if let Some(id) = token.strip_prefix("@id=") {
        let mut matching = array
            .iter()
            .enumerate()
            .filter(|(_, v)| v.get("id").and_then(Value::as_str) == Some(id));
        let index = matching.next().map(|(i, _)| i);
        if matching.next().is_some() {
            bail!("配置存在重复插件标识，未执行恢复")
        }
        Ok(index)
    } else {
        bail!("数组恢复必须使用稳定标识")
    }
}
fn lookup<'a>(value: &'a Value, path: &[String]) -> Result<Option<&'a Value>> {
    if path.is_empty() {
        return Ok(Some(value));
    }
    let child = if let Some(array) = value.as_array() {
        selector_index(array, &path[0])?.map(|i| &array[i])
    } else {
        value.get(&path[0])
    };
    match child {
        Some(child) => lookup(child, &path[1..]),
        None => Ok(None),
    }
}
fn empty_container(value: &Value, token: Option<&str>) -> bool {
    value.as_array().is_some_and(|v| v.is_empty())
        || value.as_object().is_some_and(|v| {
            v.is_empty()
                || (v.len() == 1
                    && token
                        .and_then(|s| s.strip_prefix("@id="))
                        .is_some_and(|id| v.get("id").and_then(Value::as_str) == Some(id)))
        })
}
fn set(value: &mut Value, pointer: &str, replacement: Option<Value>) -> Result<()> {
    set_path(value, &tokens(pointer)?, replacement)
}
fn set_path(value: &mut Value, path: &[String], replacement: Option<Value>) -> Result<()> {
    let key = path.first().context("无效恢复字段")?;
    if let Some(array) = value.as_array_mut() {
        let index = selector_index(array, key)?;
        if let Some(index) = index {
            if path.len() == 1 {
                if let Some(replacement) = replacement {
                    array[index] = replacement;
                } else {
                    array.remove(index);
                }
            } else {
                set_path(&mut array[index], &path[1..], replacement)?;
            }
        } else if replacement.is_some() {
            bail!("配置插件已移除，未执行恢复")
        }
        return Ok(());
    }
    let object = value
        .as_object_mut()
        .context("配置父级已改变，未执行恢复")?;
    if path.len() == 1 {
        if let Some(replacement) = replacement {
            object.insert(key.clone(), replacement);
        } else {
            object.remove(key);
        }
    } else {
        if !object.contains_key(key) {
            if replacement.is_none() {
                return Ok(());
            }
            object.insert(key.clone(), json!({}));
        }
        set_path(object.get_mut(key).unwrap(), &path[1..], replacement)?;
    }
    Ok(())
}
enum Format {
    Json,
    Toml,
    Yaml,
}
impl Format {
    fn for_path(path: &Path) -> Result<Self> {
        match path.extension().and_then(|v| v.to_str()) {
            Some("json") => Ok(Self::Json),
            Some("toml") => Ok(Self::Toml),
            Some("yaml" | "yml") => Ok(Self::Yaml),
            _ => bail!("不支持安全恢复此配置格式"),
        }
    }
    fn parse(&self, bytes: Option<&[u8]>) -> Result<Value> {
        let Some(bytes) = bytes else {
            return Ok(json!({}));
        };
        let result: Value = match self {
            Self::Json => serde_json::from_slice(bytes).context("配置不是有效 JSON，未执行恢复")?,
            Self::Yaml => serde_yaml::from_slice(bytes).context("配置不是有效 YAML，未执行恢复")?,
            Self::Toml => {
                toml_edit::de::from_str(std::str::from_utf8(bytes).context("配置编码无效")?)
                    .context("配置不是有效 TOML，未执行恢复")?
            }
        };
        if !result.is_object() && !result.is_array() {
            bail!("配置结构无效，未执行恢复")
        }
        Ok(result)
    }
    fn render(&self, bytes: Option<&[u8]>, value: &Value, touched: &[String]) -> Result<Vec<u8>> {
        match self {
            Self::Json => Ok(serde_json::to_vec_pretty(value)?),
            Self::Yaml => Ok(serde_yaml::to_string(value)?.into_bytes()),
            Self::Toml => {
                let mut document = std::str::from_utf8(bytes.unwrap_or(b""))
                    .context("配置编码无效")?
                    .parse::<toml_edit::DocumentMut>()
                    .context("配置不是有效 TOML")?;
                for pointer in touched {
                    let path = tokens(pointer)?;
                    edit_toml(document.as_item_mut(), &path, lookup(value, &path)?)?;
                }
                Ok(document.to_string().into_bytes())
            }
        }
    }
}
fn edit_toml(
    item: &mut toml_edit::Item,
    path: &[String],
    replacement: Option<&Value>,
) -> Result<()> {
    let table = item.as_table_like_mut().context("TOML 配置父级已改变")?;
    let key = &path[0];
    if path.len() == 1 {
        if let Some(value) = replacement {
            let mut next = json_toml(value)?;
            if let (Some(old), Some(new)) = (
                table.get(key).and_then(toml_edit::Item::as_value),
                next.as_value_mut(),
            ) {
                *new.decor_mut() = old.decor().clone();
            }
            table.insert(key, next);
        } else {
            table.remove(key);
        }
    } else {
        if table.get(key).is_none() {
            if replacement.is_none() {
                return Ok(());
            }
            table.insert(key, toml_edit::Item::Table(toml_edit::Table::new()));
        }
        edit_toml(
            table.get_mut(key).context("TOML 配置父级已改变")?,
            &path[1..],
            replacement,
        )?;
    }
    Ok(())
}
fn json_toml(value: &Value) -> Result<toml_edit::Item> {
    // Deserialize a one-field wrapper to preserve TOML's native type handling.
    let encoded =
        toml_edit::ser::to_string(&json!({"value":value})).context("恢复值无法写为 TOML")?;
    let mut doc = encoded.parse::<toml_edit::DocumentMut>()?;
    doc.as_table_mut().remove("value").context("恢复值无效")
}
impl RestorePlan {
    pub fn commit(&self, store: &Path, original_store: &[u8], records: &[u8]) -> Result<()> {
        if read(store)?.as_deref() != Some(original_store) {
            bail!("连接记录已改变，请刷新后重试")
        }
        // Recheck every touched file before writing any of them.
        for change in &self.changes {
            if read(&change.path)? != change.before {
                bail!("配置已改变，请重新确认恢复")
            }
        }
        let mut written: Vec<&Change> = vec![];
        for change in &self.changes {
            if change.before == change.after {
                continue;
            }
            if apply(&change.path, &change.after).is_err() {
                rollback(&written)?;
                bail!("本地恢复失败，原配置和连接记录已保留")
            }
            written.push(change);
        }
        if atomic_write(store, records).is_err() {
            rollback(&written)?;
            bail!("连接记录保存失败，原配置已恢复")
        }
        Ok(())
    }
}
fn apply(path: &Path, bytes: &Option<Vec<u8>>) -> Result<()> {
    if let Some(bytes) = bytes {
        atomic_write(path, bytes)
    } else {
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}
fn rollback(written: &[&Change]) -> Result<()> {
    for change in written.iter().rev() {
        apply(&change.path, &change.before)
            .context("本地恢复失败且回滚失败，请保留备份并检查配置")?;
    }
    Ok(())
}
