//! 确认的冲突结果先安全替换工作文件，再发布只改变该路径的索引。
use super::*;
use crate::git::{
    coordinator::{CoordinationKey, OperationReporter, RepositoryCoordinator},
    process::inspect_local_config,
    repository::next_id,
    staging::{prepare_conflict_entry, ConversionPolicy},
    write::{operation_result, IndexTransaction},
    write_guard::WriteFingerprint,
};

/// 保存计划冻结合并会话、原文件身份、完整索引、转换规则和用户确认字节。
struct SavePlan {
    git: GitExecutable,
    repo: RepositoryHandle,
    session: String,
    path: String,
    original: FileBytes,
    fingerprint: WriteFingerprint,
    policy: ConversionPolicy,
    bytes: Vec<u8>,
    entries: BTreeMap<String, BTreeMap<u8, Entry>>,
    merge_file: FileBytes,
}

/// 用户必须提交已展示文档的指纹；准备过程不修改用户文件或对象。
pub fn prepare_save(
    coordinator: &RepositoryCoordinator,
    session: &ConflictSession,
    conflict_id: &str,
    fingerprint: &str,
    content: &str,
) -> Result<WritePreview, OperationError> {
    let generation = coordinator.begin_prepare()?;
    let deadline = Instant::now() + BUDGET;
    let key = CoordinationKey::repository(&session.repo)?;
    let (plan, state) = coordinator.read_until(&key, deadline, || {
        capture_save(session, conflict_id, fingerprint, content, deadline)
    })?;
    let path = plan.path.clone();
    let prepared = coordinator.prepare(
        generation,
        Some(session.repo.id.clone()),
        key,
        OperationKind::SaveConflict,
        move |reporter| plan.execute_with(reporter, || Ok(())),
    )?;
    let mut parents = vec![session.snapshot.head.clone()];
    parents.extend(session.snapshot.merge_heads.clone());
    Ok(WritePreview {
        plan_id: prepared.plan_id,
        repository_id: session.repo.id.clone(),
        snapshot_id: state.snapshot_id,
        kind: OperationKind::SaveConflict,
        head: state.head,
        parent_oids: parents,
        paths: vec![path],
        author: None,
        message: None,
        target: None,
        warnings: vec!["请核对结果；保存并暂存不会自动创建合并提交。".into()],
        expires_at: prepared.expires_at,
    })
}

/// 在同一期限内冻结文档、配置与索引，供协调器准备和真实发布测试复用。
fn capture_save(
    session: &ConflictSession,
    conflict_id: &str,
    fingerprint: &str,
    content: &str,
    deadline: Instant,
) -> Result<(SavePlan, RepositoryState), OperationError> {
    validate_snapshot(&session.git, &session.repo, &session.snapshot.id, deadline)?;
    let file = session
        .state
        .files
        .iter()
        .find(|file| file.conflict_id == conflict_id)
        .ok_or_else(|| OperationError::new("STALE_CONFLICT"))?;
    if !matches!(file.editor_support, ConflictEditorSupport::Supported) {
        return Err(OperationError::new("UNSUPPORTED_CONFLICT"));
    }
    let current = contents(
        &session.git,
        &session.repo,
        &session.snapshot.id,
        &file.path,
        &session.snapshot.entries[&file.path],
        deadline,
    )?;
    if current.fingerprint != fingerprint {
        return Err(OperationError::new("STALE_CONFLICT"));
    }
    let original = read_regular(&session.repo.root, &file.path, MAX, deadline)?;
    if digest(&[
        session.snapshot.id.as_bytes(),
        file.path.as_bytes(),
        &original.bytes,
        original.identity.as_bytes(),
    ]) != fingerprint
    {
        return Err(OperationError::new("STALE_CONFLICT"));
    }
    let captured = guard::merge_fingerprint(&session.git, &session.repo, false, deadline)?;
    let (policy, width) = capture_policy(&session.git, &session.repo, &file.path, deadline)?;
    let bytes = encode(content, &current.result, width)?;
    let state = read_repository_state_until(&session.git, &session.repo, deadline)?;
    validate_snapshot(&session.git, &session.repo, &session.snapshot.id, deadline)?;
    if guard::merge_fingerprint(&session.git, &session.repo, false, deadline)? != captured {
        return Err(OperationError::new("STALE_WRITE_PLAN"));
    }
    Ok((
        SavePlan {
            git: session.git.clone(),
            repo: session.repo.clone(),
            session: session.snapshot.id.clone(),
            path: file.path.clone(),
            original,
            fingerprint: captured,
            policy,
            bytes,
            entries: session.snapshot.entries.clone(),
            merge_file: read_regular(&session.repo.git_dir, "MERGE_HEAD", MAX, deadline)?,
        },
        state,
    ))
}

