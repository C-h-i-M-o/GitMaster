use gitmaster_core::agent::{
    provider::{
        parse_response, Cancellation, ModelAttempt, ModelAttemptPhase, ModelClient, ModelUsage,
    },
    AgentError, AgentEvent, AgentResult, ModelParameters, ProviderProfile, ProviderProtocol,
};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// 为协议解析测试提供本地模型配置。
fn profile(protocol: ProviderProtocol, base_url: &str) -> ProviderProfile {
    ProviderProfile {
        id: "aa".into(),
        name: "本地 usage 测试".into(),
        protocol,
        base_url: base_url.into(),
        model: "usage-test-model".into(),
        credential_id: "cc".into(),
    }
}

/// 构造 Chat Completions 普通 JSON 响应并附上 usage。
fn chat_response(usage: Value) -> Value {
    json!({
        "choices": [{"message": {"role": "assistant", "content": "完成"}, "finish_reason": "stop"}],
        "usage": usage
    })
}

/// 构造 Responses 普通 JSON 响应并附上 usage。
fn responses_response(usage: Value) -> Value {
    json!({
        "status": "completed",
        "output": [{"type": "message", "content": [{"type": "output_text", "text": "完成"}]}],
        "usage": usage
    })
}

/// 构造 Anthropic Messages 普通 JSON 响应并附上 usage。
fn anthropic_response(usage: Value) -> Value {
    json!({"content": [{"type": "text", "text": "完成"}], "stop_reason": "end_turn", "usage": usage})
}

/// 三协议映射输入、输出、缓存和推理子计数，不重复累加子集。
#[test]
fn protocol_usage_maps_totals_and_subsets_without_double_counting() {
    let chat = parse_response(
        ProviderProtocol::ChatCompletions,
        chat_response(json!({
            "prompt_tokens": 100,
            "completion_tokens": 30,
            "prompt_tokens_details": {"cached_tokens": 40},
            "completion_tokens_details": {"reasoning_tokens": 12}
        })),
    )
    .unwrap()
    .usage;
    assert_eq!(chat.input_tokens, Some(100));
    assert_eq!(chat.output_tokens, Some(30));
    assert_eq!(chat.cached_input_tokens, Some(40));
    assert_eq!(chat.cache_creation_input_tokens, None);
    assert_eq!(chat.reasoning_tokens, Some(12));

    let deepseek = parse_response(
        ProviderProtocol::ChatCompletions,
        chat_response(json!({
            "prompt_tokens": 80,
            "completion_tokens": 20,
            "prompt_cache_hit_tokens": 25
        })),
    )
    .unwrap()
    .usage;
    assert_eq!(deepseek.input_tokens, Some(80));
    assert_eq!(deepseek.cached_input_tokens, Some(25));

    let responses = parse_response(
        ProviderProtocol::Responses,
        responses_response(json!({
            "input_tokens": 90,
            "output_tokens": 15,
            "input_tokens_details": {"cached_tokens": 35},
            "output_tokens_details": {"reasoning_tokens": 8}
        })),
    )
    .unwrap()
    .usage;
    assert_eq!(responses.input_tokens, Some(90));
    assert_eq!(responses.output_tokens, Some(15));
    assert_eq!(responses.cached_input_tokens, Some(35));
    assert_eq!(responses.reasoning_tokens, Some(8));

    let anthropic = parse_response(
        ProviderProtocol::AnthropicMessages,
        anthropic_response(json!({
            "input_tokens": 50,
            "cache_read_input_tokens": 12,
            "cache_creation_input_tokens": 7,
            "output_tokens": 18
        })),
    )
    .unwrap()
    .usage;
    assert_eq!(anthropic.input_tokens, Some(69));
    assert_eq!(anthropic.cached_input_tokens, Some(12));
    assert_eq!(anthropic.cache_creation_input_tokens, Some(7));
    assert_eq!(anthropic.output_tokens, Some(18));
}

/// 缺省用量字段保持未知，不以 0 冒充供应商回报。
#[test]
fn missing_usage_fields_remain_none() {
    let reply =
        parse_response(ProviderProtocol::Responses, responses_response(Value::Null)).unwrap();
    assert_eq!(reply.usage, ModelUsage::default());

    let partial_anthropic = parse_response(
        ProviderProtocol::AnthropicMessages,
        anthropic_response(json!({"input_tokens": 9, "output_tokens": 3})),
    )
    .unwrap()
    .usage;
    assert_eq!(partial_anthropic.input_tokens, Some(9));
    assert_eq!(partial_anthropic.output_tokens, Some(3));
    assert_eq!(partial_anthropic.cached_input_tokens, None);
    assert_eq!(partial_anthropic.cache_creation_input_tokens, None);
    assert_eq!(partial_anthropic.reasoning_tokens, None);
}

