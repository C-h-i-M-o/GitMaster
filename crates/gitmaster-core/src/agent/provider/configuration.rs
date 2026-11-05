use super::*;
use serde::{Deserialize, Serialize};

/// 只表示供应商明确支持的思考开关，默认行为用 Option::None 表达。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ThinkingMode {
    Enabled,
    Disabled,
}

/// 配置默认值与会话覆盖共用类型；空字段不强行覆盖供应商行为。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelParameters {
    pub context_limit: Option<u32>,
    pub max_output_tokens: Option<u32>,
    pub thinking_mode: Option<ThinkingMode>,
    pub reasoning_effort: Option<String>,
    pub thinking_budget_tokens: Option<u32>,
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
}

/// 不同服务的参数语义不能仅根据同名协议推断。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ParameterStyle {
    Deepseek,
    Openai,
    AnthropicAdaptive,
    AnthropicBudget,
    #[default]
    Generic,
}

/// 可核验能力；未知上限和工具支持不以零或 true 伪装。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelCapabilities {
    pub context_window: Option<u32>,
    pub max_output_tokens: Option<u32>,
    pub supported_efforts: Vec<String>,
    pub thinking_modes: Vec<ThinkingMode>,
    pub thinking_budget: bool,
    pub tool_support: Option<bool>,
    pub sampling_support: Option<bool>,
    pub source: String,
    pub verified_at: Option<String>,
    #[serde(default)]
    pub endpoint: Option<String>,
    pub parameter_style: ParameterStyle,
}
impl Default for ModelCapabilities {
    /// 自定义未知服务只提供本地上下文与协议输出上限，不猜测思考支持。
    fn default() -> Self {
        Self {
            context_window: None,
            max_output_tokens: None,
            supported_efforts: vec![],
            thinking_modes: vec![],
            thinking_budget: false,
            tool_support: None,
            sampling_support: None,
            source: "unknown".into(),
            verified_at: None,
            endpoint: None,
            parameter_style: ParameterStyle::Generic,
        }
    }
}
impl ModelParameters {
    /// 返回本地总预算、输出预留和安全余量，不声称未知模型具有该服务端上限。
    pub fn budget(&self, capabilities: &ModelCapabilities) -> AgentResult<(u32, u32, u32)> {
        let context = self
            .context_limit
            .or(capabilities.context_window)
            .unwrap_or(32768);
        let output = self.max_output_tokens.unwrap_or(4096);
        let reserve = 1024.max(context.div_ceil(20));
        if context < 2048
            || context > 16_777_216
            || output == 0
            || output > 1_048_576
            || capabilities.context_window.is_some_and(|max| context > max)
            || capabilities
                .max_output_tokens
                .is_some_and(|max| output > max)
            || u64::from(output) + u64::from(reserve) >= u64::from(context)
        {
            return Err(invalid_parameters(
                "上下文或输出限制超出模型上限，或没有足够的输入预算。",
            ));
        }
        Ok((context, output, reserve))
    }
    /// 在发送前拒绝不支持、会被供应商忽略和彼此冲突的参数。
    pub fn validate(
        &self,
        profile: &ProviderProfile,
        capabilities: &ModelCapabilities,
    ) -> AgentResult<()> {
        self.budget(capabilities)?;
        if self
            .reasoning_effort
            .as_ref()
            .is_some_and(|value| !capabilities.supported_efforts.contains(value))
            || self
                .thinking_mode
                .is_some_and(|value| !capabilities.thinking_modes.contains(&value))
        {
            return Err(invalid_parameters(
                "当前模型未确认支持该思考设置，请使用供应商默认或选择支持的档位。",
            ));
        }
        if let Some(budget) = self.thinking_budget_tokens {
            if !capabilities.thinking_budget
                || budget < 1024
                || budget >= self.max_output_tokens.unwrap_or(4096)
                || self.thinking_mode != Some(ThinkingMode::Enabled)
            {
                return Err(invalid_parameters(
                    "思考预算需由模型明确支持、启用思考，且小于最大输出限制。",
                ));
            }
        }
        if self.thinking_mode == Some(ThinkingMode::Disabled) && self.reasoning_effort.is_some() {
            return Err(invalid_parameters("关闭思考时不能同时指定思考强度。"));
        }
        if self
            .temperature
            .is_some_and(|value| !value.is_finite() || !(0.0..=2.0).contains(&value))
            || self
                .top_p
                .is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value))
        {
            return Err(invalid_parameters(
                "temperature 必须在 0～2，topP 必须在 0～1 范围内。",
            ));
        }
        if (self.temperature.is_some() || self.top_p.is_some())
            && capabilities.sampling_support != Some(true)
        {
            return Err(invalid_parameters(
                "当前模型未确认支持采样参数，请恢复供应商默认。",
            ));
        }
        if capabilities.parameter_style == ParameterStyle::Deepseek {
            let thinking = self.thinking_mode != Some(ThinkingMode::Disabled);
            if (thinking && self.temperature.is_some())
                || self.top_p.is_some_and(|p| {
                    if profile.model == "deepseek-flash" {
                        !thinking || p < 0.95
                    } else {
                        thinking
                    }
                })
            {
                return Err(invalid_parameters(
                    "当前 DeepSeek 模型和思考模式会忽略此采样值，请使用默认值。",
                ));
            }
        }
        if capabilities.parameter_style == ParameterStyle::Openai
            && self
                .reasoning_effort
                .as_deref()
                .is_some_and(|effort| effort != "none")
            && (self.temperature.is_some() || self.top_p.is_some())
        {
            return Err(invalid_parameters("启用推理时不能同时覆盖采样参数。"));
        }
        Ok(())
    }
}

/// 统一使用不回显用户配置的安全错误。
pub(crate) fn invalid_parameters(message: &str) -> AgentError {
    AgentError::new("AGENT_MODEL_PARAMETERS", message)
}
