use gitmaster_core::agent::{provider::Cancellation, *};
use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    thread,
    time::{Duration, Instant},
};

const API_KEY: &str = "probe-test-key-never-echo";

/// 为本地探针创建合法草稿配置，不连接真实供应商。
fn profile(protocol: ProviderProtocol, base_url: String) -> ProviderProfile {
    ProviderProfile {
        id: "aa".into(),
        name: "本地探针测试".into(),
        protocol,
        base_url,
        model: "probe-model".into(),
        credential_id: "bb".into(),
    }
}

/// 读取有长度上限与超时的单个 HTTP 请求。
fn read_request(stream: &mut TcpStream) -> String {
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 4096];
    loop {
        let count = stream.read(&mut chunk).unwrap();
        assert_ne!(count, 0, "mock 服务在收到完整请求前断开");
        bytes.extend_from_slice(&chunk[..count]);
        if let Some(header_end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&bytes[..header_end]);
            let body_len = headers
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .and_then(|value| value.trim().parse::<usize>().ok())
                })
                .expect("POST JSON 请求应包含 Content-Length");
            if bytes.len() >= header_end + 4 + body_len {
                return String::from_utf8(bytes).unwrap();
            }
        }
        assert!(bytes.len() <= 1024 * 1024, "mock 请求超过测试上限");
    }
}

