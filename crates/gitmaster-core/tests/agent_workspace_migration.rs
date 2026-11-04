use gitmaster_core::agent::{AgentStore, ProviderProfile, ProviderProtocol};
use serde_json::Value;
use std::fs;

/// 使用真实私有存储，迁移过程不要求历史项目目录仍然存在。
#[test]
fn legacy_chats_receive_stable_workspace_identity_without_touching_events() {
    let dir = tempfile::tempdir().unwrap();
    let store = AgentStore::open(dir.path().into()).unwrap();
    store
        .save_profile(ProviderProfile {
            id: "aa".into(),
            name: "测试模型".into(),
            protocol: ProviderProtocol::Responses,
            base_url: "https://example.com/v1".into(),
            model: "test".into(),
            credential_id: "bb".into(),
        })
        .unwrap();
    for root in [None, Some("C:\\missing-project".to_owned())] {
        let chat = store.create_chat("aa", root.clone()).unwrap();
        let path = dir.path().join("chats").join(&chat.id).join("session.json");
        let mut legacy = serde_json::to_value(&chat).unwrap();
        legacy["version"] = Value::from(1);
        let object = legacy.as_object_mut().unwrap();
        object.remove("workspaceId");
        object.remove("modelConfigId");
        fs::write(&path, legacy.to_string()).unwrap();
        let first = serde_json::to_value(store.read_chat(&chat.id).unwrap().session).unwrap();
        assert_eq!(first["version"], 2);
        assert_eq!(first["modelConfigId"], "aa");
        if root.is_none() {
            assert_eq!(first["workspaceId"], "default");
        } else {
            assert!(first["workspaceId"]
                .as_str()
                .unwrap()
                .starts_with("folder-"));
        }
        assert_eq!(
            first,
            serde_json::to_value(store.read_chat(&chat.id).unwrap().session).unwrap()
        );
        assert!(!path.parent().unwrap().join("events.jsonl").exists());
        let saved: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved, first);
        let backup: Value = serde_json::from_slice(
            &fs::read(path.with_file_name("session.v1-backup.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(backup, legacy);
    }
}
