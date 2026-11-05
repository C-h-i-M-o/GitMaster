use super::*;
use serde::Serialize;
use serde_json::json;

/// 模型列表只返回经过校验的展示字段与能力，不返回服务原始响应。
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredModel {
    pub model_id: String,
    pub label: String,
    pub capabilities: ModelCapabilities,
}
/// 一页结果绑定请求身份，空结果不代表应清除用户配置。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredModelPage {
    pub request_id: String,
    pub models: Vec<DiscoveredModel>,
    pub next_cursor: Option<String>,
}
/// 分阶段报告只表示实际验证的协议能力。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionTestStage {
    pub id: String,
    pub status: String,
    pub message: String,
}
/// 草稿测试不会写聊天或调用本地工具。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionTestReport {
    pub request_id: String,
    pub stages: Vec<ConnectionTestStage>,
}

impl ModelClient {
    /// 在用户主动操作时获取一页模型，取消信号可中断请求及响应读取。
    pub async fn list_models(
        &self,
        profile: &ProviderProfile,
        key: &str,
        request_id: String,
        cursor: Option<String>,
        cancel: &Cancellation,
    ) -> AgentResult<DiscoveredModelPage> {
        profile.validate()?;
        let task = async {
            let mut base = profile.base_url.trim_end_matches('/').to_owned();
            for suffix in ["/chat/completions", "/responses", "/messages"] {
                if base.ends_with(suffix) {
                    base.truncate(base.len() - suffix.len());
                    break;
                }
            }
            let mut endpoint =
                url::Url::parse(&format!("{base}/models")).map_err(|_| protocol_error())?;
            if let Some(cursor) = cursor {
                if cursor.len() > 512 || cursor.chars().any(char::is_control) {
                    return Err(protocol_error());
                }
                endpoint.query_pairs_mut().append_pair(
                    if profile.protocol == ProviderProtocol::AnthropicMessages {
                        "after_id"
                    } else {
                        "after"
                    },
                    &cursor,
                );
            }
            let mut request = self.client.get(endpoint);
            let mut header = reqwest::header::HeaderValue::from_str(&if profile.protocol
                == ProviderProtocol::AnthropicMessages
            {
                key.into()
            } else {
                format!("Bearer {key}")
            })
            .map_err(|_| AgentError::new("AGENT_INVALID_KEY", "API Key 格式不正确。"))?;
            header.set_sensitive(true);
            request = if profile.protocol == ProviderProtocol::AnthropicMessages {
                request
                    .header("anthropic-version", "2023-06-01")
                    .header("x-api-key", header)
            } else {
                request.header("authorization", header)
            };
            let mut response = request.send().await.map_err(http_error)?;
            if !response.status().is_success() {
                return Err(match response.status().as_u16() {
                    401 | 403 => {
                        AgentError::new("AGENT_AUTH", "模型列表鉴权失败，请检查 Key 与账号权限。")
                    }
                    404 | 405 | 501 => AgentError::new(
                        "AGENT_MODEL_LIST_UNSUPPORTED",
                        "服务不支持模型列表，请使用预设或手动填写模型 ID。",
                    ),
                    429 => AgentError::new(
                        "AGENT_RATE_LIMIT",
                        "获取模型被限流，请稍后重试，已有配置未更改。",
                    ),
                    300..=399 => AgentError::new(
                        "AGENT_REDIRECT",
                        "模型列表地址发生重定向，请填写最终服务地址。",
                    ),
                    _ => AgentError::new(
                        "AGENT_MODEL_LIST",
                        "暂时无法获取模型列表，已有配置未更改。",
                    ),
                });
            }
            let mut bytes = vec![];
            while let Some(chunk) = response.chunk().await.map_err(http_error)? {
                if bytes.len() + chunk.len() > 2 * 1024 * 1024 {
                    return Err(AgentError::new(
                        "AGENT_MODEL_LIST_LIMIT",
                        "模型列表过大，请使用分页或手动填写模型 ID。",
                    ));
                }
                bytes.extend_from_slice(&chunk);
            }
            parse_models(
                profile,
                key,
                request_id,
                serde_json::from_slice(&bytes).map_err(|_| protocol_error())?,
            )
        };
        tokio::select! { biased; _ = cancel.cancelled() => Err(cancelled_error()), result = task => result }
    }

