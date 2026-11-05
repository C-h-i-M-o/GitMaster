use gitmaster_core::agent::{AgentStore, EventKind, ProviderProfile, ProviderProtocol};
use serde_json::json;
use std::fs;

/// 建立带有临时存储根和有效本地测试配置的 AgentStore。
fn test_store() -> (tempfile::TempDir, AgentStore) {
    let dir = tempfile::tempdir().unwrap();
    let store = AgentStore::open(dir.path().to_path_buf()).unwrap();
    store
        .save_profile(ProviderProfile {
            id: "a1".into(),
            name: "本地搜索测试".into(),
            protocol: ProviderProtocol::Responses,
            base_url: "http://127.0.0.1:9999/v1".into(),
            model: "test-model".into(),
            credential_id: "c1".into(),
        })
        .unwrap();
    (dir, store)
}

/// 在指定工作区创建聊天并绑定临时测试配置。
fn create_chat(store: &AgentStore, repository_path: &str) -> gitmaster_core::agent::ChatSession {
    store
        .create_chat("a1", Some(repository_path.to_owned()))
        .unwrap()
}

/// 向聊天追加一条用户消息并持久化其可搜索正文。
fn append_user(store: &AgentStore, chat_id: &str, text: &str) {
    store
        .append(
            chat_id,
            "a1",
            EventKind::UserMessage {
                text: text.to_owned(),
            },
        )
        .unwrap();
}

/// 标题、用户消息和助手消息均可命中，且消息命中保留定位信息。
#[test]
fn search_matches_title_and_displayable_message_text() {
    let (_dir, store) = test_store();
    let chat = create_chat(&store, "/repo/one");
    store.update_chat(&chat.id, "Rust 项目记录", false).unwrap();
    append_user(&store, &chat.id, "User marker appears here.");
    store
        .append(
            &chat.id,
            "a1",
            EventKind::AssistantMessage {
                text: "Assistant marker appears here.".into(),
                protocol: ProviderProtocol::Responses,
                wire: json!({"private": "wire marker"}),
                calls: vec![],
            },
        )
        .unwrap();

    let title_hits = store
        .search_chats(&chat.workspace_id, "  RUST 项目  ", false)
        .unwrap();
    assert_eq!(title_hits.results.len(), 1);
    assert_eq!(title_hits.results[0].chat_id, chat.id);
    assert_eq!(title_hits.results[0].title, "Rust 项目记录");
    assert_eq!(title_hits.results[0].sequence, None);
    assert_eq!(title_hits.results[0].snippet, None);

    let user_hits = store
        .search_chats(&chat.workspace_id, "user MARKER", false)
        .unwrap();
    assert_eq!(user_hits.results.len(), 1);
    assert_eq!(user_hits.results[0].sequence, Some(1));
    assert!(user_hits.results[0]
        .snippet
        .as_deref()
        .unwrap()
        .contains("User marker"));

    let assistant_hits = store
        .search_chats(&chat.workspace_id, "assistant marker", false)
        .unwrap();
    assert_eq!(assistant_hits.results.len(), 1);
    assert_eq!(assistant_hits.results[0].sequence, Some(2));
    assert!(assistant_hits.results[0]
        .snippet
        .as_deref()
        .unwrap()
        .contains("Assistant marker"));
}

/// 搜索只检查用户和助手可展示正文，不检查协议 wire 或工具结果 JSON。
#[test]
fn search_excludes_opaque_protocol_and_tool_payloads() {
    let (_dir, store) = test_store();
    let chat = create_chat(&store, "/repo/one");
    store
        .append(
            &chat.id,
            "a1",
            EventKind::AssistantMessage {
                text: "visible response".into(),
                protocol: ProviderProtocol::Responses,
                wire: json!({"private": "wire marker"}),
                calls: vec![],
            },
        )
        .unwrap();
    store
        .append(
            &chat.id,
            "a1",
            EventKind::ToolResult {
                call_id: "call1".into(),
                name: "read_file".into(),
                result: json!({"private": "tool marker"}),
                is_error: false,
            },
        )
        .unwrap();

    assert!(store
        .search_chats(&chat.workspace_id, "wire marker", false)
        .unwrap()
        .results
        .is_empty());
    assert!(store
        .search_chats(&chat.workspace_id, "tool marker", false)
        .unwrap()
        .results
        .is_empty());
}

