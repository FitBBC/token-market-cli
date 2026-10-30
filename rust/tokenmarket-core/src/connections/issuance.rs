//! Client-only issuance protocol. Durable local receipt precedes configuration.
use super::*;
use sha2::{Digest, Sha256};

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct IssuanceReceipt {
    request_id: String,
    generation: u64,
    #[serde(default)]
    applied: bool,
}

#[derive(Serialize, Deserialize)]
struct PendingRequest {
    request_id: String,
}

#[derive(Deserialize)]
struct Reply {
    status: String,
    key_id: Option<String>,
    request_id: Option<String>,
    generation: Option<u64>,
    api_key: Option<String>,
    owner: DesktopIdentity,
    bound_workspace: DesktopIdentity,
}

/// A recoverable product condition, distinct from an inaccessible/corrupt credential store.
#[derive(Debug)]
pub struct CredentialLoss {
    pub key_id: String,
}
impl std::fmt::Display for CredentialLoss {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "本机保存的连接凭证已丢失，请确认重新接入")
    }
}
impl std::error::Error for CredentialLoss {}

pub(super) fn fingerprint(key: &str) -> String {
    format!("{:x}", Sha256::digest(key.as_bytes()))
}

impl ConnectionManager {
    pub(super) async fn validate_for_apply(
        &self,
        context: &ConnectContext,
        tool: ToolId,
        device: &str,
        key: &IssuedKey,
    ) -> Result<()> {
        let reply = self
            .key_operation(
                context,
                tool,
                device,
                "inspect",
                json!({"key_id":key.key_id,"fingerprint":fingerprint(&key.api_key)}),
            )
            .await?;
        if !matches!(reply.status.as_str(), "VALID" | "SAVED" | "APPLIED")
            || reply.key_id.as_deref() != Some(key.key_id.as_str())
            || key
                .issuance
                .as_ref()
                .is_some_and(|receipt| reply.generation != Some(receipt.generation))
        {
            bail!("接入期间连接或当前空间已改变，原工具配置已保留，请重新接入")
        }
        Ok(())
    }
    async fn key_operation(
        &self,
        context: &ConnectContext,
        tool: ToolId,
        device: &str,
        action: &str,
        extra: Value,
    ) -> Result<Reply> {
        let mut body = json!({"workspace_id":context.workspace_id,"device_id":device,"tool":tool});
        body.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let response = self
            .http
            .post(format!(
                "{}/api/client/keys/{action}",
                self.platform_url.trim_end_matches('/')
            ))
            .bearer_auth(&context.session_token)
            .header("X-Client-Owner-Id", &context.owner_id)
            .json(&body)
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await
            .context("连接服务暂时不可用，已保存的信息未清除，请重试")?;
        if !response.status().is_success() {
            bail!("连接校验失败，请检查登录和访问权限后重试")
        }
        let payload: Value = response.json().await.context("连接服务响应无效")?;
        if payload["success"] == false {
            let stage = match action {
                "inspect" => "凭证校验",
                "issue" => "凭证签发",
                "recover" => "凭证恢复",
                "confirm" => "凭证保存确认",
                "applied" => "接入完成确认",
                _ => "接入操作",
            };
            bail!("连接服务未能完成{stage}，请稍后重试；若持续失败，请联系管理员")
        }
        let reply: Reply = serde_json::from_value(payload.get("data").cloned().unwrap_or(payload))
            .context("连接服务响应不完整")?;
        if reply.owner.id != context.owner_id
            || reply.bound_workspace.id != context.workspace_id
            || reply
                .key_id
                .as_ref()
                .is_some_and(|id| id.is_empty() || !id.bytes().all(|c| c.is_ascii_digit()))
        {
            bail!("连接归属校验失败")
        }
        match reply.status.as_str() {
            "DISABLED" => bail!("连接已被停用，请到平台启用后重新接入"),
            "EXPIRED" => bail!("连接已过期，请到平台调整有效期后重新接入"),
            "VALID"
            | "NOT_FOUND"
            | "DELETED"
            | "SECRET_CHANGED"
            | "LOCAL_COPY_REQUIRED"
            | "PENDING"
            | "SAVED"
            | "APPLIED"
            | "ISSUED" => Ok(reply),
            _ => bail!("连接状态无法确认，已保存的信息未清除，请重试"),
        }
    }