/// 捕获一个路径的内建转换和冲突标记长度，不允许外部驱动进入转换器。
fn capture_policy(
    git: &GitExecutable,
    repo: &RepositoryHandle,
    path: &str,
    deadline: Instant,
) -> Result<(ConversionPolicy, usize), OperationError> {
    let config = guard::parse_config(&inspect_local_config(git, &repo.root, deadline)?)?;
    let mut input = path.as_bytes().to_vec();
    input.push(0);
    let attributes = guard::local_query(
        git,
        repo,
        &[
            "check-attr",
            "-z",
            "--stdin",
            "filter",
            "merge",
            "working-tree-encoding",
            "text",
            "eol",
            "ident",
            "crlf",
            "conflict-marker-size",
        ],
        &input,
        None,
        deadline,
    )?;
    guard::validate_attributes(&attributes)?;
    let width = attributes
        .split(|byte| *byte == 0)
        .collect::<Vec<_>>()
        .chunks_exact(3)
        .find(|fields| fields[1] == b"conflict-marker-size")
        .and_then(|fields| std::str::from_utf8(fields[2]).ok())
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|width| *width > 0)
        .unwrap_or(7);
    if width > MAX {
        return Err(OperationError::new("UNSUPPORTED_CONFLICT"));
    }
    let policy = ConversionPolicy::capture(&config, &attributes, &[path.to_owned()])?;
    Ok((policy, width))
}

