//! 三协议适配与可取消 HTTP 请求；不执行模型给出的工具。
pub mod catalog;
mod configuration;
pub mod discovery;
mod streaming;
mod usage;
mod wire;
use super::*;
pub use configuration::*;
use eventsource_stream::Eventsource;
use futures_util::StreamExt;
use serde::Serialize;
use serde_json::Value;
use std::time::Duration;
pub use usage::{ModelAttempt, ModelAttemptPhase, ModelUsage};
pub use wire::{
    build_request, build_request_configured, build_request_with_parameters, parse_response,
};

const MAX_ATTEMPTS: u8 = 3;

/// 工具定义是注册表提供的固定 JSON Schema，不接受仓库内容替换。
#[derive(Debug, Clone, Serialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

/// 完整回复保留协议原始上下文，文字与工具调用提供给统一循环。
#[derive(Debug, Clone)]
pub struct ModelReply {
    pub text: String,
    pub calls: Vec<ToolCall>,
    pub wire: Value,
    pub usage: ModelUsage,
}

/// 请求头阶段的可重试失败元数据；不含响应正文。
struct RequestFailure {
    error: AgentError,
    retryable: bool,
    retry_after: Option<Duration>,
}

impl RequestFailure {
    /// 构造读取或协议阶段的不可重试失败。
    fn permanent(error: AgentError) -> Self {
        Self {
            error,
            retryable: false,
            retry_after: None,
        }
    }
}

/// 可克隆的取消信号，取消发生在订阅前也不会丢失。
#[derive(Clone)]
pub struct Cancellation {
    signal: tokio::sync::watch::Sender<bool>,
}
impl Default for Cancellation {
    /// 新运行默认未取消。
    fn default() -> Self {
        Self::new()
    }
}
impl Cancellation {
    /// 建立独立运行的取消通道。
    pub fn new() -> Self {
        Self {
            signal: tokio::sync::watch::channel(false).0,
        }
    }
    /// 使 HTTP 请求和等待批准立即退出；不会重放已执行副作用。
    pub fn cancel(&self) {
        self.signal.send_replace(true);
    }
    /// 同步检查工具执行前是否已取消。
    pub fn is_cancelled(&self) -> bool {
        *self.signal.borrow()
    }
    /// 等待永久的取消状态，避免 Notify 的提前通知丢失。
    pub async fn cancelled(&self) {
        let mut receiver = self.signal.subscribe();
        let _ = receiver.wait_for(|value| *value).await;
    }
    /// 在启动下一步前返回稳定取消错误。
    pub fn check(&self) -> AgentResult<()> {
        if self.is_cancelled() {
            Err(cancelled_error())
        } else {
            Ok(())
        }
    }
}

