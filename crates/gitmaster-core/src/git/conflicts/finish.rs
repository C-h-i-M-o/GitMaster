//! 固定完整索引与双亲，显式提交后只清理确认过的合并元数据。
use super::*;
use crate::git::{
    coordinator::{CoordinationKey, OperationReporter, RepositoryCoordinator},
    merge::validate_merge_config,
    process::{inspect_local_config, run_local_git},
    write::{
        create_commit_object, operation_result, parse_oid, validate_message, IndexTransaction,
    },
    write_guard::{CommitIdentity, WriteFingerprint},
};
use std::ffi::OsString;

// 最后移除 MERGE_HEAD，前面的清理失败仍保留可识别的合并状态。
const MERGE_FILES: [&str; 5] = [
    "MERGE_MSG",
    "MERGE_MODE",
    "MERGE_RR",
    "AUTO_MERGE",
    "MERGE_HEAD",
];

/// 捕获元数据的内容和文件身份，缺失的可选文件也属于确认前提。
struct MergeFiles(BTreeMap<String, Option<FileBytes>>);

/// 最终提交计划不持有工作文件草稿，仅保存用户确认的完整索引与父提交。
struct FinishPlan {
    git: GitExecutable,
    repo: RepositoryHandle,
    session: String,
    fingerprint: WriteFingerprint,
    parents: Vec<String>,
    identity: CommitIdentity,
    message: String,
    metadata: MergeFiles,
}

/// 完整索引、双亲、身份和说明都在执行前展示，准备不产生用户仓库对象。
pub fn prepare_finish(
    coordinator: &RepositoryCoordinator,
    session: &ConflictSession,
    message: &str,
) -> Result<WritePreview, OperationError> {
    let generation = coordinator.begin_prepare()?;
    validate_message(message)?;
    let deadline = Instant::now() + BUDGET;
    let key = CoordinationKey::repository(&session.repo)?;
    let (plan, state, paths) =
        coordinator.read_until(&key, deadline, || capture(session, message, deadline))?;
    let parents = plan.parents.clone();
    let author = plan.identity.author_display();
    let branch = branch(&plan.fingerprint.head)?.to_owned();
    let prepared = coordinator.prepare(
        generation,
        Some(session.repo.id.clone()),
        key,
        OperationKind::FinishMerge,
        move |reporter| plan.execute_with(reporter, || Ok(())),
    )?;
    Ok(WritePreview {
        plan_id: prepared.plan_id,
        repository_id: session.repo.id.clone(),
        snapshot_id: state.snapshot_id,
        kind: OperationKind::FinishMerge,
        head: state.head,
        parent_oids: parents.clone(),
        paths,
        author: Some(author),
        message: Some(message.into()),
        target: Some(WriteTarget::LocalBranch {
            name: branch,
            oid: Some(parents[0].clone()),
        }),
        warnings: vec![
            "本次保存整个暂存区，并生成包含两个父提交的合并提交；未暂存内容保持不变。".into(),
        ],
        expires_at: prepared.expires_at,
    })
}

/// 冲突必须已经由真实索引解决；空树差异不等于没有需要保存的合并关系。
fn capture(
    session: &ConflictSession,
    message: &str,
    deadline: Instant,
) -> Result<(FinishPlan, RepositoryState, Vec<String>), OperationError> {
    validate_snapshot(&session.git, &session.repo, &session.snapshot.id, deadline)?;
    if session
        .snapshot
        .entries
        .values()
        .any(|entries| entries.keys().any(|stage| *stage != 0))
    {
        return Err(OperationError::new("UNRESOLVED_CONFLICTS"));
    }
    if session.snapshot.merge_heads.len() != 1 {
        return Err(OperationError::new("UNSUPPORTED_CONFLICT"));
    }
    let metadata = MergeFiles::capture(&session.repo, deadline)?;
    validate_finish_environment(
        &session.git,
        &session.repo,
        &session.snapshot.head,
        &session.snapshot.merge_heads[0],
        deadline,
    )?;
    let fingerprint = guard::merge_fingerprint(&session.git, &session.repo, false, deadline)?;
    let paths = guard::commit_paths(&session.git, &session.repo, &fingerprint, deadline)?;
    let identity = guard::identity(&session.git, &session.repo, deadline)?;
    let state = read_repository_state_until(&session.git, &session.repo, deadline)?;
    validate_snapshot(&session.git, &session.repo, &session.snapshot.id, deadline)?;
    metadata.validate(&session.repo, deadline)?;
    if guard::merge_fingerprint(&session.git, &session.repo, false, deadline)? != fingerprint {
        return Err(OperationError::new("STALE_WRITE_PLAN"));
    }
    Ok((
        FinishPlan {
            git: session.git.clone(),
            repo: session.repo.clone(),
            session: session.snapshot.id.clone(),
            fingerprint,
            parents: vec![
                session.snapshot.head.clone(),
                session.snapshot.merge_heads[0].clone(),
            ],
            identity,
            message: message.into(),
            metadata,
        },
        state,
        paths,
    ))
}

