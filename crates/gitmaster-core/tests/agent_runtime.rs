use gitmaster_core::agent::{runtime::*, tools::registry::Toolbox, *};
use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{Arc, Mutex},
    time::Duration,
};

/// 关闭覆盖所有聊天，并在终态落盘后返回；进入关闭阶段后拒绝新运行。
#[tokio::test(flavor = "multi_thread")]
async fn shutdown_drains_all_chats_and_rejects_new_runs() {
    let (_dir, store, profile, _server) = fixture(ProviderProtocol::ChatCompletions);
    let runtime = AgentRuntime::new(store.clone(), Arc::new(|_| {})).unwrap();
    let mut ids = vec![];
    for _ in 0..2 {
        let chat = store.create_chat(&profile.id, None).unwrap();
        runtime
            .start(StartRun {
                chat_id: chat.id.clone(),
                text: "等待模型".into(),
                profile: profile.clone(),
                key: "test-key".into(),
                mode: PermissionMode::FullAccess,
                tools: Arc::new(Toolbox::new(None, vec![]).unwrap()),
                repository_path: None,
                system_context: String::new(),
                guard: Arc::new(|| Ok(())),
            })
            .unwrap();
        ids.push(chat.id);
    }
    tokio::time::timeout(Duration::from_secs(10), runtime.shutdown())
        .await
        .unwrap()
        .unwrap();
    for id in &ids {
        assert!(runtime.snapshot(id).unwrap().is_none());
        assert!(matches!(
            store.read_chat(id).unwrap().events.last().unwrap().payload,
            EventKind::RunEnded {
                status: RunStatus::Cancelled,
                ..
            }
        ));
    }
    let error = runtime
        .start(StartRun {
            chat_id: ids[0].clone(),
            text: "不能重启".into(),
            profile,
            key: "test-key".into(),
            mode: PermissionMode::FullAccess,
            tools: Arc::new(Toolbox::new(None, vec![]).unwrap()),
            repository_path: None,
            system_context: String::new(),
            guard: Arc::new(|| Ok(())),
        })
        .unwrap_err();
    assert_eq!(error.code, "AGENT_SHUTTING_DOWN");
    runtime.shutdown().await.unwrap();
}