/// 一条本机 HTTP 服务按序返回 JSON/SSE，并统计实际请求次数。
async fn serve(replies: Vec<HttpReply>) -> (String, Arc<AtomicUsize>, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let count = Arc::new(AtomicUsize::new(0));
    let served = count.clone();
    let worker = tokio::spawn(async move {
        for reply in replies {
            let (mut stream, _) = listener.accept().await.unwrap();
            read_request(&mut stream).await;
            served.fetch_add(1, Ordering::SeqCst);
            let reason = match reply.status {
                200 => "OK",
                400 => "Bad Request",
                429 => "Too Many Requests",
                500 => "Internal Server Error",
                502 => "Bad Gateway",
                503 => "Service Unavailable",
                504 => "Gateway Timeout",
                529 => "Too Many Requests",
                _ => "Error",
            };
            let retry_after = reply
                .retry_after
                .as_ref()
                .map(|value| format!("Retry-After: {value}\r\n"))
                .unwrap_or_default();
            let response = format!(
                "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n{}\r\n{}",
                reply.status,
                reason,
                reply.content_type,
                reply.body.len(),
                retry_after,
                reply.body
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });
    (format!("http://{address}/v1"), count, worker)
}

/// 模拟建立请求后没有收到响应头的连接错误，再提供成功响应。
async fn drop_first_then_serve(
    success: HttpReply,
) -> (String, Arc<AtomicUsize>, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let count = Arc::new(AtomicUsize::new(0));
    let served = count.clone();
    let worker = tokio::spawn(async move {
        let (mut first, _) = listener.accept().await.unwrap();
        read_request(&mut first).await;
        served.fetch_add(1, Ordering::SeqCst);
        drop(first);

        let (mut second, _) = listener.accept().await.unwrap();
        read_request(&mut second).await;
        served.fetch_add(1, Ordering::SeqCst);
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            success.content_type,
            success.body.len(),
            success.body
        );
        second.write_all(response.as_bytes()).await.unwrap();
    });
    (format!("http://{address}/v1"), count, worker)
}

/// 本地 HTTP 测试响应，不含真实凭据或远端数据。
struct HttpReply {
    status: u16,
    content_type: &'static str,
    body: String,
    retry_after: Option<String>,
}

impl HttpReply {
    /// 构造指定状态的 JSON 响应。
    fn json(status: u16, body: Value) -> Self {
        Self {
            status,
            content_type: "application/json",
            body: body.to_string(),
            retry_after: None,
        }
    }

    /// 构造 SSE 响应。
    fn sse(body: String) -> Self {
        Self {
            status: 200,
            content_type: "text/event-stream",
            body,
            retry_after: None,
        }
    }

    /// 构造语法错误的 JSON 响应，用于确认已开始读取正文后不重试。
    fn raw_json(body: &str) -> Self {
        Self {
            status: 200,
            content_type: "application/json",
            body: body.into(),
            retry_after: None,
        }
    }

    /// 构造带有限退避提示的 HTTP 错误。
    fn retry(status: u16, after: &str) -> Self {
        Self {
            status,
            content_type: "application/json",
            body: "{}".into(),
            retry_after: Some(after.into()),
        }
    }
}

/// 读取完整 HTTP 请求头和 Content-Length 正文。
async fn read_request(stream: &mut tokio::net::TcpStream) {
    let mut bytes = Vec::new();
    let mut chunk = [0; 4096];
    let (body_start, body_len) = loop {
        let count = stream.read(&mut chunk).await.unwrap();
        assert!(count > 0);
        bytes.extend_from_slice(&chunk[..count]);
        if let Some(offset) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&bytes[..offset]);
            let body_len = headers
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .and_then(|value| value.trim().parse::<usize>().ok())
                })
                .unwrap_or(0);
            break (offset + 4, body_len);
        }
    };
    while bytes.len() < body_start + body_len {
        let count = stream.read(&mut chunk).await.unwrap();
        assert!(count > 0);
        bytes.extend_from_slice(&chunk[..count]);
    }
}

