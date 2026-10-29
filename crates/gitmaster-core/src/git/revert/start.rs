//! 单次撤销先应用反向修改；保存提交与中止必须另行确认。
use super::{anchor::RevertAnchor, *};
use crate::git::{
    checkout::CapturedTree,
    coordinator::{CoordinationKey, OperationReporter, RepositoryCoordinator},
    merge::validate_merge_config,
    process::run_local_git,
    repository::read_repository_state_until,
    write::operation_result,
    write_guard::{self as guard, WriteFingerprint},
};
use std::{
    collections::BTreeSet,
    ffi::OsString,
    time::{Duration, Instant},
};
const BUDGET: Duration = Duration::from_secs(120);

#[cfg(test)]
mod result_tests {
    use super::*;

    /// 冲突反馈必须保留真实快照和撤销身份，不能混入合并能力或宣告已生成提交。
    #[test]
    fn revert_start_result_distinguishes_conflict_from_applied_changes() {
        let mut state = RepositoryState {
            index_identity: [0; 32],
            repository_id: "repo".into(),
            snapshot_id: "snapshot".into(),
            root_path: "temporary".into(),
            head: HeadState::Branch {
                name: "main".into(),
                oid: "head".into(),
            },
            operations: vec!["revert".into()],
            changes: vec![FileChange {
                change_id: "file".into(),
                path: "冲突.txt".into(),
                original_path: None,
                index_status: "U".into(),
                worktree_status: "U".into(),
                kind: "conflicted".into(),
                binary: "unknown".into(),
            }],
        };
        let result = verified_start_result("operation".into(), state.clone(), "target");
        let value = serde_json::to_value(result).unwrap();
        assert_eq!(value["outcome"], "needsResolution");
        assert_eq!(value["conflict"]["kind"], "revert");
        assert_eq!(value["conflict"]["targetCommitOid"], "target");
        assert_eq!(value["conflict"]["unresolvedCount"], 1);
        assert_eq!(value["conflict"]["paths"][0], "冲突.txt");
        assert!(value["conflict"].get("mergeSessionId").is_none());
        assert!(value["conflict"].get("revertSessionId").is_none());
        assert_eq!(value["refresh"]["state"]["snapshotId"], "snapshot");
        state.changes[0].kind = "tracked".into();
        let result = verified_start_result("operation".into(), state, "target");
        assert!(matches!(
            result,
            OperationResult::Succeeded {
                commit_oid: None,
                refresh: WriteRefresh::Ready { .. },
                ..
            }
        ));
    }
}

/// 已确认的输入由核心持有，前端只得到一次性计划 ID。
struct RevertPlan {
    git: GitExecutable,
    repo: RepositoryHandle,
    fingerprint: WriteFingerprint,
    paths: Vec<String>,
    anchor: RevertAnchor,
    tree: CapturedTree,
}

/// 仅接受干净具名分支上的普通祖先提交；准备不修改用户索引、工作区或元数据。
pub fn prepare_revert(
    coordinator: &RepositoryCoordinator,
    git: &GitExecutable,
    repo: &RepositoryHandle,
    state: &RepositoryState,
    target_oid: &str,
) -> Result<WritePreview, OperationError> {
    let generation = coordinator.begin_prepare()?;
    let deadline = Instant::now() + BUDGET;
    let key = CoordinationKey::repository(repo)?;
    let plan = coordinator.read_until(&key, deadline, || {
        guard::validate_snapshot(git, repo, state, deadline)?;
        if !state.changes.is_empty() {
            return Err(OperationError::new("WORKTREE_DIRTY"));
        }
        if !state.operations.is_empty() {
            return Err(OperationError::new("REPOSITORY_OPERATION_ACTIVE"));
        }
        let anchor = RevertAnchor::new(&state.head, target_oid)?;
        validate_merge_config(git, repo, deadline)?;
        let parents = guard::local_query(
            git,
            repo,
            &[
                "--no-replace-objects",
                "rev-list",
                "--parents",
                "-n",
                "1",
                target_oid,
                "--",
            ],
            &[],
            None,
            deadline,
        )?;
        let text =
            std::str::from_utf8(&parents).map_err(|_| OperationError::new("PARSE_FAILED"))?;
        let parents = text.split_whitespace().collect::<Vec<_>>();
        if parents.len() != 2 || parents[0] != target_oid {
            return Err(OperationError::new("UNSUPPORTED_REVERT"));
        }
        let args = [
            "--no-replace-objects",
            "merge-base",
            "--is-ancestor",
            target_oid,
            &anchor.original_head_oid,
        ]
        .map(OsString::from);
        match run_local_git(git, &repo.root, &args, &[], None, deadline)?.exit_code {
            Some(0) => (),
            Some(1) => return Err(OperationError::new("UNSUPPORTED_REVERT")),
            _ => return Err(OperationError::new("GIT_EXECUTION_FAILED")),
        }
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
                parents[1],
                target_oid,
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
                guard::validate_recovery_path(path)?;
                Ok(path.to_owned())
            })
            .collect::<Result<BTreeSet<_>, OperationError>>()?
            .into_iter()
            .collect::<Vec<_>>();
        if paths.is_empty() {
            return Err(OperationError::new("NOTHING_TO_COMMIT"));
        }
        let fingerprint = guard::fingerprint(git, repo, &paths, false, deadline)?;
        guard::checkout_tree(git, repo, target_oid, deadline)?;
        let tree = CapturedTree::capture(git, repo, &fingerprint, parents[1], deadline)?
            .for_merge(git, repo, deadline)?;
        if guard::fingerprint(git, repo, &paths, false, deadline)? != fingerprint {
            return Err(OperationError::new("STALE_WRITE_PLAN"));
        }
        Ok(RevertPlan {
            git: git.clone(),
            repo: repo.clone(),
            fingerprint,
            paths,
            anchor,
            tree,
        })
    })?;
    let paths = plan.paths.clone();
    let parent_oid = plan.anchor.original_head_oid.clone();
    let prepared = coordinator.prepare(
        generation,
        Some(repo.id.clone()),
        key,
        OperationKind::Revert,
        move |reporter| plan.execute(reporter),
    )?;
    Ok(WritePreview {
        plan_id: prepared.plan_id, repository_id: repo.id.clone(), snapshot_id: state.snapshot_id.clone(),
        kind: OperationKind::Revert, head: state.head.clone(), parent_oids: vec![parent_oid],
        paths, author: None, message: None,
        target: Some(WriteTarget::Revert { oid: target_oid.into() }),
        warnings: vec!["将应用此提交的反向修改，原有提交不会删除；请检查结果、处理冲突后再确认保存新的撤销提交。".into()],
        expires_at: prepared.expires_at,
    })
}

