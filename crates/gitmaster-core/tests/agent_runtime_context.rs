use gitmaster_core::agent::{provider::ModelAttemptPhase, runtime::*, tools::registry::Toolbox, *};
use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::Arc,
    time::Duration,
};

/// 使用本地临时聊天与 HTTP 服务，不访问真实仓库或供应商。
fn fixture() -> (
    tempfile::TempDir,
    Arc<AgentStore>,
    ProviderProfile,
    TcpListener,
) {
    let directory = tempfile::tempdir().unwrap();
    let store = Arc::new(AgentStore::open(directory.path().join("agent")).unwrap());
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let profile = ProviderProfile {
        id: "aa".into(),
        name: "测试".into(),
        protocol: ProviderProtocol::ChatCompletions,
        base_url: format!("http://{}", listener.local_addr().unwrap()),
        model: "test".into(),
        credential_id: "bb".into(),
    };
    store.save_profile(profile.clone()).unwrap();
    (directory, store, profile, listener)
}

/// 有界等待并读取完整请求，返回指定状态及 JSON，不让失败用例永久挂起。
fn respond(listener: &TcpListener, status: u16, value: Value) -> Value {
    let started = std::time::Instant::now();
    let mut stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock
                    && started.elapsed() < Duration::from_secs(30) =>
            {
                std::thread::sleep(Duration::from_millis(10))
            }
            Err(error) => panic!("等待本地模型请求失败：{error}"),
        }
    };
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let mut bytes = vec![];
    let mut buffer = [0; 4096];
    let (offset, length) = loop {
        let count = stream.read(&mut buffer).unwrap();
        assert!(count > 0);
        bytes.extend_from_slice(&buffer[..count]);
        if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            let length = String::from_utf8_lossy(&bytes[..end])
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|value| value.trim().parse::<usize>().unwrap())
                })
                .unwrap();
            break (end + 4, length);
        }
    };
    while bytes.len() < offset + length {
        let count = stream.read(&mut buffer).unwrap();
        assert!(count > 0);
        bytes.extend_from_slice(&buffer[..count]);
    }
    let input = serde_json::from_slice(&bytes[offset..offset + length]).unwrap();
    let body = value.to_string();
    write!(stream, "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
    input
}

/// 建立无 Git 的只读测试工具环境。
fn request(chat_id: &str, profile: ProviderProfile) -> StartRun {
    StartRun {
        chat_id: chat_id.into(),
        text: "继续任务".into(),
        profile,
        key: "test-key".into(),
        mode: PermissionMode::RequestApproval,
        tools: Arc::new(Toolbox::new(None, vec![]).unwrap()),
        repository_path: None,
        system_context: String::new(),
        guard: Arc::new(|| Ok(())),
    }
}