impl FinishPlan {
    /// 提交引用成功后不重放操作，元数据清理失败也不得伪装成无影响失败。
    fn execute_with(
        self,
        reporter: &OperationReporter,
        after_reference: impl FnOnce() -> Result<(), OperationError>,
    ) -> OperationResult {
        let deadline = reporter.deadline(BUDGET);
        let mut attempted = false;
        let mut committed = None;
        let name = branch(&self.fingerprint.head).map(str::to_owned).ok();
        let result = (|| {
            self.revalidate(false, deadline)?;
            let transaction =
                IndexTransaction::new(&self.repo, self.fingerprint.index_bytes.as_deref())?;
            self.revalidate(true, deadline)?;
            reporter.report(OperationPhase::Writing, None)?;
            let tree = parse_oid(guard::local_query(
                &self.git,
                &self.repo,
                &["write-tree"],
                &[],
                Some(&transaction.path),
                deadline,
            )?)?;
            let oid = create_commit_object(
                &self.git,
                &self.repo,
                &tree,
                &self.parents,
                &self.message,
                &self.identity,
                deadline,
            )?;
            self.revalidate(true, deadline)?;
            if !transaction.owns_lock() {
                return Err(OperationError::new("INDEX_LOCKED"));
            }
            let reference = format!("refs/heads/{}", branch(&self.fingerprint.head)?);
            let args = [
                "update-ref",
                "--no-deref",
                "-m",
                "gitMaster: 完成合并",
                &reference,
                &oid,
                &self.parents[0],
            ]
            .map(OsString::from);
            attempted = true;
            let output = run_local_git(&self.git, &self.repo.root, &args, &[], None, deadline);
            let actual = guard::local_query(
                &self.git,
                &self.repo,
                &["rev-parse", "--verify", &reference],
                &[],
                None,
                deadline,
            )
            .and_then(parse_oid);
            if actual.as_ref().is_ok_and(|actual| actual == &oid) {
                committed = Some(oid.clone());
            } else {
                if actual
                    .as_ref()
                    .is_ok_and(|actual| actual == &self.parents[0])
                    && output.as_ref().is_ok_and(|output| !output.success)
                {
                    attempted = false;
                    return Err(OperationError::new("STALE_WRITE_PLAN"));
                }
                return Err(OperationError::new("WRITE_OUTCOME_UNKNOWN"));
            }
            after_reference()?;
            reporter.report(OperationPhase::Verifying, None)?;
            let object = guard::local_query(
                &self.git,
                &self.repo,
                &["--no-replace-objects", "cat-file", "commit", &oid],
                &[],
                None,
                deadline,
            )?;
            let header = format!(
                "tree {tree}\nparent {}\nparent {}\n",
                self.parents[0], self.parents[1]
            );
            if !object.starts_with(header.as_bytes()) {
                return Err(OperationError::new("WRITE_OUTCOME_UNKNOWN"));
            }
            self.validate_after_reference(&oid, &transaction, deadline)?;
            self.metadata.remove(&self.repo, deadline)?;
            for name in MERGE_FILES.into_iter().chain(["MERGE_AUTOSTASH"]) {
                if optional_file(&self.repo, name, deadline)?.is_some() {
                    return Err(OperationError::new("WRITE_OUTCOME_UNKNOWN"));
                }
            }
            let state = read_repository_state_until(&self.git, &self.repo, deadline)?;
            if !state.operations.is_empty()
                || !matches!(&state.head, HeadState::Branch { name: current, oid: actual } if Some(current) == name.as_ref() && actual == &oid)
                || guard::read_index_bytes(&self.repo)? != self.fingerprint.index_bytes
            {
                return Err(OperationError::new("WRITE_OUTCOME_UNKNOWN"));
            }
            Ok(())
        })();
        // 已生成引用但清理失败时不能沿用普通提交的“仅刷新失败”成功分类。
        let result = if committed.is_some() && result.is_err() {
            Err(OperationError::new("WRITE_OUTCOME_UNKNOWN"))
        } else {
            result
        };
        operation_result(
            reporter, &self.git, &self.repo, result, attempted, committed, name, deadline,
        )
    }

