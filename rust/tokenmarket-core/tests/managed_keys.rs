use tokenmarket_core::config::ManagedKeyRecord;

#[test]
fn deserializes_legacy_camel_case_key_records() {
    let json = r#"[{
        "tool": "claude-code",
        "keyName": "Token Market CLI",
        "keyId": "50032",
        "setupId": "setup_123",
        "configPath": "/tmp/config.json",
        "backupPath": null,
        "model": "glm-5.3",
        "createdAt": "2026-10-27T00:00:00.000Z",
        "revocable": true
    }]"#;
    let records: Vec<ManagedKeyRecord> = serde_json::from_str(json).unwrap();
    assert_eq!(records[0].key_id, "50032");
    assert_eq!(records[0].key_name, "Token Market CLI");
}
