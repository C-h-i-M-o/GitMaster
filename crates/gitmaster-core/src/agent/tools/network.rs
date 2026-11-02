use super::*;
use crate::agent::{
    policy::{decide, PolicyDecision, ToolRisk},
    provider::Cancellation,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    net::SocketAddr,
    time::{Duration, Instant},
};
use url::Url;

/// 固定解析结果与权限决定，防止批准后 DNS 换成另一个风险级别。
pub struct PreparedNetwork {
    pub decision: PolicyDecision,
    pub target: String,
    pub reason: String,
    url: Url,
    addresses: Vec<SocketAddr>,
    created: Instant,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Args {
    url: String,
}
/// 无 Cookie、无模型凭据、无代理和自动跳转的文本读取工具。
pub struct NetworkTools;
impl NetworkTools {
    /// 解析 URL 和目标地址，但批准前不连接目标服务器。
    pub async fn prepare(call: &ToolCall, mode: PermissionMode) -> AgentResult<PreparedNetwork> {
        let args: Args = serde_json::from_value(call.arguments.clone()).map_err(|_| invalid())?;
        let url = valid_url(&args.url)?;
        let host = url.host_str().ok_or_else(invalid)?.trim_matches(['[', ']']);
        let port = url.port_or_known_default().ok_or_else(invalid)?;
        let addresses: Vec<_> = tokio::time::timeout(
            Duration::from_secs(10),
            tokio::net::lookup_host((host, port)),
        )
        .await
        .map_err(|_| network_error())?
        .map_err(|_| network_error())?
        .take(32)
        .collect();
        if addresses.is_empty() {
            return Err(network_error());
        }
        let private = addresses
            .iter()
            .any(|address| !git::public_ip(address.ip()));
        Ok(PreparedNetwork {
            decision: decide(mode, ToolRisk::Network { private }),
            target: url.to_string(),
            reason: if private {
                "网络目标包含本机或私网地址。"
            } else {
                "即将读取外部网页。"
            }
            .into(),
            url,
            addresses,
            created: Instant::now(),
        })
    }
    /// 每个跳转作为新工具目标重新批准；响应只接受有界 UTF-8 文本。
    pub async fn execute(
        prepared: PreparedNetwork,
        approved: bool,
        cancel: &Cancellation,
    ) -> AgentResult<Value> {
        if prepared.decision == PolicyDecision::Deny
            || (prepared.decision == PolicyDecision::Ask && !approved)
        {
            return Err(AgentError::new(
                "AGENT_APPROVAL_REQUIRED",
                "联网需要用户批准。",
            ));
        }
        if prepared.created.elapsed() > Duration::from_secs(300) {
            return Err(AgentError::new(
                "AGENT_APPROVAL_EXPIRED",
                "联网批准已过期。",
            ));
        }
        cancel.check()?;
        tokio::select! {biased; _ = cancel.cancelled() => {cancel.check()?; unreachable!()}, result = fetch(prepared) => result}
    }
}
/// 拒绝 URL 内的凭据，不将其记录或传给服务器。
fn valid_url(value: &str) -> AgentResult<Url> {
    let url = Url::parse(value).map_err(|_| invalid())?;
    if value.len() > 4096
        || !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || url.query_pairs().any(|(name, _)| {
            let name = name.to_ascii_lowercase();
            [
                "key",
                "token",
                "secret",
                "password",
                "signature",
                "credential",
                "authorization",
            ]
            .iter()
            .any(|part| name.contains(part))
        })
    {
        return Err(invalid());
    }
    Ok(url)
}
/// 固定 DNS 结果和原始主机名，保留 TLS 证书校验并禁用系统代理改道。
async fn fetch(prepared: PreparedNetwork) -> AgentResult<Value> {
    let host = prepared.url.host_str().ok_or_else(invalid)?;
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .resolve_to_addrs(host, &prepared.addresses)
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(45))
        .build()
        .map_err(|_| network_error())?;
    let mut response = client
        .get(prepared.url.clone())
        .header("accept", "text/*, application/json")
        .send()
        .await
        .map_err(|_| network_error())?;
    let status = response.status();
    if status.is_redirection() {
        let location = response
            .headers()
            .get("location")
            .and_then(|value| value.to_str().ok())
            .ok_or_else(network_error)?;
        let next = prepared.url.join(location).map_err(|_| invalid())?;
        valid_url(next.as_str())?;
        return Ok(
            json!({"url":prepared.target,"status":status.as_u16(),"redirectUrl":next.as_str(),"note":"未跟随跳转；请对新目标再次调用 fetch_url。"}),
        );
    }
    if !status.is_success() {
        return Err(AgentError::new(
            "AGENT_FETCH_STATUS",
            "网页返回失败状态，未读取错误正文。",
        ));
    }
    let mime = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    if !mime.starts_with("text/")
        && !mime.starts_with("application/json")
        && !mime.starts_with("application/xml")
    {
        return Err(AgentError::new(
            "AGENT_FETCH_BINARY",
            "目标未声明文本类型，未读取二进制内容。",
        ));
    }
    let mut bytes = vec![];
    while let Some(chunk) = response.chunk().await.map_err(|_| network_error())? {
        if bytes.len() + chunk.len() > 512 * 1024 {
            return Err(AgentError::new(
                "AGENT_FETCH_LIMIT",
                "网页超过 512 KiB，请缩小读取范围。",
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    let text = String::from_utf8(bytes)
        .map_err(|_| AgentError::new("AGENT_FETCH_ENCODING", "网页不是 UTF-8 文本。"))?;
    Ok(json!({"url":prepared.target,"status":status.as_u16(),"text":text,"truncated":false}))
}
/// 不回显带有敏感参数的输入 URL。
fn invalid() -> AgentError {
    AgentError::new("AGENT_FETCH_URL", "请输入不含凭据的 HTTP(S) 地址。")
}
/// 不转发可能包含完整 URL 的底层错误。
fn network_error() -> AgentError {
    AgentError::new(
        "AGENT_FETCH_NETWORK",
        "网页连接失败或超时，请检查地址和网络。",
    )
}