impl SavePlan {
    /// 两次发布之间发生失败时保留新工作文件，不以恢复旧内容掩盖真实结果。
    fn execute_with(
        self,
        reporter: &OperationReporter,
        after_file: impl FnOnce() -> Result<(), OperationError>,
    ) -> OperationResult {
        let deadline = reporter.deadline(BUDGET);
        let mut written = false;
        let result = (|| {
            self.revalidate(false, deadline)?;
            let mut transaction =
                IndexTransaction::new(&self.repo, self.fingerprint.index_bytes.as_deref())?;
            self.revalidate(true, deadline)?;
            let (parent_dir, name) = parent(&self.repo.root, &self.path)?;
            let permissions = parent_dir
                .symlink_metadata(&name)
                .map_err(|_| OperationError::new("STALE_CONFLICT"))?
                .permissions();
            let entry = prepare_conflict_entry(
                &self.git,
                &self.repo,
                &self.policy,
                &self.path,
                &self.bytes,
                permissions,
                &self.entries[&self.path][&2].mode,
                deadline,
            )?;
            let mut input = self.path.as_bytes().to_vec();
            input.push(0);
            guard::local_query(
                &self.git,
                &self.repo,
                &["update-index", "--force-remove", "-z", "--stdin"],
                &input,
                Some(&transaction.path),
                deadline,
            )?;
            let record = format!("{} {}\t{}\0", entry.mode, entry.oid, self.path);
            guard::local_query(
                &self.git,
                &self.repo,
                &["update-index", "-z", "--index-info"],
                record.as_bytes(),
                Some(&transaction.path),
                deadline,
            )?;
            let mut expected = self.entries.clone();
            expected.insert(
                self.path.clone(),
                BTreeMap::from([(
                    0,
                    Entry {
                        mode: entry.mode,
                        oid: entry.oid,
                    },
                )]),
            );
            let parsed =
                read_captured_index(&self.git, &self.repo.root, &transaction.path, deadline)?;
            if !parsed.success || parse_index(&parsed.stdout)? != expected {
                return Err(OperationError::new("WRITE_OUTCOME_UNKNOWN"));
            }
            self.revalidate(true, deadline)?;
            if !transaction.owns_lock() {
                return Err(OperationError::new("INDEX_LOCKED"));
            }
            reporter.report(OperationPhase::Writing, None)?;
            let replacement = Replacement::new(
                &self.repo.root,
                &self.path,
                &self.original,
                &self.bytes,
                deadline,
            )?;
            replacement.publish(&self.repo.root, &self.path, &self.original, deadline)?;
            written = true;
            after_file()?;
            validate_snapshot(&self.git, &self.repo, &self.session, deadline)?;
            let after_file_inputs =
                guard::merge_fingerprint(&self.git, &self.repo, true, deadline)?;
            if !after_file_inputs.same_repository_inputs(&self.fingerprint)
                || read_regular(&self.repo.root, &self.path, MAX, deadline)?.bytes != self.bytes
            {
                return Err(OperationError::new("STALE_CONFLICT"));
            }
            if Path::new(&self.path)
                .file_name()
                .is_some_and(|name| name == ".gitattributes")
            {
                let (policy, _) = capture_policy(&self.git, &self.repo, &self.path, deadline)?;
                let (parent_dir, name) = parent(&self.repo.root, &self.path)?;
                let permissions = parent_dir
                    .symlink_metadata(name)
                    .map_err(|_| OperationError::new("STALE_CONFLICT"))?
                    .permissions();
                let entry = prepare_conflict_entry(
                    &self.git,
                    &self.repo,
                    &policy,
                    &self.path,
                    &self.bytes,
                    permissions,
                    &self.entries[&self.path][&2].mode,
                    deadline,
                )?;
                let record = format!("{} {}\t{}\0", entry.mode, entry.oid, self.path);
                guard::local_query(
                    &self.git,
                    &self.repo,
                    &["update-index", "-z", "--index-info"],
                    record.as_bytes(),
                    Some(&transaction.path),
                    deadline,
                )?;
                expected.insert(
                    self.path.clone(),
                    BTreeMap::from([(
                        0,
                        Entry {
                            mode: entry.mode,
                            oid: entry.oid,
                        },
                    )]),
                );
                let parsed =
                    read_captured_index(&self.git, &self.repo.root, &transaction.path, deadline)?;
                if !parsed.success || parse_index(&parsed.stdout)? != expected {
                    return Err(OperationError::new("WRITE_OUTCOME_UNKNOWN"));
                }
            }
            validate_snapshot(&self.git, &self.repo, &self.session, deadline)?;
            if guard::merge_fingerprint(&self.git, &self.repo, true, deadline)? != after_file_inputs
                || read_regular(&self.repo.root, &self.path, MAX, deadline)?.bytes != self.bytes
            {
                return Err(OperationError::new("STALE_CONFLICT"));
            }
            transaction.publish()?;
            reporter.report(OperationPhase::Verifying, None)?;
            let actual = snapshot(&self.git, &self.repo, deadline)?;
            let merge_file = read_regular(&self.repo.git_dir, "MERGE_HEAD", MAX, deadline)?;
            if actual.entries != expected
                || actual.head != head_oid(&self.fingerprint.head)?
                || merge_file.bytes != self.merge_file.bytes
                || merge_file.identity != self.merge_file.identity
                || read_regular(&self.repo.root, &self.path, MAX, deadline)?.bytes != self.bytes
            {
                return Err(OperationError::new("WRITE_OUTCOME_UNKNOWN"));
            }
            Ok(())
        })();
        operation_result(
            reporter, &self.git, &self.repo, result, written, None, None, deadline,
        )
    }

    /// 合并状态、转换前提和原字节都必须仍与用户确认时一致。
    fn revalidate(&self, own_lock: bool, deadline: Instant) -> Result<(), OperationError> {
        validate_snapshot(&self.git, &self.repo, &self.session, deadline)?;
        if guard::merge_fingerprint(&self.git, &self.repo, own_lock, deadline)? != self.fingerprint
        {
            return Err(OperationError::new("STALE_WRITE_PLAN"));
        }
        let file = read_regular(&self.repo.root, &self.path, MAX, deadline)?;
        if file.bytes != self.original.bytes || file.identity != self.original.identity {
            return Err(OperationError::new("STALE_CONFLICT"));
        }
        Ok(())
    }
}

/// 合并只允许具名且已经存在的 HEAD。
fn head_oid(head: &HeadState) -> Result<&str, OperationError> {
    match head {
        HeadState::Branch { oid, .. } => Ok(oid),
        _ => Err(OperationError::new("STALE_CONFLICT")),
    }
}

