//! Adapter/restore fixtures retain their scripted catalogue backends; this bridge
//! supplies the new private protocol. Protocol-failure tests use a direct server.
use super::*;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Clone)]
struct Binding {
    data: Value,
    request: String,
    generation: u64,
    stage: String,
}

pub fn bridge(directory: &Path, backend: String) -> String {
    if !backend.starts_with("http://127.0.0.1:") {
        return backend;
    }
    static PROXIES: OnceLock<Mutex<HashMap<(PathBuf, String), String>>> = OnceLock::new();
    let mut proxies = PROXIES.get_or_init(Default::default).lock().unwrap();
    let identity = (directory.to_owned(), backend.clone());
    if let Some(url) = proxies.get(&identity) {
        return url.clone();
    }
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let listener = tokio::net::TcpListener::from_std(listener).unwrap();
    proxies.insert(identity, url.clone());
    tokio::spawn(async move {
        let mut bindings: HashMap<String, Binding> = HashMap::new();
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let mut bytes = vec![];
            loop {
                let mut chunk = [0; 4096];
                let count = socket.read(&mut chunk).await.unwrap_or(0);
                if count == 0 {
                    break;
                }
                bytes.extend_from_slice(&chunk[..count]);
                if let Some(end) = bytes.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
                    let length: usize = String::from_utf8_lossy(&bytes[..end])
                        .lines()
                        .find_map(|line| {
                            line.to_lowercase()
                                .strip_prefix("content-length:")
                                .map(|value| value.trim().parse().unwrap())
                        })
                        .unwrap_or(0);
                    if bytes.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            let text = String::from_utf8(bytes).unwrap();
            let header = |name: &str| {
                text.lines()
                    .find_map(|line| {
                        line.split_once(':')
                            .filter(|(key, _)| key.eq_ignore_ascii_case(name))
                            .map(|(_, value)| value.trim().to_owned())
                    })
                    .unwrap_or_default()
            };
            let first = text.lines().next().unwrap_or("");
            let action = first
                .split_whitespace()
                .nth(1)
                .unwrap_or("")
                .rsplit('/')
                .next()
                .unwrap_or("");
            let body: Value = serde_json::from_str(
                text.split_once("\r\n\r\n")
                    .map(|(_, body)| body)
                    .unwrap_or("{}"),
            )
            .unwrap();
            let actor = header("X-Client-Owner-Id");
            let authorization = header("Authorization");
            let scope = format!(
                "{}:{}:{}:{}",
                actor, body["workspace_id"], body["device_id"], body["tool"]
            );
            let binding = bindings.get(&scope).cloned();
            let mut response = json!({"status":"NOT_FOUND","owner":{"id":actor,"name":"Fixture"},"bound_workspace":{"id":body["workspace_id"],"name":body["workspace_id"]}});
            let mut success = true;
            match action {
                "inspect" => {
                    if let Some(binding) = &binding {
                        response = binding.data.clone();
                        let valid = body["fingerprint"].as_str().is_some_and(|hash| {
                            hash == format!(
                                "{:x}",
                                Sha256::digest(
                                    binding.data["api_key"].as_str().unwrap().as_bytes()
                                )
                            )
                        });
                        response["status"] =
                            json!(if matches!(binding.stage.as_str(), "PENDING" | "SAVED") {
                                binding.stage.as_str()
                            } else if body["fingerprint"].is_null() {
                                "LOCAL_COPY_REQUIRED"
                            } else if valid {
                                "VALID"
                            } else {
                                "SECRET_CHANGED"
                            });
                        response["request_id"] = json!(binding.request);
                        response["generation"] = json!(binding.generation);
                        response.as_object_mut().unwrap().remove("api_key");
                    }
                }
                "issue" => {
                    if let Some(binding) = &binding {
                        if binding.stage != "APPLIED" {
                            response = binding.data.clone();
                            response["status"] = json!(binding.stage);
                            response["request_id"] = json!(binding.request);
                            response["generation"] = json!(binding.generation);
                            response.as_object_mut().unwrap().remove("api_key");
                        } else {
                            success = false;
                        }
                    } else {
                        let result = reqwest::Client::new()
                            .post(format!("{backend}/api/desktop/connections/keys"))
                            .header("Authorization", authorization)
                            .header("X-Workspace-Id", body["workspace_id"].as_str().unwrap())
                            .json(&body)
                            .send()
                            .await;
                        match result {
                            Ok(result) => {
                                let result: Value = result.json().await.unwrap();
                                success = result["success"] != false;
                                response = result["data"].clone();
                                if success {
                                    let binding = Binding {
                                        data: response.clone(),
                                        request: body["request_id"].as_str().unwrap().to_owned(),
                                        generation: 1,
                                        stage: "PENDING".into(),
                                    };
                                    response["status"] = json!("ISSUED");
                                    response["generation"] = json!(1);
                                    response["request_id"] = json!(binding.request);
                                    bindings.insert(scope, binding);
                                }
                            }
                            Err(_) => {
                                success = false;
                            }
                        }
                    }
                }
                "recover" | "confirm" | "applied" => {
                    if let Some(mut binding) = binding {
                        success = body["key_id"] == binding.data["key_id"]
                            && body["request_id"] == binding.request
                            && body["generation"] == binding.generation;
                        if success {
                            if action == "recover" && binding.stage == "PENDING" {
                                binding.generation += 1;
                                binding.data["api_key"] = json!(format!(
                                    "{}-rotated",
                                    binding.data["api_key"].as_str().unwrap()
                                ));
                                response = binding.data.clone();
                                response["status"] = json!("ISSUED");
                            } else {
                                if action == "confirm" && binding.stage == "PENDING" {
                                    binding.stage = "SAVED".into();
                                }
                                if action == "applied" {
                                    binding.stage = "APPLIED".into();
                                }
                                response = binding.data.clone();
                                response["status"] = json!(binding.stage);
                                response.as_object_mut().unwrap().remove("api_key");
                            }
                            response["request_id"] = json!(binding.request);
                            response["generation"] = json!(binding.generation);
                            bindings.insert(scope, binding);
                        }
                    } else {
                        success = false;
                    }
                }
                _ => {
                    success = false;
                }
            }
            let body = json!({"success":success,"data":response}).to_string();
            let _ = socket.write_all(format!("HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}", body.len(), body).as_bytes()).await;
        }
    });
    url
}
