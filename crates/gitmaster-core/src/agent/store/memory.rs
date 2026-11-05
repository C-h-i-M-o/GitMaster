use super::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

const MEMORY_VERSION: u8 = 1;
const MAX_MEMORIES: usize = 1000;
const MAX_MEMORY_BYTES: usize = 8 * 1024;
const MAX_TAGS: usize = 10;
const MAX_TAG_CHARS: usize = 64;
const MAX_SELECTED_MEMORIES: usize = 8;
const MAX_MEMORY_TOKENS: usize = 2048;
const MEMORY_ENTRY_OVERHEAD_BYTES: usize = 64;
const MAX_QUERY_TERMS: usize = 128;

/// 长期记忆的工作区或用户级作用域。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MemoryScope {
    Workspace,
    User,
}

/// 指向聊天中可展示的用户或助手消息，手工记忆则两个字段均为空。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemorySource {
    pub chat_id: Option<String>,
    pub sequence: Option<u64>,
}

/// 已持久化记忆及其并发修订号和时间。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemoryEntry {
    pub id: String,
    pub scope: MemoryScope,
    pub workspace_id: Option<String>,
    pub text: String,
    pub tags: Vec<String>,
    pub source: MemorySource,
    pub enabled: bool,
    pub revision: u64,
    pub created_at: u64,
    pub updated_at: u64,
}

/// 新增或按期望修订号更新的记忆内容。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemoryInput {
    pub id: Option<String>,
    pub expected_revision: Option<u64>,
    pub scope: MemoryScope,
    pub workspace_id: Option<String>,
    pub text: String,
    pub tags: Vec<String>,
    pub source: MemorySource,
    pub enabled: bool,
}

/// 稳定说明本轮为何没有选中一条候选记忆。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum MemoryExcludedReason {
    Disabled,
    UserExcluded,
    Budget,
    NoMatch,
}

/// 本轮未选中的记忆及其稳定原因。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemoryExcluded {
    pub id: String,
    pub reason: MemoryExcludedReason,
}

/// 已选记忆、透明排除列表及保守但不精确的预算估算。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemorySelection {
    pub selected: Vec<MemoryEntry>,
    pub excluded: Vec<MemoryExcluded>,
    pub estimated_tokens: usize,
}

/// 严格版本化的本地记忆文档。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MemoryDocument {
    version: u8,
    entries: Vec<MemoryEntry>,
}

impl Default for MemoryDocument {
    /// 创建空的首版记忆文档。
    fn default() -> Self {
        Self {
            version: MEMORY_VERSION,
            entries: Vec::new(),
        }
    }
}

