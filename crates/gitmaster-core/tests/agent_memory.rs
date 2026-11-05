use gitmaster_core::agent::{
    AgentStore, EventKind, MemoryExcludedReason, MemoryInput, MemoryScope, MemorySelection,
    MemorySource, ProviderProfile, ProviderProtocol,
};
use serde_json::json;

/// 建立启用记忆测试所需的临时 AgentStore 和有效模型配置。
fn test_store() -> (tempfile::TempDir, AgentStore) {
    let directory = tempfile::tempdir().unwrap();
    let store = AgentStore::open(directory.path().to_path_buf()).unwrap();
    store
        .save_profile(ProviderProfile {
            id: "aa".into(),
            name: "记忆测试配置".into(),
            protocol: ProviderProtocol::Responses,
            base_url: "http://127.0.0.1:9999/v1".into(),
            model: "memory-test-model".into(),
            credential_id: "cc".into(),
        })
        .unwrap();
    (directory, store)
}

/// 在指定仓库路径创建测试聊天并返回后端签发的工作区身份。
fn workspace_chat(store: &AgentStore, path: &str) -> gitmaster_core::agent::ChatSession {
    store.create_chat("aa", Some(path.to_owned())).unwrap()
}

/// 构造一条手工来源、启用状态的记忆输入。
fn input(scope: MemoryScope, workspace_id: Option<&str>, text: &str) -> MemoryInput {
    MemoryInput {
        id: None,
        expected_revision: None,
        scope,
        workspace_id: workspace_id.map(str::to_owned),
        text: text.to_owned(),
        tags: vec![],
        source: MemorySource {
            chat_id: None,
            sequence: None,
        },
        enabled: true,
    }
}

/// 工作区记忆隔离；用户偏好必须由调用者显式纳入结果。
#[test]
fn workspace_memory_is_isolated_and_user_scope_is_opt_in() {
    let (_directory, store) = test_store();
    let one = workspace_chat(&store, "C:\\repos\\one");
    let two = workspace_chat(&store, "C:\\repos\\two");
    store
        .append(
            &one.id,
            "ab",
            EventKind::UserMessage {
                text: "保存一条跨工作区偏好".into(),
            },
        )
        .unwrap();
    let first = store
        .save_memory(input(
            MemoryScope::Workspace,
            Some(&one.workspace_id),
            "workspace one fact",
        ))
        .unwrap();
    store
        .save_memory(input(
            MemoryScope::Workspace,
            Some(&two.workspace_id),
            "workspace two fact",
        ))
        .unwrap();
    let mut user_preference = input(MemoryScope::User, None, "user writing preference");
    user_preference.source = MemorySource {
        chat_id: Some(one.id.clone()),
        sequence: Some(1),
    };
    let preference = store.save_memory(user_preference).unwrap();

    let default_scope = store.list_memories(&one.workspace_id, false).unwrap();
    assert!(default_scope.iter().any(|entry| entry.id == first.id));
    assert!(!default_scope.iter().any(|entry| entry.id == preference.id));
    assert!(!default_scope
        .iter()
        .any(|entry| entry.text == "workspace two fact"));

    let explicit_user_scope = store.list_memories(&one.workspace_id, true).unwrap();
    assert!(explicit_user_scope
        .iter()
        .any(|entry| entry.id == preference.id));
    assert!(!explicit_user_scope
        .iter()
        .any(|entry| entry.text == "workspace two fact"));
}

/// 记忆编辑以 revision 乐观锁保护并跨重开持久化，停用和删除立即生效。
#[test]
fn save_memory_uses_revision_and_state_changes_are_immediate() {
    let (directory, store) = test_store();
    let chat = workspace_chat(&store, "C:\\repos\\revision");
    let saved = store
        .save_memory(input(
            MemoryScope::Workspace,
            Some(&chat.workspace_id),
            "初始记忆内容",
        ))
        .unwrap();

    let mut edited = input(
        MemoryScope::Workspace,
        Some(&chat.workspace_id),
        "更新后的记忆内容",
    );
    edited.id = Some(saved.id.clone());
    edited.expected_revision = Some(saved.revision);
    let updated = store.save_memory(edited).unwrap();
    assert_eq!(updated.revision, saved.revision + 1);
    assert!(store
        .save_memory(MemoryInput {
            id: Some(saved.id.clone()),
            expected_revision: Some(saved.revision),
            scope: saved.scope,
            workspace_id: saved.workspace_id.clone(),
            text: "过期并发编辑".into(),
            tags: saved.tags.clone(),
            source: saved.source.clone(),
            enabled: true,
        })
        .is_err());

    let reopened = AgentStore::open(directory.path().to_path_buf()).unwrap();
    let persisted = reopened.list_memories(&chat.workspace_id, false).unwrap();
    assert_eq!(persisted.len(), 1);
    assert_eq!(persisted[0].revision, updated.revision);
    assert_eq!(persisted[0].text, "更新后的记忆内容");

    let mut disabled = input(
        MemoryScope::Workspace,
        Some(&chat.workspace_id),
        "更新后的记忆内容",
    );
    disabled.id = Some(updated.id.clone());
    disabled.expected_revision = Some(updated.revision);
    disabled.enabled = false;
    let disabled = reopened.save_memory(disabled).unwrap();
    assert!(!reopened
        .list_memories(&chat.workspace_id, false)
        .unwrap()
        .iter()
        .any(|entry| entry.id == disabled.id && entry.enabled));

    reopened
        .delete_memory(&disabled.id, disabled.revision)
        .unwrap();
    assert!(!reopened
        .list_memories(&chat.workspace_id, false)
        .unwrap()
        .iter()
        .any(|entry| entry.id == disabled.id));
}

