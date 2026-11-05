use gitmaster_core::agent::{runtime::*, tools::registry::Toolbox, *};
use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::Arc,
    time::{Duration, Instant},
};

/// 创建只使用临时存储、临时工作区和本地 HTTP 服务的运行夹具。
fn fixture() -> (
    tempfile::TempDir,
    Arc<AgentStore>,
    ProviderProfile,
    TcpListener,
    std::path::PathBuf,
) {
    let directory = tempfile::tempdir().unwrap();
    let workspace = directory.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    std::fs::write(workspace.join("note.txt"), "原始内容\n").unwrap();
    let store = Arc::new(AgentStore::open(directory.path().join("agent")).unwrap());
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let profile = ProviderProfile {
        id: "aa".into(),
        name: "本地回归服务".into(),
        protocol: ProviderProtocol::ChatCompletions,
        base_url: format!("http://{}", listener.local_addr().unwrap()),
        model: "test-model".into(),
        credential_id: "bb".into(),
    };
    store.save_profile(profile.clone()).unwrap();
    (directory, store, profile, listener, workspace)
}

/// 在有界时间内读取完整 HTTP 请求，并以指定状态回送 JSON。
fn respond(listener: &TcpListener, status: u16, value: Value) -> Value {
    respond_with(listener, status, |_| value)
}

/// 先解析请求，再依请求中的实际文件版本构造响应。
fn respond_with(
    listener: &TcpListener,
    status: u16,
    response: impl FnOnce(&Value) -> Value,
) -> Value {
    let started = Instant::now();
    let mut stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock
                    && started.elapsed() < Duration::from_secs(20) =>
            {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("等待本地模型请求失败：{error}"),
        }
    };
    // 显式改为阻塞 I/O 并设置超时，避免 Windows socket 继承非阻塞状态。
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut bytes = Vec::new();
    let mut buffer = [0; 4096];
    let (body_offset, body_length) = loop {
        let count = stream.read(&mut buffer).unwrap();
        assert!(count > 0, "本地模型请求提前关闭");
        bytes.extend_from_slice(&buffer[..count]);
        if let Some(header_end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            let content_length = String::from_utf8_lossy(&bytes[..header_end])
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .and_then(|length| length.trim().parse::<usize>().ok())
                })
                .expect("请求必须有 Content-Length");
            break (header_end + 4, content_length);
        }
    };
    while bytes.len() < body_offset + body_length {
        let count = stream.read(&mut buffer).unwrap();
        assert!(count > 0, "本地模型请求正文提前关闭");
        bytes.extend_from_slice(&buffer[..count]);
    }
    let request: Value =
        serde_json::from_slice(&bytes[body_offset..body_offset + body_length]).unwrap();
    let value = response(&request);
    let body = value.to_string();
    let reason = if status == 200 { "OK" } else { "Unavailable" };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
    request
}

/// 构造无 Git 的严格工作区工具和本地服务运行请求。
fn start_request(chat_id: &str, profile: ProviderProfile, workspace: &std::path::Path) -> StartRun {
    StartRun {
        chat_id: chat_id.into(),
        text: "读取并按要求更新 note.txt".into(),
        profile,
        key: "local-test-key".into(),
        mode: PermissionMode::FullAccess,
        tools: Arc::new(Toolbox::scoped(None, workspace.into(), workspace.into(), vec![]).unwrap()),
        repository_path: Some(workspace.to_string_lossy().into_owned()),
        system_context: String::new(),
        guard: Arc::new(|| Ok(())),
    }
}

