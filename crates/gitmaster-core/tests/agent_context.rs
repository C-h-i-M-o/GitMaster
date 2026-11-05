use gitmaster_core::agent::{
    context::{build_context, CompactionRequest, ContextCheckpoint},
    provider::{build_request_configured, ModelCapabilities, ModelParameters, ToolDefinition},
    AgentEvent, AgentStore, EventKind, PermissionMode, ProviderProfile, ProviderProtocol,
    RunStatus, ToolCall,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;

/// 建立使用本地回环地址的模型配置，所有测试都不发起网络请求。
fn profile(model: &str) -> ProviderProfile {
    ProviderProfile {
        id: "aa".into(),
        name: "上下文测试模型".into(),
        protocol: ProviderProtocol::ChatCompletions,
        base_url: "http://127.0.0.1:9999/v1".into(),
        model: model.into(),
        credential_id: "bb".into(),
    }
}

/// 建立同时保存有效配置和测试聊天的临时 AgentStore。
fn test_store() -> (
    tempfile::TempDir,
    AgentStore,
    gitmaster_core::agent::ChatSession,
) {
    let directory = tempfile::tempdir().unwrap();
    let store = AgentStore::open(directory.path().to_path_buf()).unwrap();
    store.save_profile(profile("context-test-model")).unwrap();
    let chat = store
        .create_chat("aa", Some("C:\\repos\\context-test".into()))
        .unwrap();
    (directory, store, chat)
}

/// 按连续序号构造事件，运行标识使用合法十六进制值。
fn event(chat_id: &str, run_id: &str, sequence: u64, payload: EventKind) -> AgentEvent {
    AgentEvent {
        version: 1,
        sequence,
        session_id: chat_id.into(),
        run_id: run_id.into(),
        timestamp: sequence,
        payload,
    }
}

/// 构造一个完整轮次，可选择在助手回复中加入完整工具调用/结果组。
fn complete_turn(
    chat_id: &str,
    run_id: &str,
    first_sequence: u64,
    user_text: &str,
    tool_result: Option<Value>,
) -> Vec<AgentEvent> {
    let model = profile("context-test-model");
    let mut events = vec![
        event(
            chat_id,
            run_id,
            first_sequence,
            EventKind::RunStarted {
                profile: model.clone(),
                permission_mode: PermissionMode::RequestApproval,
            },
        ),
        event(
            chat_id,
            run_id,
            first_sequence + 1,
            EventKind::UserMessage {
                text: user_text.into(),
            },
        ),
    ];
    let calls = if tool_result.is_some() {
        vec![ToolCall {
            id: format!("{run_id}c"),
            name: "read_file".into(),
            arguments: json!({"path":"notes.txt"}),
        }]
    } else {
        vec![]
    };
    let mut wire = json!({"role":"assistant","content":"助手回复"});
    if !calls.is_empty() {
        wire["tool_calls"] = json!([{
            "id": calls[0].id,
            "type":"function",
            "function":{"name":calls[0].name,"arguments":calls[0].arguments.to_string()}
        }]);
    }
    events.push(event(
        chat_id,
        run_id,
        first_sequence + 2,
        EventKind::AssistantMessage {
            text: "助手回复".into(),
            protocol: model.protocol,
            wire,
            calls: calls.clone(),
        },
    ));
    let mut next = first_sequence + 3;
    if let Some(result) = tool_result {
        let call = calls[0].clone();
        events.push(event(
            chat_id,
            run_id,
            next,
            EventKind::ToolCall { call: call.clone() },
        ));
        next += 1;
        events.push(event(
            chat_id,
            run_id,
            next,
            EventKind::ToolResult {
                call_id: call.id,
                name: call.name,
                result,
                is_error: false,
            },
        ));
        next += 1;
    }
    events.push(event(
        chat_id,
        run_id,
        next,
        EventKind::RunEnded {
            status: RunStatus::Completed,
            message: None,
        },
    ));
    events
}

/// 将完整轮次按序拼接为同一聊天的历史。
fn conversation(chat_id: &str, turns: usize, text_size: usize) -> Vec<AgentEvent> {
    let mut events = Vec::new();
    for index in 0..turns {
        let run_id = format!("{:x}", index + 1);
        let mut turn = complete_turn(
            chat_id,
            &run_id,
            events.len() as u64 + 1,
            &"用户资料 ".repeat(text_size),
            None,
        );
        events.append(&mut turn);
    }
    events
}

/// 产生上下文模块使用的预算配置；能力未知时仍使用明确的本地上限。
fn budget_parameters(context_limit: u32, max_output_tokens: u32) -> ModelParameters {
    ModelParameters {
        context_limit: Some(context_limit),
        max_output_tokens: Some(max_output_tokens),
        ..Default::default()
    }
}

/// 对覆盖的解析事件前缀统一序列化后计算 SHA-256。
fn source_hash(events: &[AgentEvent], through_sequence: u64) -> String {
    let prefix = events
        .iter()
        .filter(|event| event.sequence <= through_sequence)
        .cloned()
        .collect::<Vec<_>>();
    format!("{:x}", Sha256::digest(serde_json::to_vec(&prefix).unwrap()))
}

/// 由一次压缩请求构造可持久化 checkpoint。
fn checkpoint(chat_id: &str, request: &CompactionRequest, summary: &str) -> ContextCheckpoint {
    ContextCheckpoint {
        version: 1,
        chat_id: chat_id.into(),
        through_sequence: request.through_sequence,
        source_hash: request.source_hash.clone(),
        model_fingerprint: request.model_fingerprint.clone(),
        summary: summary.into(),
        created_at: 100,
    }
}

/// 原结果截断后，后续分页仍保留原始源序号和下一页游标。
#[test]
fn paginated_tool_result_preserves_original_source_and_next_offset() {
    let page = json!({"sequence":5,"text":"\"".repeat(8192),"offset":8192,"nextOffset":16384,"totalBytes":20000,"truncated":true});
    let mut events = complete_turn(
        "c1",
        "a1",
        1,
        "读取资料",
        Some(json!({"text":"a".repeat(20000)})),
    );
    let mut page_events = complete_turn("c1", "a2", 7, "继续读取", Some(page.clone()));
    for event in &mut page_events {
        match &mut event.payload {
            EventKind::ToolResult { name, .. } => *name = "read_tool_result".into(),
            EventKind::ToolCall { call } => call.name = "read_tool_result".into(),
            EventKind::AssistantMessage { calls, wire, .. } => {
                calls[0].name = "read_tool_result".into();
                wire["tool_calls"][0]["function"]["name"] = json!("read_tool_result");
            }
            _ => {}
        }
    }
    events.extend(page_events);
    let assembly = build_context(
        &profile("context-test-model"),
        &budget_parameters(65536, 1024),
        &ModelCapabilities::default(),
        &events,
        &[],
        "系统规则",
        None,
    )
    .unwrap();
    let projected = assembly
        .events
        .iter()
        .rev()
        .find_map(|event| {
            if let EventKind::ToolResult { result, .. } = &event.payload {
                Some(result)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(projected, &page);
    assert_eq!(assembly.preview.truncated_tool_sequences, vec![5]);
}

/// 组装结果估算值等于实际协议请求体 JSON 的 UTF-8 字节数。
#[test]
fn preview_uses_serialized_request_bytes_and_budget_reserves() {
    let model = profile("context-test-model");
    let parameters = budget_parameters(8192, 1024);
    let capabilities = ModelCapabilities::default();
    let events = conversation("c1", 1, 1);
    let tools = [ToolDefinition {
        name: "read_file".into(),
        description: "读取工作区文件".into(),
        parameters: json!({"type":"object","properties":{"path":{"type":"string"}}}),
    }];

    let assembly = build_context(
        &model,
        &parameters,
        &capabilities,
        &events,
        &tools,
        "系统规则",
        None,
    )
    .unwrap();
    let request = build_request_configured(
        &model,
        &assembly.events,
        &tools,
        "系统规则",
        &parameters,
        &capabilities,
    )
    .unwrap();

    assert_eq!(
        assembly.preview.estimated_input_tokens,
        serde_json::to_vec(&request).unwrap().len() as u64
    );
    assert_eq!(assembly.preview.context_limit, 8192);
    assert_eq!(assembly.preview.output_reserve, 1024);
    assert_eq!(assembly.preview.safety_reserve, 1024);
    assert_eq!(assembly.preview.input_budget, 6144);
    assert!(assembly.preview.estimated_input_tokens < u64::from(assembly.preview.input_budget));
    assert!(assembly.compaction.is_none());
}

/// 80% 阈值精确触发压缩；低于阈值不压缩，且无旧前缀时仍可发送未超限上下文。
#[test]
fn context_compacts_at_eighty_percent_and_allows_uncompressible_headroom() {
    let model = profile("context-test-model");
    let parameters = budget_parameters(8192, 1024);
    let capabilities = ModelCapabilities::default();
    let base_events = conversation("c1", 3, 1);
    let base = build_context(
        &model,
        &parameters,
        &capabilities,
        &base_events,
        &[],
        "系统规则",
        None,
    )
    .unwrap();
    let threshold = (u64::from(base.preview.input_budget) * 80).div_ceil(100);
    let padding = usize::try_from(threshold - base.preview.estimated_input_tokens).unwrap();
    let first_user = 1;

    let mut below_events = base_events.clone();
    if let EventKind::UserMessage { text } = &mut below_events[first_user].payload {
        text.push_str(&"x".repeat(padding - 1));
    } else {
        panic!("首轮第二条事件应为用户消息");
    }
    let below = build_context(
        &model,
        &parameters,
        &capabilities,
        &below_events,
        &[],
        "系统规则",
        None,
    )
    .unwrap();
    assert_eq!(below.preview.estimated_input_tokens, threshold - 1);
    assert!(below.compaction.is_none());

    let mut threshold_events = below_events;
    if let EventKind::UserMessage { text } = &mut threshold_events[first_user].payload {
        text.push('x');
    }
    let at_threshold = build_context(
        &model,
        &parameters,
        &capabilities,
        &threshold_events,
        &[],
        "系统规则",
        None,
    )
    .unwrap();
    assert_eq!(at_threshold.preview.estimated_input_tokens, threshold);
    let request = at_threshold.compaction.unwrap();
    assert_eq!(request.through_sequence, 4);
    assert!(matches!(
        &request.events.last().unwrap().payload,
        EventKind::RunEnded { .. }
    ));

    let mut two_turns = conversation("c1", 2, 1);
    let no_prefix_target = threshold + 100;
    let base_two = build_context(
        &model,
        &parameters,
        &capabilities,
        &two_turns,
        &[],
        "系统规则",
        None,
    )
    .unwrap();
    let no_prefix_padding =
        usize::try_from(no_prefix_target - base_two.preview.estimated_input_tokens).unwrap();
    if let EventKind::UserMessage { text } = &mut two_turns[first_user].payload {
        text.push_str(&"x".repeat(no_prefix_padding));
    }
    let no_prefix = build_context(
        &model,
        &parameters,
        &capabilities,
        &two_turns,
        &[],
        "系统规则",
        None,
    )
    .unwrap();
    assert!(no_prefix.preview.estimated_input_tokens > threshold);
    assert!(no_prefix.preview.estimated_input_tokens < u64::from(no_prefix.preview.input_budget));
    assert!(no_prefix.compaction.is_none());
}

/// 压缩前缀只在完整 RunEnded 边界结束，完整工具组不拆分，未结束轮次不被吞并。
#[test]
fn compaction_keeps_tool_pairs_recent_turns_and_unfinished_tail() {
    let model = profile("context-test-model");
    let parameters = budget_parameters(4096, 512);
    let capabilities = ModelCapabilities::default();
    let chat_id = "c1";
    let mut events = Vec::new();
    for index in 0..4 {
        let result = (index == 0 || index == 2).then(|| json!({"text":"工具结果"}));
        let mut turn = complete_turn(
            chat_id,
            &format!("{:x}", index + 1),
            events.len() as u64 + 1,
            &format!("用户目标 {index} {}", "旧资料 ".repeat(600)),
            result,
        );
        events.append(&mut turn);
    }
    let incomplete_run = "e";
    let incomplete_start = events.len() as u64 + 1;
    let pending_call = ToolCall {
        id: "ef".into(),
        name: "read_file".into(),
        arguments: json!({"path":"pending.txt"}),
    };
    events.extend([
        event(
            chat_id,
            incomplete_run,
            incomplete_start,
            EventKind::RunStarted {
                profile: model.clone(),
                permission_mode: PermissionMode::RequestApproval,
            },
        ),
        event(
            chat_id,
            incomplete_run,
            incomplete_start + 1,
            EventKind::UserMessage {
                text: "未结束目标".into(),
            },
        ),
        event(
            chat_id,
            incomplete_run,
            incomplete_start + 2,
            EventKind::ToolCall { call: pending_call },
        ),
    ]);

    let assembly = build_context(
        &model,
        &parameters,
        &capabilities,
        &events,
        &[],
        "系统规则",
        None,
    )
    .unwrap();
    let compact = assembly.compaction.expect("旧的完整轮次应产生一个压缩请求");
    assert!(compact.through_sequence < incomplete_start);
    assert!(matches!(
        &compact.events.last().unwrap().payload,
        EventKind::RunEnded { .. }
    ));
    assert!(compact
        .events
        .iter()
        .any(|event| matches!(&event.payload, EventKind::ToolCall { .. })));
    assert!(compact
        .events
        .iter()
        .any(|event| matches!(&event.payload, EventKind::ToolResult { .. })));
    let call_ids = compact
        .events
        .iter()
        .filter_map(|event| match &event.payload {
            EventKind::ToolCall { call } => Some(call.id.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let result_ids = compact
        .events
        .iter()
        .filter_map(|event| match &event.payload {
            EventKind::ToolResult { call_id, .. } => Some(call_id.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(call_ids, result_ids);
    assert!(assembly.events.iter().any(|event| {
        event.sequence == incomplete_start + 2
            && matches!(&event.payload, EventKind::ToolCall { .. })
    }));
    assert!(assembly.events.iter().any(|event| {
        event.sequence == events[events.len() - 1].sequence
            && matches!(&event.payload, EventKind::ToolCall { .. })
    }));
}

/// 超过输入预算且没有可压缩前缀时明确失败，不丢弃最近两轮。
#[test]
fn context_limit_error_preserves_two_complete_turns() {
    let model = profile("context-test-model");
    let parameters = budget_parameters(4096, 512);
    let capabilities = ModelCapabilities::default();
    let events = conversation("c1", 2, 3000);

    let error = build_context(
        &model,
        &parameters,
        &capabilities,
        &events,
        &[],
        "系统规则",
        None,
    )
    .unwrap_err();
    assert_eq!(error.code, "AGENT_CONTEXT_LIMIT");

    assert!(build_context(
        &model,
        &budget_parameters(2048, 1024),
        &capabilities,
        &events,
        &[],
        "系统规则",
        None,
    )
    .is_err());
}

/// 超大工具结果仅在上下文投影中截断，并保留续读来源与完整本地事件。
#[test]
fn oversized_tool_result_is_utf8_truncated_only_in_projection() {
    let model = profile("context-test-model");
    let parameters = budget_parameters(65536, 1024);
    let capabilities = ModelCapabilities::default();
    let original_result = json!({"text":"界".repeat(5000)});
    let events = complete_turn("c1", "a1", 1, "读取测试结果", Some(original_result.clone()));
    let tool_sequence = events
        .iter()
        .find_map(|event| {
            matches!(&event.payload, EventKind::ToolResult { .. }).then_some(event.sequence)
        })
        .unwrap();

    let assembly = build_context(
        &model,
        &parameters,
        &capabilities,
        &events,
        &[],
        "系统规则",
        None,
    )
    .unwrap();
    let projected = assembly
        .events
        .iter()
        .find_map(|event| match &event.payload {
            EventKind::ToolResult { result, .. } => Some(result),
            _ => None,
        })
        .unwrap();
    assert_eq!(projected["truncated"], true);
    assert_eq!(projected["sourceSequence"], tool_sequence);
    assert!(projected["continuation"]
        .as_str()
        .unwrap()
        .contains("read_tool_result"));
    assert!(projected["text"].as_str().unwrap().as_bytes().len() <= 8 * 1024);
    assert_eq!(
        assembly.preview.truncated_tool_sequences,
        vec![tool_sequence]
    );
    let original = events
        .iter()
        .find_map(|event| match &event.payload {
            EventKind::ToolResult { result, .. } => Some(result),
            _ => None,
        })
        .unwrap();
    assert_eq!(original, &original_result);
    assert!(original["text"].as_str().unwrap().len() > 8 * 1024);
}

/// checkpoint 使用压缩请求给出的模型指纹和解析事件前缀哈希持久化，并以旧资料边界投影。
#[test]
fn checkpoint_roundtrips_and_projects_as_historical_material() {
    let (_directory, store, chat) = test_store();
    for index in 0..4 {
        let run_id = format!("{:x}", index + 1);
        for payload in complete_turn(
            &chat.id,
            &run_id,
            0,
            &format!("历史轮次 {index} {}", "旧消息 ".repeat(700)),
            None,
        ) {
            store.append(&chat.id, &run_id, payload.payload).unwrap();
        }
    }
    let original = store.read_chat(&chat.id).unwrap();
    let model = profile("context-test-model");
    // 两轮保留资料仍须能放入输入预算；四轮总量仍会跨过 80% 压缩阈值。
    let parameters = budget_parameters(24000, 2048);
    let capabilities = ModelCapabilities::default();
    let initial = build_context(
        &model,
        &parameters,
        &capabilities,
        &original.events,
        &[],
        "系统规则",
        None,
    )
    .unwrap();
    let request = initial.compaction.expect("测试历史应超过压缩阈值");
    assert_eq!(
        request.source_hash,
        source_hash(&original.events, request.through_sequence)
    );
    let saved = checkpoint(&chat.id, &request, "历史摘要：项目曾使用旧配置。");
    store.save_context_checkpoint(saved.clone()).unwrap();
    let reopened = AgentStore::open(_directory.path().to_path_buf()).unwrap();
    let restored = reopened.read_context_checkpoint(&chat.id).unwrap().unwrap();
    assert_eq!(restored.chat_id, chat.id);
    assert_eq!(restored.through_sequence, request.through_sequence);
    assert_eq!(restored.source_hash, request.source_hash);
    assert_eq!(restored.model_fingerprint, request.model_fingerprint);
    assert_eq!(restored.summary, saved.summary);

    let assembled = build_context(
        &model,
        &parameters,
        &capabilities,
        &original.events,
        &[],
        "系统规则",
        Some(&restored),
    )
    .unwrap();
    assert_eq!(
        assembled.preview.checkpoint_through,
        Some(request.through_sequence)
    );
    let summary = assembled
        .events
        .iter()
        .find_map(|event| match &event.payload {
            EventKind::UserMessage { text } if text.contains("历史摘要：项目曾使用旧配置") => {
                Some(text)
            }
            _ => None,
        });
    let summary = summary.expect("投影应包含已校验的历史摘要");
    assert!(summary.contains("旧资料"));
    assert!(summary.contains("不构成当前指令"));
    let request = build_request_configured(
        &model,
        &assembled.events,
        &[],
        "系统最高优先规则",
        &parameters,
        &capabilities,
    )
    .unwrap();
    assert_eq!(request["messages"][0]["role"], "system");
    assert_eq!(request["messages"][0]["content"], "系统最高优先规则");
    assert_eq!(
        reopened.read_chat(&chat.id).unwrap().events.len(),
        original.events.len()
    );
}

/// 错误来源哈希、聊天 ID 或模型指纹会使 checkpoint 失效并从原历史重建。
#[test]
fn checkpoint_is_rejected_when_source_chat_or_model_changes() {
    let chat_id = "c1";
    let model = profile("context-test-model");
    let parameters = budget_parameters(4096, 512);
    let capabilities = ModelCapabilities::default();
    let events = conversation(chat_id, 4, 700);
    let first = build_context(
        &model,
        &parameters,
        &capabilities,
        &events,
        &[],
        "系统规则",
        None,
    )
    .unwrap();
    let request = first.compaction.unwrap();
    let valid = checkpoint(chat_id, &request, "历史摘要：可恢复资料");
    let invalid_fingerprints = [
        {
            let mut value = valid.clone();
            value.chat_id = "c2".into();
            value
        },
        {
            let mut value = valid.clone();
            value.source_hash = "0".repeat(64);
            value
        },
        {
            let mut value = valid.clone();
            value.through_sequence -= 1;
            value
        },
        {
            let mut value = valid.clone();
            value.model_fingerprint = "f".repeat(64);
            value
        },
    ];
    for checkpoint in invalid_fingerprints {
        let rebuilt = build_context(
            &model,
            &parameters,
            &capabilities,
            &events,
            &[],
            "系统规则",
            Some(&checkpoint),
        )
        .unwrap();
        assert_eq!(rebuilt.preview.checkpoint_through, None);
        assert!(rebuilt.events.iter().all(|event| !matches!(
            &event.payload,
            EventKind::UserMessage { text } if text.contains("可恢复资料")
        )));
    }

    let changed_profiles = [
        profile("different-model"),
        {
            let mut changed = model.clone();
            changed.base_url = "http://127.0.0.1:9998/v1".into();
            changed
        },
        {
            let mut changed = model.clone();
            changed.protocol = ProviderProtocol::Responses;
            changed
        },
    ];
    for changed in changed_profiles {
        let rebuilt = build_context(
            &changed,
            &parameters,
            &capabilities,
            &events,
            &[],
            "系统规则",
            Some(&valid),
        )
        .unwrap();
        assert_eq!(rebuilt.preview.checkpoint_through, None);
        assert!(rebuilt.events.iter().all(|event| !matches!(
            &event.payload,
            EventKind::UserMessage { text } if text.contains("可恢复资料")
        )));
    }
}

/// checkpoint 存储校验摘要长度、凭据模式和 sourceHash，损坏文件不影响原始历史。
#[test]
fn checkpoint_storage_rejects_invalid_data_and_ignores_corruption() {
    let (_directory, store, chat) = test_store();
    let mut checkpoint_event = None;
    for index in 0..3 {
        let run_id = format!("{:x}", index + 1);
        for payload in complete_turn(
            &chat.id,
            &run_id,
            0,
            &format!("原始历史保持完整，轮次 {index}"),
            None,
        ) {
            let ended = matches!(&payload.payload, EventKind::RunEnded { .. });
            let event = store.append(&chat.id, &run_id, payload.payload).unwrap();
            if index == 0 && ended {
                checkpoint_event = Some(event);
            }
        }
    }
    let event = checkpoint_event.expect("checkpoint 必须绑定完整历史轮次");
    let history = store.read_chat(&chat.id).unwrap().events;
    let model = profile("context-test-model");
    let params = budget_parameters(8192, 1024);
    let assembly = build_context(
        &model,
        &params,
        &ModelCapabilities::default(),
        &history,
        &[],
        "系统规则",
        None,
    )
    .unwrap();
    let fingerprint = assembly
        .compaction
        .as_ref()
        .map(|request| request.model_fingerprint.clone())
        .unwrap_or_else(|| format!("{:x}", Sha256::digest(b"test model fingerprint")));
    let make_checkpoint = |summary: String, source_hash: String| ContextCheckpoint {
        version: 1,
        chat_id: chat.id.clone(),
        through_sequence: event.sequence,
        source_hash,
        model_fingerprint: fingerprint.clone(),
        summary,
        created_at: 200,
    };
    let valid_hash = source_hash(&history, event.sequence);
    let valid = make_checkpoint("历史摘要：保留在本地。".into(), valid_hash.clone());
    store.save_context_checkpoint(valid.clone()).unwrap();
    let stored = _directory
        .path()
        .join("chats")
        .join(&chat.id)
        .join("context-checkpoint.json");
    let bytes = fs::read(&stored).unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("credential_id"));

    assert!(store
        .save_context_checkpoint(make_checkpoint(
            "x".repeat(16 * 1024 + 1),
            valid_hash.clone()
        ))
        .is_err());
    assert!(store
        .save_context_checkpoint(make_checkpoint(
            "疑似凭据 sk-1234567890123456".into(),
            valid_hash.clone(),
        ))
        .is_err());
    assert!(store
        .save_context_checkpoint(make_checkpoint("摘要".into(), "0".repeat(64)))
        .is_err());
    assert_eq!(
        store
            .read_context_checkpoint(&chat.id)
            .unwrap()
            .unwrap()
            .summary,
        valid.summary
    );

    fs::write(&stored, b"{not-json\n").unwrap();
    assert!(store.read_context_checkpoint(&chat.id).unwrap().is_none());
    assert_eq!(
        serde_json::to_value(store.read_chat(&chat.id).unwrap().events).unwrap(),
        serde_json::to_value(&history).unwrap()
    );
}
