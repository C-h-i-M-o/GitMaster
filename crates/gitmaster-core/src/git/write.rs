//! 确认后才执行的整文件暂存、取消暂存和本地提交。

use super::{
    coordinator::{CoordinationKey, OperationReporter, RepositoryCoordinator},
    process::run_local_git,
    repository::next_id,
    write_guard::{self as guard, IndexEntry, WriteFingerprint},
    *,
};
use std::{
    collections::BTreeSet,
    ffi::OsString,
    io::Write,
    path::PathBuf,
    time::{Duration, Instant},
};

/// 本地准备与执行各有独立总预算。
const LOCAL_BUDGET: Duration = Duration::from_secs(120);

/// 只读准备整文件暂存或取消暂存；changeId 由后端当前快照映射到路径。
pub fn prepare_index_change(
    coordinator: &RepositoryCoordinator,
    git: &GitExecutable,
    repo: &RepositoryHandle,
    state: &RepositoryState,
    change_ids: &[String],
    stage: bool,
) -> Result<WritePreview, OperationError> {
    let generation = coordinator.begin_prepare()?;
    let deadline = Instant::now() + LOCAL_BUDGET;
    let key = CoordinationKey::repository(repo)?;
    let (paths, warnings, fingerprint) = coordinator.read_until(&key, deadline, || {
        guard::validate_snapshot(git, repo, state, deadline)?;
        if change_ids.is_empty() {
            return Err(OperationError::new("EMPTY_SELECTION"));
        }
        let mut paths = BTreeSet::new();
        let mut warnings = Vec::new();
        for id in change_ids {
            let change = state
                .changes
                .iter()
                .find(|change| &change.change_id == id)
                .ok_or_else(|| OperationError::new("STALE_REQUEST"))?;
            if change.kind == "conflicted" {
                return Err(OperationError::new("CONFLICT_PRESENT"));
            }
            let valid = if stage {
                change.kind == "untracked" || change.worktree_status != "."
            } else {
                change.kind == "tracked" && change.index_status != "."
            };
            if !valid {
                return Err(OperationError::new("INVALID_INPUT"));
            }
            paths.insert(change.path.clone());
            if let Some(original) = &change.original_path {
                paths.insert(original.clone());
            }
            if stage && change.index_status != "." && change.worktree_status != "." {
                warnings.push(format!(
                    "{}：重新暂存整个文件将替换其已有的部分暂存。",
                    change.path
                ));
            }
        }
        let paths = paths.into_iter().collect::<Vec<_>>();
        let fingerprint = guard::fingerprint(git, repo, &paths, false, deadline)?;
        Ok((paths, warnings, fingerprint))
    })?;
    let kind = if stage {
        OperationKind::Stage
    } else {
        OperationKind::Unstage
    };
    let prepared_git = git.clone();
    let prepared_repo = repo.clone();
    let selected = paths.clone();
    let plan = coordinator.prepare(
        generation,
        Some(repo.id.clone()),
        key,
        kind,
        move |reporter| {
            execute_index(
                reporter,
                &prepared_git,
                &prepared_repo,
                &fingerprint,
                &selected,
                stage,
            )
        },
    )?;
    Ok(WritePreview {
        plan_id: plan.plan_id,
        repository_id: repo.id.clone(),
        snapshot_id: state.snapshot_id.clone(),
        kind,
        head: state.head.clone(),
        parent_oids: parents(&state.head),
        paths,
        author: None,
        message: None,
        target: None,
        warnings,
        expires_at: plan.expires_at,
    })
}

/// 预览完整暂存内容与身份，准备阶段不调用任何会创建对象的命令。
pub fn prepare_commit(
    coordinator: &RepositoryCoordinator,
    git: &GitExecutable,
    repo: &RepositoryHandle,
    state: &RepositoryState,
    message: &str,
) -> Result<WritePreview, OperationError> {
    let generation = coordinator.begin_prepare()?;
    validate_message(message)?;
    let deadline = Instant::now() + LOCAL_BUDGET;
    let key = CoordinationKey::repository(repo)?;
    let (fingerprint, paths, identity) = coordinator.read_until(&key, deadline, || {
        guard::validate_snapshot(git, repo, state, deadline)?;
        let fingerprint = guard::fingerprint(git, repo, &[], false, deadline)?;
        let paths = guard::commit_paths(git, repo, &fingerprint, deadline)?;
        if paths.is_empty() {
            return Err(OperationError::new("NOTHING_TO_COMMIT"));
        }
        let identity = guard::identity(git, repo, deadline)?;
        Ok((fingerprint, paths, identity))
    })?;
    let author = identity.author_display();
    let prepared_git = git.clone();
    let prepared_repo = repo.clone();
    let description = message.to_owned();
    let plan = coordinator.prepare(
        generation,
        Some(repo.id.clone()),
        key,
        OperationKind::Commit,
        move |reporter| {
            execute_commit(
                reporter,
                &prepared_git,
                &prepared_repo,
                &fingerprint,
                &description,
                &identity,
            )
        },
    )?;
    Ok(WritePreview {
        plan_id: plan.plan_id,
        repository_id: repo.id.clone(),
        snapshot_id: state.snapshot_id.clone(),
        kind: OperationKind::Commit,
        head: state.head.clone(),
        parent_oids: parents(&state.head),
        paths,
        author: Some(author),
        message: Some(message.to_owned()),
        target: Some(WriteTarget::LocalBranch {
            name: branch_name(&state.head)?.to_owned(),
            oid: parents(&state.head).into_iter().next(),
        }),
        warnings: Vec::new(),
        expires_at: plan.expires_at,
    })
}