/// 等待运行写入持久终态，防止用固定睡眠猜测运行结束。
async fn wait_finished(runtime: &AgentRuntime, chat_id: &str) {
    tokio::time::timeout(Duration::from_secs(30), async {
        while runtime.snapshot(chat_id).unwrap().is_some() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("运行必须在有界时间内结束");
}

/// 成功写文件后模型请求失败，运行终止且重启恢复不重放写操作。
#[tokio::test(flavor = "multi_thread")]
async fn completed_file_write_is_not_replayed_after_followup_model_failure() {
    let (_directory, store, profile, listener, workspace) = fixture();
    let chat = store
        .create_chat(&profile.id, Some(workspace.to_string_lossy().into_owned()))
        .unwrap();
    let server = std::thread::spawn(move || {
        let read_request = respond(
            &listener,
            200,
            json!({"choices":[{"message":{"role":"assistant","tool_calls":[{"id":"read-call","type":"function","function":{"name":"read_file","arguments":"{\"path\":\"note.txt\"}"}}]},"finish_reason":"tool_calls"}]}),
        );
        let write_request = respond_with(&listener, 200, |request| {
            let version = request["messages"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|message| message["role"] == "tool")
                .find_map(|message| {
                    let content: Value = serde_json::from_str(message["content"].as_str()?).ok()?;
                    content["result"]["version"].as_str().map(str::to_owned)
                })
                .expect("read_file 工具结果必须包含实际版本");
            json!({"choices":[{"message":{"role":"assistant","tool_calls":[{"id":"write-call","type":"function","function":{"name":"write_file","arguments":format!("{{\"path\":\"note.txt\",\"content\":\"本轮写入\\n\",\"expectedVersion\":\"{version}\"}}")}}]},"finish_reason":"tool_calls"}]})
        });
        let failed_requests = (0..3)
            .map(|_| {
                respond(
                    &listener,
                    503,
                    json!({"error":{"message":"本地测试服务暂不可用"}}),
                )
            })
            .collect::<Vec<_>>();
        (read_request, write_request, failed_requests)
    });

    let runtime = AgentRuntime::new(store.clone(), Arc::new(|_| {})).unwrap();
    runtime
        .start(start_request(&chat.id, profile, &workspace))
        .unwrap();
    wait_finished(&runtime, &chat.id).await;
    let (read_request, write_request, failed_requests) = server.join().unwrap();
    assert_eq!(read_request["stream"], true);
    assert_eq!(write_request["stream"], true);
    assert_eq!(failed_requests.len(), 3, "本地端点应收到三次有限重试");
    runtime.shutdown().await.unwrap();

    let file = workspace.join("note.txt");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "本轮写入\n");
    let record = store.read_chat(&chat.id).unwrap();
    let write_calls = record
        .events
        .iter()
        .filter(|event| matches!(&event.payload, EventKind::ToolCall { call } if call.name == "write_file"))
        .count();
    let write_results = record
        .events
        .iter()
        .filter(|event| matches!(&event.payload, EventKind::ToolResult { name, is_error: false, .. } if name == "write_file"))
        .count();
    assert_eq!(write_calls, 1);
    assert_eq!(write_results, 1);

    let change = record
        .events
        .iter()
        .find_map(|event| match &event.payload {
            EventKind::FileChanged { change, .. } => Some(change),
            _ => None,
        })
        .expect("成功写入应记录历史差异工件");
    let content = store
        .read_file_change(&chat.id, &change.artifact_id)
        .unwrap();
    assert_eq!(
        content.path,
        file.canonicalize().unwrap().to_string_lossy().into_owned()
    );
    assert_eq!(content.before_content.as_deref(), Some("原始内容\n"));
    assert_eq!(content.after_content, "本轮写入\n");
    assert!(content.verified);
    assert!(matches!(
        record.events.last().unwrap().payload,
        EventKind::RunEnded {
            status: RunStatus::Failed,
            ..
        }
    ));

    // shutdown 结束任务但租约由 Arc 持有，重建恢复器前释放旧实例。
    drop(runtime);
    let recovered = AgentRuntime::new(store.clone(), Arc::new(|_| {})).unwrap();
    assert!(recovered.recovery_warnings().is_empty());
    assert_eq!(std::fs::read_to_string(file).unwrap(), "本轮写入\n");
    let recovered_events = store.read_chat(&chat.id).unwrap().events;
    assert_eq!(
        recovered_events
            .iter()
            .filter(|event| matches!(&event.payload, EventKind::ToolCall { call } if call.name == "write_file"))
            .count(),
        1,
        "恢复只能读取已完成历史，不能重新派发已完成写入"
    );
    recovered.shutdown().await.unwrap();
}
