//! 三协议适配与可取消 HTTP 请求；不执行模型给出的工具。
mod streaming;
mod wire;
use super::*;
use eventsource_stream::Eventsource;
use futures_util::StreamExt;
use serde::Serialize;
use serde_json::Value;
use std::time::Duration;
pub use wire::{build_request, parse_response};

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
        cancellation.check()?;
        let body = build_request(profile, events, tools, system)?;
        if serde_json::to_vec(&body)
            .map_err(|_| protocol_error())?
            .len()
            > 12 * 1024 * 1024
        {
            return Err(AgentError::new(
                "AGENT_CONTEXT_LIMIT",
                "对话上下文过长，请新建对话或缩小读取范围。",
            ));
        }
        let work = self.request(profile, key, body, delta);
        tokio::select! { biased; _ = cancellation.cancelled() => Err(cancelled_error()), reply = work => reply }
    }
    /// 设置敏感鉴权头，响应错误仅按状态码分类。
    async fn request(
        &self,
        profile: &ProviderProfile,
        key: &str,
        body: Value,
        delta: &(dyn Fn(&str) + Send + Sync),
    ) -> AgentResult<ModelReply> {
        use reqwest::header::HeaderValue;
        let mut request = self.client.post(profile.endpoint()?).json(&body);
        let (name, value) = if profile.protocol == ProviderProtocol::AnthropicMessages {
            request = request.header("anthropic-version", "2023-06-01");
            ("x-api-key", key.to_owned())
        } else {
            ("authorization", format!("Bearer {key}"))
        };
        let mut header = HeaderValue::from_str(&value)
            .map_err(|_| AgentError::new("AGENT_INVALID_KEY", "API Key 格式不正确。"))?;
        header.set_sensitive(true);
        let mut response = request
            .header(name, header)
            .send()
            .await
            .map_err(http_error)?;
        if !response.status().is_success() {
            return Err(match response.status().as_u16() {
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
                    AgentError::new(
                        "AGENT_STREAM",
                        "模型数据流中断或格式无效，未执行不完整的工具调用。",
                    )
                })?;
                if accumulator.push(&event.data, delta)? {
                    break;
                }
            }
            accumulator.finish()
        } else {
            let mut bytes = vec![];
            while let Some(chunk) = response.chunk().await.map_err(http_error)? {
                if bytes.len() + chunk.len() > 8 * 1024 * 1024 {
                    return Err(AgentError::new(
                        "AGENT_RESPONSE_LIMIT",
                        "模型响应超过大小限制。",
                    ));
                }
                bytes.extend_from_slice(&chunk);
            }
            let reply = parse_response(
                profile.protocol,
                serde_json::from_slice(&bytes).map_err(|_| protocol_error())?,
            )?;
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
/// 取消只表明后续工作停止，不承诺已完成副作用回滚。
pub(crate) fn cancelled_error() -> AgentError {
    AgentError::new("AGENT_CANCELLED", "已停止本轮运行。")
}
