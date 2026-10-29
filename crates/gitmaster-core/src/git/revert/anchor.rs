//! 单次撤销的持久起点；不保存凭据、正文、索引或可重放的写计划。
use super::*;
use crate::git::conflicts::{read_regular, FileBytes};
use cap_std::fs::{Dir, OpenOptions};
use serde::{Deserialize, Serialize};
use std::{io::Write, time::Instant};

pub(crate) const FILE: &str = "gitmaster-revert.json";

/// 原 HEAD 不能在重启或冲突保存后被当前 HEAD 替换。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RevertAnchor {
    version: u8,
    pub(super) branch: String,
    pub(super) original_head_oid: String,
    pub(super) target_commit_oid: String,
}

/// 捕获记录的文件身份和字节，拒绝内容相同但已被替换的记录。
pub(crate) struct AnchorRecord {
    pub(super) value: RevertAnchor,
    captured: FileBytes,
}

impl RevertAnchor {
    /// 仅从已验证的具名分支和完整 OID 构造起点。
    pub(super) fn new(head: &HeadState, target: &str) -> Result<Self, OperationError> {
        let HeadState::Branch { name, oid } = head else {
            return Err(OperationError::new("DETACHED_HEAD_WRITE_BLOCKED"));
        };
        let value = Self {
            version: 1,
            branch: name.clone(),
            original_head_oid: oid.clone(),
            target_commit_oid: target.into(),
        };
        value.validate()?;
        Ok(value)
    }

    /// 不覆盖既有起点，写入并同步成功后才允许启动 Git。
    pub(super) fn create(
        &self,
        repo: &RepositoryHandle,
        deadline: Instant,
    ) -> Result<AnchorRecord, OperationError> {
        self.validate()?;
        let bytes = serde_json::to_vec(self).map_err(|_| OperationError::new("INVALID_INPUT"))?;
        let dir = Dir::open_ambient_dir(&repo.git_dir, cap_std::ambient_authority())
            .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
        let mut file = dir
            .open_with(FILE, OpenOptions::new().write(true).create_new(true))
            .map_err(|_| OperationError::new("REPOSITORY_OPERATION_ACTIVE"))?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
        drop(file);
        let record = AnchorRecord::read(repo, deadline)?;
        if &record.value != self {
            return Err(OperationError::new("STALE_CONFLICT"));
        }
        Ok(record)
    }

    /// 读取真实记录，不用当前 HEAD 推测被遗失的起点。
    #[cfg(test)]
    pub(super) fn read(repo: &RepositoryHandle) -> Result<Self, OperationError> {
        Ok(AnchorRecord::read(repo, Instant::now() + std::time::Duration::from_secs(10))?.value)
    }

    /// 拒绝未知版本、非完整 OID 和无效分支，后续 Git 参数不接受任意表达式。
    fn validate(&self) -> Result<(), OperationError> {
        let oid = |value: &str| {
            matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
        };
        if self.version != 1
            || !oid(&self.original_head_oid)
            || !oid(&self.target_commit_oid)
            || self.original_head_oid.len() != self.target_commit_oid.len()
            || self.branch.is_empty()
            || self.branch.len() > 1024
            || self.branch.contains(['\0', '\r', '\n'])
        {
            return Err(OperationError::new("STALE_CONFLICT"));
        }
        Ok(())
    }
}

impl AnchorRecord {
    /// 有界读取使用已有普通文件能力，不跟随链接或等待特殊文件。
    pub(crate) fn read(repo: &RepositoryHandle, deadline: Instant) -> Result<Self, OperationError> {
        let captured = read_regular(&repo.git_dir, FILE, 4096, deadline)?;
        let value: RevertAnchor = serde_json::from_slice(&captured.bytes)
            .map_err(|_| OperationError::new("STALE_CONFLICT"))?;
        value.validate()?;
        Ok(Self { value, captured })
    }

    /// 写入前后确认起点记录仍属于本次操作。
    pub(crate) fn revalidate(
        &self,
        repo: &RepositoryHandle,
        deadline: Instant,
    ) -> Result<(), OperationError> {
        if Self::read(repo, deadline)?.captured != self.captured {
            return Err(OperationError::new("STALE_CONFLICT"));
        }
        Ok(())
    }

    /// 原分支、原 HEAD 与撤销目标必须全部匹配，防止重启后误用新 HEAD。
    pub(crate) fn validate_state(
        &self,
        head: &HeadState,
        target: &str,
    ) -> Result<(), OperationError> {
        if !matches!(head, HeadState::Branch { name, oid }
            if name == &self.value.branch && oid == &self.value.original_head_oid)
            || target != self.value.target_commit_oid
        {
            return Err(OperationError::new("STALE_CONFLICT"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// 起点记录不可覆盖；重新打开可读取，相同字节的外部替换也使旧记录失效。
    #[test]
    fn revert_anchor_is_exclusive_and_detects_replacement() {
        let root = tempfile::tempdir().unwrap();
        let git_dir = root.path().join(".git");
        std::fs::create_dir(&git_dir).unwrap();
        let repo = RepositoryHandle {
            id: "test".into(),
            root: root.path().into(),
            git_dir: git_dir.clone(),
            common_dir: git_dir.clone(),
        };
        let head = HeadState::Branch {
            name: "main".into(),
            oid: "a".repeat(40),
        };
        let anchor = RevertAnchor::new(&head, &"b".repeat(40)).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let record = anchor.create(&repo, deadline).unwrap();
        record.validate_state(&head, &"b".repeat(40)).unwrap();
        assert!(record.validate_state(&head, &"c".repeat(40)).is_err());
        assert!(record
            .validate_state(
                &HeadState::Branch {
                    name: "main".into(),
                    oid: "c".repeat(40),
                },
                &"b".repeat(40)
            )
            .is_err());
        assert_eq!(RevertAnchor::read(&repo).unwrap(), anchor);
        assert!(anchor.create(&repo, deadline).is_err());
        record.revalidate(&repo, deadline).unwrap();
        let bytes = std::fs::read(git_dir.join(FILE)).unwrap();
        std::fs::rename(git_dir.join(FILE), git_dir.join("old-anchor")).unwrap();
        std::fs::write(git_dir.join(FILE), bytes).unwrap();
        assert!(record.revalidate(&repo, deadline).is_err());
        assert_eq!(RevertAnchor::read(&repo).unwrap(), anchor);
    }

    /// 损坏、未知版本和超限记录不能作为继续撤销的起点。
    #[test]
    fn revert_anchor_rejects_invalid_or_oversized_records() {
        let root = tempfile::tempdir().unwrap();
        let repo = RepositoryHandle {
            id: "test".into(),
            root: root.path().into(),
            git_dir: root.path().into(),
            common_dir: root.path().into(),
        };
        for bytes in [
            b"broken".to_vec(),
            b"{\"version\":99}".to_vec(),
            vec![b' '; 4097],
        ] {
            std::fs::write(root.path().join(FILE), &bytes).unwrap();
            assert!(RevertAnchor::read(&repo).is_err());
            assert_eq!(std::fs::read(root.path().join(FILE)).unwrap(), bytes);
        }
        let head = HeadState::Branch {
            name: "main".into(),
            oid: "a".repeat(40),
        };
        assert!(RevertAnchor::new(&head, "--all").is_err());
    }
}