impl AgentStore {
    /// 列出指定工作区的记忆；用户级记忆必须显式请求。
    pub fn list_memories(
        &self,
        workspace_id: &str,
        include_user: bool,
    ) -> AgentResult<Vec<MemoryEntry>> {
        ensure_workspace_id(workspace_id)?;
        let _guard = self.lock()?;
        let document = self.read_memory_document()?;
        let mut entries: Vec<_> = document
            .entries
            .into_iter()
            .filter(|entry| match entry.scope {
                MemoryScope::Workspace => entry.workspace_id.as_deref() == Some(workspace_id),
                MemoryScope::User => include_user,
            })
            .collect();
        entries.sort_by(|left, right| {
            right
                .updated_at
                .cmp(&left.updated_at)
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(entries)
    }

    /// 新增或按 revision 乐观锁更新一条记忆，并原子发布整个文档。
    pub fn save_memory(&self, mut input: MemoryInput) -> AgentResult<MemoryEntry> {
        validate_memory_input(&input)?;
        let _guard = self.lock()?;
        let mut document = self.read_memory_document()?;
        validate_memory_source(self, &input)?;

        let now = timestamp();
        let entry = if let Some(id) = input.id.take() {
            valid_id(&id)?;
            let expected = input.expected_revision.ok_or_else(memory_revision_error)?;
            let Some(existing) = document.entries.iter_mut().find(|entry| entry.id == id) else {
                return Err(memory_missing_error());
            };
            if existing.revision != expected {
                return Err(memory_revision_error());
            }
            existing.scope = input.scope;
            existing.workspace_id = input.workspace_id;
            existing.text = input.text;
            existing.tags = input.tags;
            existing.source = input.source;
            existing.enabled = input.enabled;
            existing.revision = existing.revision.saturating_add(1);
            existing.updated_at = now;
            existing.clone()
        } else {
            if input.expected_revision.is_some() {
                return Err(memory_revision_error());
            }
            if document.entries.len() >= MAX_MEMORIES {
                return Err(AgentError::new(
                    "AGENT_MEMORY_LIMIT",
                    "已达到本地记忆条目上限。",
                ));
            }
            let entry = MemoryEntry {
                id: new_id(),
                scope: input.scope,
                workspace_id: input.workspace_id,
                text: input.text,
                tags: input.tags,
                source: input.source,
                enabled: input.enabled,
                revision: 1,
                created_at: now,
                updated_at: now,
            };
            document.entries.push(entry.clone());
            entry
        };
        self.write_memory_document(&document)?;
        Ok(entry)
    }

    /// 删除匹配期望 revision 的记忆，使后续读取和检索立即不可见。
    pub fn delete_memory(&self, id: &str, expected_revision: u64) -> AgentResult<()> {
        valid_id(id)?;
        let _guard = self.lock()?;
        let mut document = self.read_memory_document()?;
        let Some(index) = document.entries.iter().position(|entry| entry.id == id) else {
            return Err(memory_missing_error());
        };
        if document.entries[index].revision != expected_revision {
            return Err(memory_revision_error());
        }
        document.entries.remove(index);
        self.write_memory_document(&document)
    }

    /// 按正文和标签检索本地记忆，返回最多八条及预算排除原因。
    pub fn select_memories(
        &self,
        workspace_id: &str,
        include_user: bool,
        query: &str,
        excluded_ids: &[String],
        budget_tokens: usize,
    ) -> AgentResult<MemorySelection> {
        ensure_workspace_id(workspace_id)?;
        if query.chars().count() > 200 {
            return Err(AgentError::new(
                "AGENT_MEMORY_QUERY",
                "记忆检索内容不能超过 200 个字符。",
            ));
        }
        let excluded_ids: HashSet<_> = excluded_ids
            .iter()
            .map(|id| {
                valid_id(id)?;
                Ok(id.as_str())
            })
            .collect::<AgentResult<_>>()?;
        let _guard = self.lock()?;
        let document = self.read_memory_document()?;
        let mut selection = MemorySelection::default();
        let terms = query_terms(query);
        let mut candidates = Vec::new();

        for entry in document
            .entries
            .into_iter()
            .filter(|entry| match entry.scope {
                MemoryScope::Workspace => entry.workspace_id.as_deref() == Some(workspace_id),
                MemoryScope::User => include_user,
            })
        {
            if !entry.enabled {
                selection
                    .excluded
                    .push(excluded(&entry, MemoryExcludedReason::Disabled));
                continue;
            }
            if excluded_ids.contains(entry.id.as_str()) {
                selection
                    .excluded
                    .push(excluded(&entry, MemoryExcludedReason::UserExcluded));
                continue;
            }
            let score = match_terms(&entry, &terms);
            if !terms.is_empty() && score == 0 {
                selection
                    .excluded
                    .push(excluded(&entry, MemoryExcludedReason::NoMatch));
                continue;
            }
            candidates.push((score, entry));
        }

        candidates.sort_by(|(left_score, left), (right_score, right)| {
            right_score
                .cmp(left_score)
                .then_with(|| right.updated_at.cmp(&left.updated_at))
                .then_with(|| left.id.cmp(&right.id))
        });
        let budget = budget_tokens.min(MAX_MEMORY_TOKENS);
        for (_, entry) in candidates {
            let cost = estimated_memory_tokens(&entry);
            if selection.selected.len() >= MAX_SELECTED_MEMORIES
                || selection.estimated_tokens.saturating_add(cost) > budget
            {
                selection
                    .excluded
                    .push(excluded(&entry, MemoryExcludedReason::Budget));
                continue;
            }
            selection.estimated_tokens += cost;
            selection.selected.push(entry);
        }
        Ok(selection)
    }

    /// 校验、读取严格版本文档；没有文件时返回空版本。
    fn read_memory_document(&self) -> AgentResult<MemoryDocument> {
        let memory_dir = self.root.join("memory");
        match std::fs::symlink_metadata(&memory_dir) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(MemoryDocument::default());
            }
            _ => return Err(AgentError::corrupt()),
        }
        let path = memory_dir.join("entries.json");
        regular_or_missing(&path)?;
        if !path.exists() {
            return Ok(MemoryDocument::default());
        }
        let document: MemoryDocument = read_json(&path)?;
        validate_memory_document(&document)?;
        Ok(document)
    }

