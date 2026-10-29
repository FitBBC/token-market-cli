use serde_json::{json, Value};
use tempfile::tempdir;
use tokensmarket_core::{
    connections::{ConnectionManager, ConnectionTestRequest},
    inventory::{HostPlatform, InstallationEnvironment, ToolInventory},
    ToolId,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn gateway(status: u16, payload: Value) -> (String, tokio::task::JoinHandle<String>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = vec![];
            loop {
                let mut chunk = [0; 4096];
                let n = socket.read(&mut chunk).await.unwrap();
                bytes.extend_from_slice(&chunk[..n]);
                if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    let header = String::from_utf8_lossy(&bytes[..end]);
                    let len: usize = header
                        .lines()
                        .find_map(|line| {
                            line.to_lowercase()
                                .strip_prefix("content-length:")
                                .map(|s| s.trim().parse().unwrap())
                        })
                        .unwrap_or(0);
                    if bytes.len() >= end + 4 + len {
                        break;
                    }
                }
                assert_ne!(n, 0);
            }
            let request = String::from_utf8(bytes.clone()).unwrap();
            let setup = request.starts_with("POST /api/desktop/connections/keys ")
                || request.starts_with("GET /v1/models ");
            let response = if request.starts_with("GET /v1/models ") {
                json!({"data":[{"id":"alpha","model_types":["TEXT_GENERATION"]},{"id":"beta","model_types":["TEXT_GENERATION"]},{"id":"glm-5.3","model_types":["TEXT_GENERATION"]}]})
            } else if setup {
                json!({"success":true,"data":{"api_key":"sk-original-connection","key_id":"9001","owner":{"id":"old-account","name":"Original Owner"},"bound_workspace":{"id":"42","name":"Original Billing"}}})
            } else {
                payload.clone()
            };
            let body = response.to_string();
            let status = if setup { 200 } else { status };
            socket.write_all(format!("HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
            // The listener is dropped: a hidden second request would fail, not silently pass.
            if !setup {
                return String::from_utf8(bytes).unwrap();
            }
        }
    });
    (url, task)
}
fn config_path(root: &std::path::Path, tool: ToolId) -> std::path::PathBuf {
    root.join(match tool {
        ToolId::Codex => "agent-config.toml",
        ToolId::DeepSeekHarness => "agent-config.yaml",
        _ => "agent-config.json",
    })
}
async fn saved(
    root: &std::path::Path,
    url: &str,
    tool: ToolId,
    protocol: &str,
    models: Value,
) -> ConnectionManager {
    use tokensmarket_core::connections::ConnectContext;
    let directory = root.join(".tokensmarket/desktop-connections");
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    for name in [
        "claude",
        "codex",
        "dsh",
        "workbuddy",
        "openclaw",
        "opencode",
    ] {
        let exe = bin.join(name);
        std::fs::write(&exe, "fixture").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(exe, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
    }
    let inv = ToolInventory::new(InstallationEnvironment {
        platform: HostPlatform::MacOs,
        home: root.into(),
        command_directories: vec![bin],
        application_directories: vec![],
        executable_extensions: vec![],
    });
    let manager = ConnectionManager::new(directory.clone(), url.into(), url.into());
    manager
        .connect(
            ConnectContext {
                session_token: "fixture-desktop".into(),
                workspace_id: "42".into(),
            },
            tool,
            &inv,
            config_path(root, tool),
        )
        .await
        .unwrap();
    // Negative record-contract cases deliberately corrupt a genuine connection after Connect.
    let path = directory.join("connections.json");
    let mut records: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    records[0]["protocol"] = json!(protocol);
    records[0]["models"] = models;
    std::fs::write(&path, records.to_string()).unwrap();
    // After logout/account changes, current dashboard URLs cannot replace the saved route.
    ConnectionManager::new(
        directory,
        "http://127.0.0.1:1".into(),
        "http://127.0.0.1:1".into(),
    )
}
fn inventory(root: &std::path::Path) -> ToolInventory {
    ToolInventory::new(InstallationEnvironment {
        platform: HostPlatform::MacOs,
        home: root.into(),
        command_directories: vec![],
        application_directories: vec![],
        executable_extensions: vec![],
    })
}
#[tokio::test]
async fn codex_test_uses_saved_responses_credentials_and_persists_redacted_inventory_without_config_mutation(
) {
    let root = tempdir().unwrap();
    let (url, request) = gateway(200, json!({"status":"completed","output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":"OK"}]}]})).await;
    let manager = saved(
        root.path(),
        &url,
        ToolId::Codex,
        "OPENAI_RESPONSES",
        json!(["glm-5.3"]),
    )
    .await;
    let before = std::fs::read(config_path(root.path(), ToolId::Codex)).unwrap();
    let first = inventory(root.path()).list().unwrap();
    let initial =
        serde_json::to_value(first.iter().find(|e| e.id == ToolId::Codex).unwrap()).unwrap();
    assert_eq!(initial["connection"]["keyId"], "9001");
    assert_eq!(initial["connection"]["state"], "configured_untested");
    let result = manager
        .test_connection(
            ToolId::Codex,
            "9001",
            ConnectionTestRequest {
                confirmed: true,
                model: None,
            },
        )
        .await
        .unwrap();
    assert!(result.passed);
    assert!(result.output_verified);
    assert!(!result.streaming_verified && !result.tool_call_verified);
    let request = request.await.unwrap();
    assert!(
        request.starts_with("POST /v1/responses HTTP/1.1"),
        "{request}"
    );
    assert!(request.contains("Bearer sk-original-connection"));
    assert!(request.to_lowercase().contains("x-workspace-id: 42"));
    let body: Value = serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap()).unwrap();
    assert_eq!(body["model"], "glm-5.3");
    assert!(body["input"].is_string());
    assert_eq!(body["stream"], false);
    assert!(body.get("messages").is_none());
    assert_eq!(
        std::fs::read(config_path(root.path(), ToolId::Codex)).unwrap(),
        before
    );
    let reopened = inventory(root.path()).list().unwrap();
    println!(
        "serialized_inventory={}",
        serde_json::to_string(&reopened).unwrap()
    );
    let summary =
        serde_json::to_value(reopened.iter().find(|e| e.id == ToolId::Codex).unwrap()).unwrap();
    assert_eq!(summary["connection"]["state"], "test_passed");
    assert_eq!(summary["connection"]["owner"]["id"], "old-account");
    assert_eq!(summary["connection"]["testResult"]["model"], "glm-5.3");
    assert!(!summary.to_string().contains("sk-original"));
    assert!(!summary.to_string().contains("receipts"));
}

