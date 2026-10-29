//! 用户明确中止后恢复索引与冲突工作文件，真实 HEAD 和分支引用始终不写入。
use super::{anchor::AnchorRecord, *};
use crate::git::{
    checkout::{CapturedTree, ReferenceLock},
    conflicts::finish::OperationFiles,
    coordinator::{CoordinationKey, OperationReporter, RepositoryCoordinator},
    merge::validate_merge_config,
    repository::read_repository_state_until,
    write::{operation_result, IndexTransaction},
    write_guard::{self as guard, WriteFingerprint},
};
use std::{
    collections::BTreeSet,
    time::{Duration, Instant},
};

const BUDGET: Duration = Duration::from_secs(120);

/// 中止计划固定起点、原索引、受影响工作文件及全部待清理元数据。
struct AbortPlan {
    git: GitExecutable,
    repo: RepositoryHandle,
    paths: Vec<String>,
    fingerprint: WriteFingerprint,
    anchor: AnchorRecord,
    metadata: OperationFiles,
    tree: CapturedTree,
}

/// 准备只读取真实仓库；有效起点存在时也能处理 Git 启动前中断的撤销。
pub fn prepare_abort(
    coordinator: &RepositoryCoordinator,
    git: &GitExecutable,
    repo: &RepositoryHandle,
    state: &RepositoryState,
) -> Result<WritePreview, OperationError> {
    let generation = coordinator.begin_prepare()?;
    let deadline = Instant::now() + BUDGET;
    let key = CoordinationKey::repository(repo)?;
    let plan = coordinator.read_until(&key, deadline, || {
        guard::validate_snapshot(git, repo, state, deadline)?;
        let anchor = AnchorRecord::read(repo, deadline)?;
        anchor.validate_state(&state.head, &anchor.value.target_commit_oid)?;
        let metadata = OperationFiles::capture_revert_abort(repo, deadline)?;
        if metadata.marker_bytes().is_some_and(|bytes| {
            bytes != format!("{}\n", anchor.value.target_commit_oid).as_bytes()
        }) {
            return Err(OperationError::new("STALE_CONFLICT"));
        }
        let paths = state
            .changes
            .iter()
            .filter(|change| change.kind != "untracked")
            .flat_map(|change| {
                std::iter::once(change.path.clone()).chain(change.original_path.clone())
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        for path in &paths {
            guard::validate_recovery_path(path)?;
        }
        let fingerprint = guard::revert_worktree_fingerprint(git, repo, &paths, false, deadline)?;
        anchor.validate_state(&fingerprint.head, &anchor.value.target_commit_oid)?;
        validate_merge_config(git, repo, deadline)?;
        // reset --merge 可能覆盖被忽略的碰撞文件，因此原树中缺少索引条目的现存路径一律拒绝。
        let entries = guard::checkout_tree(git, repo, &anchor.value.original_head_oid, deadline)?;
        for entry in &entries {
            if !fingerprint
                .index
                .iter()
                .any(|current| current.path == entry.path)
                // 普通索引摘要不含 stage 1/2/3；冲突路径由同一索引身份的状态快照提供。
                && !state.changes.iter().any(|change| change.kind == "conflicted" && change.path == entry.path)
                && !fingerprint.file_is_missing(&entry.path)?
            {
                return Err(OperationError::new("UNTRACKED_COLLISION"));
            }
        }
        let tree = CapturedTree::capture(
            git,
            repo,
            &fingerprint,
            &anchor.value.original_head_oid,
            deadline,
        )?;
        anchor.revalidate(repo, deadline)?;
        metadata.validate(repo, deadline)?;
        if guard::revert_worktree_fingerprint(git, repo, &paths, false, deadline)? != fingerprint {
            return Err(OperationError::new("STALE_WRITE_PLAN"));
        }
        Ok(AbortPlan {
            git: git.clone(),
            repo: repo.clone(),
            paths,
            fingerprint,
            anchor,
            metadata,
            tree,
        })
    })?;
    let paths = plan.paths.clone();
    let oid = plan.anchor.value.target_commit_oid.clone();
    let original = plan.anchor.value.original_head_oid.clone();
    let prepared = coordinator.prepare(
        generation,
        Some(repo.id.clone()),
        key,
        OperationKind::AbortRevert,
        move |reporter| plan.execute(reporter),
    )?;
    Ok(WritePreview {
        plan_id: prepared.plan_id, repository_id: repo.id.clone(), snapshot_id: state.snapshot_id.clone(),
        kind: OperationKind::AbortRevert, head: state.head.clone(), parent_oids: vec![original], paths,
        author: None, message: None, target: Some(WriteTarget::Revert { oid }),
        warnings: vec!["将中止本次撤销，丢弃当前冲突编辑和暂存区修改，包括撤销开始后另外暂存的内容；不移动分支，不清理未跟踪文件。原生 Git 会保留不冲突的未暂存修改，存在覆盖冲突时可能拒绝中止。".into()],
        expires_at: prepared.expires_at,
    })
}

impl AbortPlan {
    /// 写工作树前核对完整确认输入；写入后任一核验失败均保留起点并返回未知。
    fn execute(self, reporter: &OperationReporter) -> OperationResult {
        let deadline = reporter.deadline(BUDGET);
        let mut attempted = false;
        let result = (|| {
            let branch = ReferenceLock::acquire(
                &self.repo,
                &format!("refs/heads/{}", self.anchor.value.branch),
            )?;
            let head = ReferenceLock::acquire_head(&self.repo)?;
            let mut index =
                IndexTransaction::new(&self.repo, self.fingerprint.index_bytes.as_deref())?;
            if guard::revert_worktree_fingerprint(
                &self.git,
                &self.repo,
                &self.paths,
                true,
                deadline,
            )? != self.fingerprint
            {
                return Err(OperationError::new("STALE_WRITE_PLAN"));
            }
            self.anchor.revalidate(&self.repo, deadline)?;
            self.metadata.validate(&self.repo, deadline)?;
            if !branch.owns_lock() || !head.owns_lock() || !index.owns_lock() {
                return Err(OperationError::new("STALE_WRITE_PLAN"));
            }
            reporter.report(OperationPhase::Writing, None)?;
            attempted = true;
            self.tree.reset_for_abort(
                &self.git,
                &self.repo,
                &index.path,
                &self.anchor.value.original_head_oid,
                deadline,
            )?;
            guard::local_query(
                &self.git,
                &self.repo,
                &[
                    "--no-replace-objects",
                    "diff-index",
                    "--cached",
                    "--no-ext-diff",
                    "--no-textconv",
                    "--quiet",
                    &self.anchor.value.original_head_oid,
                    "--",
                ],
                &[],
                Some(&index.path),
                deadline,
            )?;
            self.anchor.revalidate(&self.repo, deadline)?;
            self.metadata.validate(&self.repo, deadline)?;
            let state = read_repository_state_until(&self.git, &self.repo, deadline)?;
            if state.head != self.fingerprint.head
                || state.operations != ["revert"]
                || guard::read_index_bytes(&self.repo)? != self.fingerprint.index_bytes
                || !branch.owns_lock()
                || !head.owns_lock()
                || !index.owns_lock()
            {
                return Err(OperationError::new("WRITE_OUTCOME_UNKNOWN"));
            }
            index.publish()?;
            reporter.report(OperationPhase::Verifying, None)?;
            let published = guard::read_index_bytes(&self.repo)?;
            self.metadata.remove(&self.repo, deadline)?;
            let state = read_repository_state_until(&self.git, &self.repo, deadline)?;
            if state.head != self.fingerprint.head
                || !state.operations.is_empty()
                || guard::read_index_bytes(&self.repo)? != published
                || !branch.owns_lock()
                || !head.owns_lock()
            {
                return Err(OperationError::new("WRITE_OUTCOME_UNKNOWN"));
            }
            Ok(())
        })();
        operation_result(
            reporter,
            &self.git,
            &self.repo,
            result,
            attempted,
            None,
            Some(self.anchor.value.branch),
            deadline,
        )
    }
}