impl RevertPlan {
    /// 记录起点后执行固定单提交，发生未知结果时保留记录且不自动重试或回滚。
    fn execute(self, reporter: &OperationReporter) -> OperationResult {
        let deadline = reporter.deadline(BUDGET);
        let mut attempted = false;
        let result = (|| {
            if guard::fingerprint(&self.git, &self.repo, &self.paths, false, deadline)?
                != self.fingerprint
            {
                return Err(OperationError::new("STALE_WRITE_PLAN"));
            }
            validate_merge_config(&self.git, &self.repo, deadline)?;
            attempted = true;
            let record = self.anchor.create(&self.repo, deadline)?;
            let after_record = guard::revert_worktree_fingerprint(
                &self.git,
                &self.repo,
                &self.paths,
                false,
                deadline,
            )?;
            if after_record != self.fingerprint {
                return Err(OperationError::new("STALE_WRITE_PLAN"));
            }
            record.revalidate(&self.repo, deadline)?;
            reporter.report(OperationPhase::Writing, None)?;
            let args = [
                "-c",
                "rerere.enabled=false",
                "-c",
                "submodule.recurse=false",
                "revert",
                "--no-commit",
                "--no-edit",
                "--no-rerere-autoupdate",
                "--strategy=ort",
                &self.anchor.target_commit_oid,
            ]
            .map(OsString::from);
            let output = self.tree.run(&self.git, &self.repo, &args, deadline);
            reporter.report(OperationPhase::Verifying, None)?;
            record.revalidate(&self.repo, deadline)?;
            let state = read_repository_state_until(&self.git, &self.repo, deadline)?;
            if state.head != self.fingerprint.head || state.operations != ["revert"] {
                return Err(OperationError::new("WRITE_OUTCOME_UNKNOWN"));
            }
            let marker = crate::git::conflicts::read_regular(
                &self.repo.git_dir,
                "REVERT_HEAD",
                128,
                deadline,
            )?;
            if marker.bytes != format!("{}\n", self.anchor.target_commit_oid).as_bytes() {
                return Err(OperationError::new("WRITE_OUTCOME_UNKNOWN"));
            }
            if output.as_ref().is_ok_and(|value| {
                value.success
                    || (value.exit_code == Some(1)
                        && state
                            .changes
                            .iter()
                            .any(|change| change.kind == "conflicted"))
            }) {
                Ok(state)
            } else {
                Err(OperationError::new("WRITE_OUTCOME_UNKNOWN"))
            }
        })();
        match result {
            Ok(state) => verified_start_result(
                reporter.handle.operation_id.clone(),
                state,
                &self.anchor.target_commit_oid,
            ),
            Err(error) => operation_result(
                reporter,
                &self.git,
                &self.repo,
                Err(error),
                attempted,
                None,
                Some(self.anchor.branch),
                deadline,
            ),
        }
    }
}

/// 复用发起后的核验快照；有冲突需要处理，无冲突也仅表示修改应用成功。
fn verified_start_result(
    operation_id: String,
    state: RepositoryState,
    target: &str,
) -> OperationResult {
    let paths: Vec<_> = state
        .changes
        .iter()
        .filter(|change| change.kind == "conflicted")
        .map(|change| change.path.clone())
        .collect();
    if !paths.is_empty() {
        return OperationResult::NeedsResolution {
            operation_id,
            kind: OperationKind::Revert,
            conflict: ConflictSummary::Revert {
                target_commit_oid: target.into(),
                unresolved_count: paths.len() as u32,
                paths,
            },
            refresh: WriteRefresh::Ready { state },
        };
    }
    let branch_name = match &state.head {
        HeadState::Branch { name, .. } => Some(name.clone()),
        _ => None,
    };
    OperationResult::Succeeded {
        operation_id,
        kind: OperationKind::Revert,
        commit_oid: None,
        branch_name,
        clone_path: None,
        path_results: Vec::new(),
        refresh: WriteRefresh::Ready { state },
    }
}
