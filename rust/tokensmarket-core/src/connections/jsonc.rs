//! Lossless OpenCode JSONC editing through dprint's concrete syntax tree.
use anyhow::{bail, Context, Result};
use jsonc_parser::{
    cst::{CstInputValue, CstNode, CstObject, CstRootNode},
    ParseOptions,
};
use serde_json::Value;

fn options() -> ParseOptions {
    ParseOptions {
        allow_comments: true,
        allow_trailing_commas: true,
        allow_loose_object_property_names: false,
        allow_missing_commas: false,
        allow_single_quoted_strings: false,
        allow_hexadecimal_numbers: false,
        allow_unary_plus_numbers: false,
        allow_bare_decimal_point_numbers: false,
        allow_non_finite_numbers: false,
        allow_extended_string_escapes: false,
    }
}
fn document(bytes: &[u8]) -> Result<CstRootNode> {
    let root = CstRootNode::parse(std::str::from_utf8(bytes)?, &options())
        .context("OpenCode 配置不是有效 JSONC")?;
    validate(root.value().context("OpenCode 配置为空")?)?;
    Ok(root)
}
fn validate(node: CstNode) -> Result<()> {
    if let Some(object) = node.as_object() {
        let mut names = std::collections::HashSet::new();
        for prop in object.properties() {
            if !names.insert(
                prop.name()
                    .context("JSONC 字段无名称")?
                    .decoded_value()
                    .map_err(|_| anyhow::anyhow!("JSONC 字段编码无效"))?,
            ) {
                bail!("JSONC 存在重复字段，请先修复")
            }
        }
    }
    for child in node.children() {
        validate(child)?;
    }
    Ok(())
}
pub(super) fn parse(bytes: &[u8]) -> Result<Value> {
    document(bytes)?
        .to_serde_value()
        .context("OpenCode 配置结构无效")
}
fn input(value: &Value) -> CstInputValue {
    match value {
        Value::Null => CstInputValue::Null,
        Value::Bool(v) => CstInputValue::Bool(*v),
        Value::Number(v) => CstInputValue::Number(v.to_string()),
        Value::String(v) => CstInputValue::String(v.clone()),
        Value::Array(v) => CstInputValue::Array(v.iter().map(input).collect()),
        Value::Object(v) => {
            CstInputValue::Object(v.iter().map(|(k, v)| (k.clone(), input(v))).collect())
        }
    }
}
fn edit(object: CstObject, desired: &serde_json::Map<String, Value>) -> Result<()> {
    for property in object.properties() {
        let name = property
            .name()
            .context("JSONC 字段无名称")?
            .decoded_value()
            .map_err(|_| anyhow::anyhow!("JSONC 字段编码无效"))?;
        if !desired.contains_key(&name) {
            property.remove();
        }
    }
    for (name, value) in desired {
        if let Some(prop) = object.get(name) {
            if prop.value().and_then(|v| v.to_serde_value()).as_ref() == Some(value) {
                continue;
            }
            if let (Some(child), Some(desired)) = (prop.object_value(), value.as_object()) {
                edit(child, desired)?;
            } else {
                prop.set_value(input(value));
            }
        } else {
            object.append(name, input(value));
        }
    }
    Ok(())
}
fn comments(node: CstNode, out: &mut Vec<String>) {
    if node.as_comment().is_some() {
        out.push(node.to_string());
    }
    for child in node.children() {
        comments(child, out);
    }
}
pub(super) fn render(bytes: Option<&[u8]>, value: &Value) -> Result<Vec<u8>> {
    let root = document(bytes.unwrap_or(b"{}"))?;
    let mut original_comments = vec![];
    for child in root.children() {
        comments(child, &mut original_comments);
    }
    edit(
        root.object_value().context("OpenCode 配置必须是对象")?,
        value.as_object().context("OpenCode 配置必须是对象")?,
    )?;
    let mut retained = vec![];
    for child in root.children() {
        comments(child, &mut retained);
    }
    let mut result = root.to_string();
    // A deleted owned container may contain a later user comment. Retain that content
    // as a standalone comment, rather than keeping a competing provider or losing it.
    for comment in original_comments {
        if let Some(index) = retained.iter().position(|v| v == &comment) {
            retained.remove(index);
        } else {
            result.push('\n');
            result.push_str(&comment);
            result.push('\n');
        }
    }
    Ok(result.into_bytes())
}

pub(super) fn has_comments(bytes: &[u8]) -> Result<bool> {
    let mut found = vec![];
    for node in document(bytes)?.children() {
        comments(node, &mut found);
    }
    Ok(!found.is_empty())
}