/// 来源只接受同工作区聊天中的用户或助手可展示正文，不接受 wire、工具或其他工作区。
#[test]
fn source_must_reference_displayable_message_in_same_workspace() {
    let (_directory, store) = test_store();
    let chat = workspace_chat(&store, "C:\\repos\\source");
    let other = workspace_chat(&store, "C:\\repos\\other-source");
    store
        .append(
            &chat.id,
            "ab",
            EventKind::UserMessage {
                text: "用户明确提供的来源".into(),
            },
        )
        .unwrap();
    store
        .append(
            &chat.id,
            "ab",
            EventKind::AssistantMessage {
                text: "助手可展示回答".into(),
                protocol: ProviderProtocol::Responses,
                wire: json!({"private": "opaque"}),
                calls: vec![],
            },
        )
        .unwrap();
    store
        .append(
            &chat.id,
            "ab",
            EventKind::ToolResult {
                call_id: "memory-call".into(),
                name: "read_file".into(),
                result: json!({"text": "不可作为可展示来源"}),
                is_error: false,
            },
        )
        .unwrap();

    let mut from_user = input(
        MemoryScope::Workspace,
        Some(&chat.workspace_id),
        "从用户消息保存的记忆",
    );
    from_user.source = MemorySource {
        chat_id: Some(chat.id.clone()),
        sequence: Some(1),
    };
    assert!(store.save_memory(from_user).is_ok());

    let mut from_assistant = input(
        MemoryScope::Workspace,
        Some(&chat.workspace_id),
        "从助手回答保存的记忆",
    );
    from_assistant.source = MemorySource {
        chat_id: Some(chat.id.clone()),
        sequence: Some(2),
    };
    assert!(store.save_memory(from_assistant).is_ok());

    let mut from_tool = input(
        MemoryScope::Workspace,
        Some(&chat.workspace_id),
        "工具结果不应作为来源",
    );
    from_tool.source = MemorySource {
        chat_id: Some(chat.id),
        sequence: Some(3),
    };
    assert!(store.save_memory(from_tool).is_err());

    let mut cross_workspace = input(MemoryScope::User, None, "来源聊天不得扩大检索范围");
    cross_workspace.source = MemorySource {
        chat_id: Some(other.id),
        sequence: Some(1),
    };
    assert!(store.save_memory(cross_workspace).is_err());
}

/// 记忆正文和标签执行凭据过滤、长度和条目数量限制。
#[test]
fn memory_rejects_credentials_oversized_text_and_too_many_tags() {
    let (_directory, store) = test_store();
    let chat = workspace_chat(&store, "C:\\repos\\limits");

    let mut credential = input(
        MemoryScope::Workspace,
        Some(&chat.workspace_id),
        "使用 sk-1234567890abcdefghijklmnop",
    );
    assert!(store.save_memory(credential.clone()).is_err());

    credential.text = "正文".repeat(5000);
    assert!(store.save_memory(credential).is_err());

    let mut tags = input(MemoryScope::Workspace, Some(&chat.workspace_id), "合法正文");
    tags.tags = (0..11).map(|index| format!("tag-{index}")).collect();
    assert!(store.save_memory(tags).is_err());

    let mut long_tag = input(MemoryScope::Workspace, Some(&chat.workspace_id), "合法正文");
    long_tag.tags = vec!["t".repeat(65)];
    assert!(store.save_memory(long_tag).is_err());
}

