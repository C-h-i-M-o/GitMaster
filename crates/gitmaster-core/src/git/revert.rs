//! 撤销提交使用独立会话与真实 REVERT_HEAD，不复用合并完成接口。
use super::{conflicts::ConflictSession, *};
use serde::Serialize;
mod abort;
pub(crate) mod anchor;
mod start;
pub use abort::prepare_abort;
pub use start::prepare_revert;

/// 从真实撤销状态重建的只读会话；写入仍须另行准备并确认一次性计划。
pub struct RevertSession {
    conflicts: ConflictSession,
}

/// 撤销状态使用独立会话 ID，observedHeadOid 仅表示本次读取时的 HEAD。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RevertState {
    pub repository_id: String,
    pub revert_session_id: String,
    pub target_commit_oid: String,
    pub observed_head_oid: String,
    pub unresolved_count: usize,
    pub files: Vec<ConflictFile>,
}

/// 复用三方文本数据结构，但不向撤销调用方暴露或接受 mergeSessionId。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RevertConflictDocument {
    pub revert_session_id: String,
    pub conflict_id: String,
    pub base: Option<String>,
    pub local: Option<String>,
    pub incoming: Option<String>,
    pub result: String,
    pub encoding: String,
    pub line_ending: String,
    pub fingerprint: String,
}

impl RevertSession {
    /// 最终保存使用撤销起点核验和单父提交，不能调用普通合并完成入口。
    pub fn prepare_finish(
        &self,
        coordinator: &super::coordinator::RepositoryCoordinator,
        message: &str,
    ) -> Result<WritePreview, OperationError> {
        super::conflicts::prepare_revert_finish(coordinator, &self.conflicts, message)
    }
    /// 已确认的撤销冲突使用独立保存操作，不调用合并完成或合并保存入口。
    pub fn prepare_save(
        &self,
        coordinator: &super::coordinator::RepositoryCoordinator,
        conflict_id: &str,
        fingerprint: &str,
        content: &str,
    ) -> Result<WritePreview, OperationError> {
        super::conflicts::prepare_revert_save(
            coordinator,
            &self.conflicts,
            conflict_id,
            fingerprint,
            content,
        )
    }
    /// 只接受真实单次 revert；普通 merge 及同时存在的其他操作均被拒绝。
    pub fn new(git: &GitExecutable, repo: &RepositoryHandle) -> Result<Self, OperationError> {
        Ok(Self {
            conflicts: ConflictSession::new_revert(git, repo)?,
        })
    }

    /// 将内部共用快照转换为撤销契约，保持操作语义与标识分离。
    pub fn state(&self) -> RevertState {
        let state = self.conflicts.state();
        RevertState {
            repository_id: state.repository_id,
            revert_session_id: state.merge_session_id,
            target_commit_oid: state.merge_head_oids[0].clone(),
            observed_head_oid: state.head_oid,
            unresolved_count: state.files.len(),
            files: state.files,
        }
    }

    /// 三方正文只按当前撤销会话签发的文件 ID 读取，读取前后都核对原生状态。
    pub fn read_document(
        &self,
        conflict_id: &str,
    ) -> Result<RevertConflictDocument, OperationError> {
        let document = self.conflicts.read_document(conflict_id)?;
        Ok(RevertConflictDocument {
            revert_session_id: document.merge_session_id,
            conflict_id: document.conflict_id,
            base: document.base,
            local: document.local,
            incoming: document.incoming,
            result: document.result,
            encoding: document.encoding,
            line_ending: document.line_ending,
            fingerprint: document.fingerprint,
        })
    }
}

#[cfg(test)]
mod tests;