/// 编辑器可统一传 LF，落盘恢复原 BOM/换行；不裁剪正文或末尾换行。
fn encode(content: &str, original: &Text, marker_width: usize) -> Result<Vec<u8>, OperationError> {
    let text = decode(content.as_bytes())?;
    if text.bom
        || content.lines().any(|line| {
            [7, marker_width].iter().any(|width| {
                line.len() >= *width
                    && [b'<', b'=', b'>', b'|']
                        .iter()
                        .any(|marker| line.as_bytes()[..*width].iter().all(|byte| byte == marker))
            })
        })
    {
        return Err(OperationError::new("UNSUPPORTED_CONFLICT"));
    }
    let normalized = content.replace("\r\n", "\n");
    let normalized = if original.ending == "crlf" {
        normalized.replace('\n', "\r\n")
    } else {
        normalized
    };
    let mut bytes = if original.bom {
        vec![0xef, 0xbb, 0xbf]
    } else {
        Vec::new()
    };
    bytes.extend_from_slice(normalized.as_bytes());
    decode(&bytes)?;
    Ok(bytes)
}

/// 同目录新文件由能力句柄持有，失败仅清理本次仍拥有的临时文件。
struct Replacement {
    dir: Dir,
    temporary: String,
    file: cap_std::fs::File,
    permissions: cap_std::fs::Permissions,
}

impl Replacement {
    /// 打开普通原文件的父目录，在同一目录写入并同步确认字节。
    fn new(
        root: &Path,
        path: &str,
        original: &FileBytes,
        bytes: &[u8],
        deadline: Instant,
    ) -> Result<Self, OperationError> {
        guard::check_time(deadline)?;
        let (dir, name) = parent(root, path)?;
        let metadata = dir
            .symlink_metadata(&name)
            .map_err(|_| OperationError::new("STALE_CONFLICT"))?;
        if !metadata.is_file() || metadata_identity(&metadata) != original.identity {
            return Err(OperationError::new("STALE_CONFLICT"));
        }
        let permissions = metadata.permissions();
        let temporary = format!(".gitmaster-resolution-{}-{}", std::process::id(), next_id());
        let mut options = cap_std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        let file = dir
            .open_with(&temporary, &options)
            .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
        let mut result = Self {
            dir,
            temporary,
            file,
            permissions,
        };
        result
            .file
            .write_all(bytes)
            .and_then(|_| result.file.set_permissions(result.permissions.clone()))
            .and_then(|_| result.file.sync_all())
            .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
        Ok(result)
    }

    /// 最后核对实时目录身份和旧内容，再原子替换一个工作文件。
    fn publish(
        self,
        root: &Path,
        path: &str,
        original: &FileBytes,
        deadline: Instant,
    ) -> Result<(), OperationError> {
        let (current, name) = parent(root, path)?;
        let current_meta = current
            .dir_metadata()
            .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
        let owned_meta = self
            .dir
            .dir_metadata()
            .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
        let file = read_regular(root, path, MAX, deadline)?;
        if !same_inode(&current_meta, &owned_meta)
            || file.bytes != original.bytes
            || file.identity != original.identity
            || !self.owns_temporary()
        {
            return Err(OperationError::new("STALE_CONFLICT"));
        }
        self.dir
            .rename(&self.temporary, &self.dir, name)
            .map_err(|_| OperationError::new("ACCESS_DENIED"))
    }

    /// 临时目录项被替换后不得删除或发布外部创建的文件。
    fn owns_temporary(&self) -> bool {
        match (
            self.file.metadata(),
            self.dir.symlink_metadata(&self.temporary),
        ) {
            (Ok(owned), Ok(current)) => current.is_file() && same_inode(&owned, &current),
            _ => false,
        }
    }
}

impl Drop for Replacement {
    /// 成功替换后临时路径已消失；失败只移除仍由本次创建的文件。
    fn drop(&mut self) {
        if self.owns_temporary() {
            let _ = self.dir.remove_file(&self.temporary);
        }
    }
}

