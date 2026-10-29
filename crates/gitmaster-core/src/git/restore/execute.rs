//! 只发布预览时冻结的文件字节，不调用会同时改写索引的恢复命令。
use super::*;
use crate::git::{
    conflicts::{create_regular, read_regular, replace_regular, FileBytes},
    coordinator::{CoordinationKey, OperationReporter, RepositoryCoordinator},
    write::{operation_result, IndexTransaction},
};
use sha2::{Digest, Sha256};

const FILE_LIMIT: usize = 64 * 1024 * 1024;
const PLAN_LIMIT: usize = 128 * 1024 * 1024;

/// 小文本提供完整前后内容；二进制或大文件只展示大小与摘要，不截断为可编辑内容。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreContentPreview {
    pub bytes: usize,
    pub sha256: String,
    pub text: Option<String>,
}

/// 缺失工作文件使用空的 before，不能与零字节文件混同。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreFilePreview {
    pub path: String,
    pub before: Option<RestoreContentPreview>,
    pub after: RestoreContentPreview,
}

/// 只有核心持有完整字节与原文件身份，界面不能用预览内容替换已批准目标。
struct FilePlan {
    path: String,
    original: Option<FileBytes>,
    bytes: Vec<u8>,
}

/// 在同仓库读屏障内固定来源、内容与写前指纹，并签发一次性计划。
pub(super) fn prepare(
    source: &RestoreSession,
    coordinator: &RepositoryCoordinator,
    state: &RepositoryState,
    ids: &[String],
) -> Result<WritePreview, OperationError> {
    let generation = coordinator.begin_prepare()?;
    let deadline = Instant::now() + BUDGET;
    if state.repository_id != source.repo.id
        || state.snapshot_id != source.snapshot_id
        || state.head != source.head
        || state.index_identity != source.index_identity
    {
        return Err(OperationError::new("STALE_REQUEST"));
    }
    let entries = source.selected(ids)?;
    let paths: Vec<String> = entries.iter().map(|entry| entry.path.clone()).collect();
    let key = CoordinationKey::repository(&source.repo)?;
    let (fingerprint, files, previews) = coordinator.read_until(&key, deadline, || {
        guard::validate_snapshot(&source.git, &source.repo, state, deadline)?;
        let fingerprint = guard::fingerprint(&source.git, &source.repo, &paths, false, deadline)?;
        let mut size = 0usize;
        let mut files = Vec::new();
        let mut previews = Vec::new();
        for entry in &entries {
            let original = if fingerprint.file_is_missing(&entry.path)? {
                None
            } else {
                // 历史来源可以新增已删除路径，但绝不覆盖目前未跟踪的同名文件。
                if !fingerprint
                    .index
                    .iter()
                    .any(|indexed| indexed.path == entry.path)
                {
                    return Err(OperationError::new("UNTRACKED_COLLISION"));
                }
                Some(read_regular(
                    &source.repo.root,
                    &entry.path,
                    FILE_LIMIT,
                    deadline,
                )?)
            };
            let bytes =
                fingerprint
                    .conversion
                    .restore_entry(&source.git, &source.repo, entry, deadline)?;
            size = size
                .saturating_add(bytes.len())
                .saturating_add(original.as_ref().map_or(0, |file| file.bytes.len()));
            if size > PLAN_LIMIT {
                return Err(OperationError::new("OUTPUT_LIMIT"));
            }
            previews.push(RestoreFilePreview {
                path: entry.path.clone(),
                before: original
                    .as_ref()
                    .map(|file| preview(&file.bytes, entries.len() <= 16)),
                after: preview(&bytes, entries.len() <= 16),
            });
            files.push(FilePlan {
                path: entry.path.clone(),
                original,
                bytes,
            });
        }
        if fingerprint != guard::fingerprint(&source.git, &source.repo, &paths, false, deadline)? {
            return Err(OperationError::new("STALE_WRITE_PLAN"));
        }
        Ok((fingerprint, files, previews))
    })?;
    let git = source.git.clone();
    let repo = source.repo.clone();
    let plan = coordinator.prepare(
        generation,
        Some(repo.id.clone()),
        key,
        OperationKind::RestoreFiles,
        move |reporter| execute(reporter, &git, &repo, fingerprint, files),
    )?;
    Ok(WritePreview {
        plan_id: plan.plan_id,
        repository_id: source.repo.id.clone(),
        snapshot_id: state.snapshot_id.clone(),
        kind: OperationKind::RestoreFiles,
        head: state.head.clone(),
        parent_oids: match &state.head {
            HeadState::Unborn { .. } => Vec::new(),
            HeadState::Branch { oid, .. } | HeadState::Detached { oid } => vec![oid.clone()],
        },
        paths,
        author: None,
        message: None,
        target: Some(WriteTarget::Restore {
            source: source.source.clone(),
            source_oid: source.source_oid.clone(),
            files: previews,
        }),
        warnings: vec![
            "将覆盖所选文件的工作区内容，已有暂存保持不变。未保存的编辑草稿须先处理。".into(),
        ],
        expires_at: plan.expires_at,
    })
}

