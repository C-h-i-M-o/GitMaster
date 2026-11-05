use gitmaster_core::agent::{
    AgentStore, EventKind, FileChangeContent, ProviderProfile, ProviderProtocol,
};

/// 创建真实临时存储并建立可供事件记录引用的历史文件快照。
fn stored_change(
    store: &AgentStore,
    chat_id: &str,
    run_id: &str,
) -> gitmaster_core::agent::FileChangeArtifact {
    let content = FileChangeContent {
        path: "src/cache.rs".into(),
        before_content: Some("旧缓存".into()),
        after_content: "新缓存".into(),
        before_version: Some("a".repeat(64)),
        after_version: "b".repeat(64),
        verified: true,
    };
    let artifact = store.save_file_change(chat_id, &content).unwrap();
    store
        .append(
            chat_id,
            run_id,
            EventKind::FileChanged {
                call_id: "call-a".into(),
                change: artifact.clone(),
            },
        )
        .unwrap();
    artifact
}

/// 用临时目录构造隔离存储，不触碰用户数据。
fn temporary_store() -> (tempfile::TempDir, AgentStore, String, String) {
    let directory = tempfile::tempdir().unwrap();
    let store = AgentStore::open(directory.path().into()).unwrap();
    store
        .save_profile(ProviderProfile {
            id: "aa".into(),
            name: "本地测试".into(),
            protocol: ProviderProtocol::Responses,
            base_url: "http://127.0.0.1:9999/v1".into(),
            model: "test-model".into(),
            credential_id: "bb".into(),
        })
        .unwrap();
    let first = store.create_chat("aa", None).unwrap();
    let second = store.create_chat("aa", None).unwrap();
    (directory, store, first.id, second.id)
}

/// 只有同一聊天事件明确引用的历史工件可以读取。
#[test]
fn change_artifact_requires_same_chat_event_reference() {
    let (_directory, store, chat, other_chat) = temporary_store();
    let artifact = stored_change(&store, &chat, "cc");

    let content = store
        .read_file_change(&chat, &artifact.artifact_id)
        .unwrap();
    assert_eq!(content.path, "src/cache.rs");
    assert_eq!(content.after_content, "新缓存");
    assert!(store
        .read_file_change(&other_chat, &artifact.artifact_id)
        .is_err());
    assert!(store.read_file_change(&chat, "dd").is_err());
}

/// 磁盘工件内容被篡改后，事件摘要校验必须拒绝读取。
#[test]
fn change_artifact_rejects_tampered_snapshot() {
    let (directory, store, chat, _other_chat) = temporary_store();
    let artifact = stored_change(&store, &chat, "cc");
    let path = directory
        .path()
        .join("chats")
        .join(&chat)
        .join("changes")
        .join(format!("{}.json", artifact.artifact_id));
    let mut snapshot: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    snapshot["afterContent"] = serde_json::Value::String("被篡改".into());
    std::fs::write(&path, serde_json::to_vec(&snapshot).unwrap()).unwrap();

    assert_eq!(
        store
            .read_file_change(&chat, &artifact.artifact_id)
            .unwrap_err()
            .code,
        "AGENT_STORAGE_CORRUPT"
    );
}

/// 非法工件 ID 不能逃出聊天目录读取其他文件。
#[test]
fn change_artifact_rejects_directory_traversal_id() {
    let (_directory, store, chat, _other_chat) = temporary_store();
    assert!(store.read_file_change(&chat, "../settings").is_err());
}
