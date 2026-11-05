use gitmaster_core::agent::provider::{
    build_request, parse_response, Cancellation, ModelClient, ToolDefinition,
};
use gitmaster_core::agent::*;
use serde_json::json;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{Arc, Mutex},
    time::Duration,
};

/// 每种协议使用同一逻辑工具，测试真实请求格式与结果配对。
fn profile(protocol: ProviderProtocol, url: &str) -> ProviderProfile {
    ProviderProfile {
        id: "a1".into(),
        name: "测试".into(),
        protocol,
        base_url: url.into(),
        model: "test-model".into(),
        credential_id: "c1".into(),
    }
}
fn event(payload: EventKind) -> AgentEvent {
    AgentEvent {
        version: 1,
        sequence: 1,
        session_id: "a1".into(),
        run_id: "b1".into(),
        timestamp: 1,
        payload,
    }
}
fn tool() -> ToolDefinition {
    ToolDefinition {
        name: "read_file".into(),
        description: "读取文件".into(),
        parameters: json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"],"additionalProperties":false}),
    }
}

/// 同模型同端点切换凭据身份时，不跨账号回放供应商 opaque 响应。
#[test]
fn different_credential_identity_drops_opaque_wire() {
    let prior = profile(ProviderProtocol::Responses, "https://example.com/v1");
    let mut current = prior.clone();
    current.credential_id = "c2".into();
    let history = vec![
        event(EventKind::RunStarted {
            profile: prior,
            permission_mode: PermissionMode::RequestApproval,
        }),
        event(EventKind::AssistantMessage {
            text: "可移植回答".into(),
            protocol: ProviderProtocol::Responses,
            wire: json!([{"type":"reasoning","encrypted_content":"account-bound","summary":[]}]),
            calls: vec![],
        }),
    ];
    let body = build_request(&current, &history, &[], "系统规则").unwrap();
    assert!(!body.to_string().contains("account-bound"));
    assert!(body.to_string().contains("可移植回答"));
}

#[test]
fn three_protocols_replay_tool_results_in_their_own_wire_format() {
    let cases = [
        (
            ProviderProtocol::ChatCompletions,
            json!({"choices":[{"message":{"role":"assistant","content":null,"tool_calls":[{"id":"call_1","type":"function","function":{"name":"read_file","arguments":"{\"path\":\"README.md\"}"}}]},"finish_reason":"tool_calls"}]}),
        ),
        (
            ProviderProtocol::Responses,
            json!({"status":"completed","output":[{"type":"reasoning","id":"rs_1","encrypted_content":"opaque","summary":[]},{"type":"function_call","id":"fc_1","call_id":"call_1","name":"read_file","arguments":"{\"path\":\"README.md\"}"}]}),
        ),
        (
            ProviderProtocol::AnthropicMessages,
            json!({"role":"assistant","content":[{"type":"tool_use","id":"call_1","name":"read_file","input":{"path":"README.md"}}],"stop_reason":"tool_use"}),
        ),
    ];
    for (protocol, raw) in cases {
        let p = profile(protocol, "https://example.com/v1");
        let reply = parse_response(protocol, raw).unwrap();
        assert_eq!(reply.calls[0].arguments["path"], "README.md");
        let history = vec![
            event(EventKind::UserMessage {
                text: "读取说明".into(),
            }),
            event(EventKind::AssistantMessage {
                text: reply.text,
                protocol,
                wire: reply.wire,
                calls: reply.calls,
            }),
            event(EventKind::ToolResult {
                call_id: "call_1".into(),
                name: "read_file".into(),
                result: json!({"text":"内容"}),
                is_error: false,
            }),
        ];
        let body = build_request(&p, &history, &[tool()], "系统规则").unwrap();
        match protocol {
            ProviderProtocol::ChatCompletions => {
                assert_eq!(body["messages"][3]["tool_call_id"], "call_1");
                assert_eq!(body["tools"][0]["function"]["name"], "read_file");
                assert_eq!(body["store"], false);
            }
            ProviderProtocol::Responses => {
                assert_eq!(body["input"][1]["encrypted_content"], "opaque");
                assert_eq!(body["input"][3]["call_id"], "call_1");
                assert_eq!(body["input"][3]["type"], "function_call_output");
                assert_eq!(body["tools"][0]["name"], "read_file");
                assert_eq!(body["store"], false);
            }
            ProviderProtocol::AnthropicMessages => {
                assert_eq!(body["messages"][2]["content"][0]["tool_use_id"], "call_1");
                assert_eq!(body["messages"][2]["role"], "user");
                assert_eq!(body["tools"][0]["input_schema"]["required"][0], "path");
            }
        }
    }
}

