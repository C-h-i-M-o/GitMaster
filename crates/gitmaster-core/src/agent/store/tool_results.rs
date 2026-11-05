use super::*;
use serde::Serialize;

const MAX_TOOL_RESULT_PAGE_BYTES: usize = 8 * 1024;

/// 同一聊天单条工具结果的 UTF-8 安全字节分页。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolResultPage {
    pub sequence: u64,
    pub text: String,
    pub offset: usize,
    pub next_offset: Option<usize>,
    pub total_bytes: usize,
    pub truncated: bool,
}

impl AgentStore {
    /// 只读取指定聊天中已持久化的 ToolResult，不接受路径或任意事件内容。
    pub fn read_tool_result(
        &self,
        chat_id: &str,
        sequence: u64,
        offset: usize,
        limit: usize,
    ) -> AgentResult<ToolResultPage> {
        valid_id(chat_id)?;
        if sequence == 0 || limit == 0 || limit > MAX_TOOL_RESULT_PAGE_BYTES {
            return Err(tool_result_error());
        }
        let _guard = self.lock()?;
        let _session = self.session(chat_id)?;
        let (events, _) = self.events(chat_id)?;
        let event = events
            .iter()
            .find(|event| event.sequence == sequence)
            .ok_or_else(tool_result_error)?;
        let EventKind::ToolResult { result, .. } = &event.payload else {
            return Err(tool_result_error());
        };
        let text = tool_result_text(result)?;
        let total_bytes = text.len();
        if offset > total_bytes || !text.is_char_boundary(offset) {
            return Err(tool_result_offset_error());
        }
        let mut end = total_bytes.min(offset.saturating_add(limit));
        while end > offset && !text.is_char_boundary(end) {
            end -= 1;
        }
        if end == offset && offset < total_bytes {
            return Err(tool_result_offset_error());
        }
        let next_offset = (end < total_bytes).then_some(end);
        Ok(ToolResultPage {
            sequence,
            text: text[offset..end].to_owned(),
            offset,
            next_offset,
            total_bytes,
            truncated: next_offset.is_some(),
        })
    }
}

/// 提取工具结果向模型展示的文本，结构化结果使用稳定 JSON 表示。
fn tool_result_text(result: &serde_json::Value) -> AgentResult<String> {
    if let Some(text) = result.get("text").and_then(serde_json::Value::as_str) {
        return Ok(text.to_owned());
    }
    if let Some(text) = result.as_str() {
        return Ok(text.to_owned());
    }
    serde_json::to_string(result).map_err(AgentError::storage)
}

/// 不回显聊天数据的分页参数或事件错误。
fn tool_result_error() -> AgentError {
    AgentError::new(
        "AGENT_TOOL_RESULT_MISSING",
        "指定聊天中没有可读取的工具结果。",
    )
}

/// 明确拒绝越界或非 UTF-8 字符边界偏移。
fn tool_result_offset_error() -> AgentError {
    AgentError::new(
        "AGENT_TOOL_RESULT_OFFSET",
        "工具结果分页偏移无效，必须位于 UTF-8 字符边界。",
    )
}
