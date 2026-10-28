use serde_json::{json, Value};
use tempfile::tempdir;
use tokensmarket_core::{
    connections::{ConnectContext, ConnectionManager},
    inventory::{HostPlatform, InstallationEnvironment, ToolInventory},
    ToolId,
};
async fn platform(
    models: serde_json::Value,
    expected_requests: usize,
) -> (String, tokio::task::JoinHandle<Vec<String>>) {
    platform_sequence(vec![models],expected_requests).await
}
async fn platform_sequence(catalogues: Vec<Value>,expected_requests:usize)->(String,tokio::task::JoinHandle<Vec<String>>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let mut requests = vec![];
        let mut issued=0;
        let mut queried=0;
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut data = vec![];
            loop {
                let mut chunk = [0; 4096];
                let n = socket.read(&mut chunk).await.unwrap();
                if n == 0 {
                    break;
                }
                data.extend_from_slice(&chunk[..n]);
                if let Some(end) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                    let header = String::from_utf8_lossy(&data[..end]);
                    let length: usize = header
                        .lines()
                        .find_map(|line| {
                            line.to_lowercase()
                                .strip_prefix("content-length:")
                                .map(|s| s.trim().parse().unwrap())
                        })
                        .unwrap_or(0);
                    if data.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            let request = String::from_utf8(data).unwrap();
            let first = request.lines().next().unwrap().to_string();
            requests.push(first.clone());
            let response = if first.starts_with("GET /v1/models ") {
                assert!(request.contains(if queried==0 {"Bearer sk-fixture"}else{"Bearer sk-replacement"}));
                let response=catalogues[queried].clone();queried+=1;response
            } else if first.contains("/disable ") {
                json!({"success":true,"data":true})
            } else {
                assert!(request.contains("Bearer desktop-secret"));
                assert!(request.contains("\"workspace_id\":\"42\""));
                issued+=1;
                json!({"success":true,"data":{"api_key":if issued==1 {"sk-fixture"}else{"sk-replacement"},"key_id":if issued==1 {"9001"}else{"9002"},"owner":{"id":"u1","name":"Ada"},"bound_workspace":{"id":"42","name":"Research"}}})
            };
            let body = response.to_string();
            let status = if first.starts_with("GET ") && response["fail_http"] == true {
                "503 Service Unavailable"
            } else {
                "200 OK"
            };
            socket
                .write_all(
                    format!(
                        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
            if requests.len() == expected_requests {
                break;
            }
        }
        requests
    });
    (address, task)
}
fn inventory(root: &std::path::Path) -> ToolInventory {
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    for name in ["workbuddy", "openclaw", "opencode"] {
        let executable = bin.join(name);
        std::fs::write(&executable, "fixture").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
    }
    ToolInventory::new(InstallationEnvironment {
        platform: HostPlatform::MacOs,
        home: root.into(),
        command_directories: vec![bin],
        application_directories: vec![],
        executable_extensions: vec![],
    })
}
fn context() -> ConnectContext {
    ConnectContext {
        session_token: "desktop-secret".into(),
        workspace_id: "42".into(),
    }
}
fn read(path: &std::path::Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

#[tokio::test]
async fn workbuddy_imports_all_compatible_models_and_restores_only_owned_fields_after_reordering() {
    use tokensmarket_core::connections::DisconnectRequest;
    for original in [
        json!([{"id":"manual","url":"https://manual","apiKey":"manual","custom":true}]),
        json!({"models":[{"id":"manual","url":"https://manual","apiKey":"manual"}],"availableModels":["manual"],"custom":"keep"}),
    ] {
        let root = tempdir().unwrap();
        let inventory = inventory(root.path());
        let path = root.path().join("models.json");
        std::fs::write(&path, serde_json::to_vec(&original).unwrap()).unwrap();
        let catalogue = json!({"data":[{"id":"vendor/zeta~1","display_name":"Zeta","supported_endpoints":["OPENAI_CHAT_COMPLETIONS"]},{"id":"responses","supported_endpoints":["OPENAI_RESPONSES"]},{"id":"alpha","supported_endpoints":["OPENAI_CHAT_COMPLETIONS"],"context_window_tokens":65536,"max_output_tokens":8192}]});
        let (url, server) = platform(catalogue, 2).await;
        let manager = ConnectionManager::new(root.path().join("state"), url.clone(), url.clone());
        let summary = manager
            .connect(context(), ToolId::WorkBuddy, &inventory, path.clone())
            .await
            .unwrap();
        assert_eq!(summary.models, vec!["vendor/zeta~1", "alpha"]);
        assert_eq!(summary.owner["name"], "Ada");
        assert_eq!(summary.bound_workspace["id"], "42");
        let mut applied = read(&path);
        let rows = if applied.is_array() {
            applied.as_array().unwrap()
        } else {
            applied["models"].as_array().unwrap()
        };
        assert_eq!(
            rows[0],
            if original.is_array() {
                original[0].clone()
            } else {
                original["models"][0].clone()
            }
        );
        assert_eq!(
            rows[1],
            json!({"id":"vendor/zeta~1","name":"Zeta","url":format!("{url}/v1/chat/completions"),"apiKey":"sk-fixture","vendor":"Token Market","useCustomProtocol":true,"isDefault":false})
        );
        assert!(rows[2].get("supportsToolCall").is_none());
        assert!(rows[2].get("defaultEffort").is_none());
        if applied.is_object() {
            assert_eq!(
                applied["availableModels"],
                json!(["manual", "vendor/zeta~1", "alpha"])
            );
            applied["availableModels"]
                .as_array_mut()
                .unwrap()
                .push(json!("later"));
        }
        let rows = if applied.is_array() {
            applied.as_array_mut().unwrap()
        } else {
            applied["models"].as_array_mut().unwrap()
        };
        rows.reverse();
        rows.push(json!({"id":"later","url":"https://later","apiKey":"later"}));
        rows.iter_mut().find(|r| r["id"] == "alpha").unwrap()["manualNote"] = json!("keep");
        std::fs::write(&path, serde_json::to_vec(&applied).unwrap()).unwrap();
        let outcome = manager
            .disconnect(
                ToolId::WorkBuddy,
                "9001",
                None,
                DisconnectRequest::default(),
            )
            .await
            .unwrap();
        assert_eq!(outcome.state, "disconnected");
        let restored = read(&path);
        let rows = if restored.is_array() {
            restored.as_array().unwrap()
        } else {
            restored["models"].as_array().unwrap()
        };
        assert_eq!(rows.len(), 3);
        assert_eq!(
            rows.iter().find(|r| r["id"] == "alpha").unwrap(),
            &json!({"id":"alpha","manualNote":"keep"})
        );
        assert!(!rows.iter().any(|r| r["id"] == "vendor/zeta~1"));
        assert!(rows.iter().any(|r| r["id"] == "later"));
        if restored.is_object() {
            assert_eq!(restored["availableModels"], json!(["manual", "later"]));
            assert_eq!(restored["custom"], "keep");
        }
        assert!(manager.list().unwrap().is_empty());
        assert_eq!(server.await.unwrap().len(), 2);
    }
}

#[tokio::test]
async fn openclaw_preserves_selection_policy_and_other_providers_through_connect_and_disconnect() {
    use tokensmarket_core::connections::DisconnectRequest;
    let root = tempdir().unwrap();
    let inventory = inventory(root.path());
    let path = root.path().join("openclaw.json");
    let original = json!({"models":{"mode":"replace","providers":{"other":{"baseUrl":"https://other"},"tokensmarket":{"custom":"keep"}}},"agents":{"defaults":{"model":{"primary":"other/existing"},"models":{"other/existing":{}}}},"custom":true});
    std::fs::write(&path, serde_json::to_vec(&original).unwrap()).unwrap();
    let (url,server)=platform(json!({"data":[{"id":"zeta","supported_endpoints":["OPENAI_CHAT_COMPLETIONS"]},{"id":"alpha","display_name":"Alpha","supported_endpoints":["OPENAI_CHAT_COMPLETIONS"],"context_window_tokens":32768,"max_output_tokens":4096,"input_modalities":["text","image"]},{"id":"glm-5.3","supported_endpoints":["OPENAI_RESPONSES"]}]}),2).await;
    let manager = ConnectionManager::new(root.path().join("state"), url.clone(), url.clone());
    let summary = manager
        .connect(context(), ToolId::OpenClaw, &inventory, path.clone())
        .await
        .unwrap();
    assert_eq!(summary.models, vec!["zeta", "alpha"]);
    let mut applied = read(&path);
    assert_eq!(applied["agents"], original["agents"]);
    assert_eq!(applied["models"]["mode"], "replace");
    let route = &applied["models"]["providers"]["tokensmarket"];
    assert_eq!(
        *route,
        json!({"custom":"keep","baseUrl":format!("{url}/v1"),"apiKey":"sk-fixture","api":"openai-completions","models":[{"id":"zeta","name":"zeta"},{"id":"alpha","name":"Alpha","contextWindow":32768,"maxTokens":4096,"input":["text","image"]}]})
    );
    applied["models"]["providers"]["tokensmarket"]["later"] = json!("keep");
    applied["models"]["providers"]["other"]["later"] = json!(true);
    std::fs::write(&path, serde_json::to_vec(&applied).unwrap()).unwrap();
    assert_eq!(
        manager
            .disconnect(ToolId::OpenClaw, "9001", None, DisconnectRequest::default())
            .await
            .unwrap()
            .state,
        "disconnected"
    );
    let mut expected = original;
    expected["models"]["providers"]["tokensmarket"]["later"] = json!("keep");
    expected["models"]["providers"]["other"]["later"] = json!(true);
    assert_eq!(read(&path), expected);
    assert_eq!(server.await.unwrap().len(), 2);
}

#[tokio::test]
async fn opencode_imports_provider_map_without_changing_defaults_or_fabricating_partial_limits() {
    use tokensmarket_core::connections::DisconnectRequest;
    let root = tempdir().unwrap();
    let inventory = inventory(root.path());
    let path = root.path().join("opencode.json");
    let original = json!({"model":"other/manual","small_model":"other/small","provider":{"other":{"models":{"manual":{"name":"Manual"}}},"tokensmarket":{"options":{"timeout":60000},"models":{"manual":{"name":"Manual"}},"custom":true}},"theme":"system"});
    std::fs::write(&path, serde_json::to_vec(&original).unwrap()).unwrap();
    let (url,server)=platform(json!({"data":[{"id":"vendor/zeta","supported_endpoints":["OPENAI_CHAT_COMPLETIONS"],"context_window_tokens":65536},{"id":"alpha","display_name":"Alpha","supported_endpoints":["OPENAI_CHAT_COMPLETIONS"],"context_window_tokens":32768,"max_output_tokens":4096},{"id":"messages","supported_endpoints":["ANTHROPIC_MESSAGES"]}]}),2).await;
    let manager = ConnectionManager::new(root.path().join("state"), url.clone(), url.clone());
    assert_eq!(
        manager
            .connect(context(), ToolId::OpenCode, &inventory, path.clone())
            .await
            .unwrap()
            .models,
        vec!["vendor/zeta", "alpha"]
    );
    let mut applied = read(&path);
    assert_eq!(applied["model"], "other/manual");
    assert_eq!(applied["small_model"], "other/small");
    let route = &applied["provider"]["tokensmarket"];
    assert_eq!(
        *route,
        json!({"npm":"@ai-sdk/openai-compatible","name":"Token Market","options":{"timeout":60000,"baseURL":format!("{url}/v1"),"apiKey":"sk-fixture"},"models":{"manual":{"name":"Manual"},"vendor/zeta":{"name":"vendor/zeta"},"alpha":{"name":"Alpha","limit":{"context":32768,"output":4096}}},"custom":true})
    );
    applied["provider"]["tokensmarket"]["models"]["alpha"]["later"] = json!(true);
    applied["provider"]["tokensmarket"]["options"]["later"] = json!(true);
    std::fs::write(&path, serde_json::to_vec(&applied).unwrap()).unwrap();
    assert_eq!(
        manager
            .disconnect(ToolId::OpenCode, "9001", None, DisconnectRequest::default())
            .await
            .unwrap()
            .state,
        "disconnected"
    );
    let mut expected = original;
    expected["provider"]["tokensmarket"]["models"]["alpha"] = json!({"later":true});
    expected["provider"]["tokensmarket"]["options"]["later"] = json!(true);
    assert_eq!(read(&path), expected);
    assert_eq!(server.await.unwrap().len(), 2);
}

#[tokio::test]
async fn all_three_new_configs_disconnect_cleanly_and_empty_or_failed_catalogues_preserve_original_files(
) {
    use tokensmarket_core::connections::DisconnectRequest;
    for tool in [ToolId::WorkBuddy, ToolId::OpenClaw, ToolId::OpenCode] {
        let root = tempdir().unwrap();
        let inventory = inventory(root.path());
        let path = root.path().join("config.json");
        let (url,server)=platform(json!({"data":[{"id":"one","supported_endpoints":["OPENAI_CHAT_COMPLETIONS"]},{"id":"two","supported_endpoints":["OPENAI_CHAT_COMPLETIONS"]}]}),2).await;
        let manager = ConnectionManager::new(root.path().join("state"), url.clone(), url);
        manager
            .connect(context(), tool, &inventory, path.clone())
            .await
            .unwrap();
        assert_eq!(manager.list().unwrap()[0].version, 2);
        assert_eq!(
            manager
                .disconnect(tool, "9001", None, DisconnectRequest::default())
                .await
                .unwrap()
                .state,
            "disconnected"
        );
        assert!(!path.exists());
        assert_eq!(server.await.unwrap().len(), 2);
        for catalogue in [
            json!({"data":[]}),
            json!({"data":[{"id":"glm-5.3","supported_endpoints":["OPENAI_RESPONSES"]}]}),
            json!({"fail_http":true}),
            json!({"data":[{"id":"same","supported_endpoints":["OPENAI_CHAT_COMPLETIONS"]},{"id":"same","supported_endpoints":["OPENAI_CHAT_COMPLETIONS"]}]}),
        ] {
            let original = if tool == ToolId::WorkBuddy {
                "[ {\"id\":\"manual\",\"url\":\"https://manual\",\"apiKey\":\"private\"} ]"
            } else {
                "{ \"manual\": true }"
            };
            std::fs::write(&path, original).unwrap();
            let (url, server) = platform(catalogue, 3).await;
            let manager = ConnectionManager::new(root.path().join("state"), url.clone(), url);
            assert!(manager
                .connect(context(), tool, &inventory, path.clone())
                .await
                .is_err());
            assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
            assert!(manager.list().unwrap().is_empty());
            let requests = server.await.unwrap();
            assert_eq!(requests.len(), 3);
            assert_eq!(
                requests.last().unwrap(),
                "POST /api/desktop/connections/keys/9001/disable HTTP/1.1"
            );
        }
    }
}

#[tokio::test]
async fn malformed_or_ambiguous_native_shapes_fail_before_key_issuance() {
    for (tool, original) in [
        (ToolId::WorkBuddy, r#"[{"id":"manual","url":false}]"#),
        (
            ToolId::OpenClaw,
            r#"{"models":{"providers":{"tokensmarket":{"models":[{"id":"manual","name":false}]}}}}"#,
        ),
        (
            ToolId::WorkBuddy,
            "{\"models\":[{\"id\":\"same\"},{\"id\":\"same\"}]}",
        ),
        (ToolId::WorkBuddy, "{\"models\":[false]}"),
        (
            ToolId::WorkBuddy,
            "{\"models\":[],\"availableModels\":[\"one\",5]}",
        ),
        (
            ToolId::OpenClaw,
            "// JSON5 comments must not be silently discarded\n{\"models\":{}}",
        ),
        (
            ToolId::OpenClaw,
            "{\"models\":{\"providers\":{\"tokensmarket\":{\"models\":[{}]}}}}",
        ),
        (
            ToolId::OpenCode,
            "{\"provider\":{\"tokensmarket\":{\"models\":{\"one\":false}}}}",
        ),
        (
            ToolId::OpenCode,
            "{\"provider\":{\"tokensmarket\":{\"models\":{\"one\":{\"limit\":false}}}}}",
        ),
        (
            ToolId::OpenCode,
            "{\"provider\":{\"tokensmarket\":{\"options\":false}}}",
        ),
    ] {
        let root = tempdir().unwrap();
        let inventory = inventory(root.path());
        let path = root.path().join("config.json");
        std::fs::write(&path, original).unwrap();
        let manager = ConnectionManager::new(
            root.path().join("state"),
            "http://127.0.0.1:1".into(),
            "http://127.0.0.1:1".into(),
        );
        let error = manager
            .connect(context(), tool, &inventory, path.clone())
            .await
            .unwrap_err()
            .to_string();
        assert!(
            !error.contains("无法取得接入 Key"),
            "shape not preflighted: {original}: {error}"
        );
        assert!(!root.path().join("state").exists());
        assert_eq!(std::fs::read_to_string(path).unwrap(), original);
    }
}

#[tokio::test]
async fn reconnect_all_three_retires_obsolete_models_and_preserves_the_first_baseline() {
    use tokensmarket_core::connections::DisconnectRequest;
    for tool in [ToolId::WorkBuddy,ToolId::OpenClaw,ToolId::OpenCode] {
        let root=tempdir().unwrap();let inventory=inventory(root.path());let path=root.path().join("config.json");
        let original=match tool {
            ToolId::WorkBuddy=>json!({"models":[{"id":"retained","url":"https://manual","apiKey":"manual-secret","manual":true}],"availableModels":["manual"],"custom":true}),
            ToolId::OpenClaw=>json!({"models":{"mode":"replace","providers":{"tokensmarket":{"apiKey":"manual-secret","custom":true}}},"agents":{"defaults":{"model":{"primary":"manual"}}}}),
            ToolId::OpenCode=>json!({"provider":{"tokensmarket":{"options":{"apiKey":"manual-secret"},"custom":true}},"model":"manual/model","small_model":"manual/small"}),
            _=>unreachable!(),
        };
        std::fs::write(&path,serde_json::to_vec(&original).unwrap()).unwrap();
        let (url,server)=platform_sequence(vec![json!({"data":[{"id":"obsolete","supported_endpoints":["OPENAI_CHAT_COMPLETIONS"]},{"id":"retained","supported_endpoints":["OPENAI_CHAT_COMPLETIONS"],"context_window_tokens":65536,"max_output_tokens":4096}]}),json!({"data":[{"id":"retained","supported_endpoints":["OPENAI_CHAT_COMPLETIONS"]},{"id":"new","supported_endpoints":["OPENAI_CHAT_COMPLETIONS"]}]})],5).await;
        let manager=ConnectionManager::new(root.path().join("state"),url.clone(),url);
        manager.connect(context(),tool,&inventory,path.clone()).await.unwrap();
        let mut current=read(&path);current["later"]=json!(true);if tool==ToolId::WorkBuddy {current["availableModels"].as_array_mut().unwrap().push(json!("later"));}
        std::fs::write(&path,serde_json::to_vec(&current).unwrap()).unwrap();
        let outcome=manager.reconnect(context(),tool,"9001",&inventory,path.clone()).await.unwrap();
        assert_eq!(outcome.connection.models,vec!["retained","new"]);assert!(outcome.old_key_disabled);
        let current=read(&path);assert_eq!(current["later"],true);
        match tool {
            ToolId::WorkBuddy=>{let rows=current["models"].as_array().unwrap();assert!(!rows.iter().any(|r|r["id"]=="obsolete"));let row=rows.iter().find(|r|r["id"]=="retained").unwrap();assert_eq!(row["apiKey"],"sk-replacement");assert!(row.get("maxOutputTokens").is_none());assert_eq!(current["availableModels"],json!(["manual","retained","later","new"]));},
            ToolId::OpenClaw=>{let route=&current["models"]["providers"]["tokensmarket"];assert_eq!(route["apiKey"],"sk-replacement");let rows=route["models"].as_array().unwrap();assert!(!rows.iter().any(|r|r["id"]=="obsolete"));assert!(rows.iter().find(|r|r["id"]=="retained").unwrap().get("maxTokens").is_none());},
            ToolId::OpenCode=>{let route=&current["provider"]["tokensmarket"];assert_eq!(route["options"]["apiKey"],"sk-replacement");assert!(route["models"].get("obsolete").is_none());assert!(route["models"]["retained"].get("limit").is_none());},
            _=>unreachable!(),
        }
        assert_eq!(manager.disconnect(tool,"9002",None,DisconnectRequest::default()).await.unwrap().state,"disconnected");
        let mut expected=original;expected["later"]=json!(true);if tool==ToolId::WorkBuddy {expected["availableModels"].as_array_mut().unwrap().push(json!("later"));}
        assert_eq!(read(&path),expected);assert_eq!(server.await.unwrap().len(),5);
    }
}
