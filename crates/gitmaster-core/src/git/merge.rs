//! 显式快进或停止于提交前的普通整合，不执行 pull/rebase/stash。
use super::{
    checkout::CapturedTree,
    conflicts::ConflictSession,
    coordinator::{CoordinationKey, OperationReporter, RepositoryCoordinator},
    process::inspect_local_config,
    remote::RemoteSession,
    repository::read_repository_state_until,
    write::operation_result,
    write_guard::{self as guard, WriteFingerprint},
    *,
};
use std::{
    collections::BTreeSet,
    ffi::OsString,
    time::{Duration, Instant},
};
const BUDGET: Duration = Duration::from_secs(120);

/// 整合计划仅持有确认快照、固定 OID 和私有检出输入。
struct MergePlan {
    git: GitExecutable,
    repo: RepositoryHandle,
    fingerprint: WriteFingerprint,
    paths: Vec<String>,
    reference: String,
    target: String,
    mode: IntegrateMode,
    tree: CapturedTree,
}

/// 只准备当前会话签发的远端跟踪目标，实际写入由一次性计划启动。
pub fn prepare_integrate(
    coordinator: &RepositoryCoordinator,
    git: &GitExecutable,
    repo: &RepositoryHandle,
    state: &RepositoryState,
    remote: &RemoteSession,
    remote_branch_id: &str,
    mode: IntegrateMode,
) -> Result<WritePreview, OperationError> {
    let generation = coordinator.begin_prepare()?;
    let deadline = Instant::now() + BUDGET;
    let key = CoordinationKey::repository(repo)?;
    let plan = coordinator.read_until(&key, deadline, || {
        guard::validate_snapshot(git, repo, state, deadline)?;
        if !state.changes.is_empty() {
            return Err(OperationError::new("WORKTREE_DIRTY"));
        }
        let local = match &state.head {
            HeadState::Branch { oid, .. } => oid,
            HeadState::Unborn { .. } => return Err(OperationError::new("HEAD_REQUIRED")),
            HeadState::Detached { .. } => {
                return Err(OperationError::new("DETACHED_HEAD_WRITE_BLOCKED"))
            }
        };
        validate_merge_config(git, repo, deadline)?;
        let (assessment, reference) =
            remote.integration_target(state, remote_branch_id, deadline)?;
        match (assessment.relation, mode) {
            (RemoteRelation::Behind, IntegrateMode::FastForward)
            | (RemoteRelation::Diverged, IntegrateMode::Merge) => (),
            (RemoteRelation::Equal | RemoteRelation::Ahead, _) => {
                return Err(OperationError::new("NOTHING_TO_COMMIT"))
            }
            (RemoteRelation::Unrelated, _) => {
                return Err(OperationError::new("NO_COMMON_ANCESTOR"))
            }
            (RemoteRelation::Unknown, _) => {
                return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"))
            }
            (RemoteRelation::Diverged, IntegrateMode::FastForward) => {
                return Err(OperationError::new("NON_FAST_FORWARD"))
            }
            _ => return Err(OperationError::new("INVALID_INPUT")),
        }
        let target = assessment.remote_oid;
        let bytes = guard::local_query(
            git,
            repo,
            &[
                "--no-replace-objects",
                "diff",
                "--name-only",
                "--no-renames",
                "--no-ext-diff",
                "--no-textconv",
                "-z",
                local,
                &target,
                "--",
            ],
            &[],
            None,
            deadline,
        )?;
        let paths = bytes
            .split(|byte| *byte == 0)
            .filter(|path| !path.is_empty())
            .map(|path| {
                let path = std::str::from_utf8(path)
                    .map_err(|_| OperationError::new("UNSUPPORTED_PATH_ENCODING"))?;
                guard::validate_path(path)?;
                Ok(path.to_owned())
            })
            .collect::<Result<BTreeSet<_>, OperationError>>()?
            .into_iter()
            .collect::<Vec<_>>();
        let fingerprint = guard::fingerprint(git, repo, &paths, false, deadline)?;
        let bases = guard::local_query(
            git,
            repo,
            &[
                "--no-replace-objects",
                "merge-base",
                "--all",
                local,
                &target,
            ],
            &[],
            None,
            deadline,
        )?;
        for base in bases
            .split(|byte| *byte == b'\n')
            .filter(|base| !base.is_empty())
        {
            let oid = std::str::from_utf8(base).map_err(|_| OperationError::new("PARSE_FAILED"))?;
            guard::checkout_tree(git, repo, oid, deadline)?;
        }
        let tree = CapturedTree::capture(git, repo, &fingerprint, &target, deadline)?;
        let tree = if matches!(mode, IntegrateMode::Merge) {
            tree.for_merge(git, repo, deadline)?
        } else {
            tree
        };
        if guard::fingerprint(git, repo, &paths, false, deadline)? != fingerprint {
            return Err(OperationError::new("STALE_WRITE_PLAN"));
        }
        Ok(MergePlan {
            git: git.clone(),
            repo: repo.clone(),
            fingerprint,
            paths,
            reference,
            target,
            mode,
            tree,
        })
    })?;
    let paths = plan.paths.clone();
    let target = plan.target.clone();
    let local = match &state.head {
        HeadState::Branch { oid, .. } => oid.clone(),
        _ => unreachable!("准备已校验具名 HEAD"),
    };
    let prepared = coordinator.prepare(
        generation,
        Some(repo.id.clone()),
        key,
        OperationKind::Integrate,
        move |reporter| plan.execute(reporter),
    )?;
    Ok(WritePreview {
        plan_id: prepared.plan_id,
        repository_id: repo.id.clone(),
        snapshot_id: state.snapshot_id.clone(),
        kind: OperationKind::Integrate,
        head: state.head.clone(),
        parent_oids: if matches!(mode, IntegrateMode::Merge) {
            vec![local, target.clone()]
        } else {
            vec![local]
        },
        paths,
        author: None,
        message: None,
        target: Some(WriteTarget::Merge {
            remote_branch_id: remote_branch_id.into(),
            oid: target,
        }),
        warnings: if matches!(mode, IntegrateMode::Merge) {
            vec!["本次只准备合并结果；完成后仍需检查并确认保存合并提交。".into()]
        } else {
            vec![]
        },
        expires_at: prepared.expires_at,
    })
}