    /// 引用发布前每次都重新核对完整确认输入与实际合并代次。
    fn revalidate(&self, own_lock: bool, deadline: Instant) -> Result<(), OperationError> {
        validate_snapshot(&self.git, &self.repo, &self.session, deadline)?;
        self.metadata.validate(&self.repo, deadline)?;
        validate_finish_environment(
            &self.git,
            &self.repo,
            &self.parents[0],
            &self.parents[1],
            deadline,
        )?;
        if guard::merge_fingerprint(&self.git, &self.repo, own_lock, deadline)? != self.fingerprint
        {
            return Err(OperationError::new("STALE_WRITE_PLAN"));
        }
        Ok(())
    }

    /// 只在当前分支仍指向本次提交且原索引、合并元数据仍完整时进行清理。
    fn validate_after_reference(
        &self,
        oid: &str,
        transaction: &IndexTransaction,
        deadline: Instant,
    ) -> Result<(), OperationError> {
        self.metadata.validate(&self.repo, deadline)?;
        let state = read_repository_state_until(&self.git, &self.repo, deadline)?;
        if !transaction.owns_lock()
            || state.operations != ["merge"]
            || !matches!(&state.head, HeadState::Branch { name, oid: actual } if name == branch(&self.fingerprint.head)? && actual == oid)
            || guard::read_index_bytes(&self.repo)? != self.fingerprint.index_bytes
        {
            return Err(OperationError::new("WRITE_OUTCOME_UNKNOWN"));
        }
        Ok(())
    }
}

impl MergeFiles {
    /// 捕获普通元数据文件，不跟随链接或等待 FIFO；不支持自动 stash 残留。
    fn capture(repo: &RepositoryHandle, deadline: Instant) -> Result<Self, OperationError> {
        if std::fs::symlink_metadata(repo.git_dir.join("MERGE_AUTOSTASH")).is_ok() {
            return Err(OperationError::new("UNSUPPORTED_CONFLICT"));
        }
        let mut files = BTreeMap::new();
        for name in MERGE_FILES {
            files.insert(name.into(), optional_file(repo, name, deadline)?);
        }
        if files["MERGE_HEAD"].is_none() {
            return Err(OperationError::new("STALE_CONFLICT"));
        }
        Ok(Self(files))
    }

    /// 包括缺失文件在内，任何新一轮合并或外部修改都使清理前提失效。
    fn validate(&self, repo: &RepositoryHandle, deadline: Instant) -> Result<(), OperationError> {
        if Self::capture(repo, deadline)?.0 != self.0 {
            return Err(OperationError::new("STALE_CONFLICT"));
        }
        Ok(())
    }

    /// 逐项再次核对字节与身份；不删除捕获后被替换的元数据。
    fn remove(&self, repo: &RepositoryHandle, deadline: Instant) -> Result<(), OperationError> {
        self.validate(repo, deadline)?;
        let dir = Dir::open_ambient_dir(&repo.git_dir, cap_std::ambient_authority())
            .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
        for name in MERGE_FILES {
            guard::check_time(deadline)?;
            if optional_file(repo, name, deadline)? != self.0[name] {
                return Err(OperationError::new("STALE_CONFLICT"));
            }
            if self.0[name].is_some() {
                dir.remove_file(name)
                    .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
            }
        }
        Ok(())
    }
}

/// 可选合并文件的缺失与读取失败区分，权限或特殊文件错误不能当作不存在。
fn optional_file(
    repo: &RepositoryHandle,
    name: &str,
    deadline: Instant,
) -> Result<Option<FileBytes>, OperationError> {
    match std::fs::symlink_metadata(repo.git_dir.join(name)) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(OperationError::new("ACCESS_DENIED")),
        Ok(_) => read_regular(&repo.git_dir, name, MAX, deadline).map(Some),
    }
}

/// 来源已经包含于当前 HEAD 通常表示提交后清理中断，拒绝再次生成重复合并。
fn validate_finish_environment(
    git: &GitExecutable,
    repo: &RepositoryHandle,
    head: &str,
    incoming: &str,
    deadline: Instant,
) -> Result<(), OperationError> {
    validate_merge_config(git, repo, deadline)?;
    let config = guard::parse_config(&inspect_local_config(git, &repo.root, deadline)?)?;
    if config
        .get("extensions.refstorage")
        .is_some_and(|value| value != "files")
    {
        return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
    }
    let args = [
        "--no-replace-objects",
        "merge-base",
        "--is-ancestor",
        incoming,
        head,
    ]
    .map(OsString::from);
    match run_local_git(git, &repo.root, &args, &[], None, deadline)?.exit_code {
        Some(1) => Ok(()),
        Some(0) => Err(OperationError::new("STALE_CONFLICT")),
        _ => Err(OperationError::new("GIT_EXECUTION_FAILED")),
    }
}

