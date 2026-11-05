use super::*;
use sha2::{Digest, Sha256};

impl AgentStore {
    /// 将已核验前后快照原子写入当前聊天内部，随后由运行事件引用。
    pub fn save_file_change(
        &self,
        chat_id: &str,
        content: &FileChangeContent,
    ) -> AgentResult<FileChangeArtifact> {
        validate_change(content)?;
        let _guard = self.lock()?;
        let directory_path = self.chat_dir(chat_id)?.join("changes");
        if !directory_path.exists() {
            fs::create_dir(&directory_path).map_err(AgentError::storage)?;
        }
        directory(&directory_path)?;
        let artifact_id = new_id();
        atomic_json(&directory_path.join(format!("{artifact_id}.json")), content)?;
        Ok(FileChangeArtifact {
            artifact_id,
            digest: change_digest(content)?,
            path: content.path.clone(),
            before_version: content.before_version.clone(),
            after_version: content.after_version.clone(),
            created: content.before_content.is_none(),
        })
    }

    /// 只允许读取本聊天事件实际引用的工件，并核验内容哈希，不能传任意文件路径。
    pub fn read_file_change(
        &self,
        chat_id: &str,
        artifact_id: &str,
    ) -> AgentResult<FileChangeContent> {
        valid_id(artifact_id)?;
        let _guard = self.lock()?;
        let (events, _) = self.events(chat_id)?;
        let artifact = events
            .iter()
            .find_map(|event| match &event.payload {
                EventKind::FileChanged { change, .. } if change.artifact_id == artifact_id => {
                    Some(change)
                }
                _ => None,
            })
            .ok_or_else(|| {
                AgentError::new(
                    "AGENT_CHANGE_MISSING",
                    "此聊天没有该修改快照，或快照已被删除。",
                )
            })?;
        let directory_path = self.chat_dir(chat_id)?.join("changes");
        directory(&directory_path)?;
        let content: FileChangeContent =
            read_json(&directory_path.join(format!("{artifact_id}.json")))?;
        validate_change(&content)?;
        if change_digest(&content)? != artifact.digest {
            return Err(AgentError::corrupt());
        }
        Ok(content)
    }
}
/// 工件仍遵守单文件文本与隐私限制，不能伪装未知结果为已核验修改。
fn validate_change(content: &FileChangeContent) -> AgentResult<()> {
    if !content.verified
        || content.path.is_empty()
        || content.path.len() > 4096
        || content.after_content.len() > 2 * 1024 * 1024
        || content
            .before_content
            .as_ref()
            .is_some_and(|text| text.len() > 2 * 1024 * 1024)
        || content.before_content.is_some() != content.before_version.is_some()
        || content.after_version.len() != 64
    {
        return Err(AgentError::new(
            "AGENT_CHANGE_INVALID",
            "修改快照缺少有效基线或核验结果。",
        ));
    }
    privacy::ensure_safe_value(
        &serde_json::to_value(content).map_err(AgentError::storage)?,
        None,
    )
}
/// 对严格解析后的快照生成稳定摘要，JSON空白变化不影响内容身份。
fn change_digest(content: &FileChangeContent) -> AgentResult<String> {
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(content).map_err(AgentError::storage)?)
    ))
}