impl MergePlan {
    /// 确认后重新读取真实前提，任何已尝试写入的未知结果都不自动回滚。
    fn execute(self, reporter: &OperationReporter) -> OperationResult {
        let deadline = reporter.deadline(BUDGET);
        let mut attempted = false;
        let mut commit_oid = None;
        let branch_name = match &self.fingerprint.head {
            HeadState::Branch { name, .. } => Some(name.clone()),
            _ => None,
        };
        let result = (|| {
            if guard::fingerprint(&self.git, &self.repo, &self.paths, false, deadline)?
                != self.fingerprint
            {
                return Err(OperationError::new("STALE_WRITE_PLAN"));
            }
            validate_merge_config(&self.git, &self.repo, deadline)?;
            let actual = guard::local_query(
                &self.git,
                &self.repo,
                &[
                    "for-each-ref",
                    "--format=%(refname)%00%(objectname)%00%(symref)",
                    &self.reference,
                ],
                &[],
                None,
                deadline,
            )?;
            if actual != format!("{}\0{}\0\n", self.reference, self.target).as_bytes() {
                return Err(OperationError::new("REMOTE_CHANGED"));
            }
            reporter.report(OperationPhase::Writing, None)?;
            let mut args = [
                "-c",
                "rerere.enabled=false",
                "-c",
                "submodule.recurse=false",
                "-c",
                "merge.autoStash=false",
                "merge",
                "--no-edit",
                "--no-gpg-sign",
                "--no-autostash",
                "--no-overwrite-ignore",
                "--no-rerere-autoupdate",
                "--no-stat",
                "-s",
                "ort",
            ]
            .map(OsString::from)
            .to_vec();
            match self.mode {
                IntegrateMode::FastForward => args.push("--ff-only".into()),
                IntegrateMode::Merge => args.extend(["--no-ff".into(), "--no-commit".into()]),
            }
            args.push(self.target.clone().into());
            attempted = true;
            let output = self.tree.run(&self.git, &self.repo, &args, deadline);
            reporter.report(OperationPhase::Verifying, None)?;
            let state = read_repository_state_until(&self.git, &self.repo, deadline)?;
            if matches!(self.mode, IntegrateMode::FastForward)
                && state.changes.is_empty()
                && state.operations.is_empty()
                && matches!(&state.head, HeadState::Branch { name, oid } if Some(name) == branch_name.as_ref() && oid == &self.target)
            {
                commit_oid = Some(self.target.clone());
                return Ok(None);
            }
            if matches!(self.mode, IntegrateMode::Merge)
                && state.head == self.fingerprint.head
                && state.operations == ["merge"]
            {
                let conflicts =
                    ConflictSession::new_until(&self.git, &self.repo, deadline)?.state();
                if conflicts.merge_head_oids != [self.target.clone()] {
                    return Err(OperationError::new("WRITE_OUTCOME_UNKNOWN"));
                }
                if conflicts.files.is_empty() {
                    if state
                        .changes
                        .iter()
                        .any(|change| change.worktree_status != ".")
                    {
                        return Err(OperationError::new("WRITE_OUTCOME_UNKNOWN"));
                    }
                    return Ok(None);
                }
                return Ok(Some(OperationResult::NeedsResolution {
                    operation_id: reporter.handle.operation_id.clone(),
                    kind: OperationKind::Integrate,
                    conflict: ConflictSummary {
                        merge_session_id: conflicts.merge_session_id,
                        unresolved_count: conflicts.files.len() as u32,
                        paths: conflicts.files.into_iter().map(|file| file.path).collect(),
                    },
                    refresh: WriteRefresh::Ready { state },
                }));
            }
            if state.head == self.fingerprint.head
                && state.changes.is_empty()
                && state.operations.is_empty()
                && output.as_ref().is_ok_and(|result| !result.success)
            {
                attempted = false;
                return Err(OperationError::new("GIT_EXECUTION_FAILED"));
            }
            Err(OperationError::new("WRITE_OUTCOME_UNKNOWN"))
        })();
        match result {
            Ok(Some(result)) => result,
            other => operation_result(
                reporter,
                &self.git,
                &self.repo,
                other.map(|_| ()),
                attempted,
                commit_oid,
                branch_name,
                deadline,
            ),
        }
    }
}

/// 不把自定义默认驱动、签名验证或替换历史静默降级成普通合并。
pub(crate) fn validate_merge_config(
    git: &GitExecutable,
    repo: &RepositoryHandle,
    deadline: Instant,
) -> Result<(), OperationError> {
    let config = guard::parse_config(&inspect_local_config(git, &repo.root, deadline)?)?;
    if config
        .get("merge.default")
        .is_some_and(|driver| !matches!(driver.as_str(), "text" | "binary" | "union"))
        || config
            .get("merge.verifysignatures")
            .is_some_and(|value| !matches!(value.as_str(), "false" | "no" | "0" | "off"))
    {
        return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
    }
    let replacements = guard::local_query(
        git,
        repo,
        &["for-each-ref", "--format=%(refname)", "refs/replace"],
        &[],
        None,
        deadline,
    )?;
    if !replacements.is_empty() || repo.common_dir.join("info/grafts").exists() {
        return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