#[tokio::test]
async fn claude_tests_glm_via_messages_with_original_key_and_anthropic_version() {
    let root = tempdir().unwrap();
    let (url, request) = gateway(200, json!({"type":"message","role":"assistant","stop_reason":"end_turn","content":[{"type":"text","text":"OK"}]})).await;
    let manager = saved(
        root.path(),
        &url,
        ToolId::ClaudeCode,
        "ANTHROPIC_MESSAGES",
        json!(["glm-5.3"]),
    )
    .await;
    let result = manager
        .test_connection(
            ToolId::ClaudeCode,
            "9001",
            ConnectionTestRequest {
                confirmed: true,
                model: None,
            },
        )
        .await
        .unwrap();
    assert!(result.passed);
    let request = request.await.unwrap();
    assert!(request.starts_with("POST /v1/messages HTTP/1.1"));
    assert!(request
        .to_lowercase()
        .contains("x-api-key: sk-original-connection"));
    assert!(request
        .to_lowercase()
        .contains("anthropic-version: 2023-06-01"));
    assert!(!request.to_lowercase().contains("authorization:"));
    let body: Value = serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap()).unwrap();
    assert_eq!(body["model"], "glm-5.3");
    assert_eq!(body["messages"][0]["role"], "user");
    assert!(body["max_tokens"].is_number());
}

