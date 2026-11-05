use gitmaster_core::agent::{
    provider::{
        build_request_with_parameters,
        catalog::{capabilities, effective_capabilities},
    },
    *,
};

/// 固定官方端点，用于验证协议映射而不请求真实供应商。
fn profile(protocol: ProviderProtocol, base: &str, model: &str) -> ProviderProfile {
    ProviderProfile {
        id: "aa".into(),
        name: "测试".into(),
        protocol,
        base_url: base.into(),
        model: model.into(),
        credential_id: "bb".into(),
    }
}

/// 三种协议的输出限制和原生思考档位必须写进真正的请求体。
#[test]
fn parameters_reach_each_protocol_without_invented_fields() {
    for (protocol, base, model, effort) in [
        (
            ProviderProtocol::ChatCompletions,
            "https://api.deepseek.com",
            "deepseek-flash",
            "max",
        ),
        (
            ProviderProtocol::Responses,
            "https://api.openai.com/v1",
            "gpt-5.4",
            "xhigh",
        ),
        (
            ProviderProtocol::AnthropicMessages,
            "https://api.anthropic.com/v1",
            "claude-sonnet-5-5",
            "medium",
        ),
    ] {
        let p = profile(protocol, base, model);
        let parameters = ModelParameters {
            max_output_tokens: Some(8192),
            reasoning_effort: Some(effort.into()),
            ..Default::default()
        };
        let body = build_request_with_parameters(&p, &[], &[], "规则", &parameters).unwrap();
        match protocol {
            ProviderProtocol::ChatCompletions => {
                assert_eq!(body["max_tokens"], 8192);
                assert_eq!(body["reasoning_effort"], effort);
            }
            ProviderProtocol::Responses => {
                assert_eq!(body["max_output_tokens"], 8192);
                assert_eq!(body["reasoning"]["effort"], effort);
            }
            ProviderProtocol::AnthropicMessages => {
                assert_eq!(body["max_tokens"], 8192);
                assert_eq!(body["output_config"]["effort"], effort);
            }
        }
        assert!(body.get("contextLimit").is_none());
    }
}

/// 默认参数省略推理覆盖；未知服务仍可设置本地预算和输出上限。
#[test]
fn unknown_capabilities_do_not_silently_enable_reasoning() {
    let p = profile(
        ProviderProtocol::Responses,
        "https://custom.example/v1",
        "unknown",
    );
    let body =
        build_request_with_parameters(&p, &[], &[], "规则", &ModelParameters::default()).unwrap();
    assert!(body.get("reasoning").is_none());
    assert!(capabilities(&p).context_window.is_none());
    let invalid = ModelParameters {
        reasoning_effort: Some("high".into()),
        ..Default::default()
    };
    assert_eq!(
        build_request_with_parameters(&p, &[], &[], "规则", &invalid)
            .unwrap_err()
            .code,
        "AGENT_MODEL_PARAMETERS"
    );
}

/// 已知上限、输入预留和思考采样冲突在发出请求前拒绝。
#[test]
fn invalid_limits_and_ignored_sampling_are_rejected() {
    let p = profile(
        ProviderProtocol::ChatCompletions,
        "https://api.deepseek.com",
        "deepseek-flash",
    );
    for invalid in [
        ModelParameters {
            context_limit: Some(256),
            max_output_tokens: Some(8192),
            ..Default::default()
        },
        ModelParameters {
            max_output_tokens: Some(500_000),
            ..Default::default()
        },
        ModelParameters {
            temperature: Some(0.5),
            ..Default::default()
        },
        ModelParameters {
            reasoning_effort: Some("medium".into()),
            ..Default::default()
        },
    ] {
        assert!(build_request_with_parameters(&p, &[], &[], "规则", &invalid).is_err());
    }
}

/// 官方能力只适用于官方 scheme、端口、基址路径及当前协议对应的完整端点。
#[test]
fn preset_capabilities_require_the_exact_normalized_endpoint() {
    let known = [
        (
            ProviderProtocol::Responses,
            "https://api.openai.com/v1",
            "gpt-5.4",
        ),
        (
            ProviderProtocol::Responses,
            "https://api.openai.com/v1/",
            "gpt-5.4",
        ),
        (
            ProviderProtocol::Responses,
            "https://api.openai.com/v1/responses/",
            "gpt-5.4",
        ),
        (
            ProviderProtocol::ChatCompletions,
            "https://api.openai.com/v1/chat/completions",
            "gpt-5.4",
        ),
        (
            ProviderProtocol::ChatCompletions,
            "https://api.deepseek.com/chat/completions/",
            "deepseek-flash",
        ),
        (
            ProviderProtocol::AnthropicMessages,
            "https://api.anthropic.com/v1/messages/",
            "claude-sonnet-5-5",
        ),
    ];
    for (protocol, base, model) in known {
        assert_eq!(
            capabilities(&profile(protocol, base, model)).source,
            "preset",
            "{base}"
        );
    }

    let unknown = [
        (
            ProviderProtocol::Responses,
            "https://api.openai.com:8443/v1",
            "gpt-5.4",
        ),
        (
            ProviderProtocol::Responses,
            "https://api.openai.com/v2",
            "gpt-5.4",
        ),
        (
            ProviderProtocol::Responses,
            "http://api.openai.com/v1",
            "gpt-5.4",
        ),
        (
            ProviderProtocol::ChatCompletions,
            "https://api.deepseek.com:9443",
            "deepseek-flash",
        ),
        (
            ProviderProtocol::Responses,
            "https://api.deepseek.com",
            "deepseek-flash",
        ),
        (
            ProviderProtocol::AnthropicMessages,
            "https://api.anthropic.com/proxy/v1",
            "claude-sonnet-5-5",
        ),
        (
            ProviderProtocol::ChatCompletions,
            "https://api.anthropic.com/v1",
            "claude-sonnet-5-5",
        ),
    ];
    for (protocol, base, model) in unknown {
        assert_eq!(
            capabilities(&profile(protocol, base, model)).source,
            "unknown",
            "{base}"
        );
    }
}

/// 已发现能力只在实际 endpoint 完全匹配时优先于官方预设回退。
#[test]
fn discovered_capabilities_remain_bound_to_their_exact_endpoint() {
    let custom = profile(
        ProviderProtocol::Responses,
        "https://custom.example/v3",
        "model-x",
    );
    let discovered = ModelCapabilities {
        context_window: Some(64_000),
        source: "discovery".into(),
        endpoint: Some(custom.endpoint().unwrap()),
        ..Default::default()
    };
    assert_eq!(
        effective_capabilities(&custom, &discovered).context_window,
        Some(64_000)
    );

    let changed = profile(
        ProviderProtocol::Responses,
        "https://custom.example/v4",
        "model-x",
    );
    assert_eq!(
        effective_capabilities(&changed, &discovered).source,
        "unknown"
    );
}
