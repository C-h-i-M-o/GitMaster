use gitmaster_core::agent::{AgentStore, PermissionMode};
use serde_json::{json, Value};
use std::fs;

/// 旧配置使用相同供应商但不同凭据，迁移不可按地址合并账号。
fn legacy() -> Value {
    json!({"version":1,"profiles":[
        {"id":"aa","name":"账号一","protocol":"chatCompletions","baseUrl":"https://api.deepseek.com","model":"deepseek-flash","credentialId":"ab"},
        {"id":"bb","name":"账号二","protocol":"responses","baseUrl":"https://api.deepseek.com","model":"deepseek-v4-pro","credentialId":"bc"}
    ],"activeProfileId":"bb","defaultPermissionMode":"fullAccess"})
}

/// 迁移原子产生 v2，保留原凭据与旧配置备份，重复读取不重新迁移。
#[test]
fn migrates_without_merging_accounts_or_overwriting_backup() {
    let dir = tempfile::tempdir().unwrap();
    let original = legacy();
    fs::write(dir.path().join("settings.json"), original.to_string()).unwrap();
    let store = AgentStore::open(dir.path().into()).unwrap();
    let settings = store.settings().unwrap();
    assert_eq!(settings.version, 2);
    assert_eq!(
        settings.default_permission_mode,
        PermissionMode::AutoApprove
    );
    let disk: Value =
        serde_json::from_slice(&fs::read(dir.path().join("settings.json")).unwrap()).unwrap();
    assert_eq!(disk["providers"].as_array().unwrap().len(), 2);
    assert_eq!(disk["models"][0]["providerId"], "aa");
    assert_eq!(disk["providers"][0]["credentialId"], "ab");
    assert_eq!(disk["activeModelConfigId"], "bb");
    assert!(disk.get("profiles").is_none());
    let backup = fs::read(dir.path().join("settings.v1-backup.json")).unwrap();
    assert_eq!(serde_json::from_slice::<Value>(&backup).unwrap(), original);
    store.settings().unwrap();
    assert_eq!(
        fs::read(dir.path().join("settings.v1-backup.json")).unwrap(),
        backup
    );
}

/// 未来格式不能被旧程序当作默认配置覆盖。
#[test]
fn future_version_is_preserved() {
    let dir = tempfile::tempdir().unwrap();
    let original = r#"{"version":99,"future":"preserve"}"#;
    fs::write(dir.path().join("settings.json"), original).unwrap();
    let store = AgentStore::open(dir.path().into()).unwrap();
    assert!(store.settings().is_err());
    assert_eq!(
        fs::read_to_string(dir.path().join("settings.json")).unwrap(),
        original
    );
    assert!(!dir.path().join("settings.v1-backup.json").exists());
}

/// 备份路径不可写时不发布迁移后的配置。
#[test]
fn failed_backup_preserves_v1() {
    let dir = tempfile::tempdir().unwrap();
    let original = legacy().to_string();
    fs::write(dir.path().join("settings.json"), &original).unwrap();
    fs::create_dir(dir.path().join("settings.v1-backup.json")).unwrap();
    let store = AgentStore::open(dir.path().into()).unwrap();
    assert!(store.settings().is_err());
    assert_eq!(
        fs::read_to_string(dir.path().join("settings.json")).unwrap(),
        original
    );
}
