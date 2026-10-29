//! 固定恢复来源和文件身份，准备阶段只读取 Git 与应用私有临时目录。

use super::{
    repository::{next_id, query_until},
    write_guard as guard, *,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

const PAGE_SIZE: usize = 100;
const MAX_FILES: usize = 10_000;
const BUDGET: Duration = Duration::from_secs(120);

mod execute;
pub use execute::{RestoreContentPreview, RestoreFilePreview};

/// 用户明确选择的恢复来源；历史提交只接受完整对象 ID。
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RestoreSource {
    Index,
    Head,
    Commit { oid: String },
}

/// 从固定来源签发的普通文件候选，不能由前端路径构造。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreFileCandidate {
    pub file_id: String,
    pub path: String,
    pub source_oid: String,
}

/// 一页恢复文件；游标仅能用于产生它的来源会话。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreFilePage {
    pub source_id: String,
    pub source: RestoreSource,
    pub source_oid: Option<String>,
    pub files: Vec<RestoreFileCandidate>,
    pub next_cursor: Option<String>,
}

/// 来源列表固定到仓库、快照及索引字节，写准备时仍须重新校验。
pub struct RestoreSession {
    pub(crate) git: GitExecutable,
    pub(crate) repo: RepositoryHandle,
    pub(crate) snapshot_id: String,
    pub(crate) head: HeadState,
    pub(crate) index_identity: [u8; 32],
    pub(crate) source: RestoreSource,
    pub(crate) source_oid: Option<String>,
    id: String,
    entries: BTreeMap<String, guard::IndexEntry>,
    files: Vec<RestoreFileCandidate>,
}

impl RestoreSession {
    /// 将当前来源和选中文件冻结为一次性确认计划；准备不会覆盖工作文件。
    pub fn prepare(
        &self,
        coordinator: &super::coordinator::RepositoryCoordinator,
        state: &RepositoryState,
        ids: &[String],
    ) -> Result<WritePreview, OperationError> {
        execute::prepare(self, coordinator, state, ids)
    }
    /// 根据真实快照列出固定来源；历史查询不使用可变分支名称或用户 revspec。
    pub fn new(
        git: &GitExecutable,
        repo: &RepositoryHandle,
        state: &RepositoryState,
        source: RestoreSource,
    ) -> Result<Self, OperationError> {
        let deadline = Instant::now() + BUDGET;
        guard::validate_snapshot(git, repo, state, deadline)?;
        if !state.operations.is_empty() {
            return Err(OperationError::new("REPOSITORY_OPERATION_ACTIVE"));
        }
        let source_oid = match &source {
            RestoreSource::Index => None,
            RestoreSource::Head => match &state.head {
                HeadState::Branch { oid, .. } | HeadState::Detached { oid } => Some(oid.clone()),
                HeadState::Unborn { .. } => return Err(OperationError::new("HEAD_REQUIRED")),
            },
            RestoreSource::Commit { oid } => {
                if !valid_oid(oid) {
                    return Err(OperationError::new("INVALID_INPUT"));
                }
                let kind = query_until(
                    git,
                    &repo.root,
                    &["--no-replace-objects", "cat-file", "-t", oid],
                    64,
                    Some(deadline),
                )?;
                if kind != b"commit\n" {
                    return Err(OperationError::new("UNSUPPORTED_RECOVERY_TARGET"));
                }
                Some(oid.clone())
            }
        };
        let entries = match &source_oid {
            Some(oid) => guard::checkout_tree(git, repo, oid, deadline)?,
            None => guard::read_index(git, repo, None, deadline)?,
        };
        guard::validate_snapshot(git, repo, state, deadline)?;
        Self::from_entries(git, repo, state, source, source_oid, entries)
    }

    /// 为有界来源清单分配不可复用的文件身份，保持路径顺序稳定。
    fn from_entries(
        git: &GitExecutable,
        repo: &RepositoryHandle,
        state: &RepositoryState,
        source: RestoreSource,
        source_oid: Option<String>,
        mut entries: Vec<guard::IndexEntry>,
    ) -> Result<Self, OperationError> {
        if entries.len() > MAX_FILES {
            return Err(OperationError::new("OUTPUT_LIMIT"));
        }
        entries.sort_by(|left, right| left.path.cmp(&right.path));
        let mut files = Vec::with_capacity(entries.len());
        let mut by_id = BTreeMap::new();
        let mut previous: Option<String> = None;
        for entry in entries {
            guard::validate_recovery_path(&entry.path)?;
            if !matches!(entry.mode.as_str(), "100644" | "100755") || !valid_oid(&entry.oid) {
                return Err(OperationError::new("UNSUPPORTED_RECOVERY_TARGET"));
            }
            if previous.as_ref() == Some(&entry.path) {
                return Err(OperationError::new("PARSE_FAILED"));
            }
            previous = Some(entry.path.clone());
            let file_id = next_id();
            files.push(RestoreFileCandidate {
                file_id: file_id.clone(),
                path: entry.path.clone(),
                source_oid: entry.oid.clone(),
            });
            by_id.insert(file_id, entry);
        }
        Ok(Self {
            git: git.clone(),
            repo: repo.clone(),
            snapshot_id: state.snapshot_id.clone(),
            head: state.head.clone(),
            index_identity: state.index_identity,
            source,
            source_oid,
            id: next_id(),
            entries: by_id,
            files,
        })
    }

    /// 游标绑定来源会话和页边界，旧会话游标不能读取其他来源。
    pub fn page(&self, cursor: Option<&str>) -> Result<RestoreFilePage, OperationError> {
        let start = match cursor {
            None => 0,
            Some(cursor) => {
                let (id, offset) = cursor
                    .split_once(':')
                    .ok_or_else(|| OperationError::new("STALE_REQUEST"))?;
                let offset = offset
                    .parse::<usize>()
                    .map_err(|_| OperationError::new("STALE_REQUEST"))?;
                if id != self.id
                    || offset == 0
                    || offset % PAGE_SIZE != 0
                    || offset >= self.files.len()
                {
                    return Err(OperationError::new("STALE_REQUEST"));
                }
                offset
            }
        };
        let end = (start + PAGE_SIZE).min(self.files.len());
        Ok(RestoreFilePage {
            source_id: self.id.clone(),
            source: self.source.clone(),
            source_oid: self.source_oid.clone(),
            files: self.files[start..end].to_vec(),
            next_cursor: (end < self.files.len()).then(|| format!("{}:{end}", self.id)),
        })
    }

    /// 只解析当前来源签发的文件 ID，重复选择或任意路径均拒绝。
    pub(crate) fn selected(
        &self,
        ids: &[String],
    ) -> Result<Vec<guard::IndexEntry>, OperationError> {
        if ids.is_empty() {
            return Err(OperationError::new("EMPTY_SELECTION"));
        }
        let mut selected = BTreeMap::new();
        for id in ids {
            let entry = self
                .entries
                .get(id)
                .ok_or_else(|| OperationError::new("STALE_REQUEST"))?;
            if selected.insert(entry.path.clone(), entry.clone()).is_some() {
                return Err(OperationError::new("INVALID_INPUT"));
            }
        }
        Ok(selected.into_values().collect())
    }
}

/// 只接收 SHA-1 或 SHA-256 完整十六进制对象 ID，不接受路径和选项。
fn valid_oid(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests;
