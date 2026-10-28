use super::*;

#[derive(Debug, Clone, Deserialize)]
pub struct ConnectionTestRequest {
    pub confirmed: bool,
    pub model: Option<String>,
}
/// A text-output check only. No remote output, credentials or receipt data reach the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionTestResult {
    pub model: String,
    pub protocol: String,
    pub passed: bool,
    pub output_verified: bool,
    pub streaming_verified: bool,
    pub tool_call_verified: bool,
    pub detail: String,
    pub tested_at: String,
}
#[derive(Serialize, Deserialize)]
struct SavedTest {
    tool: ToolId,
    key_id: String,
    result: ConnectionTestResult,
}
impl ConnectionManager {
    fn saved_tests(&self) -> Result<Vec<SavedTest>> {
        match fs::read(self.directory.join("test-results.json")) {
            Ok(bytes) => serde_json::from_slice(&bytes).context("无法读取连接测试结果"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
            Err(_) => bail!("无法读取连接测试结果，请检查本地权限"),
        }
    }
    pub fn summaries(&self) -> Result<Vec<(ToolId, ConnectionSummary)>> {
        let tests = self.saved_tests()?;
        Ok(self
            .list()?
            .into_iter()
            .map(|record| {
                let mut summary = record.summary();
                if super::restore::verify_applied(&record).is_err() {
                    summary.state = "configuration_changed";
                } else if let Some(test) = tests
                    .iter()
                    .find(|test| test.tool == record.tool && test.key_id == record.key_id)
                {
                    summary.state = if test.result.passed {
                        "test_passed"
                    } else {
                        "test_failed"
                    };
                    summary.test_result = Some(test.result.clone());
                }
                (record.tool, summary)
            })
            .collect())
    }
    pub async fn test_connection(
        &self,
        tool: ToolId,
        expected_key_id: &str,
        request: ConnectionTestRequest,
    ) -> Result<ConnectionTestResult> {
        let _guard = self
            .gate
            .try_lock()
            .map_err(|_| anyhow::anyhow!("工具操作正在进行，请稍候"))?;
        if !request.confirmed {
            bail!("请先确认测试可能产生少量调用费用")
        }
        let record = self
            .list()?
            .into_iter()
            .find(|record| record.tool == tool && record.key_id == expected_key_id)
            .context("连接已改变，请刷新后重试")?;
        super::restore::verify_applied(&record)?;
        let single = matches!(tool, ToolId::ClaudeCode | ToolId::Codex);
        let model = if single {
            request.model.unwrap_or_else(|| "glm-5.3".into())
        } else {
            request
                .model
                .context("请明确选择本次要测试的一个已配置模型")?
        };
        let expected_protocol = match tool {
            ToolId::ClaudeCode => "ANTHROPIC_MESSAGES",
            ToolId::Codex => "OPENAI_RESPONSES",
            _ => "OPENAI_CHAT_COMPLETIONS",
        };
        let compatible = single
            || record.model_snapshot.iter().any(|row| {
                row["id"] == model
                    && row["supported_endpoints"]
                        .as_array()
                        .is_some_and(|endpoints| {
                            endpoints
                                .iter()
                                .any(|endpoint| endpoint == "OPENAI_CHAT_COMPLETIONS")
                        })
            });
        if !record.models.contains(&model)
            || (single && model != "glm-5.3")
            || record.protocol != expected_protocol
            || !compatible
        {
            bail!("请选择连接中已配置且协议匹配的模型")
        }
        let url = url::Url::parse(&record.gateway_url)
            .map_err(|_| anyhow::anyhow!("连接网关地址无效，请重新接入"))?;
        if !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || !(url.scheme() == "https"
                || (url.scheme() == "http"
                    && matches!(url.host_str(), Some("127.0.0.1" | "localhost"))))
        {
            bail!("连接网关地址无效，请重新接入")
        }
        let anthropic = record.protocol == "ANTHROPIC_MESSAGES";
        let responses = record.protocol == "OPENAI_RESPONSES";
        let endpoint = if anthropic {
            "messages"
        } else if responses {
            "responses"
        } else {
            "chat/completions"
        };
        let body = if anthropic {
            json!({"model":model,"messages":[{"role":"user","content":"Reply with OK only."}],"max_tokens":256,"stream":false})
        } else if responses {
            json!({"model":model,"input":"Reply with OK only.","max_output_tokens":256,"stream":false})
        } else {
            json!({"model":model,"messages":[{"role":"user","content":"Reply with OK only."}],"max_tokens":256,"stream":false})
        };
        let request = self
            .http
            .post(format!(
                "{}/v1/{endpoint}",
                record.gateway_url.trim_end_matches('/')
            ))
            .header("X-Workspace-Id", &record.bound_workspace.id)
            .json(&body);
        let request = if anthropic {
            request
                .header("x-api-key", record.credential())
                .header("anthropic-version", "2023-06-01")
        } else {
            request.bearer_auth(record.credential())
        };
        let response = request
            .timeout(std::time::Duration::from_secs(60))
            .send()
            .await;
        let (passed, detail) = match response {
            Err(_) => (false, "无法连接模型网关，请检查网络后手动重试".into()),
            Ok(response) if !response.status().is_success() => (false, format!("模型请求失败（HTTP {}），请检查该连接的 Key、计费空间权限、余额和模型可用性后手动重试", response.status().as_u16())),
            Ok(response) => {
                let body: Option<Value> = response.json().await.ok();
                let verified = body.as_ref().is_some_and(|body| body["error"].is_null() && if anthropic {
                    body["type"] == "message" && body["role"] == "assistant" && body["stop_reason"] == "end_turn" && body["content"].as_array().is_some_and(|parts| parts.iter().any(|part| part["type"] == "text" && part["text"].as_str().is_some_and(|text| !text.trim().is_empty())))
                } else if responses {
                    body["status"] == "completed" && body["output"].as_array().is_some_and(|items| items.iter().any(|item| item["type"] == "message" && item["role"] == "assistant" && item["content"].as_array().is_some_and(|content| content.iter().any(|part| part["type"] == "output_text" && part["text"].as_str().is_some_and(|text| !text.trim().is_empty())))))
                } else {
                    body["choices"].as_array().is_some_and(|choices| choices.iter().any(|choice| choice["finish_reason"] == "stop" && choice["message"]["role"] == "assistant" && choice["message"]["content"].as_str().is_some_and(|text| !text.trim().is_empty())))
                });
                (verified, if verified { "已验证一次文本输出；未验证流式或工具调用" } else { "网关未返回有效的完整助手文本输出；请检查模型与已配置协议后手动重试" }.into())
            }
        };
        let result = ConnectionTestResult {
            model,
            protocol: record.protocol,
            passed,
            output_verified: passed,
            streaming_verified: false,
            tool_call_verified: false,
            detail,
            tested_at: crate::config::now_iso8601(),
        };
        let mut tests = self.saved_tests()?;
        tests.retain(|test| test.tool != tool);
        tests.push(SavedTest {
            tool,
            key_id: record.key_id,
            result: result.clone(),
        });
        atomic_write(
            &self.directory.join("test-results.json"),
            &serde_json::to_vec_pretty(&tests)?,
        )
        .map_err(|_| anyhow::anyhow!("请求已结束，但测试结果未能保存；请检查本地权限后再刷新"))?;
        Ok(result)
    }
}
