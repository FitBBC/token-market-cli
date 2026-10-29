use serde_json::json;
use std::{fs, path::Path};
use tempfile::tempdir;
use tokensmarket_core::{connections::ConnectionManager, ToolId};

fn saved(directory: &Path, paths: &[&Path]) -> ConnectionManager {
    fs::create_dir_all(directory).unwrap();
    let receipts: Vec<_> = paths.iter().map(|path| json!({
        "path": path,
        "existed": true,
        "backup_path": directory.join("private-backup"),
        "fields": [{"pointer":"/apiKey", "before":"old-private-value", "applied":"stale-applied-value"}],
        "created_containers": []
    })).collect();
    fs::write(
        directory.join("connections.json"),
        json!([{
            "version": 1, "tool": ToolId::DeepSeekHarness, "device_id": "fixture-device",
            "key_id": "fixture-key", "owner": {"id":"user", "name":"User"},
            "bound_workspace": {"id":"workspace", "name":"Workspace"},
            "protocol": "OPENAI_CHAT_COMPLETIONS", "models": ["fixture-model"],
            "model_snapshot": [], "gateway_url": "http://127.0.0.1:1",
            "created_at": "2026-10-29T00:00:00Z", "receipts": receipts,
            "api_key": "record-only-secret"
        }])
        .to_string(),
    )
    .unwrap();
    ConnectionManager::new(
        directory.into(),
        "http://127.0.0.1:1".into(),
        "http://127.0.0.1:1".into(),
    )
}

#[tokio::test]
async fn reads_actual_recorded_files_verbatim_without_mutation_or_private_history() {
    let root = tempdir().unwrap();
    let profile = root.path().join("custom-profile.yml");
    let credentials = root.path().join(".credentials.yaml");
    // Invalid syntax and comments are still useful to inspect; never parse or reformat.
    let profile_text = "# 用户修改\nmodel: [unfinished\n";
    let credential_text = "TOKEN_MARKET_API_KEY: sk-visible-current\r\nother: user-value\r\n";
    fs::write(&profile, profile_text).unwrap();
    fs::write(&credentials, credential_text).unwrap();
    let directory = root.path().join("connections");
    let manager = saved(&directory, &[&profile, &credentials, &profile]);
    let record_before = fs::read(directory.join("connections.json")).unwrap();
    let files = manager
        .configuration_files(ToolId::DeepSeekHarness, "fixture-key")
        .await
        .unwrap();
    assert_eq!(files.len(), 2);
    assert_eq!(files[0].path, profile);
    assert_eq!(files[0].content.as_deref(), Some(profile_text));
    assert_eq!(files[1].path, credentials);
    assert_eq!(files[1].content.as_deref(), Some(credential_text));
    assert!(files.iter().all(|file| file.error.is_none()));
    let wire = serde_json::to_string(&files).unwrap();
    for private in [
        "private-backup",
        "old-private-value",
        "stale-applied-value",
        "record-only-secret",
    ] {
        assert!(!wire.contains(private));
    }
    fs::write(&profile, "updated after preview\n").unwrap();
    let updated = manager
        .configuration_files(ToolId::DeepSeekHarness, "fixture-key")
        .await
        .unwrap();
    assert_eq!(
        updated[0].content.as_deref(),
        Some("updated after preview\n")
    );
    assert_eq!(fs::read_to_string(&credentials).unwrap(), credential_text);
    assert_eq!(
        fs::read(directory.join("connections.json")).unwrap(),
        record_before
    );
}

#[tokio::test]
async fn reports_each_missing_or_unreadable_file_without_hiding_readable_or_empty_files() {
    let root = tempdir().unwrap();
    let missing = root.path().join("missing.yml");
    let invalid = root.path().join("invalid.yml");
    let empty = root.path().join("empty.yml");
    fs::write(&invalid, [0xff]).unwrap();
    fs::write(&empty, "").unwrap();
    let manager = saved(
        &root.path().join("connections"),
        &[&missing, &invalid, &empty],
    );
    let files = manager
        .configuration_files(ToolId::DeepSeekHarness, "fixture-key")
        .await
        .unwrap();
    assert_eq!(files.len(), 3);
    assert!(files[0].content.is_none());
    assert!(files[0].error.as_deref().unwrap().contains("文件不存在"));
    assert!(files[1].content.is_none());
    assert!(files[1].error.is_some());
    assert_eq!(files[2].content.as_deref(), Some(""));
    assert!(files[2].error.is_none());
}

#[tokio::test]
async fn rejects_stale_keys_and_other_tools_instead_of_reading_another_connection() {
    let root = tempdir().unwrap();
    let manager = saved(
        &root.path().join("connections"),
        &[&root.path().join("file.yml")],
    );
    for (tool, key) in [
        (ToolId::DeepSeekHarness, "stale-key"),
        (ToolId::Codex, "fixture-key"),
    ] {
        let error = manager.configuration_files(tool, key).await.unwrap_err();
        assert!(error.to_string().contains("连接已改变"));
    }
}