/// 校验说明而不裁剪用户输入的换行或空格。
pub(crate) fn validate_message(message: &str) -> Result<(), OperationError> {
    if message.trim().is_empty() || message.len() > 65536 || message.contains('\0') {
        Err(OperationError::new("INVALID_INPUT"))
    } else {
        Ok(())
    }
}

/// 根据真实 HEAD 生成父提交列表，unborn 没有父提交。
fn parents(head: &HeadState) -> Vec<String> {
    match head {
        HeadState::Branch { oid, .. } | HeadState::Detached { oid } => vec![oid.clone()],
        HeadState::Unborn { .. } => Vec::new(),
    }
}

/// 本地保存只允许具名分支，不把 detached 伪装成 main。
fn branch_name(head: &HeadState) -> Result<&str, OperationError> {
    match head {
        HeadState::Branch { name, .. } | HeadState::Unborn { name } => Ok(name),
        _ => Err(OperationError::new("DETACHED_HEAD_WRITE_BLOCKED")),
    }
}

/// 出队后再次校验确认前提，不只比较状态字母。
fn revalidate(
    git: &GitExecutable,
    repo: &RepositoryHandle,
    expected: &WriteFingerprint,
    paths: &[String],
    own_lock: bool,
    deadline: Instant,
) -> Result<(), OperationError> {
    let actual = guard::fingerprint(git, repo, paths, own_lock, deadline)?;
    if &actual != expected {
        return Err(OperationError::new("STALE_WRITE_PLAN"));
    }
    Ok(())
}

/// 在独立索引完成 Git 调整，确认无外部变化和无额外路径影响后才发布。
fn execute_index(
    reporter: &OperationReporter,
    git: &GitExecutable,
    repo: &RepositoryHandle,
    expected: &WriteFingerprint,
    paths: &[String],
    stage: bool,
) -> OperationResult {
    let deadline = Instant::now() + LOCAL_BUDGET;
    let mut published = false;
    let result = (|| -> Result<(), OperationError> {
        revalidate(git, repo, expected, paths, false, deadline)?;
        let mut transaction = IndexTransaction::new(repo, expected.index_bytes.as_deref())?;
        revalidate(git, repo, expected, paths, true, deadline)?;
        transaction.initialize(git, repo, deadline)?;
        reporter.report(OperationPhase::Writing, None)?;
        let mut input = Vec::new();
        for path in paths {
            input.extend_from_slice(path.as_bytes());
            input.push(0);
        }
        if stage {
            let entries = super::staging::prepare_entries(git, repo, expected, paths, deadline)?;
            guard::local_query(
                git,
                repo,
                &["update-index", "--force-remove", "-z", "--stdin"],
                &input,
                Some(&transaction.path),
                deadline,
            )?;
            let mut index_input = Vec::new();
            for entry in entries {
                index_input.extend_from_slice(
                    format!("{} {}\t{}", entry.mode, entry.oid, entry.path).as_bytes(),
                );
                index_input.push(0);
            }
            if !index_input.is_empty() {
                guard::local_query(
                    git,
                    repo,
                    &["update-index", "-z", "--index-info"],
                    &index_input,
                    Some(&transaction.path),
                    deadline,
                )?;
            }
        } else if matches!(expected.head, HeadState::Unborn { .. }) {
            guard::local_query(
                git,
                repo,
                &["update-index", "--force-remove", "-z", "--stdin"],
                &input,
                Some(&transaction.path),
                deadline,
            )?;
        } else {
            let parent = parents(&expected.head)
                .into_iter()
                .next()
                .ok_or_else(|| OperationError::new("HEAD_REQUIRED"))?;
            guard::local_query(
                git,
                repo,
                &[
                    "--literal-pathspecs",
                    "reset",
                    "--no-refresh",
                    &parent,
                    "--pathspec-from-file=-",
                    "--pathspec-file-nul",
                ],
                &input,
                Some(&transaction.path),
                deadline,
            )?;
        }
        let after = guard::read_index(git, repo, Some(&transaction.path), deadline)?;
        if unselected(&expected.index, paths) != unselected(&after, paths) {
            return Err(OperationError::new("WRITE_OUTCOME_UNKNOWN"));
        }
        revalidate(git, repo, expected, paths, true, deadline)?;
        transaction.publish()?;
        published = true;
        reporter.report(OperationPhase::Verifying, None)?;
        if guard::read_index(git, repo, None, deadline)? != after {
            return Err(OperationError::new("WRITE_OUTCOME_UNKNOWN"));
        }
        Ok(())
    })();
    operation_result(reporter, git, repo, result, published, None, None, deadline)
}