/// 摘要覆盖完整字节；只有有界 UTF-8 文本才发送完整对比内容。
fn preview(bytes: &[u8], include_text: bool) -> RestoreContentPreview {
    RestoreContentPreview {
        bytes: bytes.len(),
        sha256: format!("{:x}", Sha256::digest(bytes)),
        text: (include_text && bytes.len() <= 32 * 1024 && !bytes.contains(&0))
            .then(|| std::str::from_utf8(bytes).ok().map(str::to_owned))
            .flatten(),
    }
}

/// 持有索引锁但不发布索引，逐个核对并原子发布；后续失败保留已完成的真实记录。
fn execute(
    reporter: &OperationReporter,
    git: &GitExecutable,
    repo: &RepositoryHandle,
    expected: guard::WriteFingerprint,
    files: Vec<FilePlan>,
) -> OperationResult {
    let deadline = reporter.deadline(BUDGET);
    let paths: Vec<String> = files.iter().map(|file| file.path.clone()).collect();
    let mut results: Vec<RecoveryPathResult> = paths
        .iter()
        .map(|path| RecoveryPathResult {
            path: path.clone(),
            status: RecoveryPathStatus::NotExecuted,
            error: None,
        })
        .collect();
    let mut written = false;
    let result = (|| {
        let transaction = IndexTransaction::new(repo, expected.index_bytes.as_deref())?;
        if guard::fingerprint(git, repo, &paths, true, deadline)? != expected {
            return Err(OperationError::new("STALE_WRITE_PLAN"));
        }
        reporter.report(OperationPhase::Writing, None)?;
        for (file, record) in files.iter().zip(results.iter_mut()) {
            let applied = (|| {
                guard::check_time(deadline)?;
                if !transaction.owns_lock() {
                    return Err(OperationError::new("INDEX_LOCKED"));
                }
                match &file.original {
                    Some(original) => replace_regular(
                        &repo.root,
                        &file.path,
                        original,
                        &file.bytes,
                        FILE_LIMIT,
                        deadline,
                    )?,
                    None => {
                        create_regular(&repo.root, &file.path, &file.bytes, FILE_LIMIT, deadline)?
                    }
                }
                written = true;
                record.status = RecoveryPathStatus::Unknown;
                if read_regular(&repo.root, &file.path, FILE_LIMIT, deadline)?.bytes != file.bytes {
                    return Err(OperationError::new("WRITE_OUTCOME_UNKNOWN"));
                }
                record.status = RecoveryPathStatus::Succeeded;
                Ok(())
            })();
            if let Err(error) = applied {
                if record.status == RecoveryPathStatus::NotExecuted {
                    record.status = RecoveryPathStatus::Failed;
                }
                record.error = Some(error.clone());
                return Err(error);
            }
        }
        reporter.report(OperationPhase::Verifying, None)?;
        if !transaction.owns_lock()
            || !expected.same_repository_inputs(&guard::fingerprint(
                git,
                repo,
                &[],
                true,
                deadline,
            )?)
        {
            return Err(OperationError::new("WRITE_OUTCOME_UNKNOWN"));
        }
        Ok(())
    })();
    operation_result(reporter, git, repo, result, written, None, None, deadline)
        .with_path_results(results)
}