#[test]
fn truncated_and_malformed_tool_responses_never_become_executable_calls() {
    assert!(parse_response(
        ProviderProtocol::ChatCompletions,
        json!({"choices":[{"message":{"content":"部分"},"finish_reason":"length"}]})
    )
    .is_err());
    assert!(parse_response(
        ProviderProtocol::Responses,
        json!({"status":"incomplete","output":[]})
    )
    .is_err());
    assert!(parse_response(
        ProviderProtocol::AnthropicMessages,
        json!({"stop_reason":"max_tokens","content":[]})
    )
    .is_err());
    assert!(parse_response(ProviderProtocol::Responses, json!({"status":"completed","output":[{"type":"function_call","call_id":"x","name":"read_file","arguments":"{"}]})).is_err());
}

/// 用真实回环 HTTP 服务检查请求端点、密钥头与流式分片；不连接真实模型。
fn server(
    body: String,
    status: &str,
    content_type: &str,
) -> (String, std::thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/v1", listener.local_addr().unwrap());
    let status = status.to_owned();
    let content_type = content_type.to_owned();
    let handle = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut request = Vec::new();
        let mut buffer = [0; 4096];
        loop {
            let count = socket.read(&mut buffer).unwrap();
            request.extend_from_slice(&buffer[..count]);
            if let Some(index) = request.windows(4).position(|part| part == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&request[..index]);
                let length: usize = headers
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length: ")
                            .map(str::to_owned)
                    })
                    .unwrap_or_else(|| "0".into())
                    .parse()
                    .unwrap();
                if request.len() >= index + 4 + length {
                    break;
                }
            }
            assert_ne!(count, 0);
        }
        write!(socket, "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
        for part in body.as_bytes().chunks(7) {
            socket.write_all(part).unwrap();
        }
        String::from_utf8(request).unwrap()
    });
    (url, handle)
}

/// 发现模型使用当前草稿端点和分页参数，元数据优先且严格绑定实际请求端点。
#[tokio::test]
async fn model_discovery_preserves_capabilities_and_pagination() {
    let (url, handle) = server(json!({"data":[{"id":"custom-model","display_name":"自定义模型","context_window":64000,"max_output_tokens":8000,"supports_tools":true,"effort":{"supported_levels":["low","high"]}}],"has_more":true,"last_id":"custom-model"}).to_string(), "200 OK", "application/json");
    let profile = profile(ProviderProtocol::AnthropicMessages, &url);
    let result = ModelClient::new()
        .unwrap()
        .list_models(
            &profile,
            "model-test-key",
            "aa".into(),
            Some("previous/model".into()),
            &Cancellation::new(),
        )
        .await
        .unwrap();
    assert_eq!(result.next_cursor.as_deref(), Some("custom-model"));
    assert_eq!(result.models[0].capabilities.context_window, Some(64000));
    assert_eq!(
        result.models[0].capabilities.supported_efforts,
        ["low", "high"]
    );
    assert_eq!(result.models[0].capabilities.source, "discovery");
    assert_eq!(
        result.models[0].capabilities.endpoint.as_deref(),
        Some(profile.endpoint().unwrap().as_str())
    );
    let request = handle.join().unwrap();
    assert!(request.starts_with("GET /v1/models?after_id=previous%2Fmodel "));
    assert!(request
        .to_ascii_lowercase()
        .contains("x-api-key: model-test-key"));
}

/// 目录接口不可用与返回凭据时均不产生可保存模型配置。
#[tokio::test]
async fn unsupported_discovery_and_credential_echo_are_rejected() {
    let (url, handle) = server("unknown".into(), "404 Not Found", "application/json");
    let error = ModelClient::new()
        .unwrap()
        .list_models(
            &profile(ProviderProtocol::Responses, &url),
            "private-discovery-key",
            "aa".into(),
            None,
            &Cancellation::new(),
        )
        .await
        .err()
        .unwrap();
    assert_eq!(error.code, "AGENT_MODEL_LIST_UNSUPPORTED");
    handle.join().unwrap();
    let input = json!({"data":[{"id":"safe","name":"private-discovery-key"}]});
    assert!(provider::discovery::parse_models(
        &profile(ProviderProtocol::Responses, "https://example.com/v1"),
        "private-discovery-key",
        "aa".into(),
        input
    )
    .is_err());
}

#[tokio::test]
async fn fragmented_stream_is_parsed_and_request_contains_no_cloud_session_reference() {
    let body = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"你好\"},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
        "data: [DONE]\n\n"
    );
    let (url, handle) = server(body.into(), "200 OK", "text/event-stream");
    let p = profile(ProviderProtocol::ChatCompletions, &url);
    let chunks = Arc::new(Mutex::new(String::new()));
    let sink = chunks.clone();
    let result = ModelClient::new()
        .unwrap()
        .complete(
            &p,
            "test-secret",
            &[],
            &[],
            "规则",
            &Cancellation::new(),
            &move |text| sink.lock().unwrap().push_str(text),
        )
        .await
        .unwrap();
    assert_eq!(result.text, "你好");
    assert_eq!(*chunks.lock().unwrap(), "你好");
    let request = handle.join().unwrap();
    assert!(request.starts_with("POST /v1/chat/completions "));
    assert!(request
        .to_ascii_lowercase()
        .contains("authorization: bearer test-secret"));
    assert!(!request.contains("previous_response_id"));
}