/// 真实 Git 提交已完成但结果通知被延迟时，关闭必须继续等待收尾。
#[tokio::test(flavor = "multi_thread")]
async fn shutdown_waits_for_git_write_result_delivery() {
    enable_git_timing_diagnostics();
    use gitmaster_core::agent::tools::git::GitContext;
    use gitmaster_core::git::{
        coordinator::RepositoryCoordinator, environment::resolve_git, repository::open_repository,
    };
    use std::sync::mpsc;
    let (dir, store, profile, server) = fixture(ProviderProtocol::ChatCompletions);
    let project = dir.path().join("project");
    std::fs::create_dir(&project).unwrap();
    // 所有写入只在测试仓库完成，真实 Git 结果通知通过通道模拟缓慢收尾。
    let git = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .current_dir(&project)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    };
    git(&["init", "-b", "main"]);
    git(&["config", "user.name", "测试"]);
    git(&["config", "user.email", "test@example.invalid"]);
    std::fs::write(project.join("note.txt"), "真实 Git 写入").unwrap();
    git(&["add", "note.txt"]);
    let executable = resolve_git(None).unwrap();
    let (repository, _) = open_repository(&executable, &project).unwrap();
    let tools = Arc::new(
        Toolbox::new(
            Some(GitContext {
                git: executable,
                repository,
                coordinator: RepositoryCoordinator::new(),
            }),
            vec![],
        )
        .unwrap(),
    );
    let worker = std::thread::spawn(move || {
        respond(
            &server,
            json!({"choices":[{"message":{"role":"assistant","tool_calls":[{"id":"write","type":"function","function":{"name":"git_commit","arguments":"{\"message\":\"受控收尾\"}"}}]},"finish_reason":"tool_calls"}]}),
        );
    });
    let (entered, waiting) = mpsc::channel();
    let (release, held) = mpsc::channel();
    let held = Mutex::new(held);
    let runtime = AgentRuntime::new(store.clone(), Arc::new(move |notice| {
        if matches!(notice, RunNotice::Record {event:AgentEvent {payload:EventKind::ToolResult {ref name,..},..}} if name=="git_commit") {
            entered.send(()).unwrap();
            held.lock().unwrap().recv_timeout(Duration::from_secs(30)).unwrap();
        }
    })).unwrap();
    let path = Some(project.to_string_lossy().into_owned());
    let chat = store.create_chat(&profile.id, path.clone()).unwrap();
    runtime
        .start(StartRun {
            chat_id: chat.id.clone(),
            text: "提交已暂存文件".into(),
            profile,
            key: "test-key".into(),
            mode: PermissionMode::FullAccess,
            tools,
            repository_path: path,
            system_context: String::new(),
            guard: Arc::new(|| Ok(())),
        })
        .unwrap();
    // 保持原有等待预算，失败时区分工具尚未结束与通知未送达。
    let arrived =
        tokio::task::spawn_blocking(move || waiting.recv_timeout(Duration::from_secs(120)))
            .await
            .unwrap();
    assert!(
        arrived.is_ok(),
        "Git 结果通知未到达：等待={arrived:?}，运行快照={:?}，末条事件={:?}",
        runtime
            .snapshot(&chat.id)
            .map(|snapshot| snapshot.map(|state| state.phase)),
        store.read_chat(&chat.id).unwrap().events.last()
    );
    let owner = runtime.clone();
    let shutdown = tokio::spawn(async move { owner.shutdown().await });
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!shutdown.is_finished(), "Git 写结果仍在收尾时不能退出");
    release.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(10), shutdown)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    worker.join().unwrap();
    assert_eq!(git(&["log", "-1", "--format=%s"]), "受控收尾");
    let record = store.read_chat(&chat.id).unwrap();
    let has_successful_tool_result = record.events.iter().any(|event| {
        matches!(
            event.payload,
            EventKind::ToolResult {
                is_error: false,
                ..
            }
        )
    });
    assert!(
        has_successful_tool_result,
        "未记录成功工具结果，持久事件为：{:#?}",
        record.events
    );
    assert!(matches!(
        record.events.last().unwrap().payload,
        EventKind::RunEnded {
            status: RunStatus::Cancelled,
            ..
        }
    ));
}

/// 专项诊断复用核心脱敏阶段日志，普通回归不输出额外信息。
fn enable_git_timing_diagnostics() {
    if std::env::var_os("GITMASTER_TEST_TIMINGS").is_none() {
        return;
    }
    struct TimingLogger;
    impl log::Log for TimingLogger {
        /// 只接受已有阶段日志，不输出 HTTP 请求或环境变量。
        fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
            metadata.target() == "gitmaster::diagnostic"
        }
        /// 给每条阶段日志附加测试启动后的相对时间。
        fn log(&self, record: &log::Record<'_>) {
            if self.enabled(record.metadata()) {
                static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
                eprintln!(
                    "t={} {}",
                    START
                        .get_or_init(std::time::Instant::now)
                        .elapsed()
                        .as_millis(),
                    record.args()
                );
            }
        }
        /// 标准错误直接输出，不持有额外缓冲。
        fn flush(&self) {}
    }
    static LOGGER: TimingLogger = TimingLogger;
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(log::LevelFilter::Trace);
}

