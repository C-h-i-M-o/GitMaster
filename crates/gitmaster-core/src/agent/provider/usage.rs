use super::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 供应商报告的 token 计数；缺失计数保留为 None。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ModelUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cached_input_tokens: Option<u64>,
    pub cache_creation_input_tokens: Option<u64>,
    pub reasoning_tokens: Option<u64>,
}

/// 单次模型请求的阶段记录，不携带响应原文或鉴权信息。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ModelAttempt {
    pub id: String,
    pub attempt: u8,
    pub phase: ModelAttemptPhase,
    pub usage: Option<ModelUsage>,
    pub error_code: Option<String>,
}

/// 单次尝试的开始或唯一终态。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ModelAttemptPhase {
    Started,
    Completed,
    Failed,
}

/// 从完整供应商响应提取协议化 token 计数。
pub(super) fn parse_usage(protocol: ProviderProtocol, raw: &Value) -> ModelUsage {
    let Some(usage) = raw.get("usage").filter(|value| value.is_object()) else {
        return ModelUsage::default();
    };
    parse_usage_object(protocol, usage)
}

/// 从 usage 对象提取计数，并保持各协议缓存/推理子项为独立子集。
pub(super) fn parse_usage_object(protocol: ProviderProtocol, usage: &Value) -> ModelUsage {
    match protocol {
        ProviderProtocol::ChatCompletions => ModelUsage {
            input_tokens: usage.get("prompt_tokens").and_then(Value::as_u64),
            output_tokens: usage.get("completion_tokens").and_then(Value::as_u64),
            cached_input_tokens: usage
                .get("prompt_cache_hit_tokens")
                .and_then(Value::as_u64)
                .or_else(|| {
                    usage
                        .get("prompt_tokens_details")
                        .and_then(|details| details.get("cached_tokens"))
                        .and_then(Value::as_u64)
                }),
            cache_creation_input_tokens: None,
            reasoning_tokens: usage
                .get("completion_tokens_details")
                .and_then(|details| details.get("reasoning_tokens"))
                .and_then(Value::as_u64),
        },
        ProviderProtocol::Responses => ModelUsage {
            input_tokens: usage.get("input_tokens").and_then(Value::as_u64),
            output_tokens: usage.get("output_tokens").and_then(Value::as_u64),
            cached_input_tokens: usage
                .get("input_tokens_details")
                .and_then(|details| details.get("cached_tokens"))
                .and_then(Value::as_u64),
            cache_creation_input_tokens: None,
            reasoning_tokens: usage
                .get("output_tokens_details")
                .and_then(|details| details.get("reasoning_tokens"))
                .and_then(Value::as_u64),
        },
        ProviderProtocol::AnthropicMessages => {
            let base_input = usage.get("input_tokens").and_then(Value::as_u64);
            let cached = usage.get("cache_read_input_tokens").and_then(Value::as_u64);
            let created = usage
                .get("cache_creation_input_tokens")
                .and_then(Value::as_u64);
            ModelUsage {
                input_tokens: anthropic_total_input(base_input, cached, created),
                output_tokens: usage.get("output_tokens").and_then(Value::as_u64),
                cached_input_tokens: cached,
                cache_creation_input_tokens: created,
                reasoning_tokens: None,
            }
        }
    }
}

/// 合并流式累计快照；出现的新字段覆盖旧值，未出现的字段保留旧快照。
pub(super) fn merge_usage(current: &mut ModelUsage, update: ModelUsage) {
    if update.input_tokens.is_some() {
        current.input_tokens = update.input_tokens;
    }
    if update.output_tokens.is_some() {
        current.output_tokens = update.output_tokens;
    }
    if update.cached_input_tokens.is_some() {
        current.cached_input_tokens = update.cached_input_tokens;
    }
    if update.cache_creation_input_tokens.is_some() {
        current.cache_creation_input_tokens = update.cache_creation_input_tokens;
    }
    if update.reasoning_tokens.is_some() {
        current.reasoning_tokens = update.reasoning_tokens;
    }
}

/// 组合 Anthropic 未缓存输入及缓存读写量；缺失缓存计数按协议省略的零参与总量。
pub(super) fn anthropic_total_input(
    base: Option<u64>,
    cached: Option<u64>,
    created: Option<u64>,
) -> Option<u64> {
    base?
        .checked_add(cached.unwrap_or(0))?
        .checked_add(created.unwrap_or(0))
}

/// 返回非负整数计数；负数、浮点和字符串值视为供应商未提供。
pub(super) fn usage_count(value: &Value, key: &str) -> Option<u64> {
    value.get(key).and_then(Value::as_u64)
}