/// 通过 ModelClient 调用本机服务并回传 attempt 观察器。
async fn complete(
    protocol: ProviderProtocol,
    base_url: &str,
    cancellation: &Cancellation,
    on_attempt: &(dyn Fn(ModelAttempt) -> AgentResult<()> + Send + Sync),
    delta: &(dyn Fn(&str) + Send + Sync),
) -> AgentResult<gitmaster_core::agent::provider::ModelReply> {
    let profile = profile(protocol, base_url);
    ModelClient::new()?
        .complete_with_attempts(
            &profile,
            "local-test-key",
            &[] as &[AgentEvent],
            &[],
            "本地测试系统提示",
            &ModelParameters::default(),
            &gitmaster_core::agent::provider::catalog::capabilities(&profile),
            cancellation,
            delta,
            on_attempt,
        )
        .await
}

/// 所有允许重试的状态在响应正文前重试，且每次尝试只有一个终态。
#[tokio::test(flavor = "multi_thread")]
async fn retryable_status_retries_before_body_and_records_attempts() {
    for status in [429, 500, 502, 503, 504, 529] {
        let (base_url, count, server) = serve(vec![
            HttpReply::retry(status, "0"),
            HttpReply::json(
                200,
                chat_response(json!({"prompt_tokens": 9, "completion_tokens": 4})),
            ),
        ])
        .await;
        let attempts = Arc::new(Mutex::new(Vec::<ModelAttempt>::new()));
        let received = attempts.clone();
        let callback = move |attempt| {
            received.lock().unwrap().push(attempt);
            Ok(())
        };
        let reply = complete(
            ProviderProtocol::ChatCompletions,
            &base_url,
            &Cancellation::new(),
            &callback,
            &|_| {},
        )
        .await
        .unwrap();
        server.await.unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 2, "status {status}");
        assert_eq!(reply.usage.input_tokens, Some(9));

        let attempts = attempts.lock().unwrap();
        assert_eq!(attempts.len(), 4, "status {status}");
        assert_eq!(attempts[0].phase, ModelAttemptPhase::Started);
        assert_eq!(attempts[1].phase, ModelAttemptPhase::Failed);
        assert_eq!(attempts[2].phase, ModelAttemptPhase::Started);
        assert_eq!(attempts[3].phase, ModelAttemptPhase::Completed);
        assert_eq!(attempts[0].attempt, 1);
        assert_eq!(attempts[2].attempt, 2);
        assert_ne!(attempts[0].id, attempts[2].id);
        assert!(attempts[1].usage.is_none());
        assert!(attempts[3].usage.is_some());
        assert!(attempts
            .iter()
            .all(|attempt| !format!("{attempt:?}").contains("local-test-key")));
    }
}

/// 超过重试次数后以最后错误退出，且总发送数不超过三次。
#[tokio::test(flavor = "multi_thread")]
async fn retry_count_is_limited_to_three_total_attempts() {
    let (base_url, count, server) = serve(vec![
        HttpReply::retry(503, "0"),
        HttpReply::retry(503, "0"),
        HttpReply::retry(503, "0"),
        HttpReply::json(200, chat_response(Value::Null)),
    ])
    .await;
    let attempts = Arc::new(Mutex::new(Vec::<ModelAttempt>::new()));
    let received = attempts.clone();
    let callback = move |attempt| {
        received.lock().unwrap().push(attempt);
        Ok(())
    };
    let result = complete(
        ProviderProtocol::ChatCompletions,
        &base_url,
        &Cancellation::new(),
        &callback,
        &|_| {},
    )
    .await;
    server.abort();
    assert_eq!(result.unwrap_err().code, "AGENT_HTTP");
    assert_eq!(count.load(Ordering::SeqCst), 3);
    let attempts = attempts.lock().unwrap();
    assert_eq!(attempts.len(), 6);
    assert_eq!(attempts[4].attempt, 3);
    assert_eq!(attempts[5].phase, ModelAttemptPhase::Failed);
}

