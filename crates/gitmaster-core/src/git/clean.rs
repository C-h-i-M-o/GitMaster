//! 对明确选择的未跟踪普通文件签发删除计划，不使用递归目录清理。
use super::{
    conflicts::{read_regular, remove_regular, FileBytes},
    coordinator::{CoordinationKey, OperationReporter, RepositoryCoordinator},
    repository::query_until,
    write::{operation_result, IndexTransaction},
    write_guard as guard, *,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    time::{Duration, Instant},
};

const BUDGET: Duration = Duration::from_secs(120);
const FILE_LIMIT: usize = 64 * 1024 * 1024;
const PLAN_LIMIT: usize = 128 * 1024 * 1024;

/// 删除预览包含完整路径、大小与摘要，不把文件正文发送到确认界面。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanFilePreview {
    pub path: String,
    pub bytes: usize,
    pub sha256: String,
}

/// 原内容仅由一次性计划持有，执行前按句柄再次核对身份与完整字节。
struct CleanFile {
    path: String,
    original: FileBytes,
}

/// 只接受状态快照签发的未跟踪 changeId，首次选择之后还必须进入删除确认。
pub fn prepare(
    coordinator: &RepositoryCoordinator,
    git: &GitExecutable,
    repo: &RepositoryHandle,
    state: &RepositoryState,
    ids: &[String],
) -> Result<WritePreview, OperationError> {
    let generation = coordinator.begin_prepare()?;
    let deadline = Instant::now() + BUDGET;
    if ids.is_empty() {
        return Err(OperationError::new("EMPTY_SELECTION"));
    }
    let mut paths = BTreeSet::new();
    for id in ids {
        let change = state
            .changes
            .iter()
            .find(|change| &change.change_id == id)
            .ok_or_else(|| OperationError::new("STALE_REQUEST"))?;
        if change.kind != "untracked" || !paths.insert(change.path.clone()) {
            return Err(OperationError::new("INVALID_INPUT"));
        }
        guard::validate_recovery_path(&change.path)?;
    }
    let paths: Vec<String> = paths.into_iter().collect();
    let key = CoordinationKey::repository(repo)?;
    let (fingerprint, files, previews) = coordinator.read_until(&key, deadline, || {
        guard::validate_snapshot(git, repo, state, deadline)?;
        let fingerprint = guard::fingerprint(git, repo, &paths, false, deadline)?;
        let allowed = untracked(git, repo, deadline)?;
        let mut files = Vec::new();
        let mut previews = Vec::new();
        let mut total = 0usize;
        for path in &paths {
            if !allowed.contains(path) || fingerprint.index.iter().any(|entry| &entry.path == path)
            {
                return Err(OperationError::new("STALE_WRITE_PLAN"));
            }
            let original = read_regular(&repo.root, path, FILE_LIMIT, deadline)?;
            total = total.saturating_add(original.bytes.len());
            if total > PLAN_LIMIT {
                return Err(OperationError::new("OUTPUT_LIMIT"));
            }
            previews.push(CleanFilePreview {
                path: path.clone(),
                bytes: original.bytes.len(),
                sha256: format!("{:x}", Sha256::digest(&original.bytes)),
            });
            files.push(CleanFile {
                path: path.clone(),
                original,
            });
        }
        if fingerprint != guard::fingerprint(git, repo, &paths, false, deadline)? {
            return Err(OperationError::new("STALE_WRITE_PLAN"));
        }
        Ok((fingerprint, files, previews))
    })?;
    let prepared_git = git.clone();
    let prepared_repo = repo.clone();
    let plan = coordinator.prepare(
        generation,
        Some(repo.id.clone()),
        key,
        OperationKind::CleanUntracked,
        move |reporter| execute(reporter, &prepared_git, &prepared_repo, fingerprint, files),
    )?;
    Ok(WritePreview {
        plan_id: plan.plan_id,
        repository_id: repo.id.clone(),
        snapshot_id: state.snapshot_id.clone(),
        kind: OperationKind::CleanUntracked,
        head: state.head.clone(),
        parent_oids: Vec::new(),
        paths,
        author: None,
        message: None,
        target: Some(WriteTarget::Clean { files: previews }),
        warnings: vec![
            "这些文件尚未由 Git 跟踪。删除后无法通过 Git 恢复，也不会移入回收站；请再次核对清单。"
                .into(),
        ],
        expires_at: plan.expires_at,
    })
}