/// 不跟随任何祖先链接，返回当前父目录句柄和最终字面文件名。
fn parent(root: &Path, path: &str) -> Result<(Dir, String), OperationError> {
    guard::validate_path(path)?;
    let mut dir = Dir::open_ambient_dir(root, cap_std::ambient_authority())
        .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
    let mut parts = path.split('/').collect::<Vec<_>>();
    let name = parts
        .pop()
        .ok_or_else(|| OperationError::new("INVALID_INPUT"))?
        .to_owned();
    for part in parts {
        #[cfg(unix)]
        {
            use cap_std::fs::OpenOptionsExt;
            let mut options = cap_std::fs::OpenOptions::new();
            options
                .read(true)
                .custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY | libc::O_NONBLOCK);
            let file = dir
                .open_with(part, &options)
                .map_err(|_| OperationError::new("STALE_CONFLICT"))?;
            dir = Dir::from_std_file(file.into_std());
        }
        #[cfg(windows)]
        {
            dir = crate::git::windows_fs::open_directory(&dir, Path::new(part))?;
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = part;
            return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
        }
        if dir.symlink_metadata(".git").is_ok() {
            return Err(OperationError::new("UNSUPPORTED_CONFLICT"));
        }
    }
    Ok((dir, name))
}

/// 目录内容时间会因创建临时文件改变，只比较文件系统身份。
fn same_inode(left: &Metadata, right: &Metadata) -> bool {
    #[cfg(unix)]
    {
        use cap_std::fs::MetadataExt;
        left.dev() == right.dev() && left.ino() == right.ino()
    }
    #[cfg(windows)]
    {
        crate::git::windows_fs::same(left, right)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (left, right);
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::repository::{open_repository, tests::Fixture};
    use std::fs;

    /// 同一个后台操作只执行一次，轮询其真实终态。
    fn execute(
        plan: SavePlan,
        after_file: impl FnOnce() -> Result<(), OperationError> + Send + 'static,
    ) -> OperationResult {
        let coordinator = RepositoryCoordinator::new();
        let id = plan.repo.id.clone();
        let prepared = coordinator
            .prepare(
                coordinator.begin_prepare().unwrap(),
                Some(id.clone()),
                CoordinationKey::repository(&plan.repo).unwrap(),
                OperationKind::SaveConflict,
                move |reporter| plan.execute_with(reporter, after_file),
            )
            .unwrap();
        let handle = coordinator.execute(Some(&id), &prepared.plan_id).unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if let Some(result) = coordinator
                .read_operation(Some(&handle.operation_id))
                .unwrap()
                .and_then(|record| record.result)
            {
                return result;
            }
            assert!(Instant::now() < deadline, "保存操作超出测试期限");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    /// 捕获真实 Git 生成的冲突，不以手工拼索引模拟业务状态。
    fn plan() -> (Fixture, RepositoryHandle, SavePlan) {
        let (fixture, _) = super::super::tests::branch_conflict();
        let (repo, _) = open_repository(&fixture.git, &fixture.root).unwrap();
        let session = ConflictSession::new(&fixture.git, &repo).unwrap();
        let id = session.state.files[0].conflict_id.clone();
        let document = session.read_document(&id).unwrap();
        let (plan, _) = capture_save(
            &session,
            &id,
            &document.fingerprint,
            "resolved\n",
            Instant::now() + BUDGET,
        )
        .unwrap();
        (fixture, repo, plan)
    }

    /// 工作文件已发布而索引未发布时，保留确认内容和原索引并返回不确定。
    #[test]
    fn failure_after_file_publication_keeps_result_and_unmerged_index() {
        let (fixture, repo, plan) = plan();
        let before = fs::read(repo.git_dir.join("index")).unwrap();
        let result = execute(plan, || Err(OperationError::new("ACCESS_DENIED")));
        assert!(
            matches!(result, OperationResult::Unknown { .. }),
            "{result:?}"
        );
        assert_eq!(fs::read(fixture.root.join("file")).unwrap(), b"resolved\n");
        assert_eq!(fs::read(repo.git_dir.join("index")).unwrap(), before);
        assert!(!repo.git_dir.join("index.lock").exists());
        assert_eq!(
            ConflictSession::new(&fixture.git, &repo)
                .unwrap()
                .state()
                .files
                .len(),
            1
        );
    }

    /// 发布工作文件后外部编辑或替换锁，不得继续覆盖索引或删除外部锁。
    #[test]
    fn late_edits_and_replaced_index_locks_remain_untouched() {
        for replace_lock in [false, true] {
            let (fixture, repo, plan) = plan();
            let before = fs::read(repo.git_dir.join("index")).unwrap();
            let root = fixture.root.clone();
            let git_dir = repo.git_dir.clone();
            let result = execute(plan, move || {
                if replace_lock {
                    fs::rename(git_dir.join("index.lock"), git_dir.join("owned-lock")).unwrap();
                    fs::write(git_dir.join("index.lock"), b"external-lock").unwrap();
                } else {
                    fs::write(root.join("file"), b"external\n").unwrap();
                }
                Ok(())
            });
            assert!(
                matches!(result, OperationResult::Unknown { .. }),
                "{result:?}"
            );
            assert_eq!(fs::read(repo.git_dir.join("index")).unwrap(), before);
            if replace_lock {
                assert_eq!(
                    fs::read(repo.git_dir.join("index.lock")).unwrap(),
                    b"external-lock"
                );
            } else {
                assert_eq!(fs::read(fixture.root.join("file")).unwrap(), b"external\n");
            }
        }
    }

    /// 既有锁在写工作文件之前拒绝，锁和原结果都保留。
    #[test]
    fn existing_index_lock_prevents_any_workfile_publication() {
        let (fixture, repo, plan) = plan();
        let before = fs::read(fixture.root.join("file")).unwrap();
        fs::write(repo.git_dir.join("index.lock"), b"external-lock").unwrap();
        let result = execute(plan, || Ok(()));
        assert!(
            matches!(result, OperationResult::Failed { ref error, .. } if error.code == "INDEX_LOCKED"),
            "{result:?}"
        );
        assert_eq!(fs::read(fixture.root.join("file")).unwrap(), before);
        assert_eq!(
            fs::read(repo.git_dir.join("index.lock")).unwrap(),
            b"external-lock"
        );
    }

    /// 确认后以链接替换结果时不写到仓库之外，也不删除链接。
    #[cfg(unix)]
    #[test]
    fn replaced_workfile_symlink_does_not_escape() {
        let (fixture, _, plan) = plan();
        let outside = tempfile::NamedTempFile::new().unwrap();
        fs::write(outside.path(), b"outside").unwrap();
        fs::remove_file(fixture.root.join("file")).unwrap();
        std::os::unix::fs::symlink(outside.path(), fixture.root.join("file")).unwrap();
        let result = execute(plan, || Ok(()));
        assert!(
            matches!(result, OperationResult::Failed { .. }),
            "{result:?}"
        );
        assert_eq!(fs::read(outside.path()).unwrap(), b"outside");
        assert!(fs::symlink_metadata(fixture.root.join("file"))
            .unwrap()
            .is_symlink());
    }

    /// Git 可配置较短冲突标记，不能只检查默认七个字符就暂存未解决内容。
    #[test]
    fn configured_short_conflict_markers_are_rejected() {
        let (fixture, _) = super::super::tests::branch_conflict();
        fixture.command(&["merge", "--abort"]);
        fs::write(
            fixture.root.join(".git/info/attributes"),
            b"file conflict-marker-size=3\n",
        )
        .unwrap();
        let output =
            super::super::tests::git(&fixture, &["merge", "--no-commit", "--no-ff", "incoming"]);
        assert_eq!(output.status.code(), Some(1));
        let (repo, _) = open_repository(&fixture.git, &fixture.root).unwrap();
        let session = ConflictSession::new(&fixture.git, &repo).unwrap();
        let id = session.state.files[0].conflict_id.clone();
        let doc = session.read_document(&id).unwrap();
        assert!(doc.result.starts_with("<<< HEAD\n"));
        let result = prepare_save(
            &RepositoryCoordinator::new(),
            &session,
            &id,
            &doc.fingerprint,
            &doc.result,
        );
        assert!(
            matches!(result, Err(ref error) if error.code == "UNSUPPORTED_CONFLICT"),
            "{result:?}"
        );
    }
    /// 属性文件解决后的自身 text 规则必须与原生 add 相同，不能沿用冲突中的旧规则。
    #[test]
    fn resolved_attribute_file_uses_its_new_conversion_rules() {
        let fixture = Fixture::new();
        fixture.write(".gitattributes", b"#header\r\n#base\r\n");
        fixture.command(&["add", "."]);
        fixture.command(&["commit", "-m", "base"]);
        fixture.command(&["switch", "-c", "incoming"]);
        fixture.write(".gitattributes", b"#header\r\n#incoming\r\n");
        fixture.command(&["add", "."]);
        fixture.command(&["commit", "-m", "incoming"]);
        fixture.command(&["switch", "main"]);
        fixture.write(".gitattributes", b"#header\r\n#local\r\n");
        fixture.command(&["add", "."]);
        fixture.command(&["commit", "-m", "local"]);
        assert_eq!(
            super::super::tests::git(&fixture, &["merge", "--no-commit", "--no-ff", "incoming"])
                .status
                .code(),
            Some(1)
        );
        let (repo, _) = open_repository(&fixture.git, &fixture.root).unwrap();
        let session = ConflictSession::new(&fixture.git, &repo).unwrap();
        let id = session.state.files[0].conflict_id.clone();
        let doc = session.read_document(&id).unwrap();
        assert_eq!(doc.line_ending, "crlf");
        let (plan, _) = capture_save(
            &session,
            &id,
            &doc.fingerprint,
            ".gitattributes text\n#resolved\n",
            Instant::now() + BUDGET,
        )
        .unwrap();
        let result = execute(plan, || Ok(()));
        assert!(
            matches!(result, OperationResult::Succeeded { .. }),
            "{result:?}"
        );
        let ours = super::super::query_until(
            &fixture.git,
            &repo.root,
            &["show", ":0:.gitattributes"],
            MAX,
            None,
        )
        .unwrap();
        fixture.command(&["add", "--", ".gitattributes"]);
        let native = super::super::query_until(
            &fixture.git,
            &repo.root,
            &["show", ":0:.gitattributes"],
            MAX,
            None,
        )
        .unwrap();
        assert_eq!(ours, native);
        assert_eq!(native, b".gitattributes text\n#resolved\n");
        assert_eq!(
            fs::read(repo.root.join(".gitattributes")).unwrap(),
            b".gitattributes text\r\n#resolved\r\n"
        );
    }

    /// filemode=false 时原生 add 沿用本地 stage 2 模式，不能把可执行文件改成普通文件。
    #[cfg(unix)]
    #[test]
    fn ignored_workfile_mode_preserves_local_conflict_stage_mode() {
        use std::os::unix::fs::PermissionsExt;
        let (fixture, _) = super::super::tests::branch_conflict();
        let (repo, _) = open_repository(&fixture.git, &fixture.root).unwrap();
        let local = super::super::query_until(
            &fixture.git,
            &repo.root,
            &["rev-parse", ":2:file"],
            1024,
            None,
        )
        .unwrap();
        let local = String::from_utf8(local).unwrap();
        guard::local_query(
            &fixture.git,
            &repo,
            &["update-index", "-z", "--index-info"],
            format!("100755 {} 2\tfile\0", local.trim()).as_bytes(),
            None,
            Instant::now() + BUDGET,
        )
        .unwrap();
        fs::set_permissions(repo.root.join("file"), fs::Permissions::from_mode(0o755)).unwrap();
        fixture.command(&["config", "core.filemode", "false"]);
        let session = ConflictSession::new(&fixture.git, &repo).unwrap();
        let id = session.state.files[0].conflict_id.clone();
        let doc = session.read_document(&id).unwrap();
        let (plan, _) = capture_save(
            &session,
            &id,
            &doc.fingerprint,
            "resolved\n",
            Instant::now() + BUDGET,
        )
        .unwrap();
        let result = execute(plan, || Ok(()));
        assert!(
            matches!(result, OperationResult::Succeeded { .. }),
            "{result:?}"
        );
        let index = super::super::query_until(
            &fixture.git,
            &repo.root,
            &["ls-files", "--stage"],
            1024,
            None,
        )
        .unwrap();
        assert!(
            index.starts_with(b"100755 "),
            "{}",
            String::from_utf8_lossy(&index)
        );
        assert_eq!(
            fs::metadata(repo.root.join("file"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o755
        );
    }
}