/// 索引树在独立副本创建；提交目标通过完整引用和旧 OID 比较交换。
fn execute_commit(
    reporter: &OperationReporter,
    git: &GitExecutable,
    repo: &RepositoryHandle,
    expected: &WriteFingerprint,
    message: &str,
    identity: &guard::CommitIdentity,
) -> OperationResult {
    let deadline = Instant::now() + LOCAL_BUDGET;
    let mut commit_oid = None;
    let mut ref_attempted = false;
    let result = (|| -> Result<(), OperationError> {
        revalidate(git, repo, expected, &[], false, deadline)?;
        let transaction = IndexTransaction::new(repo, expected.index_bytes.as_deref())?;
        revalidate(git, repo, expected, &[], true, deadline)?;
        transaction.initialize(git, repo, deadline)?;
        let tree = parse_oid(guard::local_query(
            git,
            repo,
            &["write-tree"],
            &[],
            Some(&transaction.path),
            deadline,
        )?)?;
        let parent_oids = parents(&expected.head);
        reporter.report(OperationPhase::Writing, None)?;
        let oid =
            create_commit_object(git, repo, &tree, &parent_oids, message, identity, deadline)?;
        revalidate(git, repo, expected, &[], true, deadline)?;
        let branch = branch_name(&expected.head)?;
        let reference = format!("refs/heads/{branch}");
        let zero = "0".repeat(oid.len());
        let old = parent_oids.first().unwrap_or(&zero);
        let args = [
            "update-ref",
            "--no-deref",
            "-m",
            "gitMaster: 保存到本机",
            &reference,
            &oid,
            old,
        ]
        .map(OsString::from);
        ref_attempted = true;
        let output = run_local_git(git, &repo.root, &args, &[], None, deadline);
        // 即使响应失败，仍查询目标，不能自动重放 update-ref。
        let actual = guard::local_query(
            git,
            repo,
            &["rev-parse", "--verify", &reference],
            &[],
            None,
            deadline,
        )
        .and_then(parse_oid);
        if actual.as_ref().is_ok_and(|actual| actual == &oid) {
            commit_oid = Some(oid.clone());
        } else {
            return match output {
                Ok(out) if !out.success => {
                    ref_attempted = false;
                    Err(OperationError::new("STALE_WRITE_PLAN"))
                }
                _ => Err(OperationError::new("WRITE_OUTCOME_UNKNOWN")),
            };
        }
        reporter.report(OperationPhase::Verifying, None)?;
        let actual_tree = parse_oid(guard::local_query(
            git,
            repo,
            &["rev-parse", &format!("{oid}^{{tree}}")],
            &[],
            None,
            deadline,
        )?)?;
        if actual_tree != tree {
            return Err(OperationError::new("WRITE_OUTCOME_UNKNOWN"));
        }
        Ok(())
    })();
    let branch = branch_name(&expected.head).ok().map(str::to_owned);
    operation_result(
        reporter,
        git,
        repo,
        result,
        ref_attempted,
        commit_oid,
        branch,
        deadline,
    )
}

/// 用固定索引树、父提交和说明创建对象，引用发布由调用方单独核验。
pub(crate) fn create_commit_object(
    git: &GitExecutable,
    repo: &RepositoryHandle,
    tree: &str,
    parent_oids: &[String],
    message: &str,
    identity: &guard::CommitIdentity,
    deadline: Instant,
) -> Result<String, OperationError> {
    let mut args = identity.arguments();
    args.extend(["commit-tree".to_owned(), tree.to_owned()]);
    for parent in parent_oids {
        args.extend(["-p".to_owned(), parent.clone()]);
    }
    let args = args.iter().map(String::as_str).collect::<Vec<_>>();
    parse_oid(guard::local_query(
        git,
        repo,
        &args,
        message.as_bytes(),
        None,
        deadline,
    )?)
}

/// 从结果中保留实际成功和刷新失败，不让界面误以为需要重复提交。
pub(crate) fn operation_result(
    reporter: &OperationReporter,
    git: &GitExecutable,
    repo: &RepositoryHandle,
    result: Result<(), OperationError>,
    may_have_written: bool,
    commit_oid: Option<String>,
    branch_name: Option<String>,
    deadline: Instant,
) -> OperationResult {
    let refresh = match super::repository::read_repository_state_until(git, repo, deadline) {
        Ok(state) => WriteRefresh::Ready { state },
        Err(error) => WriteRefresh::Failed { error },
    };
    let operation_id = reporter.handle.operation_id.clone();
    let kind = reporter.kind;
    match result {
        Ok(()) => OperationResult::Succeeded {
            operation_id,
            kind,
            commit_oid,
            branch_name,
            clone_path: None,
            refresh,
        },
        Err(error) if commit_oid.is_some() && error.code != "WRITE_OUTCOME_UNKNOWN" => {
            OperationResult::Succeeded {
                operation_id,
                kind,
                commit_oid,
                branch_name,
                clone_path: None,
                refresh: WriteRefresh::Failed { error },
            }
        }
        Err(error) if may_have_written || error.code == "WRITE_OUTCOME_UNKNOWN" => {
            OperationResult::Unknown {
                operation_id,
                kind,
                error: OperationError::new("WRITE_OUTCOME_UNKNOWN"),
                clone_recovery: None,
                refresh,
            }
        }
        Err(error) => OperationResult::Failed {
            operation_id,
            kind,
            error,
            clone_recovery: None,
            refresh,
        },
    }
}

/// 只比较未选择的条目，防止 Git 参数或重命名解析扩大本次影响范围。
fn unselected<'a>(entries: &'a [IndexEntry], paths: &[String]) -> Vec<&'a IndexEntry> {
    entries
        .iter()
        .filter(|entry| !paths.contains(&entry.path))
        .collect()
}