/// 构造独立磁盘与真实回环 HTTP 服务，不接触真实仓库和服务密钥。
fn fixture(
    protocol: ProviderProtocol,
) -> (
    tempfile::TempDir,
    Arc<AgentStore>,
    ProviderProfile,
    TcpListener,
) {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(AgentStore::open(dir.path().join("agent")).unwrap());
    let server = TcpListener::bind("127.0.0.1:0").unwrap();
    let profile = ProviderProfile {
        id: "a1".into(),
        name: "测试".into(),
        protocol,
        base_url: format!("http://{}", server.local_addr().unwrap()),
        model: "test".into(),
        credential_id: "b1".into(),
    };
    store.save_profile(profile.clone()).unwrap();
    (dir, store, profile, server)
}
/// 读取 HTTP JSON 正文并返回完整 JSON 响应。
fn respond(server: &TcpListener, value: Value) -> Value {
    respond_body(server, "application/json", &value.to_string())
}
/// 用相同真实 HTTP 夹具返回 JSON 或 SSE，保持完整请求读取。
fn respond_body(server: &TcpListener, content_type: &str, data: &str) -> Value {
    let (mut stream, _) = server.accept().unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let mut bytes = vec![];
    let mut buf = [0; 4096];
    let (offset, len) = loop {
        let n = stream.read(&mut buf).unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&buf[..n]);
        if let Some(offset) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&bytes[..offset]);
            let len = head
                .lines()
                .find_map(|line| {
                    line.to_lowercase()
                        .strip_prefix("content-length:")
                        .map(|value| value.trim().parse::<usize>().unwrap())
                })
                .unwrap();
            break (offset + 4, len);
        }
    };
    while bytes.len() < offset + len {
        let n = stream.read(&mut buf).unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&buf[..n]);
    }
    let body = serde_json::from_slice(&bytes[offset..offset + len]).unwrap();
    write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",data.len(),data).unwrap();
    body
}