/// 检索按正文/标签匹配，透明列出排除原因并遵守条数与硬预算上限。
#[test]
fn selection_matches_text_and_tags_and_reports_budget_exclusions() {
    let (_directory, store) = test_store();
    let chat = workspace_chat(&store, "C:\\repos\\selection");
    let mut by_tag = input(MemoryScope::Workspace, Some(&chat.workspace_id), "偏好内容");
    by_tag.tags = vec!["typescript".into()];
    let tagged = store.save_memory(by_tag).unwrap();
    let body_match = store
        .save_memory(input(
            MemoryScope::Workspace,
            Some(&chat.workspace_id),
            "项目使用 TypeScript strict 模式",
        ))
        .unwrap();
    let no_match = store
        .save_memory(input(
            MemoryScope::Workspace,
            Some(&chat.workspace_id),
            "不相关的记忆",
        ))
        .unwrap();

    let selected: MemorySelection = store
        .select_memories(&chat.workspace_id, false, "typescript", &[], 256)
        .unwrap();
    assert!(selected.selected.iter().any(|entry| entry.id == tagged.id));
    assert!(selected
        .selected
        .iter()
        .any(|entry| entry.id == body_match.id));
    assert!(!selected
        .selected
        .iter()
        .any(|entry| entry.id == no_match.id));
    assert!(selected
        .excluded
        .iter()
        .any(|entry| { entry.id == no_match.id && entry.reason == MemoryExcludedReason::NoMatch }));
    assert!(selected.selected.len() <= 8);
    let selected_bytes = selected
        .selected
        .iter()
        .map(|entry| entry.text.len() + entry.tags.iter().map(String::len).sum::<usize>() + 64)
        .sum::<usize>();
    assert_eq!(selected.estimated_tokens, selected_bytes);
    assert!(selected.estimated_tokens <= 256);
    assert!(selected.estimated_tokens <= 2048);

    let excluded: MemorySelection = store
        .select_memories(
            &chat.workspace_id,
            false,
            "typescript",
            &[tagged.id.clone()],
            2048,
        )
        .unwrap();
    assert!(!excluded.selected.iter().any(|entry| entry.id == tagged.id));
    assert!(excluded.excluded.iter().any(|entry| {
        entry.id == tagged.id && entry.reason == MemoryExcludedReason::UserExcluded
    }));

    let tiny_budget = store
        .select_memories(&chat.workspace_id, false, "typescript", &[], 1)
        .unwrap();
    assert!(tiny_budget.estimated_tokens <= 1);
    assert!(tiny_budget
        .excluded
        .iter()
        .any(|entry| entry.reason == MemoryExcludedReason::Budget));

    let oversized_budget = store
        .select_memories(&chat.workspace_id, false, "typescript", &[], 5000)
        .unwrap();
    assert!(oversized_budget.estimated_tokens <= 2048);
}

/// 连续中文查询按双字片段匹配标签，并严格隔离其他工作区。
#[test]
fn chinese_query_matches_tag_without_leaking_other_workspace_entries() {
    let (_directory, store) = test_store();
    let current = workspace_chat(&store, "C:\\repos\\current");
    let other = workspace_chat(&store, "C:\\repos\\other");
    let mut current_entry = input(
        MemoryScope::Workspace,
        Some(&current.workspace_id),
        "项目遵循团队约定",
    );
    current_entry.tags = vec!["缓存".into()];
    let current_saved = store.save_memory(current_entry).unwrap();

    let mut other_entry = input(
        MemoryScope::Workspace,
        Some(&other.workspace_id),
        "另一个项目的记忆",
    );
    other_entry.tags = vec!["缓存".into()];
    let other_saved = store.save_memory(other_entry).unwrap();

    let result = store
        .select_memories(
            &current.workspace_id,
            false,
            "请继续优化项目缓存逻辑",
            &[],
            2048,
        )
        .unwrap();
    assert!(result
        .selected
        .iter()
        .any(|entry| entry.id == current_saved.id));
    assert!(!result
        .selected
        .iter()
        .any(|entry| entry.id == other_saved.id));
    assert!(!result
        .excluded
        .iter()
        .any(|entry| entry.id == other_saved.id));
}

/// 空查询按最近启用时间返回，来源不存在或工作区身份不合法时拒绝。
#[test]
fn empty_query_uses_recent_enabled_entries_and_invalid_scope_is_rejected() {
    let (_directory, store) = test_store();
    let chat = workspace_chat(&store, "C:\\repos\\recent");
    let first = store
        .save_memory(input(
            MemoryScope::Workspace,
            Some(&chat.workspace_id),
            "较早的记忆",
        ))
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(2));
    let second = store
        .save_memory(input(
            MemoryScope::Workspace,
            Some(&chat.workspace_id),
            "较新的记忆",
        ))
        .unwrap();
    let recent = store
        .select_memories(&chat.workspace_id, false, "  ", &[], 2048)
        .unwrap();
    assert_eq!(
        recent.selected.first().map(|entry| entry.id.as_str()),
        Some(second.id.as_str())
    );
    assert!(recent.selected.iter().any(|entry| entry.id == first.id));

    let mut missing_source = input(
        MemoryScope::Workspace,
        Some(&chat.workspace_id),
        "不存在的来源事件",
    );
    missing_source.source = MemorySource {
        chat_id: Some(chat.id),
        sequence: Some(999),
    };
    assert!(store.save_memory(missing_source).is_err());

    let invalid_workspace = input(
        MemoryScope::Workspace,
        Some("folder-not-a-sha256"),
        "工作区身份非法",
    );
    assert!(store.save_memory(invalid_workspace).is_err());
}