    /// 在已验证的记忆目录中原子发布首版文档。
    fn write_memory_document(&self, document: &MemoryDocument) -> AgentResult<()> {
        validate_memory_document(document)?;
        let memory_dir = self.root.join("memory");
        match std::fs::symlink_metadata(&memory_dir) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                std::fs::create_dir(&memory_dir).map_err(AgentError::storage)?;
                directory(&memory_dir)?;
            }
            _ => return Err(AgentError::corrupt()),
        }
        atomic_json(&memory_dir.join("entries.json"), document)
    }
}

/// 校验工作区身份只能是默认 ID 或 SHA-256 文件夹 ID。
fn ensure_workspace_id(workspace_id: &str) -> AgentResult<()> {
    let valid = workspace_id == "default"
        || workspace_id.strip_prefix("folder-").is_some_and(|hash| {
            hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
        });
    if valid {
        Ok(())
    } else {
        Err(AgentError::new(
            "AGENT_MEMORY_WORKSPACE",
            "当前工作区标识无效，请重新打开工作区后重试。",
        ))
    }
}

/// 校验记忆作用域、长度、标签和敏感信息。
fn validate_memory_input(input: &MemoryInput) -> AgentResult<()> {
    match (input.scope, input.workspace_id.as_deref()) {
        (MemoryScope::Workspace, Some(workspace_id)) => ensure_workspace_id(workspace_id)?,
        (MemoryScope::User, None) => {}
        _ => {
            return Err(AgentError::new(
                "AGENT_MEMORY_SCOPE",
                "用户记忆不能绑定工作区，工作区记忆必须绑定有效工作区。",
            ));
        }
    }
    if input.text.trim().is_empty() || input.text.len() > MAX_MEMORY_BYTES {
        return Err(AgentError::new(
            "AGENT_MEMORY_TEXT",
            "记忆正文须为非空内容且不超过 8 KiB。",
        ));
    }
    privacy::ensure_safe_text(&input.text, None)?;
    if input.tags.len() > MAX_TAGS {
        return Err(AgentError::new(
            "AGENT_MEMORY_TAGS",
            "每条记忆最多可设置 10 个标签。",
        ));
    }
    for tag in &input.tags {
        if tag.trim().is_empty()
            || tag.chars().count() > MAX_TAG_CHARS
            || tag.chars().any(char::is_control)
        {
            return Err(AgentError::new(
                "AGENT_MEMORY_TAGS",
                "记忆标签须为 1～64 个可见字符。",
            ));
        }
        privacy::ensure_safe_text(tag, None)?;
    }
    match (&input.source.chat_id, input.source.sequence) {
        (None, None) | (Some(_), Some(_)) => Ok(()),
        _ => Err(AgentError::new(
            "AGENT_MEMORY_SOURCE",
            "记忆来源聊天和事件序号必须同时提供。",
        )),
    }
}

/// 在保存事务锁内验证来源聊天、工作区和可展示消息事件。
fn validate_memory_source(store: &AgentStore, input: &MemoryInput) -> AgentResult<()> {
    let (Some(chat_id), Some(sequence)) = (&input.source.chat_id, input.source.sequence) else {
        return Ok(());
    };
    valid_id(chat_id)?;
    let chat = store.session(chat_id)?;
    if input.scope == MemoryScope::Workspace
        && input.workspace_id.as_deref() != Some(chat.workspace_id.as_str())
    {
        return Err(AgentError::new(
            "AGENT_MEMORY_SOURCE",
            "来源聊天不属于当前记忆工作区。",
        ));
    }
    let (events, _) = store.events(chat_id)?;
    if !events.iter().any(|event| {
        event.sequence == sequence
            && matches!(
                &event.payload,
                EventKind::UserMessage { .. }
                    | EventKind::AssistantMessage { .. }
                    | EventKind::MemorySuggested { .. }
            )
    }) {
        return Err(AgentError::new(
            "AGENT_MEMORY_SOURCE",
            "记忆来源必须是聊天中的用户或助手可展示正文。",
        ));
    }
    Ok(())
}