/// 大量细小流片不会逐片广播全文，完成事件仍保存全部内容。
#[tokio::test(flavor = "multi_thread")]
async fn tiny_stream_deltas_are_batched_and_final_text_is_complete() {
    let (_dir, store, profile, server) = fixture(ProviderProtocol::ChatCompletions);
    let mut body = String::new();
    for _ in 0..1200 {
        body.push_str(&format!(
            "data: {}\n\n",
            json!({"choices":[{"delta":{"content":"片"}}]})
        ));
    }
    body.push_str(&format!(
        "data: {}\n\ndata: [DONE]\n\n",
        json!({"choices":[{"delta":{},"finish_reason":"stop"}]})
    ));
    let worker = std::thread::spawn(move || {
        respond_body(&server, "text/event-stream", &body);
    });
    let notices = Arc::new(Mutex::new(vec![]));
    let target = notices.clone();
    let runtime = AgentRuntime::new(
        store.clone(),
        Arc::new(move |notice| target.lock().unwrap().push(notice)),
    )
    .unwrap();
    let chat = store.create_chat(&profile.id, None).unwrap();
    runtime
        .start(StartRun {
            chat_id: chat.id.clone(),
            text: "输出长文".into(),
            profile,
            key: "test-key".into(),
            mode: PermissionMode::FullAccess,
            tools: Arc::new(Toolbox::new(None, vec![]).unwrap()),
            repository_path: None,
            system_context: String::new(),
            guard: Arc::new(|| Ok(())),
        })
        .unwrap();
    tokio::time::timeout(Duration::from_secs(20), async {
        while runtime.snapshot(&chat.id).unwrap().is_some() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    worker.join().unwrap();
    let count = notices
        .lock()
        .unwrap()
        .iter()
        .filter(|notice| matches!(notice, RunNotice::Progress {state,..} if !state.text.is_empty()))
        .count();
    assert!(count < 30, "1200 个细片产生了 {count} 次全文通知");
    assert!(store.read_chat(&chat.id).unwrap().events.iter().any(|event| matches!(&event.payload, EventKind::AssistantMessage {text,..} if text == &"片".repeat(1200))));
}
/// 凭据即使拆成不同片段，也不能进入预览通知或最终历史。
#[tokio::test(flavor = "multi_thread")]
async fn split_credentials_never_reach_progress_or_history() {
    let (_dir, store, profile, server) = fixture(ProviderProtocol::ChatCompletions);
    let body = ["test-", "key"]
        .iter()
        .map(|part| {
            format!(
                "data: {}\n\n",
                json!({"choices":[{"delta":{"content":part}}]})
            )
        })
        .collect::<String>()
        + &format!(
            "data: {}\n\ndata: [DONE]\n\n",
            json!({"choices":[{"delta":{},"finish_reason":"stop"}]})
        );
    let worker = std::thread::spawn(move || {
        respond_body(&server, "text/event-stream", &body);
    });
    let notices = Arc::new(Mutex::new(vec![]));
    let received = notices.clone();
    let runtime = AgentRuntime::new(
        store.clone(),
        Arc::new(move |notice| received.lock().unwrap().push(notice)),
    )
    .unwrap();
    let chat = store.create_chat(&profile.id, None).unwrap();
    runtime
        .start(StartRun {
            chat_id: chat.id.clone(),
            text: "输出内容".into(),
            profile,
            key: "test-key".into(),
            mode: PermissionMode::FullAccess,
            tools: Arc::new(Toolbox::new(None, vec![]).unwrap()),
            repository_path: None,
            system_context: String::new(),
            guard: Arc::new(|| Ok(())),
        })
        .unwrap();
    tokio::time::timeout(Duration::from_secs(20), async {
        while runtime.snapshot(&chat.id).unwrap().is_some() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    worker.join().unwrap();
    assert!(!serde_json::to_string(&*notices.lock().unwrap())
        .unwrap()
        .contains("test-key"));
    let record = store.read_chat(&chat.id).unwrap();
    assert!(!serde_json::to_string(&record).unwrap().contains("test-key"));
    assert!(matches!(
        record.events.last().unwrap().payload,
        EventKind::RunEnded {
            status: RunStatus::Failed,
            ..
        }
    ));
}

/// 生成同一逻辑工具的各协议回复。
fn response(protocol: ProviderProtocol, tool: bool, path: &str) -> Value {
    match (protocol, tool) {
        (ProviderProtocol::ChatCompletions, true) => {
            json!({"choices":[{"message":{"role":"assistant","content":null,"tool_calls":[{"id":"call_1","type":"function","function":{"name":"read_file","arguments":json!({"path":path}).to_string()}}]},"finish_reason":"tool_calls"}]})
        }
        (ProviderProtocol::ChatCompletions, false) => {
            json!({"choices":[{"message":{"role":"assistant","content":"读取完成"},"finish_reason":"stop"}]})
        }
        (ProviderProtocol::Responses, true) => {
            json!({"status":"completed","output":[{"type":"function_call","call_id":"call_1","name":"read_file","arguments":json!({"path":path}).to_string()}]})
        }
        (ProviderProtocol::Responses, false) => {
            json!({"status":"completed","output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":"读取完成"}]}]})
        }
        (ProviderProtocol::AnthropicMessages, true) => {
            json!({"content":[{"type":"tool_use","id":"call_1","name":"read_file","input":{"path":path}}],"stop_reason":"tool_use"})
        }
        (ProviderProtocol::AnthropicMessages, false) => {
            json!({"content":[{"type":"text","text":"读取完成"}],"stop_reason":"end_turn"})
        }
    }
}
/// 三协议真正经历模型→工具→批准→结果→模型，而非只验证请求模板。
#[tokio::test(flavor = "multi_thread")]
async fn three_protocol_roundtrips_wait_for_one_shot_approval() {
    for protocol in [
        ProviderProtocol::ChatCompletions,
        ProviderProtocol::Responses,
        ProviderProtocol::AnthropicMessages,
    ] {
        let (dir, store, profile, server) = fixture(protocol);
        let file = dir.path().join("README.md");
        std::fs::write(&file, "可核验的本地内容").unwrap();
        let path = file.to_str().unwrap().to_owned();
        let worker = std::thread::spawn(move || {
            respond(&server, response(protocol, true, &path));
            let body = respond(&server, response(protocol, false, ""));
            assert!(body.to_string().contains("可核验的本地内容"));
        });
        let notices = Arc::new(Mutex::new(vec![]));
        let target = notices.clone();
        let runtime = AgentRuntime::new(
            store.clone(),
            Arc::new(move |notice| target.lock().unwrap().push(notice)),
        )
        .unwrap();
        let chat = store.create_chat(&profile.id, None).unwrap();
        let run = runtime
            .start(StartRun {
                chat_id: chat.id.clone(),
                text: "读取说明".into(),
                profile,
                key: "test-key".into(),
                mode: PermissionMode::RequestApproval,
                tools: Arc::new(Toolbox::new(None, vec![dir.path().join("agent")]).unwrap()),
                repository_path: None,
                system_context: String::new(),
                guard: Arc::new(|| Ok(())),
            })
            .unwrap();
        let approval = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                if let Some(state) = runtime.snapshot(&chat.id).unwrap() {
                    if let Some(value) = state.approval {
                        break value;
                    }
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert!(runtime
            .resolve(&chat.id, "wrong", &approval.id, true)
            .is_err());
        runtime.resolve(&chat.id, &run, &approval.id, true).unwrap();
        assert!(runtime.resolve(&chat.id, &run, &approval.id, true).is_err());
        tokio::time::timeout(Duration::from_secs(10), async {
            while runtime.snapshot(&chat.id).unwrap().is_some() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        worker.join().unwrap();
        let record = store.read_chat(&chat.id).unwrap();
        assert!(matches!(
            record.events.last().unwrap().payload,
            EventKind::RunEnded {
                status: RunStatus::Completed,
                ..
            }
        ));
        assert_eq!(
            record
                .events
                .iter()
                .filter(|event| matches!(event.payload, EventKind::ToolResult { .. }))
                .count(),
            1
        );
        assert!(!std::fs::read_to_string(
            dir.path()
                .join("agent/chats")
                .join(&chat.id)
                .join("events.jsonl")
        )
        .unwrap()
        .contains("test-key"));
    }
}
/// 重启补齐缺失结果并标记中断；绝不重新执行已经发出的写调用。
#[test]
fn recovery_pairs_abandoned_calls_without_replaying_them() {
    let (_dir, store, profile, _server) = fixture(ProviderProtocol::ChatCompletions);
    let chat = store.create_chat(&profile.id, None).unwrap();
    store
        .append(
            &chat.id,
            "ab",
            EventKind::RunStarted {
                profile,
                permission_mode: PermissionMode::FullAccess,
            },
        )
        .unwrap();
    store
        .append(
            &chat.id,
            "ab",
            EventKind::AssistantMessage {
                text: String::new(),
                protocol: ProviderProtocol::ChatCompletions,
                wire: json!({}),
                calls: vec![ToolCall {
                    id: "call_1".into(),
                    name: "write_file".into(),
                    arguments: json!({"path":"never"}),
                }],
            },
        )
        .unwrap();
    let runtime = AgentRuntime::new(store.clone(), Arc::new(|_| {})).unwrap();
    assert!(runtime.recovery_warnings().is_empty());
    let record = store.read_chat(&chat.id).unwrap();
    assert!(matches!(
        record.events.last().unwrap().payload,
        EventKind::RunEnded {
            status: RunStatus::Interrupted,
            ..
        }
    ));
    assert!(record
        .events
        .iter()
        .any(|event| matches!(event.payload, EventKind::ToolResult { is_error: true, .. })));
    drop(runtime);
    let _again = AgentRuntime::new(store.clone(), Arc::new(|_| {})).unwrap();
    assert_eq!(
        store.read_chat(&chat.id).unwrap().events.len(),
        record.events.len()
    );
}

/// 拒绝不读取文件；取消等待批准后没有第二个模型请求且配对完整。
#[tokio::test(flavor = "multi_thread")]
async fn rejection_and_cancellation_never_execute_pending_tool() {
    for (cancelled, closing) in [(false, false), (true, false), (true, true)] {
        let (dir, store, profile, server) = fixture(ProviderProtocol::ChatCompletions);
        let path = dir
            .path()
            .join("missing.txt")
            .to_string_lossy()
            .into_owned();
        let worker = std::thread::spawn(move || {
            respond(
                &server,
                response(ProviderProtocol::ChatCompletions, true, &path),
            );
            if !cancelled {
                let body = respond(
                    &server,
                    response(ProviderProtocol::ChatCompletions, false, ""),
                );
                assert!(body.to_string().contains("AGENT_APPROVAL_DENIED"));
            }
        });
        let runtime = AgentRuntime::new(store.clone(), Arc::new(|_| {})).unwrap();
        let chat = store.create_chat(&profile.id, None).unwrap();
        let id = runtime
            .start(StartRun {
                chat_id: chat.id.clone(),
                text: "读取文件".into(),
                profile,
                key: "test-key".into(),
                mode: PermissionMode::RequestApproval,
                tools: Arc::new(Toolbox::new(None, vec![]).unwrap()),
                repository_path: None,
                system_context: String::new(),
                guard: Arc::new(|| Ok(())),
            })
            .unwrap();
        let approval = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                if let Some(state) = runtime.snapshot(&chat.id).unwrap() {
                    if let Some(value) = state.approval {
                        break value;
                    }
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap_or_else(|error| {
            let snapshot = runtime
                .snapshot(&chat.id)
                .ok()
                .flatten()
                .and_then(|value| serde_json::to_value(value).ok());
            let last_event = store.read_chat(&chat.id).ok().and_then(|record| {
                record
                    .events
                    .last()
                    .map(|event| format!("{:?}", event.payload))
            });
            panic!("等待审批超时：{error}; snapshot={snapshot:#?}; last_event={last_event:#?}");
        });
        if cancelled {
            if closing {
                runtime.shutdown().await.unwrap();
            } else {
                runtime.cancel(&chat.id, &id).unwrap();
            }
        } else {
            runtime.resolve(&chat.id, &id, &approval.id, false).unwrap();
        }
        tokio::time::timeout(Duration::from_secs(10), async {
            while runtime.snapshot(&chat.id).unwrap().is_some() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert!(runtime.resolve(&chat.id, &id, &approval.id, true).is_err());
        worker.join().unwrap();
        let events = store.read_chat(&chat.id).unwrap().events;
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(
                    event.payload,
                    EventKind::ToolResult { is_error: true, .. }
                ))
                .count(),
            1
        );
        assert!(
            matches!(events.last().unwrap().payload,EventKind::RunEnded{status,..} if status==if cancelled{RunStatus::Cancelled}else{RunStatus::Completed})
        );
    }
}

/// 连续工具调用不会无限循环，20 次模型请求后仍为最后调用保留结果。
#[tokio::test(flavor = "multi_thread")]
async fn twenty_request_budget_stops_tool_loop() {
    let (_dir, store, profile, server) = fixture(ProviderProtocol::ChatCompletions);
    let worker = std::thread::spawn(move || {
        for index in 0..20 {
            respond(
                &server,
                json!({"choices":[{"message":{"role":"assistant","tool_calls":[{"id":format!("call_{index}"),"type":"function","function":{"name":"unknown_tool","arguments":"{}"}}]},"finish_reason":"tool_calls"}]}),
            );
        }
    });
    let runtime = AgentRuntime::new(store.clone(), Arc::new(|_| {})).unwrap();
    let chat = store.create_chat(&profile.id, None).unwrap();
    runtime
        .start(StartRun {
            chat_id: chat.id.clone(),
            text: "测试预算".into(),
            profile,
            key: "test-key".into(),
            mode: PermissionMode::FullAccess,
            tools: Arc::new(Toolbox::new(None, vec![]).unwrap()),
            repository_path: None,
            system_context: String::new(),
            guard: Arc::new(|| Ok(())),
        })
        .unwrap();
    // Windows 同步落盘实测超过 15 秒；预算仍按实际 20 次请求与终态断言。
    tokio::time::timeout(Duration::from_secs(120), async {
        while runtime.snapshot(&chat.id).unwrap().is_some() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    worker.join().unwrap();
    let record = store.read_chat(&chat.id).unwrap();
    assert_eq!(
        record
            .events
            .iter()
            .filter(|event| matches!(event.payload, EventKind::ToolResult { .. }))
            .count(),
        20
    );
    assert!(
        matches!(&record.events.last().unwrap().payload,EventKind::RunEnded{status:RunStatus::Failed,message:Some(message)} if message.contains("20"))
    );
}

/// 同一数据目录只允许一个恢复控制器，第二实例不能中断第一实例。
#[test]
fn runtime_lease_prevents_concurrent_recovery() {
    let (_dir, store, _profile, _server) = fixture(ProviderProtocol::ChatCompletions);
    let first = AgentRuntime::new(store.clone(), Arc::new(|_| {})).unwrap();
    assert!(AgentRuntime::new(store.clone(), Arc::new(|_| {})).is_err());
    drop(first);
    assert!(AgentRuntime::new(store, Arc::new(|_| {})).is_ok());
}

/// 总结工具实际执行专用模型生成，并把指定提交证据和完整五节报告持久化。
#[tokio::test(flavor = "multi_thread")]
async fn development_summary_executes_generation_with_committed_evidence() {
    use gitmaster_core::agent::tools::git::GitContext;
    use gitmaster_core::git::{
        coordinator::RepositoryCoordinator, environment::resolve_git, repository::open_repository,
    };
    let (dir, store, profile, server) = fixture(ProviderProtocol::ChatCompletions);
    let project = dir.path().join("project");
    std::fs::create_dir(&project).unwrap();
    let git = |args: &[&str]| {
        let result = std::process::Command::new("git")
            .current_dir(&project)
            .args(args)
            .output()
            .unwrap();
        assert!(result.status.success());
        String::from_utf8(result.stdout).unwrap().trim().to_owned()
    };
    git(&["init", "-b", "main"]);
    git(&["config", "user.name", "测试"]);
    git(&["config", "user.email", "test@example.invalid"]);
    std::fs::write(project.join("README.md"), "指定提交中的项目说明\n").unwrap();
    git(&["add", "README.md"]);
    git(&["commit", "-m", "初始功能"]);
    let oid = git(&["rev-parse", "HEAD"]);
    std::fs::write(
        project.join("README.md"),
        "尚未提交的内容不能成为报告证据\n",
    )
    .unwrap();
    let executable = resolve_git(None).unwrap();
    let (repository, _) = open_repository(&executable, &project).unwrap();
    let repository_id = repository.id.clone();
    let tools = Arc::new(
        Toolbox::new(
            Some(GitContext {
                git: executable,
                repository,
                coordinator: RepositoryCoordinator::new(),
            }),
            vec![dir.path().join("agent")],
        )
        .unwrap(),
    );
    let worker = std::thread::spawn(move || {
        respond(
            &server,
            json!({"choices":[{"message":{"role":"assistant","tool_calls":[{"id":"summary_1","type":"function","function":{"name":"git_development_summary","arguments":json!({"repositoryId":repository_id,"commitIds":[oid]}).to_string()}}]},"finish_reason":"tool_calls"}]}),
        );
        let report="## 整体做了什么功能\n建立项目说明。\n## 设计思路\n以提交对象为依据。\n## 核心代码流程\n本次只有 README，无可执行代码。\n## 重要问题点\n未运行测试。\n## 防御与兜底措施\n不包含工作区未提交改动。";
        let summary = respond(
            &server,
            json!({"choices":[{"message":{"role":"assistant","content":report},"finish_reason":"stop"}]}),
        );
        assert!(summary.to_string().contains("指定提交中的项目说明"));
        assert!(!summary
            .to_string()
            .contains("尚未提交的内容不能成为报告证据"));
        assert!(summary.get("tools").is_none() || summary["tools"].as_array().unwrap().is_empty());
        let resumed = respond(
            &server,
            json!({"choices":[{"message":{"role":"assistant","content":report},"finish_reason":"stop"}]}),
        );
        assert!(resumed.to_string().contains("提交范围"));
    });
    let runtime = AgentRuntime::new(store.clone(), Arc::new(|_| {})).unwrap();
    let path = Some(project.to_string_lossy().into_owned());
    let chat = store.create_chat(&profile.id, path.clone()).unwrap();
    runtime
        .start(StartRun {
            chat_id: chat.id.clone(),
            text: "生成指定提交的开发总结".into(),
            profile,
            key: "test-key".into(),
            mode: PermissionMode::RequestApproval,
            tools,
            repository_path: path,
            system_context: String::new(),
            guard: Arc::new(|| Ok(())),
        })
        .unwrap();
    tokio::time::timeout(Duration::from_secs(20), async {
        while runtime.snapshot(&chat.id).unwrap().is_some() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    worker.join().unwrap();
    let events = store.read_chat(&chat.id).unwrap().events;
    assert!(events
        .iter()
        .any(|event| matches!(event.payload, EventKind::SummaryEvidence { .. })));
    assert!(events.iter().any(|event|matches!(&event.payload,EventKind::ToolResult{result,is_error:false,..} if result["report"].as_str().unwrap().contains("防御与兜底措施"))));
    assert!(matches!(
        events.last().unwrap().payload,
        EventKind::RunEnded {
            status: RunStatus::Completed,
            ..
        }
    ));
}