#[tokio::test]
async fn authentication_errors_never_echo_server_response_or_key() {
    let (url, handle) = server(
        "private-key-should-not-appear".into(),
        "401 Unauthorized",
        "application/json",
    );
    let error = ModelClient::new()
        .unwrap()
        .complete(
            &profile(ProviderProtocol::Responses, &url),
            "secret",
            &[],
            &[],
            "规则",
            &Cancellation::new(),
            &|_| {},
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, "AGENT_AUTH");
    assert!(!serde_json::to_string(&error)
        .unwrap()
        .contains("private-key"));
    handle.join().unwrap();
}

#[tokio::test]
async fn cancellation_stops_a_pending_network_request() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/v1", listener.local_addr().unwrap());
    let cancellation = Cancellation::new();
    let trigger = cancellation.clone();
    let worker = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(30)).await;
        trigger.cancel();
    });
    let result = tokio::time::timeout(
        Duration::from_secs(2),
        ModelClient::new().unwrap().complete(
            &profile(ProviderProtocol::Responses, &url),
            "secret",
            &[],
            &[],
            "规则",
            &cancellation,
            &|_| {},
        ),
    )
    .await
    .unwrap();
    assert_eq!(result.unwrap_err().code, "AGENT_CANCELLED");
    worker.await.unwrap();
}

#[tokio::test]
async fn responses_and_anthropic_streams_produce_complete_tool_arguments() {
    let response = json!({"status":"completed","output":[{"type":"function_call","id":"fc_1","call_id":"call_1","name":"read_file","arguments":"{\"path\":\"README.md\"}"}]});
    let responses = format!(
        "data: {}\n\n",
        json!({"type":"response.completed","response":response})
    );
    let anthropic_events = vec![
        json!({"type":"message_start","message":{"content":[]}}),
        json!({"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"call_1","name":"read_file","input":{}}}),
        json!({"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"path\":"}}),
        json!({"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"\"README.md\"}"}}),
        json!({"type":"content_block_stop","index":0}),
        json!({"type":"message_delta","delta":{"stop_reason":"tool_use"}}),
        json!({"type":"message_stop"}),
    ];
    let anthropic: String = anthropic_events
        .iter()
        .map(|v| format!("data: {v}\n\n"))
        .collect();
    for (protocol, body) in [
        (ProviderProtocol::Responses, responses),
        (ProviderProtocol::AnthropicMessages, anthropic),
    ] {
        let (url, handle) = server(body, "200 OK", "text/event-stream");
        let result = ModelClient::new()
            .unwrap()
            .complete(
                &profile(protocol, &url),
                "test-key",
                &[],
                &[tool()],
                "规则",
                &Cancellation::new(),
                &|_| {},
            )
            .await
            .unwrap();
        assert_eq!(result.calls[0].arguments["path"], "README.md");
        let request = handle.join().unwrap().to_ascii_lowercase();
        if protocol == ProviderProtocol::AnthropicMessages {
            assert!(request.contains("x-api-key: test-key"));
            assert!(request.contains("anthropic-version: 2023-06-01"));
            assert!(!request.contains("authorization:"));
        } else {
            assert!(request.starts_with("post /v1/responses "));
        }
    }
}

#[test]
fn changing_model_does_not_replay_opaque_reasoning_from_the_old_model() {
    let mut old = profile(ProviderProtocol::Responses, "https://example.com/v1");
    old.model = "old-model".into();
    let reply = parse_response(
        ProviderProtocol::Responses,
        json!({"status":"completed","output":[
            {"type":"reasoning","id":"rs_1","encrypted_content":"private-context","summary":[]},
            {"type":"message","role":"assistant","content":[{"type":"output_text","text":"原答复"}]}
        ]}),
    )
    .unwrap();
    let history = vec![
        event(EventKind::RunStarted {
            profile: old,
            permission_mode: PermissionMode::RequestApproval,
        }),
        event(EventKind::AssistantMessage {
            text: reply.text,
            protocol: ProviderProtocol::Responses,
            wire: reply.wire,
            calls: reply.calls,
        }),
    ];
    let body = build_request(
        &profile(ProviderProtocol::Responses, "https://example.com/v1"),
        &history,
        &[],
        "规则",
    )
    .unwrap();
    assert!(!body.to_string().contains("private-context"));
    assert!(body.to_string().contains("原答复"));
}