#[tokio::test]
async fn multimodel_test_requires_explicit_one_compatible_saved_model_and_never_changes_config() {
    for tool in [
        ToolId::DeepSeekHarness,
        ToolId::WorkBuddy,
        ToolId::OpenClaw,
        ToolId::OpenCode,
    ] {
        let root = tempdir().unwrap();
        let (url, request) = gateway(200, json!({"choices":[{"finish_reason":"stop","message":{"role":"assistant","content":"OK"}}]})).await;
        let manager = saved(
            root.path(),
            &url,
            tool,
            "OPENAI_CHAT_COMPLETIONS",
            json!(["alpha", "beta"]),
        )
        .await;
        for model in [None, Some("not-configured".into()), Some("glm-5.3".into())] {
            assert!(manager
                .test_connection(
                    tool,
                    "9001",
                    ConnectionTestRequest {
                        confirmed: true,
                        model
                    }
                )
                .await
                .is_err());
        }
        assert!(manager
            .test_connection(
                tool,
                "9001",
                ConnectionTestRequest {
                    confirmed: false,
                    model: Some("beta".into())
                }
            )
            .await
            .unwrap_err()
            .to_string()
            .contains("费用"));
        assert!(manager
            .test_connection(
                tool,
                "replaced-key",
                ConnectionTestRequest {
                    confirmed: true,
                    model: Some("beta".into())
                }
            )
            .await
            .is_err());
        let config = std::fs::read(config_path(root.path(), tool)).unwrap();
        let result = manager
            .test_connection(
                tool,
                "9001",
                ConnectionTestRequest {
                    confirmed: true,
                    model: Some("beta".into()),
                },
            )
            .await
            .unwrap();
        assert!(result.passed);
        let request = request.await.unwrap();
        assert!(request.starts_with("POST /v1/chat/completions HTTP/1.1"));
        let body: Value = serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(body["model"], "beta");
        assert_eq!(body["stream"], false);
        assert!(request.contains("Bearer sk-original-connection"));
        assert!(request.to_lowercase().contains("x-workspace-id: 42"));
        assert_eq!(
            std::fs::read(config_path(root.path(), tool)).unwrap(),
            config
        );
        assert_eq!(manager.list().unwrap()[0].models, vec!["alpha", "beta"]);
    }
}

#[tokio::test]
async fn http_success_without_completed_visible_output_is_failed_and_remote_secrets_are_never_displayed(
) {
    for (tool, protocol, payload) in [
        (ToolId::Codex, "OPENAI_RESPONSES", json!({})),
        (
            ToolId::Codex,
            "OPENAI_RESPONSES",
            json!({"status":"completed","output":[{"type":"function_call","name":"secret-tool","arguments":"{}"}]}),
        ),
        (
            ToolId::Codex,
            "OPENAI_RESPONSES",
            json!({"status":"incomplete","output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":"partial"}]}]}),
        ),
        (
            ToolId::ClaudeCode,
            "ANTHROPIC_MESSAGES",
            json!({"type":"message","role":"assistant","stop_reason":"tool_use","content":[{"type":"tool_use","name":"test","input":{}}]}),
        ),
        (
            ToolId::DeepSeekHarness,
            "OPENAI_CHAT_COMPLETIONS",
            json!({"choices":[{"finish_reason":"tool_calls","message":{"role":"assistant","tool_calls":[{}]}}]}),
        ),
        (
            ToolId::DeepSeekHarness,
            "OPENAI_CHAT_COMPLETIONS",
            json!({"choices":[{"finish_reason":"stop","message":{"role":"assistant","content":"   ","reasoning_content":"private thought"}}]}),
        ),
        (
            ToolId::Codex,
            "OPENAI_RESPONSES",
            json!({"error":{"message":"sk-original-connection and private upstream receipt"},"status":"completed"}),
        ),
    ] {
        let root = tempdir().unwrap();
        let (url, request) = gateway(200, payload).await;
        let models = if tool == ToolId::DeepSeekHarness {
            json!(["alpha", "beta"])
        } else {
            json!(["glm-5.3"])
        };
        let manager = saved(root.path(), &url, tool, protocol, models).await;
        let result = manager
            .test_connection(
                tool,
                "9001",
                ConnectionTestRequest {
                    confirmed: true,
                    model: if tool == ToolId::DeepSeekHarness {
                        Some("beta".into())
                    } else {
                        None
                    },
                },
            )
            .await
            .unwrap();
        assert!(
            !result.passed
                && !result.output_verified
                && !result.tool_call_verified
                && !result.streaming_verified
        );
        let public = serde_json::to_string(&result).unwrap();
        assert!(!public.contains("sk-original") && !public.contains("private upstream"));
        assert!(result.detail.contains("文本输出"));
        assert_eq!(manager.summaries().unwrap()[0].1.state, "test_failed");
        request.await.unwrap();
    }
}

