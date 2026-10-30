//! Workspace/account operations share platform preference and never touch Agent files.
use crate::desktop::DesktopSession;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use url::Url;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceNode {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub role_code: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub switchable: Option<bool>,
    #[serde(default)]
    pub r#type: Option<String>,
    #[serde(default)]
    pub default_workspace: Option<bool>,
    #[serde(default)]
    pub children: Vec<WorkspaceNode>,
}
impl WorkspaceNode {
    pub fn selectable(&self) -> bool {
        self.switchable != Some(false) && self.status.as_deref().is_none_or(|s| s == "ACTIVE")
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct WorkspaceBalance {
    pub account_workspace_id: String,
    pub amount: String,
    pub currency: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct WorkspaceSnapshot {
    pub owner_user_id: String,
    pub current_workspace: WorkspaceNode,
    pub switchable_workspaces: Vec<WorkspaceNode>,
    pub user: Value,
    pub menus: Vec<Value>,
    pub balance: Option<WorkspaceBalance>,
    pub balance_error: Option<String>,
    pub can_recharge: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ContextResponse {
    current_workspace: WorkspaceNode,
    #[serde(default)]
    switchable_workspaces: Vec<WorkspaceNode>,
    #[serde(default)]
    billing_workspaces: Vec<WorkspaceNode>,
    #[serde(default)]
    menus: Vec<Value>,
}

pub struct DesktopWorkspace {
    base_url: String,
    web_url: String,
    http: reqwest::Client,
}
impl DesktopWorkspace {
    pub fn new(base_url: String, web_url: String) -> Self {
        Self {
            base_url,
            web_url,
            http: reqwest::Client::new(),
        }
    }
    async fn request<T: for<'de> Deserialize<'de>>(
        &self,
        session: &DesktopSession,
        method: reqwest::Method,
        path: &str,
        workspace: Option<&str>,
        body: Option<Value>,
    ) -> Result<T> {
        let mut request = self
            .http
            .request(
                method,
                format!("{}{}", self.base_url.trim_end_matches('/'), path),
            )
            .bearer_auth(&session.session_token)
            .timeout(std::time::Duration::from_secs(30));
        if let Some(workspace) = workspace {
            request = request.header("X-Workspace-Id", workspace);
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.context("无法连接空间服务，请重试")?;
        let status = response.status();
        let payload: Value = response.json().await.context("空间服务返回无效响应")?;
        if !status.is_success() || payload.get("success").and_then(Value::as_bool) == Some(false) {
            bail!("空间操作失败，请刷新登录状态并检查访问权限");
        }
        serde_json::from_value(payload.get("data").cloned().unwrap_or(payload))
            .context("空间服务响应缺少必要字段")
    }
    pub async fn load(&self, session: &DesktopSession) -> Result<WorkspaceSnapshot> {
        let context: ContextResponse = self
            .request(
                session,
                reqwest::Method::GET,
                "/api/client/workspaces/context",
                None,
                None,
            )
            .await?;
        self.snapshot(session, context).await
    }
    pub async fn switch(
        &self,
        session: &DesktopSession,
        target: &str,
    ) -> Result<WorkspaceSnapshot> {
        let context: ContextResponse = self
            .request(
                session,
                reqwest::Method::GET,
                "/api/client/workspaces/context",
                None,
                None,
            )
            .await?;
        if !contains_selectable(&context.switchable_workspaces, target) {
            bail!("该空间不可切换，请刷新后重试");
        }
        let saved: bool = self
            .request(
                session,
                reqwest::Method::POST,
                "/api/client/workspaces/current",
                Some(target),
                Some(json!({"workspaceId":target})),
            )
            .await?;
        if !saved {
            bail!("默认空间未保存，请重试");
        }
        // A read of the selected context preserves server object permission checks.
        let path = format!("/api/client/workspaces/context?workspaceId={}", id(target)?);
        let context: ContextResponse = self
            .request(session, reqwest::Method::GET, &path, Some(target), None)
            .await?;
        if context.current_workspace.id != target {
            bail!("空间上下文已改变，请刷新后重试");
        }
        self.snapshot(session, context).await
    }
    async fn snapshot(
        &self,
        session: &DesktopSession,
        context: ContextResponse,
    ) -> Result<WorkspaceSnapshot> {
        let target = id(&context.current_workspace.id)?;
        let profile: Value = self
            .request(
                session,
                reqwest::Method::GET,
                "/api/client/profile",
                Some(target),
                None,
            )
            .await?;
        let user = json!({"display_name":profile["display_name"],"user_name":profile["user_name"],"email":profile["email"],"avatar_url":profile["avatar_url"],"role":profile["role"]});
        let can_recharge = contains_selectable(&context.billing_workspaces, target)
            && menu_allows(&context.menus, "billing");
        let result = self.balance(session, target).await;
        let (balance, balance_error) = match result {
            Ok(balance) => (Some(balance), None),
            Err(_) => (None, Some("无法读取当前空间余额，请刷新重试".into())),
        };
        Ok(WorkspaceSnapshot {
            owner_user_id: session.user.id.clone(),
            current_workspace: context.current_workspace,
            switchable_workspaces: context.switchable_workspaces,
            user,
            menus: context.menus,
            balance,
            balance_error,
            can_recharge,
        })
    }
    pub async fn balance(
        &self,
        session: &DesktopSession,
        target: &str,
    ) -> Result<WorkspaceBalance> {
        let target = id(target)?;
        let value: Value = self
            .request(
                session,
                reqwest::Method::GET,
                &format!("/api/client/workspaces/{target}/balance"),
                Some(target),
                None,
            )
            .await?;
        let amount = match &value["balanceAmount"] {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            _ => bail!("余额缺少金额"),
        };
        if !decimal(&amount) {
            bail!("余额金额无效");
        }
        let currency = value["currency"]
            .as_str()
            .filter(|s| s.len() == 3 && s.bytes().all(|b| b.is_ascii_uppercase()))
            .context("余额缺少服务端币种")?;
        let account = value["workspaceId"].as_str().context("余额缺少账户空间")?;
        Ok(WorkspaceBalance {
            account_workspace_id: account.into(),
            amount,
            currency: currency.into(),
        })
    }
    pub fn browser_url(&self, user: &str, action: &str) -> Result<String> {
        id(user)?;
        let mut url = Url::parse(&self.web_url).context("浏览器站点地址无效")?;
        if (url.scheme() != "https"
            && !(url.scheme() == "http"
                && matches!(url.host_str(), Some("localhost" | "127.0.0.1"))))
            || !url.username().is_empty()
            || url.password().is_some()
        {
            bail!("浏览器站点必须使用安全连接");
        }
        url.set_path(if action == "recharge" {
            "/desktop/recharge"
        } else {
            "/desktop/console"
        });
        url.set_query(None);
        url.set_fragment(None);
        url.query_pairs_mut().append_pair("user_id", user);
        if action != "recharge" {
            if !matches!(action, "profile" | "tokens" | "billing" | "workspaces") {
                bail!("不可用的账号菜单");
            }
            url.query_pairs_mut().append_pair("tab", action);
        }
        Ok(url.to_string())
    }
}
fn id(value: &str) -> Result<&str> {
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
        bail!("空间标识无效");
    }
    Ok(value)
}
fn decimal(value: &str) -> bool {
    let unsigned = value.strip_prefix('-').unwrap_or(value);
    let parts: Vec<_> = unsigned.split('.').collect();
    parts.len() <= 2
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}
fn contains_selectable(nodes: &[WorkspaceNode], target: &str) -> bool {
    nodes
        .iter()
        .any(|n| n.selectable() && (n.id == target || contains_selectable(&n.children, target)))
}
pub fn menu_allows(menus: &[Value], action: &str) -> bool {
    let keys: &[&str] = match action {
        "tokens" => &["api-token-list"],
        "billing" => &[
            "billing-account-summary",
            "billing-transaction-list",
            "billing-recharge-order",
        ],
        "workspaces" => &["workspace-list"],
        "profile" => return true,
        _ => return false,
    };
    menus.iter().any(|m| {
        m["visible"].as_bool() != Some(false)
            && (m["path"]
                == match action {
                    "tokens" => "/console/tokens",
                    "billing" => "/console/billing",
                    "workspaces" => "/console/spaces",
                    _ => "",
                }
                || keys.iter().any(|k| {
                    m["pageKey"] == *k
                        || m["pageKeys"]
                            .as_array()
                            .is_some_and(|a| a.iter().any(|v| v == k))
                })
                || m["children"]
                    .as_array()
                    .is_some_and(|c| menu_allows(c, action)))
    })
}

/// Native command boundary: stale reads are discarded, preference writes are serialized.
pub struct DesktopWorkspaceController {
    client: DesktopWorkspace,
    current: std::sync::Mutex<Option<WorkspaceSnapshot>>,
    generation: std::sync::atomic::AtomicU64,
    selection_gate: tokio::sync::Mutex<()>,
}
impl DesktopWorkspaceController {
    pub fn new(client: DesktopWorkspace) -> Self {
        Self {
            client,
            current: std::sync::Mutex::new(None),
            generation: std::sync::atomic::AtomicU64::new(0),
            selection_gate: tokio::sync::Mutex::new(()),
        }
    }
    pub fn current(&self) -> Option<WorkspaceSnapshot> {
        self.current.lock().expect("workspace lock").clone()
    }
    pub fn clear(&self) {
        self.generation
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        *self.current.lock().expect("workspace lock") = None;
    }
    pub async fn refresh(&self, session: &DesktopSession) -> Result<Option<WorkspaceSnapshot>> {
        let gate = self.selection_gate.lock().await;
        let generation = self
            .generation
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            + 1;
        let context: ContextResponse = self
            .client
            .request(
                session,
                reqwest::Method::GET,
                "/api/client/workspaces/context",
                None,
                None,
            )
            .await?;
        drop(gate);
        let result = self.client.snapshot(session, context).await?;
        Ok(self.accept(generation, result))
    }
    pub async fn switch(
        &self,
        session: &DesktopSession,
        target: &str,
    ) -> Result<Option<WorkspaceSnapshot>> {
        let _gate = self.selection_gate.lock().await;
        let generation = self
            .generation
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            + 1;
        let result = self.client.switch(session, target).await?;
        Ok(self.accept(generation, result))
    }
    fn accept(&self, generation: u64, snapshot: WorkspaceSnapshot) -> Option<WorkspaceSnapshot> {
        let mut current = self.current.lock().expect("workspace lock");
        if self.generation.load(std::sync::atomic::Ordering::SeqCst) != generation {
            return None;
        }
        *current = Some(snapshot.clone());
        Some(snapshot)
    }
}