/// 完成合并必须写回明确本地分支，不能更新 detached HEAD。
fn branch(head: &HeadState) -> Result<&str, OperationError> {
    match head {
        HeadState::Branch { name, .. } => Ok(name),
        _ => Err(OperationError::new("DETACHED_HEAD_WRITE_BLOCKED")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::repository::{open_repository, tests::Fixture};
    use std::fs;

    /// 在真实冲突上采用本地版本，让索引无差异而仍需要保存双亲关系。
    fn resolved() -> (Fixture, RepositoryHandle, ConflictSession) {
        let (fixture, _) = super::super::tests::branch_conflict();
        fixture.command(&["checkout", "--ours", "--", "file"]);
        fixture.command(&["add", "--", "file"]);
        let (repo, _) = open_repository(&fixture.git, &fixture.root).unwrap();
        let session = ConflictSession::new(&fixture.git, &repo).unwrap();
        (fixture, repo, session)
    }

    /// 对同一真实计划只启动一次，并等待该句柄的实际终态。
    fn wait(
        coordinator: &RepositoryCoordinator,
        repo: &RepositoryHandle,
        id: &str,
    ) -> OperationResult {
        let handle = coordinator.execute(Some(&repo.id), id).unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if let Some(result) = coordinator
                .read_operation(Some(&handle.operation_id))
                .unwrap()
                .and_then(|record| record.result)
            {
                return result;
            }
            assert!(Instant::now() < deadline, "合并提交没有在测试期限内完成");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    /// 引用发布后允许测试注入真实外部变更，业务实现始终使用同一执行路径。
    fn execute(
        plan: FinishPlan,
        after_reference: impl FnOnce() -> Result<(), OperationError> + Send + 'static,
    ) -> OperationResult {
        let coordinator = RepositoryCoordinator::new();
        let repo = plan.repo.clone();
        let prepared = coordinator
            .prepare(
                coordinator.begin_prepare().unwrap(),
                Some(repo.id.clone()),
                CoordinationKey::repository(&repo).unwrap(),
                OperationKind::FinishMerge,
                move |reporter| plan.execute_with(reporter, after_reference),
            )
            .unwrap();
        wait(&coordinator, &repo, &prepared.plan_id)
    }

    /// 引用成功而清理失败必须保留合并状态，并阻止再次准备出重复提交。
    #[test]
    fn cleanup_failure_is_unknown_and_cannot_create_duplicate_merge() {
        let (fixture, repo, session) = resolved();
        let head = session.snapshot.head.clone();
        let index = fs::read(repo.git_dir.join("index")).unwrap();
        let (plan, _, _) = capture(&session, "完成合并", Instant::now() + BUDGET).unwrap();
        let result = execute(plan, || Err(OperationError::new("ACCESS_DENIED")));
        assert!(
            matches!(result, OperationResult::Unknown { .. }),
            "{result:?}"
        );
        let current =
            read_repository_state_until(&fixture.git, &repo, Instant::now() + BUDGET).unwrap();
        assert!(matches!(current.head, HeadState::Branch { ref oid, .. } if oid != &head));
        assert_eq!(current.operations, ["merge"]);
        assert_eq!(fs::read(repo.git_dir.join("index")).unwrap(), index);
        let current_session = ConflictSession::new(&fixture.git, &repo).unwrap();
        let error = prepare_finish(&RepositoryCoordinator::new(), &current_session, "再次合并")
            .unwrap_err();
        assert_eq!(error.code, "STALE_CONFLICT");
    }

    /// 后来的 MERGE_HEAD 即使内容相同也属于外部文件，不能被本次清理删除。
    #[test]
    fn replaced_merge_metadata_after_reference_is_preserved() {
        let (_fixture, repo, session) = resolved();
        let original = fs::read(repo.git_dir.join("MERGE_HEAD")).unwrap();
        let message = fs::read(repo.git_dir.join("MERGE_MSG")).unwrap();
        let (plan, _, _) = capture(&session, "完成合并", Instant::now() + BUDGET).unwrap();
        let directory = repo.git_dir.clone();
        let replacement = original.clone();
        let result = execute(plan, move || {
            fs::rename(
                directory.join("MERGE_HEAD"),
                directory.join("previous-merge-head"),
            )
            .unwrap();
            fs::write(directory.join("MERGE_HEAD"), replacement).unwrap();
            Ok(())
        });
        assert!(
            matches!(result, OperationResult::Unknown { .. }),
            "{result:?}"
        );
        assert_eq!(fs::read(repo.git_dir.join("MERGE_HEAD")).unwrap(), original);
        assert_eq!(fs::read(repo.git_dir.join("MERGE_MSG")).unwrap(), message);
    }

    /// 既有锁在写对象或引用前拒绝，用户锁文件和原 HEAD 保持不变。
    #[test]
    fn existing_index_lock_prevents_merge_commit() {
        let (fixture, repo, session) = resolved();
        let (plan, _, _) = capture(&session, "完成合并", Instant::now() + BUDGET).unwrap();
        fs::write(repo.git_dir.join("index.lock"), b"external-lock").unwrap();
        let result = execute(plan, || Ok(()));
        assert!(
            matches!(result, OperationResult::Failed { ref error, .. } if error.code == "INDEX_LOCKED"),
            "{result:?}"
        );
        let state =
            read_repository_state_until(&fixture.git, &repo, Instant::now() + BUDGET).unwrap();
        assert!(
            matches!(state.head, HeadState::Branch { oid, .. } if oid == session.snapshot.head)
        );
        assert_eq!(
            fs::read(repo.git_dir.join("index.lock")).unwrap(),
            b"external-lock"
        );
    }

    /// 外部自动 stash 和 octopus 元数据不进入首版双亲完成路径。
    #[test]
    fn autostash_and_multiple_merge_heads_are_rejected() {
        for autostash in [false, true] {
            let (fixture, repo, session) = resolved();
            if autostash {
                fs::write(
                    repo.git_dir.join("MERGE_AUTOSTASH"),
                    b"not-a-supported-stash",
                )
                .unwrap();
            } else {
                let heads = format!(
                    "{}\n{}\n",
                    session.snapshot.merge_heads[0], session.snapshot.head
                );
                fs::write(repo.git_dir.join("MERGE_HEAD"), heads).unwrap();
            }
            let session = ConflictSession::new(&fixture.git, &repo).unwrap();
            let error =
                prepare_finish(&RepositoryCoordinator::new(), &session, "完成合并").unwrap_err();
            assert_eq!(error.code, "UNSUPPORTED_CONFLICT");
        }
    }

    /// 编辑器保存与最终提交在两个确认计划中串联，最后才结束真实 merge 状态。
    #[test]
    fn save_then_finish_commits_confirmed_result_without_touching_unstaged_files() {
        let (fixture, _) = super::super::tests::branch_conflict();
        let (repo, _) = open_repository(&fixture.git, &fixture.root).unwrap();
        let session = ConflictSession::new(&fixture.git, &repo).unwrap();
        let id = session.state.files[0].conflict_id.clone();
        let doc = session.read_document(&id).unwrap();
        let coordinator = RepositoryCoordinator::new();
        let preview = super::super::prepare_save(
            &coordinator,
            &session,
            &id,
            &doc.fingerprint,
            "编辑器确认结果\n",
        )
        .unwrap();
        let result = wait(&coordinator, &repo, &preview.plan_id);
        assert!(
            matches!(
                result,
                OperationResult::Succeeded {
                    commit_oid: None,
                    ..
                }
            ),
            "{result:?}"
        );
        assert!(repo.git_dir.join("MERGE_HEAD").exists());
        // 重建句柄和协调器，验证完成路径不依赖保存时的内存计划或文件 ID。
        let (repo, _) = open_repository(&fixture.git, &fixture.root).unwrap();
        let coordinator = RepositoryCoordinator::new();
        let session = ConflictSession::new(&fixture.git, &repo).unwrap();
        let preview = prepare_finish(&coordinator, &session, "合并结果\n\n保留说明").unwrap();
        fixture.write("file", b"later draft\n");
        let result = wait(&coordinator, &repo, &preview.plan_id);
        let oid = match result {
            OperationResult::Succeeded {
                commit_oid: Some(oid),
                ..
            } => oid,
            _ => panic!("{result:?}"),
        };
        let committed = query_until(
            &fixture.git,
            &repo.root,
            &["show", &format!("{oid}:file")],
            MAX,
            None,
        )
        .unwrap();
        assert_eq!(committed, "编辑器确认结果\n".as_bytes());
        assert_eq!(fs::read(repo.root.join("file")).unwrap(), b"later draft\n");
        assert!(!repo.git_dir.join("MERGE_HEAD").exists());
    }
}