/// 用 Git 的真实忽略规则限定候选；超限或不可解码时拒绝扩大删除范围。
fn untracked(
    git: &GitExecutable,
    repo: &RepositoryHandle,
    deadline: Instant,
) -> Result<BTreeSet<String>, OperationError> {
    let bytes = query_until(
        git,
        &repo.root,
        &["ls-files", "--others", "--exclude-standard", "-z"],
        8 * 1024 * 1024,
        Some(deadline),
    )?;
    let mut paths = BTreeSet::new();
    for row in bytes.split(|byte| *byte == 0).filter(|row| !row.is_empty()) {
        let path = std::str::from_utf8(row)
            .map_err(|_| OperationError::new("UNSUPPORTED_PATH_ENCODING"))?;
        paths.insert(path.to_owned());
        if paths.len() > 10_000 {
            return Err(OperationError::new("OUTPUT_LIMIT"));
        }
    }
    Ok(paths)
}

/// 每次删除前重新检查忽略/跟踪状态，只移除已捕获的文件对象，部分完成绝不自动重试。
fn execute(
    reporter: &OperationReporter,
    git: &GitExecutable,
    repo: &RepositoryHandle,
    expected: guard::WriteFingerprint,
    files: Vec<CleanFile>,
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
                if !transaction.owns_lock() {
                    return Err(OperationError::new("INDEX_LOCKED"));
                }
                if !untracked(git, repo, deadline)?.contains(&file.path) {
                    return Err(OperationError::new("STALE_WRITE_PLAN"));
                }
                remove_regular(&repo.root, &file.path, &file.original, deadline)
            })();
            match applied {
                Ok(()) => {
                    written = true;
                    record.status = RecoveryPathStatus::Succeeded;
                }
                Err(error) => {
                    record.status = if error.code == "WRITE_OUTCOME_UNKNOWN" {
                        RecoveryPathStatus::Unknown
                    } else {
                        RecoveryPathStatus::Failed
                    };
                    record.error = Some(error.clone());
                    return Err(error);
                }
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

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::git::repository::{open_repository, tests::Fixture};

    /// 准备不删除，执行仅移除指定未跟踪文件；忽略、已跟踪及未选中内容保持原样。
    #[test]
    fn clean_selected_untracked_preserves_every_other_path() {
        let f = Fixture::new();
        f.write(".gitignore", b"ignored\n");
        f.write("tracked", b"tracked\n");
        f.command(&["add", "."]);
        f.command(&["commit", "-m", "base"]);
        f.write("remove", b"remove\0");
        f.write("keep", b"keep\n");
        f.write("ignored", b"ignored\n");
        let (repo, state) = open_repository(&f.git, &f.root).unwrap();
        let id = state
            .changes
            .iter()
            .find(|change| change.path == "remove")
            .unwrap()
            .change_id
            .clone();
        let coordinator = RepositoryCoordinator::new();
        let index = std::fs::read(repo.git_dir.join("index")).unwrap();
        let preview = prepare(&coordinator, &f.git, &repo, &state, &[id]).unwrap();
        assert!(f.root.join("remove").exists());
        let handle = coordinator
            .execute(Some(&repo.id), &preview.plan_id)
            .unwrap();
        let deadline = Instant::now() + BUDGET + Duration::from_secs(5);
        let result = loop {
            if let Some(result) = coordinator
                .read_operation(Some(&handle.operation_id))
                .unwrap()
                .unwrap()
                .result
            {
                break result;
            }
            assert!(Instant::now() < deadline, "清理未在预算内完成");
            std::thread::sleep(Duration::from_millis(10));
        };
        assert!(
            matches!(result, OperationResult::Succeeded { .. }),
            "{result:?}"
        );
        assert!(!f.root.join("remove").exists());
        for (path, content) in [
            ("keep", b"keep\n".as_slice()),
            ("ignored", b"ignored\n"),
            ("tracked", b"tracked\n"),
        ] {
            assert_eq!(std::fs::read(f.root.join(path)).unwrap(), content);
        }
        assert_eq!(std::fs::read(repo.git_dir.join("index")).unwrap(), index);
    }
}
