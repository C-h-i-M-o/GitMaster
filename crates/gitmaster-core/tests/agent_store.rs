use gitmaster_core::agent::{
    AgentStore, EventKind, PermissionMode, ProviderProfile, ProviderProtocol,
};
use serde_json::json;

/// 构造没有密钥的有效配置，用真实临时文件验证持久化。
fn profile() -> ProviderProfile {
    ProviderProfile {
        id: "a1".into(),
        name: "本地测试".into(),
        protocol: ProviderProtocol::Responses,
        base_url: "http://127.0.0.1:9999/v1".into(),
        model: "test-model".into(),
        credential_id: "c1".into(),
    }
}

#[test]
fn settings_roundtrip_and_invalid_endpoint_does_not_replace_saved_profile() {
    let dir = tempfile::tempdir().unwrap();
    let store = AgentStore::open(dir.path().into()).unwrap();
    store.save_profile(profile()).unwrap();
    store.set_permission(PermissionMode::AutoApprove).unwrap();
    let mut bad = profile();
    bad.base_url = "https://secret@example.com/v1?api_key=secret".into();
    assert!(store.save_profile(bad).is_err());
    let reopened = AgentStore::open(dir.path().into()).unwrap();
    let settings = reopened.settings().unwrap();
    assert_eq!(settings.profiles[0].base_url, "http://127.0.0.1:9999/v1");
    assert_eq!(
        settings.default_permission_mode,
        PermissionMode::AutoApprove
    );
    let stored = std::fs::read_to_string(dir.path().join("settings.json")).unwrap();
    assert!(!stored.contains("apiKey"));
    assert!(!stored.contains("secret"));
}

#[test]
fn chat_survives_restart_archive_delete_and_accounts_real_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let store = AgentStore::open(dir.path().into()).unwrap();
    store.save_profile(profile()).unwrap();
    let chat = store
        .create_chat("a1", Some("/example/project".into()))
        .unwrap();
    store
        .append(
            &chat.id,
            "f1",
            EventKind::UserMessage {
                text: "你好".into(),
            },
        )
        .unwrap();
    let reopened = AgentStore::open(dir.path().into()).unwrap();
    assert_eq!(reopened.read_chat(&chat.id).unwrap().events.len(), 1);
    let usage = reopened.usage().unwrap();
    assert!(usage.chat_bytes > 0);
    reopened.update_chat(&chat.id, "开发记录", true).unwrap();
    let archived = reopened.usage().unwrap();
    assert_eq!(archived.chat_bytes, 0);
    assert!(archived.archived_bytes > 0);
    reopened.delete_chat(&chat.id).unwrap();
    assert!(reopened.list_chats().unwrap().chats.is_empty());
    assert_eq!(reopened.usage().unwrap().archived_bytes, 0);
    assert!(reopened.read_chat("../settings").is_err());
}

#[test]
fn interrupted_tail_is_repaired_before_append_but_middle_corruption_is_reported() {
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let store = AgentStore::open(dir.path().into()).unwrap();
    store.save_profile(profile()).unwrap();
    let chat = store.create_chat("a1", None).unwrap();
    store
        .append(&chat.id, "f1", EventKind::UserMessage { text: "一".into() })
        .unwrap();
    let path = dir.path().join("chats").join(&chat.id).join("events.jsonl");
    std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(b"{\"unfinished\":")
        .unwrap();
    assert_eq!(store.read_chat(&chat.id).unwrap().events.len(), 1);
    store
        .append(&chat.id, "f1", EventKind::UserMessage { text: "二".into() })
        .unwrap();
    assert_eq!(store.read_chat(&chat.id).unwrap().events.len(), 2);
    std::fs::write(&path, b"not-json\n{}").unwrap();
    assert_eq!(
        store.read_chat(&chat.id).unwrap_err().code,
        "AGENT_STORAGE_CORRUPT"
    );
    let other = store.create_chat("a1", None).unwrap();
    assert!(store.read_chat(&other.id).unwrap().events.is_empty());
}

#[test]
fn unknown_settings_fields_and_future_versions_are_not_silently_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let _store = AgentStore::open(dir.path().into()).unwrap();
    std::fs::write(
        dir.path().join("settings.json"),
        json!({"version":99}).to_string(),
    )
    .unwrap();
    let store = AgentStore::open(dir.path().into()).unwrap();
    assert!(store.save_profile(profile()).is_err());
    assert!(std::fs::read_to_string(dir.path().join("settings.json"))
        .unwrap()
        .contains("99"));
}

#[cfg(unix)]
#[test]
fn storage_rejects_symlink_chat_directory() {
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let store = AgentStore::open(dir.path().into()).unwrap();
    std::os::unix::fs::symlink(outside.path(), dir.path().join("chats").join("a1")).unwrap();
    assert!(store.delete_chat("a1").is_err());
    assert!(outside.path().exists());
}
