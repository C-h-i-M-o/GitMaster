use gitmaster_core::agent::*;

/// 模型、权限及完整参数覆盖按聊天保存，重新打开存储后不会污染其他聊天。
#[test]
fn configuration_is_per_chat_and_persistent() {
    let directory = tempfile::tempdir().unwrap();
    let store = AgentStore::open(directory.path().into()).unwrap();
    assert_eq!(
        store.settings().unwrap().default_permission_mode,
        PermissionMode::AutoApprove
    );
    for id in ["aa", "bb"] {
        store
            .save_profile(ProviderProfile {
                id: id.into(),
                name: id.into(),
                protocol: ProviderProtocol::Responses,
                base_url: "https://example.com/v1".into(),
                model: "custom".into(),
                credential_id: "cc".into(),
            })
            .unwrap();
    }
    store
        .set_permission(PermissionMode::RequestApproval)
        .unwrap();
    let first = store.create_chat("aa", None).unwrap();
    let second = store.create_chat("aa", None).unwrap();
    let parameters = ModelParameters {
        max_output_tokens: Some(2048),
        ..Default::default()
    };
    let saved = store
        .configure_chat(
            &first.id,
            "bb",
            PermissionMode::AutoApprove,
            Some(parameters.clone()),
        )
        .unwrap();
    assert_eq!(saved.model_config_id, "bb");
    assert_eq!(saved.parameters, Some(parameters.clone()));
    drop(store);
    let store = AgentStore::open(directory.path().into()).unwrap();
    let saved = store.read_chat(&first.id).unwrap().session;
    assert_eq!(saved.parameters, Some(parameters));
    assert_eq!(saved.permission_mode, PermissionMode::AutoApprove);
    let untouched = store.read_chat(&second.id).unwrap().session;
    assert_eq!(untouched.model_config_id, "aa");
    assert!(untouched.parameters.is_none());
    assert_eq!(untouched.permission_mode, PermissionMode::RequestApproval);
    assert!(store
        .configure_chat(&first.id, "ff", PermissionMode::AutoApprove, None)
        .is_err());
    assert_eq!(
        store.read_chat(&first.id).unwrap().session.model_config_id,
        "bb"
    );
    assert!(store
        .configure_chat(
            &first.id,
            "bb",
            PermissionMode::AutoApprove,
            Some(ModelParameters {
                reasoning_effort: Some("invented".into()),
                ..Default::default()
            })
        )
        .is_err());
    store
        .configure_chat(&first.id, "bb", PermissionMode::AutoApprove, None)
        .unwrap();
    assert!(store
        .read_chat(&first.id)
        .unwrap()
        .session
        .parameters
        .is_none());
    assert_eq!(
        store.read_chat(&first.id).unwrap().session.memory,
        ChatMemoryConfig::default()
    );
    let memory = ChatMemoryConfig {
        enabled: true,
        include_user: true,
        excluded_ids: vec!["dd".into()],
    };
    store.set_chat_memory(&first.id, memory.clone()).unwrap();
    assert_eq!(store.read_chat(&first.id).unwrap().session.memory, memory);
    assert_eq!(
        store.read_chat(&second.id).unwrap().session.memory,
        ChatMemoryConfig::default()
    );
    store.clear_memory_exclusions(&first.id).unwrap();
    let cleared = store.read_chat(&first.id).unwrap().session.memory;
    assert!(cleared.include_user && cleared.excluded_ids.is_empty());
    store.update_chat(&first.id, "归档聊天", true).unwrap();
    assert_eq!(
        store
            .set_chat_memory(&first.id, ChatMemoryConfig::default())
            .unwrap_err()
            .code,
        "AGENT_ARCHIVED"
    );
    assert_eq!(store.read_chat(&first.id).unwrap().session.memory, cleared);
    let count = store.list_chats().unwrap().chats.len();
    assert!(store
        .create_chat_configured(
            "aa",
            None,
            Some(PermissionMode::AutoApprove),
            Some(ModelParameters {
                context_limit: Some(2048),
                max_output_tokens: Some(2048),
                ..Default::default()
            })
        )
        .is_err());
    assert_eq!(store.list_chats().unwrap().chats.len(), count);
    let parameters = ModelParameters {
        max_output_tokens: Some(1024),
        ..Default::default()
    };
    let created = store
        .create_chat_configured(
            "aa",
            None,
            Some(PermissionMode::AutoApprove),
            Some(parameters.clone()),
        )
        .unwrap();
    let persisted = store.read_chat(&created.id).unwrap().session;
    assert_eq!(persisted.parameters, Some(parameters));
    assert_eq!(persisted.permission_mode, PermissionMode::AutoApprove);
}