/// Git OID 长度随对象格式变化，校验十六进制而不限定 SHA-1。
pub(crate) fn parse_oid(bytes: Vec<u8>) -> Result<String, OperationError> {
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| OperationError::new("PARSE_FAILED"))?
        .trim();
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(OperationError::new("PARSE_FAILED"));
    }
    Ok(text.to_owned())
}

/// 真实索引锁及本次专用索引；只能清理当前任务成功创建的文件。
pub(crate) struct IndexTransaction {
    dir: cap_std::fs::Dir,
    lock: Option<cap_std::fs::File>,
    name: String,
    pub(crate) path: PathBuf,
    empty: bool,
    temp_owned: bool,
    published: bool,
}

impl IndexTransaction {
    /// 用 create_new 获取 Git 索引锁，已有锁绝不删除。
    pub(crate) fn new(
        repo: &RepositoryHandle,
        bytes: Option<&[u8]>,
    ) -> Result<Self, OperationError> {
        let dir = cap_std::fs::Dir::open_ambient_dir(&repo.git_dir, cap_std::ambient_authority())
            .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
        let mut options = cap_std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        let lock = dir.open_with("index.lock", &options).map_err(|error| {
            OperationError::new(if error.kind() == std::io::ErrorKind::AlreadyExists {
                "INDEX_LOCKED"
            } else {
                "ACCESS_DENIED"
            })
        })?;
        let name = format!("gitmaster-{}-{}.index", std::process::id(), next_id());
        let mut transaction = Self {
            path: repo.git_dir.join(&name),
            dir,
            lock: Some(lock),
            name,
            empty: bytes.is_none(),
            temp_owned: false,
            published: false,
        };
        let mut file = transaction
            .dir
            .open_with(&transaction.name, &options)
            .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
        transaction.temp_owned = true;
        if let Some(bytes) = bytes {
            file.write_all(bytes)
                .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
        } else {
            drop(file);
            transaction
                .dir
                .remove_file(&transaction.name)
                .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
        }
        Ok(transaction)
    }

    /// unborn 没有索引文件时，仅在执行阶段初始化临时空索引。
    fn initialize(
        &self,
        git: &GitExecutable,
        repo: &RepositoryHandle,
        deadline: Instant,
    ) -> Result<(), OperationError> {
        if self.empty {
            guard::local_query(
                git,
                repo,
                &["read-tree", "--empty"],
                &[],
                Some(&self.path),
                deadline,
            )?;
        }
        Ok(())
    }

    /// 用已打开文件的身份检查锁路径，外部替换后不再拥有删除或替换权限。
    pub(crate) fn owns_lock(&self) -> bool {
        let Some(file) = &self.lock else {
            return false;
        };
        let (Ok(opened), Ok(current)) = (file.metadata(), self.dir.symlink_metadata("index.lock"))
        else {
            return false;
        };
        #[cfg(unix)]
        {
            use cap_std::fs::MetadataExt;
            opened.dev() == current.dev() && opened.ino() == current.ino() && current.is_file()
        }
        #[cfg(windows)]
        {
            current.is_file()
                && !super::windows_fs::is_reparse(&current)
                && super::windows_fs::same(&opened, &current)
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = (opened, current);
            false
        }
    }

    /// 完整写入锁文件并同步，再将其原子替换为真实索引。
    pub(crate) fn publish(&mut self) -> Result<(), OperationError> {
        let bytes = guard::read_index_file(&self.dir, &self.name)?
            .ok_or_else(|| OperationError::new("STALE_WRITE_PLAN"))?;
        if !self.owns_lock() {
            return Err(OperationError::new("INDEX_LOCKED"));
        }
        let file = self.lock.as_mut().expect("任务持有索引锁");
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
        if !self.owns_lock() {
            return Err(OperationError::new("INDEX_LOCKED"));
        }
        self.dir
            .rename("index.lock", &self.dir, "index")
            .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
        self.published = true;
        Ok(())
    }
}

