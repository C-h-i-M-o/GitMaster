use super::*;
use serde::Serialize;

/// 预设中保留真实模型 ID 和核验能力，不用展示名构造请求 ID。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetModel {
    pub model_id: String,
    pub label: String,
    pub protocol: ProviderProtocol,
    pub capabilities: ModelCapabilities,
    pub defaults: ModelParameters,
}
/// 官方连接预设，不包含密钥，也不在加载时触发联网。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderPreset {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub protocol: ProviderProtocol,
    pub default_model_id: String,
    pub models: Vec<PresetModel>,
    pub verified_at: String,
}

/// 能力只绑定核验过的官方端点与模型；自定义端点通过发现元数据另行声明能力。
pub fn capabilities(profile: &ProviderProfile) -> ModelCapabilities {
    let (preset_base, context, output, effort, modes, style, sampling) =
        match (profile.model.as_str(), profile.protocol) {
            ("deepseek-flash" | "deepseek-v4-pro", ProviderProtocol::ChatCompletions) => (
                "https://api.deepseek.com",
                1048576,
                393216,
                vec!["low", "high", "max"],
                vec![ThinkingMode::Enabled, ThinkingMode::Disabled],
                ParameterStyle::Deepseek,
                true,
            ),
            (
                "gpt-5.4" | "gpt-5.4-2026-03-05",
                ProviderProtocol::Responses | ProviderProtocol::ChatCompletions,
            ) => (
                "https://api.openai.com/v1",
                1050000,
                128000,
                vec!["none", "low", "medium", "high", "xhigh"],
                vec![],
                ParameterStyle::Openai,
                true,
            ),
            (
                "claude-sonnet-5-5" | "claude-opus-5-5" | "claude-haiku-5-5",
                ProviderProtocol::AnthropicMessages,
            ) => (
                "https://api.anthropic.com/v1",
                1000000,
                128000,
                vec!["low", "medium", "high", "xhigh", "max"],
                vec![ThinkingMode::Enabled],
                ParameterStyle::AnthropicAdaptive,
                false,
            ),
            _ => return ModelCapabilities::default(),
        };
    if !matches_preset_endpoint(profile, preset_base) {
        return ModelCapabilities::default();
    }
    ModelCapabilities {
        context_window: Some(context),
        max_output_tokens: Some(output),
        supported_efforts: effort.into_iter().map(str::to_owned).collect(),
        thinking_modes: modes,
        thinking_budget: false,
        tool_support: Some(true),
        sampling_support: Some(sampling),
        source: "preset".into(),
        verified_at: Some("2026-11-05".into()),
        endpoint: profile.endpoint().ok(),
        parameter_style: style,
    }
}

/// 仅在规范化后的完整协议端点与预设精确匹配时授予官方能力。
fn matches_preset_endpoint(profile: &ProviderProfile, preset_base: &str) -> bool {
    let Ok(expected_base) = url::Url::parse(preset_base) else {
        return false;
    };
    let Ok(endpoint) = profile.endpoint() else {
        return false;
    };
    let Ok(actual) = url::Url::parse(&endpoint) else {
        return false;
    };
    let suffix = match profile.protocol {
        ProviderProtocol::ChatCompletions => "/chat/completions",
        ProviderProtocol::Responses => "/responses",
        ProviderProtocol::AnthropicMessages => "/messages",
    };
    let expected_path = format!("{}{suffix}", expected_base.path().trim_end_matches('/'));
    actual.scheme() == expected_base.scheme()
        && actual.host_str().zip(expected_base.host_str()).is_some_and(
            |(actual_host, expected_host)| actual_host.eq_ignore_ascii_case(expected_host),
        )
        && actual.port_or_known_default() == expected_base.port_or_known_default()
        && actual.path().trim_end_matches('/') == expected_path.as_str()
}

/// 发现能力必须绑定原端点，修改地址或协议不会沿用旧服务的能力声明。
pub fn effective_capabilities(
    profile: &ProviderProfile,
    stored: &ModelCapabilities,
) -> ModelCapabilities {
    let preset = capabilities(profile);
    if stored.source == "discovery" && stored.endpoint.as_ref() == profile.endpoint().ok().as_ref()
    {
        stored.clone()
    } else {
        preset
    }
}

/// 返回本轮核验的官方预设；未知服务仍可手动配置且不会被当成官方站点。
pub fn presets() -> Vec<ProviderPreset> {
    let rows = [
        (
            "deepseek",
            "DeepSeek",
            "https://api.deepseek.com",
            ProviderProtocol::ChatCompletions,
            vec![
                ("deepseek-flash", "DeepSeek Flash"),
                ("deepseek-v4-pro", "DeepSeek Pro"),
            ],
        ),
        (
            "openai",
            "OpenAI",
            "https://api.openai.com/v1",
            ProviderProtocol::Responses,
            vec![("gpt-5.4", "GPT-5.4")],
        ),
        (
            "anthropic",
            "Anthropic",
            "https://api.anthropic.com/v1",
            ProviderProtocol::AnthropicMessages,
            vec![
                ("claude-sonnet-5-5", "Claude Sonnet 5.5"),
                ("claude-opus-5-5", "Claude Opus 5.5"),
                ("claude-haiku-5-5", "Claude Haiku 5.5"),
            ],
        ),
    ];
    rows.into_iter()
        .map(|(id, name, base, protocol, models)| {
            let default_model_id = models[0].0.into();
            let models = models
                .into_iter()
                .map(|(model, label)| {
                    let profile = ProviderProfile {
                        id: "aa".into(),
                        name: name.into(),
                        protocol,
                        base_url: base.into(),
                        model: model.into(),
                        credential_id: "bb".into(),
                    };
                    PresetModel {
                        model_id: model.into(),
                        label: label.into(),
                        protocol,
                        capabilities: capabilities(&profile),
                        defaults: ModelParameters {
                            max_output_tokens: Some(8192),
                            ..Default::default()
                        },
                    }
                })
                .collect();
            ProviderPreset {
                id: id.into(),
                name: name.into(),
                base_url: base.into(),
                protocol,
                default_model_id,
                models,
                verified_at: "2026-11-05".into(),
            }
        })
        .collect()
}