/// 共享 HTTP 连接池，禁止跨站重定向传递 API Key。
#[derive(Clone)]
pub struct ModelClient {
    client: reqwest::Client,
}
impl ModelClient {
    /// 使用成熟 HTTP/TLS 客户端与有界连接、读取期限。
    pub fn new() -> AgentResult<Self> {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(20))
            .read_timeout(Duration::from_secs(90))
            .timeout(Duration::from_secs(240))
            .build()
            .map_err(|_| AgentError::new("AGENT_HTTP_INIT", "无法初始化模型连接。"))?;
        Ok(Self { client })
    }
    /// 请求模型，支持 SSE 和兼容服务返回的普通 JSON；不向日志返回原始错误。
    pub async fn complete(
        &self,
        profile: &ProviderProfile,
        key: &str,
        events: &[AgentEvent],
        tools: &[ToolDefinition],
        system: &str,
        cancellation: &Cancellation,
        delta: &(dyn Fn(&str) + Send + Sync),
    ) -> AgentResult<ModelReply> {
        self.complete_configured(
            profile,
            key,
            events,
            tools,
            system,
            &ModelParameters::default(),
            &catalog::capabilities(profile),
            cancellation,
            delta,
        )
        .await
    }
    /// 高级参数和能力快照随运行冻结，不受界面后续修改影响。
    pub async fn complete_configured(
        &self,
        profile: &ProviderProfile,
        key: &str,
        events: &[AgentEvent],
        tools: &[ToolDefinition],
        system: &str,
        parameters: &ModelParameters,
        capabilities: &ModelCapabilities,
        cancellation: &Cancellation,
        delta: &(dyn Fn(&str) + Send + Sync),
    ) -> AgentResult<ModelReply> {
        self.complete_with_attempts(
            profile,
            key,
            events,
            tools,
            system,
            parameters,
            capabilities,
            cancellation,
            delta,
            &|_| Ok(()),
        )
        .await
    }
    /// 有界重试模型请求，并在 HTTP 前和每次终态同步报告尝试记录。
    pub async fn complete_with_attempts(
        &self,
        profile: &ProviderProfile,
        key: &str,
        events: &[AgentEvent],
        tools: &[ToolDefinition],
        system: &str,
        parameters: &ModelParameters,
        capabilities: &ModelCapabilities,
        cancellation: &Cancellation,
        delta: &(dyn Fn(&str) + Send + Sync),
        on_attempt: &(dyn Fn(ModelAttempt) -> AgentResult<()> + Send + Sync),
    ) -> AgentResult<ModelReply> {
        cancellation.check()?;
        let body =
            build_request_configured(profile, events, tools, system, parameters, capabilities)?;
        let request_bytes = serde_json::to_vec(&body)
            .map_err(|_| protocol_error())?
            .len();
        let (context, output, safety) = parameters.budget(capabilities)?;
        // 专用开发总结和配置测试也使用相同输入预算，不能绕过主循环的上下文保护。
        if request_bytes > 12 * 1024 * 1024
            || request_bytes as u64 > u64::from(context - output - safety)
        {
            return Err(AgentError::new(
                "AGENT_CONTEXT_LIMIT",
                "对话上下文过长，请新建对话或缩小读取范围。",
            ));
        }
        for attempt in 1..=MAX_ATTEMPTS {
            cancellation.check()?;
            let id = new_id();
            on_attempt(ModelAttempt {
                id: id.clone(),
                attempt,
                phase: ModelAttemptPhase::Started,
                usage: None,
                error_code: None,
            })?;

            let result = tokio::select! {
                biased;
                _ = cancellation.cancelled() => Err(RequestFailure::permanent(cancelled_error())),
                result = self.request(profile, key, body.clone(), delta) => result,
            };
            match result {
                Ok(reply) => {
                    on_attempt(ModelAttempt {
                        id,
                        attempt,
                        phase: ModelAttemptPhase::Completed,
                        usage: Some(reply.usage.clone()),
                        error_code: None,
                    })?;
                    return Ok(reply);
                }
                Err(failure) => {
                    let final_attempt = attempt == MAX_ATTEMPTS || !failure.retryable;
                    on_attempt(ModelAttempt {
                        id,
                        attempt,
                        phase: ModelAttemptPhase::Failed,
                        usage: None,
                        error_code: Some(failure.error.code.clone()),
                    })?;
                    if final_attempt {
                        return Err(failure.error);
                    }
                    let delay = retry_delay(attempt, failure.retry_after);
                    tokio::select! {
                        biased;
                        _ = cancellation.cancelled() => return Err(cancelled_error()),
                        _ = tokio::time::sleep(delay) => {}
                    }
                }
            }
        }
        Err(AgentError::new(
            "AGENT_HTTP",
            "模型服务暂时不可用，请稍后重试。",
        ))
    }
    /// 设置敏感鉴权头，响应错误仅按状态码分类。
    async fn request(
        &self,
        profile: &ProviderProfile,
        key: &str,
        body: Value,
        delta: &(dyn Fn(&str) + Send + Sync),
    ) -> Result<ModelReply, RequestFailure> {
        use reqwest::header::HeaderValue;
        let endpoint = profile.endpoint().map_err(RequestFailure::permanent)?;
        let mut request = self.client.post(endpoint).json(&body);
        let (name, value) = if profile.protocol == ProviderProtocol::AnthropicMessages {
            request = request.header("anthropic-version", "2023-06-01");
            ("x-api-key", key.to_owned())
        } else {
            ("authorization", format!("Bearer {key}"))
        };
        let mut header = HeaderValue::from_str(&value).map_err(|_| {
            RequestFailure::permanent(AgentError::new("AGENT_INVALID_KEY", "API Key 格式不正确。"))
        })?;
        header.set_sensitive(true);
        let mut response =
            request
                .header(name, header)
                .send()
                .await
                .map_err(|error| RequestFailure {
                    error: http_error(error),
                    retryable: true,
                    retry_after: None,
                })?;
        if !response.status().is_success() {
            let status = response.status().as_u16();
            let error = match status {
                401 | 403 => AgentError::new(
                    "AGENT_AUTH",
                    "模型服务拒绝访问，请检查 API Key、模型权限和服务地址。",
                ),
                429 => AgentError::new("AGENT_RATE_LIMIT", "模型服务限流或额度不足，请稍后重试。"),
                400 | 404 | 422 => AgentError::new(
                    "AGENT_API_REQUEST",
                    "模型服务不接受当前请求，请检查协议、模型 ID 与 API 地址是否匹配。",
                ),
                300..=399 => AgentError::new(
                    "AGENT_REDIRECT",
                    "API 地址返回重定向，请在设置中填写最终服务地址。",
                ),
                _ => AgentError::new("AGENT_HTTP", "模型服务暂时不可用，请稍后重试。"),
            };
            let retryable = matches!(status, 429 | 500 | 502 | 503 | 504 | 529);
            let retry_after = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.trim().parse::<u64>().ok())
                .map(Duration::from_secs);
            return Err(RequestFailure {
                error,
                retryable,
                retry_after,
            });
        }
        if response
            .headers()
            .get("content-type")
            .and_then(|h| h.to_str().ok())
            .is_some_and(|h| h.to_ascii_lowercase().contains("text/event-stream"))
        {
            let mut received = 0usize;
            let bytes = response.bytes_stream().map(move |chunk| {
                let chunk = chunk.map_err(http_error)?;
                received += chunk.len();
                if received > 8 * 1024 * 1024 {
                    return Err(AgentError::new(
                        "AGENT_RESPONSE_LIMIT",
                        "模型响应超过大小限制，请缩小任务范围。",
                    ));
                }
                Ok(chunk)
            });
            let mut stream = bytes.eventsource();
            let mut accumulator = streaming::Accumulator::new(profile.protocol);
            while let Some(event) = stream.next().await {
                let event = event.map_err(|_| {
                    RequestFailure::permanent(AgentError::new(
                        "AGENT_STREAM",
                        "模型数据流中断或格式无效，未执行不完整的工具调用。",
                    ))
                })?;
                if accumulator
                    .push(&event.data, delta)
                    .map_err(RequestFailure::permanent)?
                {
                    break;
                }
            }
            accumulator.finish().map_err(RequestFailure::permanent)
        } else {
            let mut bytes = vec![];
            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|error| RequestFailure::permanent(http_error(error)))?
            {
                if bytes.len() + chunk.len() > 8 * 1024 * 1024 {
                    return Err(RequestFailure::permanent(AgentError::new(
                        "AGENT_RESPONSE_LIMIT",
                        "模型响应超过大小限制。",
                    )));
                }
                bytes.extend_from_slice(&chunk);
            }
            let raw: Value = serde_json::from_slice(&bytes)
                .map_err(|_| RequestFailure::permanent(protocol_error()))?;
            let reply = parse_response(profile.protocol, raw).map_err(RequestFailure::permanent)?;
            delta(&reply.text);
            Ok(reply)
        }
    }
}
/// 不把包含服务器地址、请求或凭据的客户端错误透传。
fn http_error(error: reqwest::Error) -> AgentError {
    if error.is_timeout() {
        AgentError::new("AGENT_TIMEOUT", "模型响应超时，请稍后重试。")
    } else {
        AgentError::new("AGENT_NETWORK", "连接模型服务失败，请检查网络和 API 地址。")
    }
}
/// 所有协议异常使用不含服务原文的提示。
fn protocol_error() -> AgentError {
    AgentError::new(
        "AGENT_PROTOCOL",
        "模型响应格式不完整或不符合所选协议，未执行工具调用。",
    )
}
/// 为重试选择短指数退避或有界的 Retry-After 秒数。
fn retry_delay(attempt: u8, retry_after: Option<Duration>) -> Duration {
    retry_after
        .unwrap_or_else(|| Duration::from_millis(100_u64 << u32::from(attempt.saturating_sub(1))))
        .min(Duration::from_secs(2))
}
/// 取消只表明后续工作停止，不承诺已完成副作用回滚。
pub(crate) fn cancelled_error() -> AgentError {
    AgentError::new("AGENT_CANCELLED", "已停止本轮运行。")
}