impl Drop for IndexTransaction {
    /// 释放本次拥有的锁和临时索引，不触碰任何预先存在的 Git 锁。
    fn drop(&mut self) {
        if !self.published && self.owns_lock() {
            let _ = self.dir.remove_file("index.lock");
        }
        self.lock.take();
        if self.temp_owned {
            let _ = self.dir.remove_file(&self.name);
            let _ = self.dir.remove_file(format!("{}.lock", self.name));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::repository::{open_repository, query, read_repository_state, tests::Fixture};
    use std::{
        fs,
        time::{Duration, Instant},
    };

    /// 等待真实后台结果，超过期限明确失败。
    fn finish(
        coordinator: &RepositoryCoordinator,
        repo: &RepositoryHandle,
        preview: &WritePreview,
    ) -> OperationResult {
        let handle = coordinator
            .execute(Some(&repo.id), &preview.plan_id)
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let record = coordinator
                .read_operation(Some(&handle.operation_id))
                .unwrap()
                .unwrap();
            if let Some(result) = record.result {
                return result;
            }
            assert!(Instant::now() < deadline, "写操作超出测试期限");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    /// 整文件暂存 MM 时只更新选择项，已暂存的其他文件和工作区均保留。
    #[test]
    fn stages_selected_mm_and_preserves_other_index_entries() {
        let f = Fixture::new();
        f.write("a", b"base\n");
        f.write("b", b"base\n");
        f.command(&["add", "."]);
        f.command(&["commit", "-m", "base"]);
        f.write("a", b"partial\n");
        f.write("b", b"other staged\n");
        f.command(&["add", "."]);
        f.write("a", b"working\n");
        f.write("b", b"other working\n");
        let (repo, state) = open_repository(&f.git, &f.root).unwrap();
        let coordinator = RepositoryCoordinator::new();
        let index_before = fs::read(repo.git_dir.join("index")).unwrap();
        let id = state
            .changes
            .iter()
            .find(|file| file.path == "a")
            .unwrap()
            .change_id
            .clone();
        let preview =
            prepare_index_change(&coordinator, &f.git, &repo, &state, &[id], true).unwrap();
        assert_eq!(fs::read(repo.git_dir.join("index")).unwrap(), index_before);
        assert!(!preview.warnings.is_empty());
        assert!(matches!(
            finish(&coordinator, &repo, &preview),
            OperationResult::Succeeded { .. }
        ));
        assert_eq!(
            query(&f.git, &repo.root, &["show", ":a"], 4096).unwrap(),
            b"working\n"
        );
        assert_eq!(
            query(&f.git, &repo.root, &["show", ":b"], 4096).unwrap(),
            b"other staged\n"
        );
        assert_eq!(fs::read(f.root.join("b")).unwrap(), b"other working\n");
    }

    /// unborn 取消暂存保留文件；重新暂存后的首次提交使用完整确认索引。
    #[test]
    fn unborn_unstage_and_first_commit_preserve_unstaged_content() {
        let f = Fixture::new();
        f.write("中文 空格", b"first\n");
        f.command(&["add", "."]);
        let coordinator = RepositoryCoordinator::new();
        let (repo, state) = open_repository(&f.git, &f.root).unwrap();
        let preview = prepare_index_change(
            &coordinator,
            &f.git,
            &repo,
            &state,
            &[state.changes[0].change_id.clone()],
            false,
        )
        .unwrap();
        assert!(matches!(
            finish(&coordinator, &repo, &preview),
            OperationResult::Succeeded { .. }
        ));
        assert_eq!(fs::read(f.root.join("中文 空格")).unwrap(), b"first\n");
        assert!(query(&f.git, &repo.root, &["ls-files", "-z"], 4096)
            .unwrap()
            .is_empty());
        f.command(&["add", "."]);
        f.write("中文 空格", b"working\n");
        let state = read_repository_state(&f.git, &repo).unwrap();
        let preview = prepare_commit(
            &coordinator,
            &f.git,
            &repo,
            &state,
            "首个版本\n\n保留说明\n",
        )
        .unwrap();
        assert_eq!(preview.paths, ["中文 空格"]);
        assert!(matches!(
            finish(&coordinator, &repo, &preview),
            OperationResult::Succeeded {
                commit_oid: Some(_),
                ..
            }
        ));
        assert_eq!(
            query(&f.git, &repo.root, &["show", "HEAD:中文 空格"], 4096).unwrap(),
            b"first\n"
        );
        assert_eq!(fs::read(f.root.join("中文 空格")).unwrap(), b"working\n");
    }

    /// 状态仍为 M 但工作内容变化，执行必须拒绝旧确认。
    #[test]
    fn same_status_external_edit_invalidates_prepared_stage() {
        let f = Fixture::new();
        f.write("a", b"base");
        f.command(&["add", "."]);
        f.command(&["commit", "-m", "base"]);
        f.write("a", b"one");
        let (repo, state) = open_repository(&f.git, &f.root).unwrap();
        let coordinator = RepositoryCoordinator::new();
        let preview = prepare_index_change(
            &coordinator,
            &f.git,
            &repo,
            &state,
            &[state.changes[0].change_id.clone()],
            true,
        )
        .unwrap();
        f.write("a", b"two");
        assert!(
            matches!(finish(&coordinator, &repo, &preview), OperationResult::Failed { error, .. } if error.code == "STALE_WRITE_PLAN")
        );
        assert_eq!(
            query(&f.git, &repo.root, &["show", ":a"], 4096).unwrap(),
            b"base"
        );
    }

    /// 恶意路径作为字面文件名处理，二进制整文件暂存不被误转码。
    #[test]
    fn literal_untracked_names_and_binary_stage() {
        let f = Fixture::new();
        let target = if cfg!(windows) {
            "-[target].txt"
        } else {
            ":(glob)*"
        };
        f.write(target, b"a\0b\xff");
        f.write("other", b"keep untracked");
        let (repo, state) = open_repository(&f.git, &f.root).unwrap();
        let coordinator = RepositoryCoordinator::new();
        let id = state
            .changes
            .iter()
            .find(|file| file.path == target)
            .unwrap()
            .change_id
            .clone();
        let preview =
            prepare_index_change(&coordinator, &f.git, &repo, &state, &[id], true).unwrap();
        assert!(matches!(
            finish(&coordinator, &repo, &preview),
            OperationResult::Succeeded { .. }
        ));
        assert_eq!(
            query(&f.git, &repo.root, &["show", &format!(":{target}")], 4096).unwrap(),
            b"a\0b\xff"
        );
        assert_eq!(
            query(&f.git, &repo.root, &["ls-files", "-z"], 4096).unwrap(),
            [target.as_bytes(), b"\0"].concat()
        );
    }

    /// 已识别重命名取消暂存同时处理新旧路径，磁盘上的新文件保持不变。
    #[test]
    fn rename_unstage_and_delete_stage() {
        let f = Fixture::new();
        f.write("old", b"base");
        f.write("remove", b"gone");
        f.command(&["add", "."]);
        f.command(&["commit", "-m", "base"]);
        f.command(&["mv", "old", "new"]);
        let (repo, state) = open_repository(&f.git, &f.root).unwrap();
        let coordinator = RepositoryCoordinator::new();
        let change = state
            .changes
            .iter()
            .find(|file| file.original_path.is_some())
            .unwrap();
        let preview = prepare_index_change(
            &coordinator,
            &f.git,
            &repo,
            &state,
            &[change.change_id.clone()],
            false,
        )
        .unwrap();
        assert_eq!(preview.paths, ["new", "old"]);
        assert!(matches!(
            finish(&coordinator, &repo, &preview),
            OperationResult::Succeeded { .. }
        ));
        assert_eq!(fs::read(f.root.join("new")).unwrap(), b"base");
        assert!(!f.root.join("old").exists());
        assert_eq!(
            query(&f.git, &repo.root, &["show", ":old"], 4096).unwrap(),
            b"base"
        );
        fs::remove_file(f.root.join("remove")).unwrap();
        let state = read_repository_state(&f.git, &repo).unwrap();
        let change = state
            .changes
            .iter()
            .find(|file| file.path == "remove")
            .unwrap();
        let preview = prepare_index_change(
            &coordinator,
            &f.git,
            &repo,
            &state,
            &[change.change_id.clone()],
            true,
        )
        .unwrap();
        assert!(matches!(
            finish(&coordinator, &repo, &preview),
            OperationResult::Succeeded { .. }
        ));
        assert!(!query(&f.git, &repo.root, &["ls-files", "-z"], 4096)
            .unwrap()
            .split(|byte| *byte == 0)
            .any(|path| path == b"remove"));
    }

    /// Git 内建换行转换在临时索引工作，原始 CRLF 工作文件不被修改。
    #[test]
    fn crlf_stage_preserves_worktree_bytes() {
        let f = Fixture::new();
        f.command(&["config", "core.autocrlf", "true"]);
        f.write("lines", b"one\r\ntwo\r\n");
        let (repo, state) = open_repository(&f.git, &f.root).unwrap();
        let coordinator = RepositoryCoordinator::new();
        let preview = prepare_index_change(
            &coordinator,
            &f.git,
            &repo,
            &state,
            &[state.changes[0].change_id.clone()],
            true,
        )
        .unwrap();
        assert!(matches!(
            finish(&coordinator, &repo, &preview),
            OperationResult::Succeeded { .. }
        ));
        assert_eq!(
            query(&f.git, &repo.root, &["show", ":lines"], 4096).unwrap(),
            b"one\ntwo\n"
        );
        assert_eq!(fs::read(f.root.join("lines")).unwrap(), b"one\r\ntwo\r\n");
    }

    /// 提交预览完整包含其他客户端的暂存文件，外部索引更新会使确认过期。
    #[test]
    fn whole_index_preview_is_readonly_and_rejects_external_index() {
        let f = Fixture::new();
        f.write("a", b"one");
        f.write("b", b"two");
        f.command(&["add", "."]);
        let (repo, state) = open_repository(&f.git, &f.root).unwrap();
        let coordinator = RepositoryCoordinator::new();
        let before = query(&f.git, &repo.root, &["count-objects", "-v"], 4096).unwrap();
        let index = fs::read(repo.git_dir.join("index")).unwrap();
        let preview = prepare_commit(&coordinator, &f.git, &repo, &state, "both").unwrap();
        assert_eq!(preview.paths, ["a", "b"]);
        assert_eq!(
            query(&f.git, &repo.root, &["count-objects", "-v"], 4096).unwrap(),
            before
        );
        assert_eq!(fs::read(repo.git_dir.join("index")).unwrap(), index);
        f.write("b", b"external");
        f.command(&["add", "b"]);
        assert!(
            matches!(finish(&coordinator, &repo, &preview), OperationResult::Failed { error, .. } if error.code == "STALE_WRITE_PLAN")
        );
        assert!(matches!(
            read_repository_state(&f.git, &repo).unwrap().head,
            HeadState::Unborn { .. }
        ));
    }

    /// 启用的 hook、filter、签名和稀疏条目都必须在准备阶段拒绝。
    #[cfg(unix)]
    #[test]
    fn configured_programs_and_unsafe_files_are_rejected() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        for mode in [
            "hook",
            "filter",
            "lfs",
            "signature",
            "sparse",
            "symlink",
            "identity",
        ] {
            let f = Fixture::new();
            f.write("a", b"content");
            f.command(&["add", "."]);
            let (repo, state) = open_repository(&f.git, &f.root).unwrap();
            let coordinator = RepositoryCoordinator::new();
            match mode {
                "hook" => {
                    let path = repo.common_dir.join("hooks/pre-commit");
                    fs::write(&path, b"#!/bin/sh\ntouch executed\n").unwrap();
                    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
                }
                "filter" => {
                    f.write(".gitattributes", b"a filter=evil\n");
                    f.command(&["config", "filter.evil.clean", "touch executed"]);
                }
                "lfs" => {
                    f.write(".gitattributes", b"a filter=lfs diff=lfs merge=lfs -text\n");
                    f.command(&["config", "filter.lfs.process", "touch executed"]);
                    f.command(&["config", "filter.lfs.required", "true"]);
                }
                "signature" => f.command(&["config", "commit.gpgsign", "true"]),
                "sparse" => f.command(&["update-index", "--skip-worktree", "a"]),
                "symlink" => {
                    fs::remove_file(f.root.join("a")).unwrap();
                    symlink("/etc/passwd", f.root.join("a")).unwrap();
                }
                "identity" => {
                    f.command(&["config", "user.name", ""]);
                    f.command(&["config", "user.email", ""]);
                }
                _ => unreachable!(),
            }
            let state = read_repository_state(&f.git, &repo).unwrap_or(state);
            let error = prepare_commit(&coordinator, &f.git, &repo, &state, "safe").unwrap_err();
            assert_eq!(
                error.code,
                if mode == "identity" {
                    "IDENTITY_REQUIRED"
                } else {
                    "UNSUPPORTED_WRITE_CONFIGURATION"
                },
                "情形：{mode}"
            );
            assert!(!f.root.join("executed").exists());
        }
    }

    /// 外部替换锁文件后，当前任务不能删除或发布别人的锁。
    #[cfg(unix)]
    #[test]
    fn replaced_index_lock_is_preserved() {
        let f = Fixture::new();
        let (repo, _) = open_repository(&f.git, &f.root).unwrap();
        let mut transaction = IndexTransaction::new(&repo, Some(b"temporary")).unwrap();
        fs::rename(
            repo.git_dir.join("index.lock"),
            repo.git_dir.join("old-owned-lock"),
        )
        .unwrap();
        fs::write(repo.git_dir.join("index.lock"), b"external").unwrap();
        assert_eq!(transaction.publish().unwrap_err().code, "INDEX_LOCKED");
        drop(transaction);
        assert_eq!(
            fs::read(repo.git_dir.join("index.lock")).unwrap(),
            b"external"
        );
    }

    /// 已确认目标引用指向提交时，后续刷新超时不能丢失成功 OID。
    #[test]
    fn verified_commit_survives_refresh_failure() {
        let f = Fixture::new();
        f.write("a", b"saved");
        f.command(&["add", "."]);
        f.command(&["commit", "-m", "saved"]);
        let (repo, _) = open_repository(&f.git, &f.root).unwrap();
        let actual =
            parse_oid(query(&f.git, &repo.root, &["rev-parse", "HEAD"], 4096).unwrap()).unwrap();
        let coordinator = RepositoryCoordinator::new();
        let saved = actual.clone();
        let git = f.git.clone();
        let captured = repo.clone();
        let plan = coordinator
            .prepare(
                coordinator.begin_prepare().unwrap(),
                Some(repo.id.clone()),
                CoordinationKey::repository(&repo).unwrap(),
                OperationKind::Commit,
                move |reporter| {
                    operation_result(
                        reporter,
                        &git,
                        &captured,
                        Err(OperationError::new("TIMEOUT")),
                        true,
                        Some(saved),
                        Some("main".into()),
                        Instant::now(),
                    )
                },
            )
            .unwrap();
        let handle = coordinator.execute(Some(&repo.id), &plan.plan_id).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(result) = coordinator
                .read_operation(Some(&handle.operation_id))
                .unwrap()
                .unwrap()
                .result
            {
                assert!(
                    matches!(result, OperationResult::Succeeded { commit_oid: Some(oid), refresh: WriteRefresh::Failed { .. }, .. } if oid == actual)
                );
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    /// SHA-256 仓库首次提交使用完整 OID，不把对象长度写死为 SHA-1。
    #[test]
    fn commit_supports_sha256_repository() {
        let f = Fixture::new();
        f.command(&["init", "--object-format=sha256", "-b", "main", "sha256"]);
        for (key, value) in [
            ("user.name", "测试"),
            ("user.email", "test@example.invalid"),
            ("commit.gpgsign", "false"),
        ] {
            f.command(&["-C", "sha256", "config", key, value]);
        }
        f.write("sha256/a", b"sha256");
        let (repo, state) = open_repository(&f.git, &f.root.join("sha256")).unwrap();
        let coordinator = RepositoryCoordinator::new();
        let staged = prepare_index_change(
            &coordinator,
            &f.git,
            &repo,
            &state,
            &[state.changes[0].change_id.clone()],
            true,
        )
        .unwrap();
        assert!(matches!(
            finish(&coordinator, &repo, &staged),
            OperationResult::Succeeded { .. }
        ));
        let state = read_repository_state(&f.git, &repo).unwrap();
        let preview = prepare_commit(&coordinator, &f.git, &repo, &state, "sha256").unwrap();
        let OperationResult::Succeeded {
            commit_oid: Some(oid),
            ..
        } = finish(&coordinator, &repo, &preview)
        else {
            panic!("SHA-256 首次提交应成功");
        };
        assert_eq!(oid.len(), 64);
        assert_eq!(
            query(&f.git, &repo.root, &["show", "HEAD:a"], 4096).unwrap(),
            b"sha256"
        );
    }

    /// 已暂存重命名后再次整文件暂存，旧路径不存在也不产生 pathspec 错误。
    #[test]
    fn stages_modified_file_after_already_staged_rename() {
        let f = Fixture::new();
        f.write("old", b"base\n");
        f.command(&["add", "."]);
        f.command(&["commit", "-m", "base"]);
        f.command(&["mv", "old", "new"]);
        f.write("new", b"latest\n");
        let (repo, state) = open_repository(&f.git, &f.root).unwrap();
        let change = state
            .changes
            .iter()
            .find(|change| change.path == "new")
            .unwrap();
        assert_eq!(change.original_path.as_deref(), Some("old"));
        let coordinator = RepositoryCoordinator::new();
        let preview = prepare_index_change(
            &coordinator,
            &f.git,
            &repo,
            &state,
            &[change.change_id.clone()],
            true,
        )
        .unwrap();
        assert!(matches!(
            finish(&coordinator, &repo, &preview),
            OperationResult::Succeeded { .. }
        ));
        assert_eq!(
            query(&f.git, &repo.root, &["ls-files", "-z"], 4096).unwrap(),
            b"new\0"
        );
        assert_eq!(
            query(&f.git, &repo.root, &["show", ":new"], 4096).unwrap(),
            b"latest\n"
        );
        assert_eq!(fs::read(f.root.join("new")).unwrap(), b"latest\n");
    }

    /// 身份捕获后配置变化不能改变对象作者、提交者或 UTF-8 说明。
    #[test]
    fn commit_object_uses_captured_roles_and_utf8_message() {
        let f = Fixture::new();
        for (key, value) in [
            ("author.name", "原作者"),
            ("author.email", "author@example.invalid"),
            ("committer.name", "原提交者"),
            ("committer.email", "committer@example.invalid"),
        ] {
            f.command(&["config", key, value]);
        }
        f.write("a", b"content");
        f.command(&["add", "."]);
        let (repo, _) = open_repository(&f.git, &f.root).unwrap();
        let deadline = Instant::now() + Duration::from_secs(20);
        let identity = guard::identity(&f.git, &repo, deadline).unwrap();
        assert_eq!(identity.author_display(), "原作者 <author@example.invalid>");
        let tree = parse_oid(
            guard::local_query(&f.git, &repo, &["write-tree"], &[], None, deadline).unwrap(),
        )
        .unwrap();
        for (key, value) in [
            ("author.name", "changed"),
            ("author.email", "changed@example.invalid"),
            ("committer.name", "changed"),
            ("committer.email", "changed@example.invalid"),
            ("i18n.commitEncoding", "ISO-8859-1"),
            ("commit.gpgsign", "true"),
        ] {
            f.command(&["config", key, value]);
        }
        let message = "保存中文\n\n完整说明\n";
        let oid =
            create_commit_object(&f.git, &repo, &tree, &[], message, &identity, deadline).unwrap();
        let bytes = query(&f.git, &repo.root, &["cat-file", "commit", &oid], 65536).unwrap();
        let raw = String::from_utf8(bytes).unwrap();
        assert!(raw.contains("\nauthor 原作者 <author@example.invalid> "));
        assert!(raw.contains("\ncommitter 原提交者 <committer@example.invalid> "));
        assert!(!raw.contains("\nencoding "));
        assert!(!raw.contains("\ngpgsig "));
        assert!(raw.ends_with(message));
    }

    /// 外部客户端移动 HEAD 后不能把旧索引树保存到另一条历史上。
    #[test]
    fn external_commit_invalidates_pending_commit() {
        let f = Fixture::new();
        f.write("a", b"base");
        f.command(&["add", "."]);
        f.command(&["commit", "-m", "base"]);
        f.write("a", b"next");
        f.command(&["add", "."]);
        let (repo, state) = open_repository(&f.git, &f.root).unwrap();
        let coordinator = RepositoryCoordinator::new();
        let preview = prepare_commit(&coordinator, &f.git, &repo, &state, "old preview").unwrap();
        f.command(&["commit", "-m", "external"]);
        let actual = query(&f.git, &repo.root, &["rev-parse", "HEAD"], 4096).unwrap();
        assert!(
            matches!(finish(&coordinator, &repo, &preview), OperationResult::Failed { error, .. } if error.code == "STALE_WRITE_PLAN")
        );
        assert_eq!(
            query(&f.git, &repo.root, &["rev-parse", "HEAD"], 4096).unwrap(),
            actual
        );
    }

    /// 提交准备不能将已有锁删除，也不能接受空白/NUL/超限说明。
    #[test]
    fn lock_and_invalid_message_are_rejected_without_writes() {
        let f = Fixture::new();
        f.write("a", b"a");
        f.command(&["add", "."]);
        let (repo, state) = open_repository(&f.git, &f.root).unwrap();
        let coordinator = RepositoryCoordinator::new();
        for message in [" \n", "a\0b", &"x".repeat(65537)] {
            assert_eq!(
                prepare_commit(&coordinator, &f.git, &repo, &state, message)
                    .unwrap_err()
                    .code,
                "INVALID_INPUT"
            );
        }
        fs::write(repo.git_dir.join("index.lock"), b"external").unwrap();
        assert_eq!(
            prepare_commit(&coordinator, &f.git, &repo, &state, "valid")
                .unwrap_err()
                .code,
            "INDEX_LOCKED"
        );
        assert_eq!(
            fs::read(repo.git_dir.join("index.lock")).unwrap(),
            b"external"
        );
    }
}