/// 启动只监听回环地址的短时 HTTP mock，接受 socket 显式恢复阻塞模式。
fn mock_server(
    replies: Vec<(u16, String)>,
    max_requests: usize,
    quiet_after_request: Duration,
) -> (String, thread::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let base_url = format!("http://{}/v1", listener.local_addr().unwrap());
    let worker = thread::spawn(move || {
        let mut requests = Vec::new();
        let initial_deadline = Instant::now() + Duration::from_secs(5);
        let mut last_request = None;
        while requests.len() < max_requests {
            if last_request.is_some_and(|at: Instant| at.elapsed() >= quiet_after_request) {
                break;
            }
            if last_request.is_none() && Instant::now() >= initial_deadline {
                break;
            }
            match listener.accept() {
                Ok((mut stream, _)) => {
                    stream.set_nonblocking(false).unwrap();
                    let request = read_request(&mut stream);
                    let index = requests.len();
                    let (status, body) = replies.get(index).cloned().unwrap_or((500, "{}".into()));
                    let reason = match status {
                        200 => "OK",
                        401 => "Unauthorized",
                        _ => "Mock",
                    };
                    write!(
                        stream,
                        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .unwrap();
                    stream.flush().unwrap();
                    requests.push(request);
                    last_request = Some(Instant::now());
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("mock accept 失败：{error}"),
            }
        }
        requests
    });
    (base_url, worker)
}

/// 构造不同协议格式的模拟探针调用。
fn tool_call(protocol: ProviderProtocol, name: &str, arguments: Value, complete: bool) -> String {
    let raw = match protocol {
        ProviderProtocol::ChatCompletions => json!({
            "choices": [{
                "message": {"role":"assistant","content":null,"tool_calls":[{
                    "id":"call-probe","type":"function","function":{
                        "name":name,"arguments":arguments.to_string()
                    }
                }]},
                "finish_reason": if complete { "tool_calls" } else { "length" }
            }]
        }),
        ProviderProtocol::Responses => json!({
            "status": if complete { "completed" } else { "incomplete" },
            "output": [{
                "type":"function_call","id":"fc-probe","call_id":"call-probe",
                "name":name,"arguments":arguments.to_string()
            }]
        }),
        ProviderProtocol::AnthropicMessages => json!({
            "role":"assistant",
            "content":[{"type":"tool_use","id":"call-probe","name":name,"input":arguments}],
            "stop_reason": if complete { "tool_use" } else { "max_tokens" }
        }),
    };
    raw.to_string()
}

/// 构造不同协议格式的普通文本终态响应。
fn final_text(protocol: ProviderProtocol) -> String {
    match protocol {
        ProviderProtocol::ChatCompletions => json!({
            "choices":[{"message":{"role":"assistant","content":"连接成功"},"finish_reason":"stop"}]
        }),
        ProviderProtocol::Responses => json!({
            "status":"completed","output":[{"type":"message","role":"assistant","content":[
                {"type":"output_text","text":"连接成功"}
            ]}]
        }),
        ProviderProtocol::AnthropicMessages => json!({
            "role":"assistant","content":[{"type":"text","text":"连接成功"}],"stop_reason":"end_turn"
        }),
    }
    .to_string()
}

/// 检查实际请求头与 JSON 请求体，同时确认密钥不会进入模型请求内容。
fn assert_probe_request(request: &str, protocol: ProviderProtocol) {
    let (headers, body) = request.split_once("\r\n\r\n").unwrap();
    let header_text = headers.to_ascii_lowercase();
    let body: Value = serde_json::from_str(body).unwrap();
    match protocol {
        ProviderProtocol::ChatCompletions => {
            assert!(headers.starts_with("POST /v1/chat/completions "));
            assert!(header_text.contains(&format!("authorization: bearer {API_KEY}")));
            assert_eq!(body["tools"][0]["function"]["name"], "connection_probe");
        }
        ProviderProtocol::Responses => {
            assert!(headers.starts_with("POST /v1/responses "));
            assert!(header_text.contains(&format!("authorization: bearer {API_KEY}")));
            assert_eq!(body["tools"][0]["name"], "connection_probe");
        }
        ProviderProtocol::AnthropicMessages => {
            assert!(headers.starts_with("POST /v1/messages "));
            assert!(header_text.contains(&format!("x-api-key: {API_KEY}")));
            assert!(header_text.contains("anthropic-version: 2023-06-01"));
            assert_eq!(body["tools"][0]["name"], "connection_probe");
        }
    }
    assert!(!body.to_string().contains(API_KEY));
    assert!(body.to_string().contains("请调用 connection_probe"));
}

/// 三种协议均完成真实两阶段草稿探针，且仅在模型端验证模拟工具格式。
#[tokio::test]
async fn draft_probe_sends_two_protocol_requests_without_executing_local_tools() {
    for protocol in [
        ProviderProtocol::ChatCompletions,
        ProviderProtocol::Responses,
        ProviderProtocol::AnthropicMessages,
    ] {
        let (base_url, server) = mock_server(
            vec![
                (
                    200,
                    tool_call(protocol, "connection_probe", json!({}), true),
                ),
                (200, final_text(protocol)),
            ],
            3,
            Duration::from_millis(80),
        );
        let profile = profile(protocol, base_url);
        let report = gitmaster_core::agent::provider::ModelClient::new()
            .unwrap()
            .test_model(
                &profile,
                API_KEY,
                &ModelParameters::default(),
                &ModelCapabilities::default(),
                "cc".into(),
                &Cancellation::new(),
            )
            .await
            .unwrap();
        let requests = server.join().unwrap();
        assert_eq!(requests.len(), 2, "协议 {protocol:?}");
        assert_probe_request(&requests[0], protocol);
        let (_, final_body) = requests[1].split_once("\r\n\r\n").unwrap();
        assert!(final_body.contains("模拟工具调用") || final_body.contains("连接成功"));
        assert_eq!(
            report
                .stages
                .iter()
                .find(|stage| stage.id == "tools")
                .unwrap()
                .status,
            "passed"
        );
        assert!(!serde_json::to_string(&report).unwrap().contains(API_KEY));
    }
}

/// 错误工具名或参数不会被草稿探针当作可用工具协议。
#[tokio::test]
async fn draft_probe_rejects_wrong_tool_names_and_arguments() {
    for (name, arguments) in [
        ("other_tool", json!({})),
        ("connection_probe", json!({"path":"README.md"})),
    ] {
        let protocol = ProviderProtocol::Responses;
        let (base_url, server) = mock_server(
            vec![(200, tool_call(protocol, name, arguments, true))],
            2,
            Duration::from_millis(80),
        );
        let result = gitmaster_core::agent::provider::ModelClient::new()
            .unwrap()
            .test_model(
                &profile(protocol, base_url),
                API_KEY,
                &ModelParameters::default(),
                &ModelCapabilities::default(),
                "dd".into(),
                &Cancellation::new(),
            )
            .await
            .unwrap();
        assert_eq!(
            result
                .stages
                .iter()
                .find(|stage| stage.id == "tools")
                .unwrap()
                .status,
            "unsupported"
        );
        assert_eq!(server.join().unwrap().len(), 1);
    }
}

/// 三协议不完整工具响应均返回通用协议错误且不发出第二阶段请求。
#[tokio::test]
async fn draft_probe_rejects_incomplete_tool_responses() {
    for protocol in [
        ProviderProtocol::ChatCompletions,
        ProviderProtocol::Responses,
        ProviderProtocol::AnthropicMessages,
    ] {
        let (base_url, server) = mock_server(
            vec![(
                200,
                tool_call(protocol, "connection_probe", json!({}), false),
            )],
            2,
            Duration::from_millis(80),
        );
        let error = gitmaster_core::agent::provider::ModelClient::new()
            .unwrap()
            .test_model(
                &profile(protocol, base_url),
                API_KEY,
                &ModelParameters::default(),
                &ModelCapabilities::default(),
                "ee".into(),
                &Cancellation::new(),
            )
            .await
            .err()
            .expect("不完整工具响应应返回协议错误");
        assert_eq!(error.code, "AGENT_PROTOCOL", "协议 {protocol:?}");
        assert!(!serde_json::to_string(&error).unwrap().contains(API_KEY));
        assert_eq!(server.join().unwrap().len(), 1);
    }
}

/// 401 不重试，错误信息不包含 API Key 或供应商响应正文。
#[tokio::test]
async fn authentication_failure_does_not_retry_or_echo_sensitive_values() {
    let secret_response = "mock-provider-secret-response";
    let (base_url, server) = mock_server(
        vec![(401, json!({"error":secret_response}).to_string())],
        2,
        Duration::from_millis(400),
    );
    let error = gitmaster_core::agent::provider::ModelClient::new()
        .unwrap()
        .test_model(
            &profile(ProviderProtocol::Responses, base_url),
            API_KEY,
            &ModelParameters::default(),
            &ModelCapabilities::default(),
            "ff".into(),
            &Cancellation::new(),
        )
        .await
        .err()
        .expect("401 鉴权失败应返回错误");
    assert_eq!(error.code, "AGENT_AUTH");
    let serialized = serde_json::to_string(&error).unwrap();
    assert!(!serialized.contains(API_KEY));
    assert!(!serialized.contains(secret_response));
    assert_eq!(server.join().unwrap().len(), 1);
}

/// 取消草稿探针会停止请求且不修改传入配置快照。
#[tokio::test]
async fn cancelling_draft_probe_does_not_change_configuration_snapshot() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let base_url = format!("http://{}/v1", listener.local_addr().unwrap());
    let (accepted_tx, accepted_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let server = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(3);
        let (mut stream, _) = loop {
            match listener.accept() {
                Ok(accepted) => break accepted,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < deadline, "mock 未在时限内收到探针请求");
                    thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("mock accept 失败：{error}"),
            }
        };
        stream.set_nonblocking(false).unwrap();
        let request = read_request(&mut stream);
        accepted_tx.send(()).unwrap();
        release_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("测试取消后未释放 mock socket");
        request
    });
    let profile = profile(ProviderProtocol::Responses, base_url);
    let original = profile.clone();
    let cancellation = Cancellation::new();
    let request_cancellation = cancellation.clone();
    let request_profile = profile.clone();
    let request = tokio::spawn(async move {
        gitmaster_core::agent::provider::ModelClient::new()
            .unwrap()
            .test_model(
                &request_profile,
                API_KEY,
                &ModelParameters::default(),
                &ModelCapabilities::default(),
                "11".into(),
                &request_cancellation,
            )
            .await
    });
    tokio::time::timeout(Duration::from_secs(3), accepted_rx)
        .await
        .expect("mock 未在时限内确认收到请求")
        .expect("mock 请求确认通道意外关闭");
    cancellation.cancel();
    let error = tokio::time::timeout(Duration::from_secs(2), request)
        .await
        .expect("取消后探针未及时结束")
        .unwrap()
        .err()
        .expect("取消后草稿测试应返回取消错误");
    release_tx.send(()).unwrap();
    assert_eq!(error.code, "AGENT_CANCELLED");
    assert_eq!(profile.id, original.id);
    assert_eq!(profile.model, original.model);
    assert_eq!(profile.base_url, original.base_url);
    let captured = server.join().unwrap();
    assert!(captured.contains("POST /v1/responses"));
}