#[tokio::test]
async fn responses_accepts_official_null_error_and_new_key_does_not_inherit_previous_pass() {
    let root = tempdir().unwrap();
    let (url, request) = gateway(200,json!({"error":null,"status":"completed","output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":"OK"}]}]})).await;
    let manager = saved(
        root.path(),
        &url,
        ToolId::Codex,
        "OPENAI_RESPONSES",
        json!(["glm-5.3"]),
    )
    .await;
    assert!(
        manager
            .test_connection(
                ToolId::Codex,
                "9001",
                ConnectionTestRequest {
                    confirmed: true,
                    model: None
                }
            )
            .await
            .unwrap()
            .passed
    );
    request.await.unwrap();
    let path = root
        .path()
        .join(".tokensmarket/desktop-connections/connections.json");
    let mut records: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    records[0]["key_id"] = json!("9002");
    std::fs::write(path, records.to_string()).unwrap();
    assert_eq!(
        manager.summaries().unwrap()[0].1.state,
        "configured_untested"
    );
    assert!(manager.summaries().unwrap()[0].1.test_result.is_none());
}

#[tokio::test]
async fn rejected_key_returns_redacted_actionable_failure_and_network_failure_is_persisted() {
    for remote in [true, false] {
        let root = tempdir().unwrap();
        let (url, request) = gateway(
            401,
            json!({"error":{"message":"sk-original-connection private provider credential"}}),
        )
        .await;
        let manager = saved(
            root.path(),
            &url,
            ToolId::Codex,
            "OPENAI_RESPONSES",
            json!(["glm-5.3"]),
        )
        .await;
        if !remote {
            request.abort();
            tokio::task::yield_now().await;
        }
        let result = manager
            .test_connection(
                ToolId::Codex,
                "9001",
                ConnectionTestRequest {
                    confirmed: true,
                    model: None,
                },
            )
            .await
            .unwrap();
        assert!(!result.passed);
        assert!(!serde_json::to_string(&result)
            .unwrap()
            .contains("sk-original"));
        assert!(result.detail.contains("手动重试"));
        if remote {
            assert!(result.detail.contains("HTTP 401"));
            request.await.unwrap();
        } else {
            request.abort();
        }
        assert_eq!(manager.summaries().unwrap()[0].1.state, "test_failed");
    }
}