    pub(super) async fn obtain_key(
        &self,
        context: &ConnectContext,
        tool: ToolId,
        device: &str,
        cached: Option<IssuedKey>,
        recovered: Option<String>,
        loss_confirmation: Option<&str>,
    ) -> Result<(IssuedKey, bool)> {
        let raw = cached.as_ref().map(|key| key.api_key.clone()).or(recovered);
        let inspect = self.key_operation(context, tool, device, "inspect", json!({
            "key_id":cached.as_ref().map(|key| &key.key_id),"fingerprint":raw.as_deref().map(fingerprint)
        })).await?;
        let reusable = matches!(
            inspect.status.as_str(),
            "VALID" | "SAVED" | "PENDING" | "APPLIED"
        );
        if reusable && raw.is_some() {
            let key = self.key_from_reply(inspect, raw.unwrap())?;
            self.remember_key(context, tool, device, &key)?;
            self.confirm_key(context, tool, device, &key, false).await?;
            return Ok((key, true));
        }
        let lost = matches!(
            inspect.status.as_str(),
            "LOCAL_COPY_REQUIRED" | "SAVED" | "APPLIED"
        );
        if lost && loss_confirmation != inspect.key_id.as_deref() {
            return Err(CredentialLoss {
                key_id: inspect.key_id.context("连接凭证标识缺失")?,
            }
            .into());
        }
        if loss_confirmation.is_some() && (!lost || loss_confirmation != inspect.key_id.as_deref())
        {
            bail!("连接已改变，请重新确认接入")
        }
        let scope = fingerprint(&format!(
            "{}\n{}\n{}\n{}\n{tool}",
            self.platform_url, context.owner_id, context.workspace_id, device
        ));
        let path = self
            .directory
            .join("issuances")
            .join(format!("{scope}.json"));
        let pending = match fs::read(&path) {
            Ok(bytes) => {
                serde_json::from_slice::<PendingRequest>(&self.private.decode(&path, &bytes)?)
                    .context("无法读取未完成的接入，请检查本地存储")?
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let pending = PendingRequest {
                    request_id: crate::auth::create_pkce().code_verifier,
                };
                self.private.write(&path, &serde_json::to_vec(&pending)?)?;
                pending
            }
            Err(_) => bail!("无法读取未完成的接入"),
        };
        let mut issued = if inspect.status == "PENDING" {
            self.key_operation(context, tool, device, "recover", json!({"key_id":inspect.key_id,"request_id":inspect.request_id,"generation":inspect.generation})).await?
        } else {
            self.key_operation(context, tool, device, "issue", json!({
                "key_id":inspect.key_id,"fingerprint":raw.as_deref().map(fingerprint),"request_id":pending.request_id,"replace_lost":lost
            })).await?
        };
        if issued.status == "PENDING" {
            issued = self.key_operation(context, tool, device, "recover", json!({"key_id":issued.key_id,"request_id":issued.request_id,"generation":issued.generation})).await?;
        }
        if issued.status != "ISSUED" {
            if matches!(
                issued.status.as_str(),
                "LOCAL_COPY_REQUIRED" | "SAVED" | "APPLIED"
            ) {
                return Err(CredentialLoss {
                    key_id: issued.key_id.context("连接凭证标识缺失")?,
                }
                .into());
            }
            bail!("接入状态已改变，请刷新后重试")
        }
        let raw = issued
            .api_key
            .take()
            .filter(|key| !key.is_empty())
            .context("接入服务未返回新的连接凭证")?;
        let key = self.key_from_reply(issued, raw)?;
        // If this fails, leave the server's pending issuance recoverable. Never start another issuance silently.
        self.remember_key(context, tool, device, &key)
            .context("无法保存连接信息，原连接未改变，请检查本地存储后重试")?;
        self.confirm_key(context, tool, device, &key, false).await?;
        Ok((key, false))
    }

    fn key_from_reply(&self, reply: Reply, raw: String) -> Result<IssuedKey> {
        let receipt = match (reply.request_id, reply.generation) {
            (Some(request_id), Some(generation)) => Some(IssuanceReceipt {
                request_id,
                generation,
                applied: matches!(reply.status.as_str(), "VALID" | "APPLIED"),
            }),
            _ if matches!(reply.status.as_str(), "VALID" | "APPLIED") => None,
            _ => bail!("签发确认信息不完整"),
        };
        Ok(IssuedKey {
            api_key: raw,
            key_id: reply.key_id.context("接入信息缺少标识")?,
            owner: reply.owner,
            bound_workspace: reply.bound_workspace,
            issuance: receipt,
        })
    }

    pub(super) async fn confirm_key(
        &self,
        context: &ConnectContext,
        tool: ToolId,
        device: &str,
        key: &IssuedKey,
        applied: bool,
    ) -> Result<()> {
        let Some(receipt) = &key.issuance else {
            return Ok(());
        };
        if receipt.applied {
            return Ok(());
        }
        let reply = self.key_operation(context, tool, device, if applied { "applied" } else { "confirm" }, json!({
            "key_id":key.key_id,"request_id":receipt.request_id,"generation":receipt.generation,"fingerprint":fingerprint(&key.api_key)
        })).await?;
        if !matches!(reply.status.as_str(), "SAVED" | "APPLIED")
            || applied && reply.status != "APPLIED"
        {
            bail!("连接确认未完成，已保存的信息和配置已保留，请重试")
        }
        Ok(())
    }
}