/// HTTP send 阶段的网络断连可重试，成功后返回最终 usage。
#[tokio::test(flavor = "multi_thread")]
async fn network_error_before_response_headers_can_retry() {
    let (base_url, count, server) = drop_first_then_serve(HttpReply::json(
        200,
        chat_response(json!({"prompt_tokens": 6, "completion_tokens": 2})),
    ))
    .await;
    let attempts = Arc::new(Mutex::new(Vec::<ModelAttempt>::new()));
    let received = attempts.clone();
    let callback = move |attempt| {
        received.lock().unwrap().push(attempt);
        Ok(())
    };
    let reply = complete(
        ProviderProtocol::ChatCompletions,
        &base_url,
        &Cancellation::new(),
        &callback,
        &|_| {},
    )
    .await
    .unwrap();
    server.await.unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 2);
    assert_eq!(reply.usage.input_tokens, Some(6));
    let attempts = attempts.lock().unwrap();
    assert_eq!(attempts.len(), 4);
    assert_eq!(attempts[1].phase, ModelAttemptPhase::Failed);
    assert_eq!(attempts[3].phase, ModelAttemptPhase::Completed);
}

/// 非重试状态只执行一次并返回无敏感原文的错误和失败终态。
#[tokio::test(flavor = "multi_thread")]
async fn non_retryable_http_status_is_not_retried() {
    let (base_url, count, server) =
        serve(vec![HttpReply::json(400, json!({"error": "private"}))]).await;
    let attempts = Arc::new(Mutex::new(Vec::<ModelAttempt>::new()));
    let received = attempts.clone();
    let callback = move |attempt| {
        received.lock().unwrap().push(attempt);
        Ok(())
    };
    let result = complete(
        ProviderProtocol::Responses,
        &base_url,
        &Cancellation::new(),
        &callback,
        &|_| {},
    )
    .await;
    server.await.unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 1);
    let error = result.unwrap_err();
    assert_eq!(error.code, "AGENT_API_REQUEST");
    assert!(!error.message.contains("private"));
    let attempts = attempts.lock().unwrap();
    assert_eq!(attempts.len(), 2);
    assert_eq!(attempts[0].phase, ModelAttemptPhase::Started);
    assert_eq!(attempts[1].phase, ModelAttemptPhase::Failed);
    assert_eq!(attempts[1].usage, None);
}

/// Started 回调拒绝预算时不发送请求，且未开始的尝试没有终态。
#[tokio::test(flavor = "multi_thread")]
async fn rejected_started_attempt_does_not_send_or_emit_terminal() {
    let (base_url, count, server) =
        serve(vec![HttpReply::json(200, chat_response(Value::Null))]).await;
    let attempts = Arc::new(Mutex::new(Vec::<ModelAttempt>::new()));
    let received = attempts.clone();
    let callback = move |attempt: ModelAttempt| {
        let started = matches!(&attempt.phase, ModelAttemptPhase::Started);
        received.lock().unwrap().push(attempt);
        if started {
            Err(AgentError::new("AGENT_BUDGET", "测试预算已用完。"))
        } else {
            Ok(())
        }
    };
    let result = complete(
        ProviderProtocol::ChatCompletions,
        &base_url,
        &Cancellation::new(),
        &callback,
        &|_| {},
    )
    .await;
    assert_eq!(result.unwrap_err().code, "AGENT_BUDGET");
    assert_eq!(count.load(Ordering::SeqCst), 0);
    assert_eq!(attempts.lock().unwrap().len(), 1);
    server.abort();
}

/// 收到流式正文或 JSON 协议错误后不重试请求。
#[tokio::test(flavor = "multi_thread")]
async fn response_body_protocol_failure_is_never_retried() {
    let stream = format!(
        "data: {}\n\ndata: not-json\n\n",
        json!({"choices":[{"delta":{"content":"已收到内容"}}]})
    );
    let (base_url, count, server) = serve(vec![
        HttpReply::sse(stream),
        HttpReply::json(200, chat_response(Value::Null)),
    ])
    .await;
    let deltas = Arc::new(Mutex::new(String::new()));
    let captured = deltas.clone();
    let delta = move |text: &str| captured.lock().unwrap().push_str(text);
    let result = complete(
        ProviderProtocol::ChatCompletions,
        &base_url,
        &Cancellation::new(),
        &|_| Ok(()),
        &delta,
    )
    .await;
    server.abort();
    assert!(result.is_err());
    assert_eq!(count.load(Ordering::SeqCst), 1);
    assert_eq!(deltas.lock().unwrap().as_str(), "已收到内容");

    let (base_url, count, server) = serve(vec![
        HttpReply::raw_json("not-json"),
        HttpReply::json(200, responses_response(Value::Null)),
    ])
    .await;
    let result = complete(
        ProviderProtocol::Responses,
        &base_url,
        &Cancellation::new(),
        &|_| Ok(()),
        &|_| {},
    )
    .await;
    server.abort();
    assert_eq!(result.unwrap_err().code, "AGENT_PROTOCOL");
    assert_eq!(count.load(Ordering::SeqCst), 1);
}

