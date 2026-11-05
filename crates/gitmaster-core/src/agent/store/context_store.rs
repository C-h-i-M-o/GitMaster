use super::*;
use crate::agent::context::{
    checkpoint_error, is_safe_checkpoint_boundary, source_hash, valid_sha256, ContextCheckpoint,
};

const CONTEXT_CHECKPOINT_FILE: &str = "context-checkpoint.json";
const MAX_CONTEXT_SUMMARY_BYTES: usize = 16 * 1024;

impl AgentStore {
    /// 读取并验证聊天摘要快照；损坏或过期时仅忽略快照，不修改原始历史。
    pub fn read_context_checkpoint(&self, chat_id: &str) -> AgentResult<Option<ContextCheckpoint>> {
        valid_id(chat_id)?;
        let _guard = self.lock()?;
        let _session = self.session(chat_id)?;
        let path = self.chat_dir(chat_id)?.join(CONTEXT_CHECKPOINT_FILE);
        if regular_or_missing(&path).is_err() || !path.exists() {
            return Ok(None);
        }
        let checkpoint: ContextCheckpoint = match read_json(&path) {
            Ok(value) => value,
            Err(_) => return Ok(None),
        };
        let (events, _) = self.events(chat_id)?;
        if !checkpoint_is_valid(&checkpoint, chat_id, &events) {
            return Ok(None);
        }
        Ok(Some(checkpoint))
    }

    /// 仅保存与当前完整事件前缀匹配且不含凭据的摘要快照。
    pub fn save_context_checkpoint(&self, checkpoint: ContextCheckpoint) -> AgentResult<()> {
        valid_id(&checkpoint.chat_id)?;
        let _guard = self.lock()?;
        let _session = self.session(&checkpoint.chat_id)?;
        let (events, _) = self.events(&checkpoint.chat_id)?;
        if !checkpoint_is_valid(&checkpoint, &checkpoint.chat_id, &events) {
            return Err(checkpoint_error());
        }
        let path = self
            .chat_dir(&checkpoint.chat_id)?
            .join(CONTEXT_CHECKPOINT_FILE);
        atomic_json(&path, &checkpoint)
    }
}

/// 验证版本、隐私、哈希和安全轮次边界，拒绝来源不匹配的摘要。
fn checkpoint_is_valid(
    checkpoint: &ContextCheckpoint,
    chat_id: &str,
    events: &[AgentEvent],
) -> bool {
    checkpoint.version == 1
        && checkpoint.chat_id == chat_id
        && checkpoint.through_sequence > 0
        && checkpoint.summary.len() <= MAX_CONTEXT_SUMMARY_BYTES
        && !checkpoint.summary.trim().is_empty()
        && super::super::privacy::ensure_safe_text(&checkpoint.summary, None).is_ok()
        && valid_sha256(&checkpoint.source_hash)
        && valid_sha256(&checkpoint.model_fingerprint)
        && is_safe_checkpoint_boundary(events, checkpoint.through_sequence)
        && source_hash(events, checkpoint.through_sequence)
            .is_ok_and(|hash| hash == checkpoint.source_hash)
}
