use super::*;
use regex::Regex;
use std::sync::LazyLock;

/// 阻止明显凭据进入会话；这是已知模式检测，不是任意秘密识别器。
static CREDENTIALS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
    r"(?im)-----BEGIN (?:[A-Z ]+ )?PRIVATE KEY-----|",
    r"\b(?:sk-[a-zA-Z0-9_-]{16,}|gh[pousr]_[a-zA-Z0-9]{20,}|github_pat_[a-zA-Z0-9_]{20,})\b|",
    r"https?://[^\s/]+@|",
    r"(?:[?&](?:api[_-]?key|access[_-]?token|token|password|signature|secret)=)[^\s&#]+|",
    r#"(?:\b(?:[a-z0-9]+[_-])*(?:api[_-]?key|access[_-]?token|token|client[_-]?secret|secret|password|authorization)\b[\"']?\s*[:=]\s*[\"']?)[^\s\"']{6,}"#
)).expect("固定凭据识别表达式有效")
});

/// 输入和输出在持久化/模型请求前核验；已配置 Key 始终按精确值拦截。
pub fn ensure_safe_text(text: &str, key: Option<&str>) -> AgentResult<()> {
    if CREDENTIALS.is_match(text)
        || key.is_some_and(|value| !value.is_empty() && text.contains(value))
    {
        Err(AgentError::new(
            "AGENT_SENSITIVE_CONTENT",
            "内容含疑似凭据或私钥，未发送或保存；请移除敏感内容后重试。",
        ))
    } else {
        Ok(())
    }
}
/// 递归检查字符串，保留正常协议不透明字段的原始结构。
pub fn ensure_safe_value(value: &serde_json::Value, key: Option<&str>) -> AgentResult<()> {
    match value {
        serde_json::Value::String(text) => ensure_safe_text(text, key),
        serde_json::Value::Array(values) => {
            for value in values {
                ensure_safe_value(value, key)?;
            }
            Ok(())
        }
        serde_json::Value::Object(values) => {
            for (name, value) in values {
                ensure_safe_text(name, key)?;
                if let Some(text) = value.as_str() {
                    ensure_safe_text(&format!("{name}={text}"), key)?;
                }
                ensure_safe_value(value, key)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}