/// 取消信号在首次失败后生效时，不开始下一次请求。
#[tokio::test(flavor = "multi_thread")]
async fn cancellation_during_backoff_prevents_next_attempt() {
    let (base_url, count, server) = serve(vec![
        HttpReply::retry(503, "1"),
        HttpReply::json(200, chat_response(Value::Null)),
    ])
    .await;
    let cancellation = Cancellation::new();
    let cancel = cancellation.clone();
    let callback = move |attempt: ModelAttempt| {
        if matches!(&attempt.phase, ModelAttemptPhase::Failed) {
            let cancel = cancel.clone();
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                cancel.cancel();
            });
        }
        Ok(())
    };
    let result = complete(
        ProviderProtocol::ChatCompletions,
        &base_url,
        &cancellation,
        &callback,
        &|_| {},
    )
    .await;
    server.abort();
    assert_eq!(result.unwrap_err().code, "AGENT_CANCELLED");
    assert_eq!(count.load(Ordering::SeqCst), 1);
}

/// 三协议流式 usage 均被收集，Anthropic 累计字段覆盖而不是重复相加。
#[tokio::test(flavor = "multi_thread")]
async fn streaming_usage_is_merged_by_protocol_without_recounting() {
    let chat_stream = format!(
        "data: {}\n\ndata: {}\n\ndata: {}\n\ndata: [DONE]\n\n",
        json!({"choices":[{"delta":{"content":"Chat"}}]}),
        json!({"choices":[],"usage":{"prompt_tokens":12,"completion_tokens":5,"prompt_tokens_details":{"cached_tokens":4},"completion_tokens_details":{"reasoning_tokens":2}}}),
        json!({"choices":[{"delta":{},"finish_reason":"stop"}]})
    );
    let responses_stream = format!(
        "data: {}\n\ndata: {}\n\n",
        json!({"type":"response.output_text.delta","delta":"Responses"}),
        json!({"type":"response.completed","response":{"status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":"Responses"}]}],"usage":{"input_tokens":21,"output_tokens":8,"input_tokens_details":{"cached_tokens":6},"output_tokens_details":{"reasoning_tokens":3}}}})
    );
    let anthropic_stream = format!(
        "event: message_start\ndata: {}\n\nevent: content_block_start\ndata: {}\n\nevent: content_block_delta\ndata: {}\n\nevent: content_block_stop\ndata: {}\n\nevent: message_delta\ndata: {}\n\nevent: message_stop\ndata: {}\n\n",
        json!({"type":"message_start","message":{"id":"msg-1","role":"assistant","content":[],"model":"test","stop_reason":null,"usage":{"input_tokens":10,"cache_read_input_tokens":1,"cache_creation_input_tokens":2,"output_tokens":0}}}),
        json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
        json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Anthropic"}}),
        json!({"type":"content_block_stop","index":0}),
        json!({"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":7,"cache_read_input_tokens":5,"cache_creation_input_tokens":3}}),
        json!({"type":"message_stop"})
    );

    for (protocol, stream, expected) in [
        (
            ProviderProtocol::ChatCompletions,
            chat_stream,
            ModelUsage {
                input_tokens: Some(12),
                output_tokens: Some(5),
                cached_input_tokens: Some(4),
                cache_creation_input_tokens: None,
                reasoning_tokens: Some(2),
            },
        ),
        (
            ProviderProtocol::Responses,
            responses_stream,
            ModelUsage {
                input_tokens: Some(21),
                output_tokens: Some(8),
                cached_input_tokens: Some(6),
                cache_creation_input_tokens: None,
                reasoning_tokens: Some(3),
            },
        ),
        (
            ProviderProtocol::AnthropicMessages,
            anthropic_stream,
            ModelUsage {
                input_tokens: Some(18),
                output_tokens: Some(7),
                cached_input_tokens: Some(5),
                cache_creation_input_tokens: Some(3),
                reasoning_tokens: None,
            },
        ),
    ] {
        let (base_url, _, server) = serve(vec![HttpReply::sse(stream)]).await;
        let reply = complete(
            protocol,
            &base_url,
            &Cancellation::new(),
            &|_| Ok(()),
            &|_| {},
        )
        .await
        .unwrap();
        server.await.unwrap();
        assert_eq!(reply.usage, expected, "protocol {protocol:?}");
    }
}