/// 搜索按工作区和归档状态隔离，不返回其他项目的相同命中。
#[test]
fn search_filters_workspace_and_archive_state() {
    let (_dir, store) = test_store();
    let active = create_chat(&store, "/repo/one");
    let archived = create_chat(&store, "/repo/one");
    let other_workspace = create_chat(&store, "/repo/two");
    append_user(&store, &active.id, "shared marker");
    append_user(&store, &archived.id, "shared marker");
    append_user(&store, &other_workspace.id, "shared marker");
    store.update_chat(&archived.id, "已归档", true).unwrap();

    let active_hits = store
        .search_chats(&active.workspace_id, "shared marker", false)
        .unwrap();
    assert_eq!(active_hits.results.len(), 1);
    assert_eq!(active_hits.results[0].chat_id, active.id);

    let archived_hits = store
        .search_chats(&active.workspace_id, "shared marker", true)
        .unwrap();
    assert_eq!(archived_hits.results.len(), 1);
    assert_eq!(archived_hits.results[0].chat_id, archived.id);
}

/// 损坏聊天只标记为不可用，且不影响同筛选范围内的有效命中。
#[test]
fn search_reports_corrupt_content_without_cross_workspace_leakage() {
    let (_dir, store) = test_store();
    let valid = create_chat(&store, "/repo/one");
    let corrupt = create_chat(&store, "/repo/one");
    let other_workspace_corrupt = create_chat(&store, "/repo/two");
    append_user(&store, &valid.id, "visible marker");

    for chat_id in [&corrupt.id, &other_workspace_corrupt.id] {
        let events_path = _dir.path().join("chats").join(chat_id).join("events.jsonl");
        fs::write(events_path, b"not-json\n").unwrap();
    }

    let results = store
        .search_chats(&valid.workspace_id, "visible marker", false)
        .unwrap();
    assert_eq!(results.results.len(), 1);
    assert_eq!(results.results[0].chat_id, valid.id);
    assert_eq!(results.unavailable, vec![corrupt.id]);
}

/// 删除聊天后下一次搜索立即从磁盘状态读取，不依赖过期重复索引。
#[test]
fn search_reflects_chat_deletion_immediately() {
    let (_dir, store) = test_store();
    let chat = create_chat(&store, "/repo/one");
    append_user(&store, &chat.id, "deletion marker");
    assert_eq!(
        store
            .search_chats(&chat.workspace_id, "deletion marker", false)
            .unwrap()
            .results
            .len(),
        1
    );

    store.delete_chat(&chat.id).unwrap();

    assert!(store
        .search_chats(&chat.workspace_id, "deletion marker", false)
        .unwrap()
        .results
        .is_empty());
}

/// 空白、超长查询及非法工作区身份均明确拒绝。
#[test]
fn search_rejects_invalid_query_and_workspace() {
    let (_dir, store) = test_store();
    let chat = create_chat(&store, "/repo/one");

    assert_eq!(
        store
            .search_chats(&chat.workspace_id, " \t\n ", false)
            .unwrap_err()
            .code,
        "AGENT_SEARCH_QUERY"
    );
    assert_eq!(
        store
            .search_chats(&chat.workspace_id, &"字".repeat(201), false)
            .unwrap_err()
            .code,
        "AGENT_SEARCH_QUERY"
    );
    assert_eq!(
        store
            .search_chats("folder-invalid", "needle", false)
            .unwrap_err()
            .code,
        "AGENT_SEARCH_WORKSPACE"
    );
    assert_eq!(
        store.search_chats("", "needle", false).unwrap_err().code,
        "AGENT_SEARCH_WORKSPACE"
    );
}

/// 查询匹配超过上限时只返回最新工作区列表中的前 100 条。
#[test]
fn search_caps_results_at_one_hundred() {
    let (_dir, store) = test_store();
    let mut workspace_id = String::new();
    for index in 0..105 {
        let chat = create_chat(&store, "/repo/one");
        workspace_id = chat.workspace_id.clone();
        store
            .update_chat(&chat.id, &format!("needle chat {index}"), false)
            .unwrap();
    }

    let results = store.search_chats(&workspace_id, "needle", false).unwrap();
    assert_eq!(results.results.len(), 100);
    assert!(results
        .results
        .iter()
        .all(|result| result.sequence.is_none()));
}