#[tokio::test]
async fn saved_catalogue_protocol_and_charge_confirmation_are_checked_before_any_request() {
    let root = tempdir().unwrap();
    let (url, task) = gateway(200, json!({})).await;
    let manager = saved(
        root.path(),
        &url,
        ToolId::DeepSeekHarness,
        "OPENAI_CHAT_COMPLETIONS",
        json!(["not-compatible"]),
    )
    .await;
    let result = manager
        .test_connection(
            ToolId::DeepSeekHarness,
            "9001",
            ConnectionTestRequest {
                confirmed: true,
                model: Some("not-compatible".into()),
            },
        )
        .await;
    assert!(result.is_err());
    assert_eq!(
        manager.summaries().unwrap()[0].1.state,
        "configured_untested"
    );
    task.abort();
    std::fs::remove_dir_all(root.path().join(".tokensmarket")).unwrap();
    let (url, task) = gateway(200, json!({})).await;
    let manager = saved(
        root.path(),
        &url,
        ToolId::Codex,
        "OPENAI_CHAT_COMPLETIONS",
        json!(["glm-5.3"]),
    )
    .await;
    assert!(manager
        .test_connection(
            ToolId::Codex,
            "9001",
            ConnectionTestRequest {
                confirmed: true,
                model: None
            }
        )
        .await
        .is_err());
    assert_eq!(
        manager.summaries().unwrap()[0].1.state,
        "configured_untested"
    );
    task.abort();
}

#[tokio::test]
async fn owned_config_drift_or_deletion_refuses_paid_request_but_unrelated_edits_remain_testable() {
    for drift in [
        "model",
        "wire_api",
        "experimental_bearer_token",
        "base_url",
        "env_key",
        "auth",
        "http_headers",
        "delete",
    ] {
        let root = tempdir().unwrap();
        let (url, task) = gateway(200, json!({"status":"completed","output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":"OK"}]}]})).await;
        let manager = saved(
            root.path(),
            &url,
            ToolId::Codex,
            "OPENAI_RESPONSES",
            json!(["glm-5.3"]),
        )
        .await;
        let path = config_path(root.path(), ToolId::Codex);
        if drift == "delete" {
            std::fs::remove_file(&path).unwrap();
        } else {
            let mut document = std::fs::read_to_string(&path)
                .unwrap()
                .parse::<toml_edit::DocumentMut>()
                .unwrap();
            if drift == "http_headers" {
                document["model_providers"]["tokensmarket"]["http_headers"]["Authorization"] =
                    toml_edit::value("manual");
            } else if drift == "model" {
                document[drift] = toml_edit::value("manual");
            } else {
                document["model_providers"]["tokensmarket"][drift] = toml_edit::value("manual");
            }
            std::fs::write(&path, document.to_string()).unwrap();
        }
        let result = manager
            .test_connection(
                ToolId::Codex,
                "9001",
                ConnectionTestRequest {
                    confirmed: true,
                    model: None,
                },
            )
            .await;
        assert!(result.is_err(), "drift={drift}");
        assert_eq!(
            manager.summaries().unwrap()[0].1.state,
            "configuration_changed"
        );
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(40), async {
                while !task.is_finished() {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .is_err()
        );
        task.abort();
    }
    let root = tempdir().unwrap();
    let (url, task) = gateway(200, json!({"status":"completed","output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":"OK"}]}]})).await;
    let manager = saved(
        root.path(),
        &url,
        ToolId::Codex,
        "OPENAI_RESPONSES",
        json!(["glm-5.3"]),
    )
    .await;
    let path = config_path(root.path(), ToolId::Codex);
    let mut bytes = std::fs::read_to_string(&path).unwrap();
    bytes.push_str("\n[unrelated]\nnote = 'keep'\n");
    std::fs::write(&path, &bytes).unwrap();
    assert!(
        manager
            .test_connection(
                ToolId::Codex,
                "9001",
                ConnectionTestRequest {
                    confirmed: true,
                    model: None
                }
            )
            .await
            .unwrap()
            .passed
    );
    task.await.unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), bytes);
    std::fs::remove_file(path).unwrap();
    assert_eq!(
        manager.summaries().unwrap()[0].1.state,
        "configuration_changed"
    );
    assert!(manager.summaries().unwrap()[0].1.test_result.is_none());
    println!(
        "drift_inventory={}",
        serde_json::to_string(&inventory(root.path()).list().unwrap()).unwrap()
    );
    assert!(manager
        .test_connection(
            ToolId::Codex,
            "9001",
            ConnectionTestRequest {
                confirmed: true,
                model: None
            }
        )
        .await
        .is_err());
}