/// 等待运行实际终态，不通过固定睡眠推断完成。
async fn finished(runtime: &AgentRuntime, id: &str) {
    tokio::time::timeout(Duration::from_secs(30), async {
        while runtime.snapshot(id).unwrap().is_some() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}

/// 重试计为两个独立尝试；失败用量未知，成功用量不会被重复累计。
#[tokio::test(flavor = "multi_thread")]
async fn retry_records_unique_attempts_and_context() {
    let (_directory, store, profile, listener) = fixture();
    let worker = std::thread::spawn(move || {
        respond(&listener, 503, json!({"error":"不透传"}));
        respond(
            &listener,
            200,
            json!({"choices":[{"message":{"role":"assistant","content":"完成"},"finish_reason":"stop"}],"usage":{"prompt_tokens":15,"completion_tokens":5}}),
        )
    });
    let chat = store.create_chat(&profile.id, None).unwrap();
    let runtime = AgentRuntime::new(store.clone(), Arc::new(|_| {})).unwrap();
    let request = request(&chat.id, profile);
    let preview = runtime
        .preview_context(
            &chat.id,
            &request.text,
            &request.tools,
            &request.system_context,
            vec![],
        )
        .unwrap();
    assert!(store.read_chat(&chat.id).unwrap().events.is_empty());
    runtime.start(request).unwrap();
    finished(&runtime, &chat.id).await;
    let input = worker.join().unwrap();
    assert_eq!(input["stream_options"]["include_usage"], true);
    let record = store.read_chat(&chat.id).unwrap();
    let attempts = record
        .events
        .iter()
        .filter_map(|event| {
            if let EventKind::ModelAttempt { attempt, purpose } = &event.payload {
                assert_eq!(purpose, "reply");
                Some(attempt)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(attempts.len(), 4);
    assert_eq!(attempts[0].id, attempts[1].id);
    assert_eq!(attempts[2].id, attempts[3].id);
    assert_ne!(attempts[0].id, attempts[2].id);
    assert_eq!(attempts[1].phase, ModelAttemptPhase::Failed);
    assert!(attempts[1].usage.is_none());
    assert_eq!(attempts[3].usage.as_ref().unwrap().input_tokens, Some(15));
    assert!(record.events.iter().any(|event| matches!(&event.payload, EventKind::ContextPrepared{preview:actual,..} if actual.estimated_input_tokens==preview.preview.estimated_input_tokens)));
    assert!(matches!(
        record.events.last().unwrap().payload,
        EventKind::RunEnded {
            status: RunStatus::Completed,
            ..
        }
    ));
}

/// 安全前缀压缩只产生一次摘要调用，保存 checkpoint 并继续原任务，原始历史不删除。
#[tokio::test(flavor = "multi_thread")]
async fn compaction_preserves_history_and_counts_summary_usage() {
    let (_directory, store, profile, listener) = fixture();
    let chat = store.create_chat(&profile.id, None).unwrap();
    store
        .configure_chat(
            &chat.id,
            &profile.id,
            PermissionMode::RequestApproval,
            Some(ModelParameters {
                context_limit: Some(18000),
                max_output_tokens: Some(1000),
                ..Default::default()
            }),
        )
        .unwrap();
    for run in ["a1", "a2", "a3"] {
        store
            .append(
                &chat.id,
                run,
                EventKind::RunStarted {
                    profile: profile.clone(),
                    permission_mode: PermissionMode::RequestApproval,
                },
            )
            .unwrap();
        store
            .append(
                &chat.id,
                run,
                EventKind::UserMessage {
                    text: "x".repeat(4000),
                },
            )
            .unwrap();
        store
            .append(
                &chat.id,
                run,
                EventKind::AssistantMessage {
                    text: "旧响应".into(),
                    protocol: profile.protocol,
                    wire: json!({"role":"assistant","content":"旧响应"}),
                    calls: vec![],
                },
            )
            .unwrap();
        store
            .append(
                &chat.id,
                run,
                EventKind::RunEnded {
                    status: RunStatus::Completed,
                    message: None,
                },
            )
            .unwrap();
    }
    let before = store.read_chat(&chat.id).unwrap().events;
    let worker = std::thread::spawn(move || {
        let first = respond(
            &listener,
            200,
            json!({"choices":[{"message":{"role":"assistant","content":"旧任务参考摘要"},"finish_reason":"stop"}],"usage":{"prompt_tokens":40,"completion_tokens":4}}),
        );
        assert!(first.get("tools").is_none());
        respond(
            &listener,
            200,
            json!({"choices":[{"message":{"role":"assistant","content":"继续完成"},"finish_reason":"stop"}],"usage":{"prompt_tokens":80,"completion_tokens":8}}),
        )
    });
    let runtime = AgentRuntime::new(store.clone(), Arc::new(|_| {})).unwrap();
    runtime.start(request(&chat.id, profile)).unwrap();
    finished(&runtime, &chat.id).await;
    let final_request = worker.join().unwrap();
    assert!(final_request.to_string().contains("旧任务参考摘要"));
    let after = store.read_chat(&chat.id).unwrap().events;
    assert_eq!(
        serde_json::to_value(&before).unwrap(),
        serde_json::to_value(&after[..before.len()]).unwrap()
    );
    assert_eq!(
        store
            .read_context_checkpoint(&chat.id)
            .unwrap()
            .unwrap()
            .through_sequence,
        4
    );
    assert!(after.iter().any(|event|matches!(&event.payload,EventKind::ModelAttempt{purpose,attempt} if purpose=="compaction" && attempt.phase==ModelAttemptPhase::Completed)));
    assert!(matches!(
        after.last().unwrap().payload,
        EventKind::RunEnded {
            status: RunStatus::Completed,
            ..
        }
    ));
}

/// 模型只能写聊天中的建议卡；未点击保存时记忆库保持为空。
#[tokio::test(flavor = "multi_thread")]
async fn memory_suggestion_is_not_automatically_persisted() {
    let (_directory, store, profile, listener) = fixture();
    let chat = store.create_chat(&profile.id, None).unwrap();
    let worker = std::thread::spawn(move || {
        respond(
            &listener,
            200,
            json!({"choices":[{"message":{"role":"assistant","tool_calls":[{"id":"aa","type":"function","function":{"name":"suggest_memory","arguments":json!({"text":"项目使用中文文档","tags":["文档"]}).to_string()}}]},"finish_reason":"tool_calls"}]}),
        );
        respond(
            &listener,
            200,
            json!({"choices":[{"message":{"role":"assistant","content":"等待保存建议"},"finish_reason":"stop"}]}),
        )
    });
    let runtime = AgentRuntime::new(store.clone(), Arc::new(|_| {})).unwrap();
    runtime.start(request(&chat.id, profile)).unwrap();
    finished(&runtime, &chat.id).await;
    let input = worker.join().unwrap();
    assert!(input["messages"]
        .as_array()
        .unwrap()
        .iter()
        .any(|message| message["role"] == "tool"
            && message["content"]
                .as_str()
                .unwrap()
                .contains("\"saved\":false")));
    assert!(store
        .list_memories(&chat.workspace_id, true)
        .unwrap()
        .is_empty());
    assert!(store
        .read_chat(&chat.id)
        .unwrap()
        .events
        .iter()
        .any(|event| matches!(event.payload, EventKind::MemorySuggested { .. })));
}

/// 用户停止在途请求后，已记账的开始必须对应一个未知用量终态。
#[tokio::test(flavor = "multi_thread")]
async fn cancelled_attempt_is_finalized_once() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let (_directory, store, profile, _listener) = fixture();
    let chat = store.create_chat(&profile.id, None).unwrap();
    let started = Arc::new(AtomicBool::new(false));
    let signal = started.clone();
    let runtime = AgentRuntime::new(
        store.clone(),
        Arc::new(move |notice| {
            if matches!(
                notice,
                RunNotice::Record {
                    event: AgentEvent {
                        payload: EventKind::ModelAttempt {
                            attempt: provider::ModelAttempt {
                                phase: ModelAttemptPhase::Started,
                                ..
                            },
                            ..
                        },
                        ..
                    }
                }
            ) {
                signal.store(true, Ordering::Release);
            }
        }),
    )
    .unwrap();
    runtime.start(request(&chat.id, profile)).unwrap();
    tokio::time::timeout(Duration::from_secs(30), async {
        while !started.load(Ordering::Acquire) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    runtime.shutdown().await.unwrap();
    let record = store.read_chat(&chat.id).unwrap();
    let attempts = record
        .events
        .iter()
        .filter_map(|event| {
            if let EventKind::ModelAttempt { attempt, .. } = &event.payload {
                Some(attempt)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(attempts.len(), 2);
    assert_eq!(attempts[0].id, attempts[1].id);
    assert_eq!(attempts[1].phase, ModelAttemptPhase::Failed);
    assert!(attempts[1].usage.is_none());
    assert!(matches!(
        record.events.last().unwrap().payload,
        EventKind::RunEnded {
            status: RunStatus::Cancelled,
            ..
        }
    ));
}

/// 已知不支持工具的模型在发请求前明确拒绝，不靠供应商报错消耗额度。
#[tokio::test(flavor = "multi_thread")]
async fn known_tool_incompatible_model_is_rejected_before_request() {
    let (directory, store, profile, listener) = fixture();
    let chat = store.create_chat(&profile.id, None).unwrap();
    let path = directory.path().join("agent/settings.json");
    let mut document: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    document["models"][0]["capabilities"]["source"] = json!("discovery");
    document["models"][0]["capabilities"]["endpoint"] = json!(profile.endpoint().unwrap());
    document["models"][0]["capabilities"]["toolSupport"] = json!(false);
    std::fs::write(path, serde_json::to_vec(&document).unwrap()).unwrap();
    let runtime = AgentRuntime::new(store.clone(), Arc::new(|_| {})).unwrap();
    let error = runtime
        .start(request(&chat.id, profile.clone()))
        .unwrap_err();
    assert_eq!(error.code, "AGENT_MODEL_TOOLS_UNSUPPORTED");
    assert!(store.read_chat(&chat.id).unwrap().events.is_empty());
    assert!(listener.accept().is_err());
    let request = request(&chat.id, profile);
    assert_eq!(
        runtime
            .preview_context(&chat.id, &request.text, &request.tools, "", vec![])
            .err()
            .unwrap()
            .code,
        "AGENT_MODEL_TOOLS_UNSUPPORTED"
    );
}

/// 摘要结果已返回但用户同时停止时，不再保存 checkpoint 或启动主回复。
#[tokio::test(flavor = "multi_thread")]
async fn cancellation_after_compaction_reply_prevents_checkpoint_write() {
    use std::sync::{Mutex, Weak};
    let (_directory, store, profile, listener) = fixture();
    let chat = store.create_chat(&profile.id, None).unwrap();
    store
        .configure_chat(
            &chat.id,
            &profile.id,
            PermissionMode::RequestApproval,
            Some(ModelParameters {
                context_limit: Some(18000),
                max_output_tokens: Some(1000),
                ..Default::default()
            }),
        )
        .unwrap();
    for run in ["a1", "a2", "a3"] {
        store
            .append(
                &chat.id,
                run,
                EventKind::RunStarted {
                    profile: profile.clone(),
                    permission_mode: PermissionMode::RequestApproval,
                },
            )
            .unwrap();
        store
            .append(
                &chat.id,
                run,
                EventKind::UserMessage {
                    text: "x".repeat(4000),
                },
            )
            .unwrap();
        store
            .append(
                &chat.id,
                run,
                EventKind::AssistantMessage {
                    text: "旧响应".into(),
                    protocol: profile.protocol,
                    wire: json!({"role":"assistant","content":"旧响应"}),
                    calls: vec![],
                },
            )
            .unwrap();
        store
            .append(
                &chat.id,
                run,
                EventKind::RunEnded {
                    status: RunStatus::Completed,
                    message: None,
                },
            )
            .unwrap();
    }
    let owner = Arc::new(Mutex::new(Weak::<AgentRuntime>::new()));
    let callback = owner.clone();
    let runtime = AgentRuntime::new(
        store.clone(),
        Arc::new(move |notice| {
            if let RunNotice::Record {
                event:
                    AgentEvent {
                        session_id,
                        run_id,
                        payload: EventKind::ModelAttempt { purpose, attempt },
                        ..
                    },
            } = notice
            {
                if purpose == "compaction" && attempt.phase == ModelAttemptPhase::Completed {
                    callback
                        .lock()
                        .unwrap()
                        .upgrade()
                        .unwrap()
                        .cancel(&session_id, &run_id)
                        .unwrap();
                }
            }
        }),
    )
    .unwrap();
    *owner.lock().unwrap() = Arc::downgrade(&runtime);
    let worker = std::thread::spawn(move || {
        respond(
            &listener,
            200,
            json!({"choices":[{"message":{"role":"assistant","content":"停止后不保存的摘要"},"finish_reason":"stop"}]}),
        )
    });
    runtime.start(request(&chat.id, profile)).unwrap();
    finished(&runtime, &chat.id).await;
    worker.join().unwrap();
    assert!(store.read_context_checkpoint(&chat.id).unwrap().is_none());
    let record = store.read_chat(&chat.id).unwrap();
    assert!(!record
        .events
        .iter()
        .any(|event| matches!(event.payload, EventKind::ContextPrepared { .. })));
    assert!(matches!(
        record.events.last().unwrap().payload,
        EventKind::RunEnded {
            status: RunStatus::Cancelled,
            ..
        }
    ));
}