/// 验证读取到的磁盘文档版本、唯一标识和每条记忆的内容边界。
fn validate_memory_document(document: &MemoryDocument) -> AgentResult<()> {
    if document.version != MEMORY_VERSION || document.entries.len() > MAX_MEMORIES {
        return Err(AgentError::corrupt());
    }
    let mut ids = HashSet::new();
    for entry in &document.entries {
        valid_id(&entry.id).map_err(|_| AgentError::corrupt())?;
        if !ids.insert(entry.id.as_str())
            || entry.revision == 0
            || entry.created_at > entry.updated_at
            || entry.source.chat_id.is_some() != entry.source.sequence.is_some()
        {
            return Err(AgentError::corrupt());
        }
        let input = MemoryInput {
            id: Some(entry.id.clone()),
            expected_revision: Some(entry.revision),
            scope: entry.scope,
            workspace_id: entry.workspace_id.clone(),
            text: entry.text.clone(),
            tags: entry.tags.clone(),
            source: entry.source.clone(),
            enabled: entry.enabled,
        };
        validate_memory_input(&input).map_err(|_| AgentError::corrupt())?;
    }
    Ok(())
}

/// 将 ASCII 词/路径片段与连续中文双字拆成有界且去重的查询词。
fn query_terms(query: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let mut seen = HashSet::new();
    let mut ascii = String::new();
    let mut chinese = Vec::new();

    let push = |term: String, terms: &mut Vec<String>, seen: &mut HashSet<String>| {
        if !term.is_empty() && terms.len() < MAX_QUERY_TERMS && seen.insert(term.clone()) {
            terms.push(term);
        }
    };
    let flush_ascii = |ascii: &mut String, terms: &mut Vec<String>, seen: &mut HashSet<String>| {
        if !ascii.is_empty() {
            push(std::mem::take(ascii).to_lowercase(), terms, seen);
        }
    };
    let flush_chinese =
        |chinese: &mut Vec<char>, terms: &mut Vec<String>, seen: &mut HashSet<String>| {
            if chinese.len() == 1 {
                if let Some(character) = chinese.first() {
                    push(character.to_string(), terms, seen);
                }
            } else {
                for pair in chinese.windows(2) {
                    push(pair.iter().collect(), terms, seen);
                    if terms.len() >= MAX_QUERY_TERMS {
                        break;
                    }
                }
            }
            chinese.clear();
        };

    for character in query.chars() {
        if character.is_ascii_alphanumeric() || character == '_' {
            flush_chinese(&mut chinese, &mut terms, &mut seen);
            ascii.push(character);
        } else if is_chinese(character) {
            flush_ascii(&mut ascii, &mut terms, &mut seen);
            chinese.push(character);
        } else {
            flush_ascii(&mut ascii, &mut terms, &mut seen);
            flush_chinese(&mut chinese, &mut terms, &mut seen);
        }
        if terms.len() >= MAX_QUERY_TERMS {
            break;
        }
    }
    flush_ascii(&mut ascii, &mut terms, &mut seen);
    flush_chinese(&mut chinese, &mut terms, &mut seen);
    terms
}

/// 判断字符是否属于常见中日韩统一表意文字区。
fn is_chinese(character: char) -> bool {
    matches!(character as u32, 0x3400..=0x4dbf | 0x4e00..=0x9fff | 0xf900..=0xfaff)
}

/// 统计查询词对正文和标签的字面命中数。
fn match_terms(entry: &MemoryEntry, terms: &[String]) -> usize {
    let text = entry.text.to_lowercase();
    terms
        .iter()
        .filter(|term| {
            text.contains(term.as_str())
                || entry
                    .tags
                    .iter()
                    .any(|tag| tag.to_lowercase().contains(term.as_str()))
        })
        .count()
}

/// 以正文/标签 UTF-8 字节和固定序列化开销保守估算，不承诺等于模型 token 数。
fn estimated_memory_tokens(entry: &MemoryEntry) -> usize {
    entry
        .text
        .len()
        .saturating_add(entry.tags.iter().map(String::len).sum::<usize>())
        .saturating_add(MEMORY_ENTRY_OVERHEAD_BYTES)
        .max(1)
}

/// 创建包含稳定排除原因的结果项。
fn excluded(entry: &MemoryEntry, reason: MemoryExcludedReason) -> MemoryExcluded {
    MemoryExcluded {
        id: entry.id.clone(),
        reason,
    }
}

/// 构造找不到记忆时的稳定错误。
fn memory_missing_error() -> AgentError {
    AgentError::new("AGENT_MEMORY_MISSING", "记忆不存在或已被删除。")
}

/// 构造乐观锁冲突时的稳定错误。
fn memory_revision_error() -> AgentError {
    AgentError::new(
        "AGENT_MEMORY_REVISION",
        "记忆已被其他操作更新，请刷新后重试。",
    )
}