#[tokio::test]
async fn all_tools_reject_removed_owned_auth_configuration_before_generation() {
    for tool in [
        ToolId::ClaudeCode,
        ToolId::DeepSeekHarness,
        ToolId::WorkBuddy,
        ToolId::OpenClaw,
        ToolId::OpenCode,
    ] {
        let root = tempdir().unwrap();
        let (url, task) = gateway(200, json!({"choices":[{"finish_reason":"stop","message":{"role":"assistant","content":"OK"}}]})).await;
        let single = tool == ToolId::ClaudeCode;
        let manager = saved(
            root.path(),
            &url,
            tool,
            if single {
                "ANTHROPIC_MESSAGES"
            } else {
                "OPENAI_CHAT_COMPLETIONS"
            },
            if single {
                json!(["glm-5.3"])
            } else {
                json!(["alpha", "beta"])
            },
        )
        .await;
        let path = if tool == ToolId::DeepSeekHarness {
            root.path().join(".credentials.yaml")
        } else {
            config_path(root.path(), tool)
        };
        let original = std::fs::read_to_string(&path).unwrap();
        let changed = original.replace("sk-original-connection", "sk-manually-replaced");
        assert_ne!(original, changed);
        std::fs::write(&path, &changed).unwrap();
        assert!(manager
            .test_connection(
                tool,
                "9001",
                ConnectionTestRequest {
                    confirmed: true,
                    model: if single { None } else { Some("beta".into()) }
                }
            )
            .await
            .is_err());
        assert_eq!(std::fs::read_to_string(path).unwrap(), changed);
        assert_eq!(
            manager.summaries().unwrap()[0].1.state,
            "configuration_changed"
        );
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(40), async {
                while !task.is_finished() {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .is_err()
        );
        task.abort();
    }
}

#[tokio::test]
async fn harness_unrelated_credential_records_remain_testable_and_survive_disconnect() {
    use tokensmarket_core::connections::DisconnectRequest;
    let root = tempdir().unwrap();
    let (url, task) = gateway(
        200,
        json!({"choices":[{"finish_reason":"stop","message":{"role":"assistant","content":"OK"}}]}),
    )
    .await;
    let manager = saved(
        root.path(),
        &url,
        ToolId::DeepSeekHarness,
        "OPENAI_CHAT_COMPLETIONS",
        json!(["alpha", "beta"]),
    )
    .await;
    let path = root.path().join(".credentials.yaml");
    let mut credentials: Value = serde_yaml::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    credentials["records"]["other-provider"] = json!({"secret":"other-secret"});
    credentials["refs"]["OTHER_KEY"] = json!("other-secret");
    std::fs::write(&path, serde_yaml::to_string(&credentials).unwrap()).unwrap();
    assert_eq!(
        manager.summaries().unwrap()[0].1.state,
        "configured_untested"
    );
    assert!(
        manager
            .test_connection(
                ToolId::DeepSeekHarness,
                "9001",
                ConnectionTestRequest {
                    confirmed: true,
                    model: Some("beta".into())
                }
            )
            .await
            .unwrap()
            .passed
    );
    task.await.unwrap();
    manager
        .disconnect(
            ToolId::DeepSeekHarness,
            "9001",
            None,
            DisconnectRequest::default(),
        )
        .await
        .unwrap();
    let restored: Value = serde_yaml::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(
        restored,
        json!({"version":1,"refs":{"OTHER_KEY":"other-secret"},"records":{"other-provider":{"secret":"other-secret"}}})
    );
}
