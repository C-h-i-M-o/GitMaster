use super::{AgentError, AgentResult, AgentStore, EventKind};
use serde::Serialize;

const MAX_SEARCH_RESULTS: usize = 100;
const MAX_QUERY_CHARS: usize = 200;

/// 单条聊天搜索命中；标题命中没有消息序号或正文片段。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChatSearchResult {
    pub chat_id: String,
    pub title: String,
    pub sequence: Option<u64>,
    pub snippet: Option<String>,
    pub updated_at: u64,
}

/// 搜索命中与当前筛选范围内无法读取的聊天。
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChatSearchResults {
    pub results: Vec<ChatSearchResult>,
    pub unavailable: Vec<String>,
}

impl AgentStore {
    /// 在指定工作区和归档状态内搜索标题及用户、助手可展示正文。
    pub fn search_chats(
        &self,
        workspace_id: &str,
        query: &str,
        archived: bool,
    ) -> AgentResult<ChatSearchResults> {
        let query = query.trim();
        let query_len = query.chars().count();
        if query_len == 0 || query_len > MAX_QUERY_CHARS {
            return Err(AgentError::new(
                "AGENT_SEARCH_QUERY",
                "搜索内容须为 1～200 个字符。",
            ));
        }
        if !valid_workspace_id(workspace_id) {
            return Err(AgentError::new(
                "AGENT_SEARCH_WORKSPACE",
                "当前工作区标识无效，请重新打开工作区后重试。",
            ));
        }

        let query_lower = query.to_lowercase();
        let _guard = self.lock()?;
        let chats = self.list_unlocked()?;
        let mut results = Vec::new();
        let mut unavailable = Vec::new();

        for chat in chats
            .chats
            .into_iter()
            .filter(|chat| chat.workspace_id.as_str() == workspace_id && chat.archived == archived)
        {
            if contains_case_insensitive(&chat.title, &query_lower)
                && results.len() < MAX_SEARCH_RESULTS
            {
                results.push(ChatSearchResult {
                    chat_id: chat.id.clone(),
                    title: chat.title.clone(),
                    sequence: None,
                    snippet: None,
                    updated_at: chat.updated_at,
                });
            }

            match self.events(&chat.id) {
                Ok((events, _)) => {
                    for event in events.into_iter().rev() {
                        let text = match event.payload {
                            EventKind::UserMessage { text }
                            | EventKind::AssistantMessage { text, .. } => text,
                            _ => continue,
                        };
                        let Some((start, end)) = find_case_insensitive(&text, &query_lower) else {
                            continue;
                        };
                        if results.len() < MAX_SEARCH_RESULTS {
                            results.push(ChatSearchResult {
                                chat_id: chat.id.clone(),
                                title: chat.title.clone(),
                                sequence: Some(event.sequence),
                                snippet: Some(make_snippet(&text, start, end)),
                                updated_at: chat.updated_at,
                            });
                        }
                    }
                }
                Err(_) => unavailable.push(chat.id),
            }
        }

        Ok(ChatSearchResults {
            results,
            unavailable,
        })
    }
}

/// 只接受默认工作区或由 SHA-256 仓库身份生成的工作区 ID。
fn valid_workspace_id(workspace_id: &str) -> bool {
    if workspace_id == "default" {
        return true;
    }
    let Some(hash) = workspace_id.strip_prefix("folder-") else {
        return false;
    };
    hash.len() == 64
        && hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// 对显示文本执行大小写不敏感的字面子串匹配。
fn contains_case_insensitive(text: &str, query_lower: &str) -> bool {
    text.to_lowercase().contains(query_lower)
}

/// 返回小写匹配区间对应的原文 UTF-8 字节区间。
fn find_case_insensitive(text: &str, query_lower: &str) -> Option<(usize, usize)> {
    let lowered = text.to_lowercase();
    let lower_start = lowered.find(query_lower)?;
    let lower_end = lower_start + query_lower.len();
    let mut folded_offset = 0;
    let mut original_start = None;

    for (start, character) in text.char_indices() {
        let folded_len = character.to_lowercase().map(char::len_utf8).sum::<usize>();
        let folded_end = folded_offset + folded_len;
        if original_start.is_none() && lower_start < folded_end {
            original_start = Some(start);
        }
        if lower_end <= folded_end {
            return Some((original_start?, start + character.len_utf8()));
        }
        folded_offset = folded_end;
    }
    None
}
/// 生成保留匹配上下文且不切断 UTF-8 字符的短片段。
fn make_snippet(text: &str, match_start: usize, match_end: usize) -> String {
    const CONTEXT_CHARS: usize = 60;

    let prefix_start = text[..match_start]
        .char_indices()
        .rev()
        .nth(CONTEXT_CHARS - 1)
        .map_or(0, |(index, _)| index);
    let suffix_end = text[match_end..]
        .char_indices()
        .nth(CONTEXT_CHARS - 1)
        .map_or(text.len(), |(index, character)| {
            match_end + index + character.len_utf8()
        });

    let mut snippet = String::new();
    if prefix_start > 0 {
        snippet.push('…');
    }
    snippet.push_str(&text[prefix_start..suffix_end]);
    if suffix_end < text.len() {
        snippet.push('…');
    }
    snippet
}