    /// 用模拟探针检查工具协议；模型调用不会触及文件、Git 或命令行。
    pub async fn test_model(
        &self,
        profile: &ProviderProfile,
        key: &str,
        parameters: &ModelParameters,
        capabilities: &ModelCapabilities,
        request_id: String,
        cancel: &Cancellation,
    ) -> AgentResult<ConnectionTestReport> {
        let event = |sequence, payload| AgentEvent {
            version: 1,
            sequence,
            session_id: request_id.clone(),
            run_id: request_id.clone(),
            timestamp: timestamp(),
            payload,
        };
        let mut events = vec![event(
            1,
            EventKind::UserMessage {
                text: "请调用 connection_probe，然后简短回复连接成功。".into(),
            },
        )];
        let definitions = [ToolDefinition {
            name: "connection_probe".into(),
            description: "模拟探针，只返回连接测试成功，没有本地操作。".into(),
            parameters: json!({"type":"object","properties":{},"additionalProperties":false}),
        }];
        let reply = self
            .complete_configured(
                profile,
                key,
                &events,
                &definitions,
                "只验证连接与工具调用格式，不执行真实工具。",
                parameters,
                capabilities,
                cancel,
                &|_| {},
            )
            .await?;
        privacy::ensure_safe_value(&reply.wire, Some(key))?;
        let tool_supported = !reply.calls.is_empty()
            && reply.calls.iter().all(|call| {
                call.name == "connection_probe"
                    && call
                        .arguments
                        .as_object()
                        .is_some_and(|args| args.is_empty())
            });
        let mut text_supported = !reply.text.trim().is_empty();
        if tool_supported {
            events.push(event(
                2,
                EventKind::AssistantMessage {
                    text: reply.text,
                    protocol: profile.protocol,
                    wire: reply.wire,
                    calls: reply.calls.clone(),
                },
            ));
            for call in &reply.calls {
                events.push(event(
                    events.len() as u64 + 1,
                    EventKind::ToolResult {
                        call_id: call.id.clone(),
                        name: call.name.clone(),
                        result: json!({"ok":true,"simulated":true}),
                        is_error: false,
                    },
                ));
            }
            let final_reply = self
                .complete_configured(
                    profile,
                    key,
                    &events,
                    &[],
                    "工具探针已完成，请简短回复连接成功。",
                    parameters,
                    capabilities,
                    cancel,
                    &|_| {},
                )
                .await?;
            privacy::ensure_safe_value(&final_reply.wire, Some(key))?;
            text_supported = !final_reply.text.trim().is_empty();
        }
        let stage = |id: &str, passed, message: &str| ConnectionTestStage {
            id: id.into(),
            status: if passed { "passed" } else { "unsupported" }.into(),
            message: message.into(),
        };
        Ok(ConnectionTestReport {
            request_id,
            stages: vec![
                stage("authentication", true, "服务接受了本次鉴权。"),
                stage("model", true, "模型返回完整协议响应。"),
                stage(
                    "text",
                    text_supported,
                    if text_supported {
                        "已收到文本响应。"
                    } else {
                        "未确认文本响应能力。"
                    },
                ),
                stage(
                    "tools",
                    tool_supported,
                    if tool_supported {
                        "模拟工具调用及回传通过；未执行本地操作或验证沙箱。"
                    } else {
                        "未确认工具调用能力，普通文本响应不等于工具可用。"
                    },
                ),
            ],
        })
    }
}

/// 校验列表与元数据边界，能力仍保留服务端原始档位并绑定请求端点。
pub fn parse_models(
    profile: &ProviderProfile,
    key: &str,
    request_id: String,
    raw: Value,
) -> AgentResult<DiscoveredModelPage> {
    let rows = raw["data"].as_array().ok_or_else(protocol_error)?;
    if rows.len() > 2000 {
        return Err(protocol_error());
    }
    let mut models = vec![];
    let mut ids = std::collections::HashSet::new();
    for row in rows {
        let id = row["id"].as_str().ok_or_else(protocol_error)?;
        let label = row["name"]
            .as_str()
            .or_else(|| row["display_name"].as_str())
            .unwrap_or(id);
        let mut candidate = profile.clone();
        candidate.model = id.into();
        candidate.name = label.into();
        candidate.validate()?;
        privacy::ensure_safe_text(id, Some(key))?;
        privacy::ensure_safe_text(label, Some(key))?;
        if !ids.insert(id) {
            continue;
        }
        let mut capabilities = catalog::capabilities(&candidate);
        let mut metadata = false;
        for (field, target) in [
            ("context_window", &mut capabilities.context_window),
            ("max_output_tokens", &mut capabilities.max_output_tokens),
        ] {
            if let Some(value) = row.get(field) {
                let value = value
                    .as_u64()
                    .filter(|n| *n > 0 && *n <= 16_777_216)
                    .ok_or_else(protocol_error)?;
                *target = Some(value as u32);
                metadata = true;
            }
        }
        if let Some(levels) = row
            .get("effort")
            .and_then(|effort| effort.get("supported_levels"))
        {
            let levels = levels
                .as_array()
                .filter(|items| items.len() <= 16)
                .ok_or_else(protocol_error)?;
            capabilities.supported_efforts = levels
                .iter()
                .map(|level| {
                    level
                        .as_str()
                        .filter(|text| {
                            !text.is_empty()
                                && text.len() <= 24
                                && text
                                    .bytes()
                                    .all(|c| c.is_ascii_lowercase() || c == b'_' || c == b'-')
                        })
                        .map(str::to_owned)
                        .ok_or_else(protocol_error)
                })
                .collect::<AgentResult<Vec<_>>>()?;
            metadata = true;
        }
        if let Some(value) = row.get("supports_tools") {
            capabilities.tool_support = Some(value.as_bool().ok_or_else(protocol_error)?);
            metadata = true;
        }
        if metadata {
            capabilities.source = "discovery".into();
            capabilities.verified_at = Some(timestamp().to_string());
            capabilities.endpoint = Some(candidate.endpoint()?);
        }
        models.push(DiscoveredModel {
            model_id: id.into(),
            label: label.into(),
            capabilities,
        });
    }
    let next_cursor = if raw["has_more"].as_bool() == Some(true) {
        Some(
            raw["last_id"]
                .as_str()
                .ok_or_else(protocol_error)?
                .to_owned(),
        )
    } else {
        raw["next_cursor"].as_str().map(str::to_owned)
    };
    if let Some(cursor) = &next_cursor {
        if cursor.len() > 512 || cursor.chars().any(char::is_control) {
            return Err(protocol_error());
        }
        privacy::ensure_safe_text(cursor, Some(key))?;
    }
    Ok(DiscoveredModelPage {
        request_id,
        models,
        next_cursor,
    })
}
