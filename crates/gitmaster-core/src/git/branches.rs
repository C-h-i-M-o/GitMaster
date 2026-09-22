//! 分支引用快照与不自动切换的分支创建。
use super::{
    checkout::CapturedCheckout,
    coordinator::{CoordinationKey, OperationReporter, RepositoryCoordinator},
    process::run_local_git,
    repository::{next_id, query_until, read_repository_state_until},
    write::operation_result,
    write_guard::{self as guard, WriteFingerprint},
    *,
};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    ffi::OsString,
    path::Path,
    time::{Duration, Instant},
};

const LOCAL_BUDGET: Duration = Duration::from_secs(120);

/// 将分支 ID 绑定到一个仓库读取会话。
pub struct BranchSession {
    list: BranchList,
}

impl BranchSession {
    /// 读取本地、远端分支及 linked worktree 占用关系。
    pub fn new(git: &GitExecutable, repo: &RepositoryHandle) -> Result<Self, OperationError> {
        Self::new_until(git, repo, Instant::now() + LOCAL_BUDGET)
    }

    /// 分支查询可复用写操作的剩余总预算，不另启独立等待周期。
    pub(crate) fn new_until(
        git: &GitExecutable,
        repo: &RepositoryHandle,
        deadline: Instant,
    ) -> Result<Self, OperationError> {
        let state = read_repository_state_until(git, repo, deadline)?;
        let output = query_until(
            git,
            &repo.root,
            &[
                "for-each-ref",
                "--format=%(refname)%00%(objectname)%00%(symref)%00%(upstream)",
                "refs/heads",
                "refs/remotes",
            ],
            8 * 1024 * 1024,
            Some(deadline),
        )?;
        let worktrees = query_until(
            git,
            &repo.root,
            &["worktree", "list", "--porcelain", "-z"],
            8 * 1024 * 1024,
            Some(deadline),
        )?;
        let occupied = occupied_branches(&worktrees, &repo.root)?;
        let mut records = Vec::new();
        for row in output
            .split(|byte| *byte == b'\n')
            .filter(|row| !row.is_empty())
        {
            let fields = row.split(|byte| *byte == 0).collect::<Vec<_>>();
            if fields.len() != 4 {
                return Err(OperationError::new("PARSE_FAILED"));
            }
            if !fields[2].is_empty() {
                continue;
            }
            let reference = text(fields[0])?;
            let (name, kind) = if let Some(name) = reference.strip_prefix("refs/heads/") {
                (name.to_owned(), ReferenceKind::Local)
            } else if let Some(name) = reference.strip_prefix("refs/remotes/") {
                (name.to_owned(), ReferenceKind::Remote)
            } else {
                return Err(OperationError::new("PARSE_FAILED"));
            };
            records.push((reference, BranchSummary {
                branch_id: next_id(),
                current: kind == ReferenceKind::Local && matches!(&state.head, HeadState::Branch { name: current, .. } if current == &name),
                name, oid: text(fields[1])?, kind,
                occupied_by_other_worktree: false, upstream_ref_id: None,
            }, text(fields[3])?));
            if records.len() > 1000 {
                return Err(OperationError::new("OUTPUT_LIMIT"));
            }
        }
        let ids: HashMap<_, _> = records
            .iter()
            .map(|(reference, branch, _)| (reference.clone(), branch.branch_id.clone()))
            .collect();
        let branches = records
            .into_iter()
            .map(|(reference, mut branch, upstream)| {
                branch.occupied_by_other_worktree =
                    occupied.get(&reference).copied().unwrap_or(false);
                branch.upstream_ref_id = ids.get(&upstream).cloned();
                branch
            })
            .collect();
        Ok(Self {
            list: BranchList {
                repository_id: repo.id.clone(),
                branches,
            },
        })
    }

    /// 只返回可展示 DTO，不提供前端任意引用执行入口。
    pub fn list(&self) -> BranchList {
        self.list.clone()
    }
}

/// 从确认的当前提交创建具名分支，准备不修改仓库。
pub fn prepare_create_branch(
    coordinator: &RepositoryCoordinator,
    git: &GitExecutable,
    repo: &RepositoryHandle,
    state: &RepositoryState,
    name: &str,
) -> Result<WritePreview, OperationError> {
    let generation = coordinator.begin_prepare()?;
    let deadline = Instant::now() + LOCAL_BUDGET;
    let key = CoordinationKey::repository(repo)?;
    let reference = format!("refs/heads/{name}");
    let (fingerprint, oid) = coordinator.read_until(&key, deadline, || {
        guard::validate_snapshot(git, repo, state, deadline)?;
        validate_name(git, repo, name, &reference, deadline)?;
        let fingerprint = guard::branch_fingerprint(git, repo, deadline)?;
        let oid = match &fingerprint.head {
            HeadState::Branch { oid, .. } | HeadState::Detached { oid } => oid.clone(),
            HeadState::Unborn { .. } => return Err(OperationError::new("HEAD_REQUIRED")),
        };
        let existing = guard::local_query(
            git,
            repo,
            &["for-each-ref", "--format=%(refname)", "refs/heads"],
            &[],
            None,
            deadline,
        )?;
        if existing
            .split(|byte| *byte == b'\n')
            .filter(|row| !row.is_empty())
            .any(|row| {
                row == reference.as_bytes()
                    || reference
                        .as_bytes()
                        .strip_prefix(row)
                        .is_some_and(|suffix| suffix.starts_with(b"/"))
                    || row
                        .strip_prefix(reference.as_bytes())
                        .is_some_and(|suffix| suffix.starts_with(b"/"))
            })
        {
            return Err(OperationError::new("BRANCH_EXISTS"));
        }
        Ok((fingerprint, oid))
    })?;
    let prepared_git = git.clone();
    let prepared_repo = repo.clone();
    let prepared_name = name.to_owned();
    let source = oid.clone();
    let plan = coordinator.prepare(
        generation,
        Some(repo.id.clone()),
        key,
        OperationKind::CreateBranch,
        move |reporter| {
            execute_create(
                reporter,
                &prepared_git,
                &prepared_repo,
                &fingerprint,
                &prepared_name,
                &reference,
                &source,
            )
        },
    )?;
    Ok(WritePreview {
        plan_id: plan.plan_id,
        repository_id: repo.id.clone(),
        snapshot_id: state.snapshot_id.clone(),
        kind: OperationKind::CreateBranch,
        head: state.head.clone(),
        parent_oids: vec![oid.clone()],
        paths: Vec::new(),
        author: None,
        message: None,
        target: Some(WriteTarget::LocalBranch {
            name: name.to_owned(),
            oid: Some(oid),
        }),
        warnings: Vec::new(),
        expires_at: plan.expires_at,
    })
}

/// 切换仅接受本会话本地分支 ID，准备阶段检查干净状态与目标树能力。
pub fn prepare_switch_branch(
    coordinator: &RepositoryCoordinator,
    git: &GitExecutable,
    repo: &RepositoryHandle,
    state: &RepositoryState,
    branches: &BranchSession,
    branch_id: &str,
) -> Result<WritePreview, OperationError> {
    let generation = coordinator.begin_prepare()?;
    let deadline = Instant::now() + LOCAL_BUDGET;
    let key = CoordinationKey::repository(repo)?;
    if branches.list.repository_id != repo.id {
        return Err(OperationError::new("STALE_REQUEST"));
    }
    let selected = branches
        .list
        .branches
        .iter()
        .find(|branch| branch.branch_id == branch_id && branch.kind == ReferenceKind::Local)
        .ok_or_else(|| OperationError::new("INVALID_INPUT"))?
        .clone();
    let (fingerprint, paths, checkout) = coordinator.read_until(&key, deadline, || {
        guard::validate_snapshot(git, repo, state, deadline)?;
        if !state.changes.is_empty() {
            return Err(OperationError::new("WORKTREE_DIRTY"));
        }
        validate_switch_target(git, repo, &selected, deadline)?;
        let fingerprint = guard::branch_fingerprint(git, repo, deadline)?;
        let entries = guard::checkout_tree(git, repo, &selected.oid, deadline)?;
        let current: BTreeMap<_, _> = fingerprint
            .index
            .iter()
            .map(|entry| (&entry.path, (&entry.mode, &entry.oid)))
            .collect();
        let target: BTreeMap<_, _> = entries
            .iter()
            .map(|entry| (&entry.path, (&entry.mode, &entry.oid)))
            .collect();
        let candidates: BTreeSet<_> = current.keys().chain(target.keys()).copied().collect();
        let paths = candidates
            .into_iter()
            .filter(|path| current.get(path) != target.get(path))
            .cloned()
            .collect::<Vec<_>>();
        let checkout = CapturedCheckout::capture(git, repo, &fingerprint, &selected, deadline)?;
        if guard::branch_fingerprint(git, repo, deadline)? != fingerprint {
            return Err(OperationError::new("STALE_WRITE_PLAN"));
        }
        Ok((fingerprint, paths, checkout))
    })?;
    let prepared_git = git.clone();
    let prepared_repo = repo.clone();
    let target = selected.clone();
    let plan = coordinator.prepare(
        generation,
        Some(repo.id.clone()),
        key,
        OperationKind::SwitchBranch,
        move |reporter| {
            execute_switch(
                reporter,
                &prepared_git,
                &prepared_repo,
                &fingerprint,
                &target,
                &checkout,
            )
        },
    )?;
    Ok(WritePreview {
        plan_id: plan.plan_id,
        repository_id: repo.id.clone(),
        snapshot_id: state.snapshot_id.clone(),
        kind: OperationKind::SwitchBranch,
        head: state.head.clone(),
        parent_oids: vec![selected.oid.clone()],
        paths,
        author: None,
        message: None,
        target: Some(WriteTarget::LocalBranch {
            name: selected.name,
            oid: Some(selected.oid),
        }),
        warnings: Vec::new(),
        expires_at: plan.expires_at,
    })
}

/// 重新读取占用和 OID，不复用旧列表对外部客户端作出的假设。
fn validate_switch_target(
    git: &GitExecutable,
    repo: &RepositoryHandle,
    selected: &BranchSummary,
    deadline: Instant,
) -> Result<(), OperationError> {
    let fresh = BranchSession::new_until(git, repo, deadline)?;
    let current = fresh
        .list
        .branches
        .iter()
        .find(|branch| branch.kind == ReferenceKind::Local && branch.name == selected.name)
        .ok_or_else(|| OperationError::new("STALE_WRITE_PLAN"))?;
    if current.oid != selected.oid {
        return Err(OperationError::new("STALE_WRITE_PLAN"));
    }
    if current.occupied_by_other_worktree {
        return Err(OperationError::new("BRANCH_IN_USE"));
    }
    if current.current {
        return Err(OperationError::new("INVALID_INPUT"));
    }
    Ok(())
}

/// Git 自身保护未提交和被忽略文件，返回后再核实 HEAD/工作区，部分失败不回滚。
fn execute_switch(
    reporter: &OperationReporter,
    git: &GitExecutable,
    repo: &RepositoryHandle,
    expected: &WriteFingerprint,
    target: &BranchSummary,
    checkout: &CapturedCheckout,
) -> OperationResult {
    let deadline = reporter.deadline(LOCAL_BUDGET);
    let mut attempted = false;
    let result = (|| {
        if guard::branch_fingerprint(git, repo, deadline)? != *expected {
            return Err(OperationError::new("STALE_WRITE_PLAN"));
        }
        let before = super::repository::read_repository_state_until(git, repo, deadline)?;
        if !before.changes.is_empty() {
            return Err(OperationError::new("WORKTREE_DIRTY"));
        }
        validate_switch_target(git, repo, target, deadline)?;
        guard::checkout_tree(git, repo, &target.oid, deadline)?;
        reporter.report(OperationPhase::Writing, None)?;
        let output = checkout.execute_with(git, repo, deadline, || {
            attempted = true;
            Ok(())
        });
        if !attempted {
            return output.map(|_| ());
        }
        let actual = super::repository::read_repository_state_until(git, repo, deadline)?;
        let clean = actual.changes.is_empty() && actual.operations.is_empty();
        if clean
            && matches!(&actual.head, HeadState::Branch { name, oid } if name == &target.name && oid == &target.oid)
        {
            return Ok(());
        }
        if clean && actual.head == expected.head && output.as_ref().is_ok_and(|out| !out.success) {
            attempted = false;
            return Err(OperationError::new("GIT_EXECUTION_FAILED"));
        }
        Err(OperationError::new("WRITE_OUTCOME_UNKNOWN"))
    })();
    operation_result(
        reporter,
        git,
        repo,
        result,
        attempted,
        None,
        Some(target.name.clone()),
        deadline,
    )
}

/// 显式完整引用校验不会展开 @{-1} 等 checkout 简写。
fn validate_name(
    git: &GitExecutable,
    repo: &RepositoryHandle,
    name: &str,
    reference: &str,
    deadline: Instant,
) -> Result<(), OperationError> {
    if name.is_empty() || name == "HEAD" || name.starts_with('-') || name.contains('\0') {
        return Err(OperationError::new("INVALID_BRANCH_NAME"));
    }
    let args = ["check-ref-format", reference].map(OsString::from);
    let out = run_local_git(git, &repo.root, &args, &[], None, deadline)?;
    if out.success {
        Ok(())
    } else {
        Err(OperationError::new("INVALID_BRANCH_NAME"))
    }
}

/// 新分支通过零旧 OID 创建，执行后查询实际引用，丢失响应不重放。
fn execute_create(
    reporter: &OperationReporter,
    git: &GitExecutable,
    repo: &RepositoryHandle,
    expected: &WriteFingerprint,
    name: &str,
    reference: &str,
    oid: &str,
) -> OperationResult {
    let deadline = Instant::now() + LOCAL_BUDGET;
    let mut attempted = false;
    let result = (|| {
        if guard::branch_fingerprint(git, repo, deadline)? != *expected {
            return Err(OperationError::new("STALE_WRITE_PLAN"));
        }
        reporter.report(OperationPhase::Writing, None)?;
        let zero = "0".repeat(oid.len());
        let args = [
            "update-ref",
            "--no-deref",
            "-m",
            "gitMaster: 创建分支",
            reference,
            oid,
            &zero,
        ]
        .map(OsString::from);
        attempted = true;
        let output = run_local_git(git, &repo.root, &args, &[], None, deadline);
        let actual = guard::local_query(
            git,
            repo,
            &["show-ref", "--verify", "--hash", reference],
            &[],
            None,
            deadline,
        );
        if actual
            .as_ref()
            .is_ok_and(|bytes| bytes.strip_suffix(b"\n").unwrap_or(bytes) == oid.as_bytes())
        {
            Ok(())
        } else if output.as_ref().is_ok_and(|out| !out.success) {
            attempted = false;
            Err(OperationError::new(if actual.is_ok() {
                "BRANCH_EXISTS"
            } else {
                "GIT_EXECUTION_FAILED"
            }))
        } else {
            Err(OperationError::new("WRITE_OUTCOME_UNKNOWN"))
        }
    })();
    operation_result(
        reporter,
        git,
        repo,
        result,
        attempted,
        None,
        Some(name.to_owned()),
        deadline,
    )
}

/// worktree porcelain 以 NUL 分隔，路径可含换行；同分支任一其他目录即标记占用。
fn occupied_branches(
    bytes: &[u8],
    current_root: &Path,
) -> Result<HashMap<String, bool>, OperationError> {
    let mut result = HashMap::new();
    let mut location = None;
    for field in bytes.split(|byte| *byte == 0) {
        if field.is_empty() {
            location = None;
            continue;
        }
        if let Some(path) = field.strip_prefix(b"worktree ") {
            location = Some(text(path)?);
        } else if let Some(reference) = field.strip_prefix(b"branch ") {
            let location = location
                .as_ref()
                .ok_or_else(|| OperationError::new("PARSE_FAILED"))?;
            let path = Path::new(location);
            let same = path.canonicalize().as_deref().unwrap_or(path) == current_root;
            let reference = text(reference)?;
            result
                .entry(reference)
                .and_modify(|occupied| *occupied |= !same)
                .or_insert(!same);
        }
    }
    Ok(result)
}

/// 分支与工作树显示路径要求无损 UTF-8，不向 UI 透传命令错误原文。
fn text(bytes: &[u8]) -> Result<String, OperationError> {
    std::str::from_utf8(bytes)
        .map(str::to_owned)
        .map_err(|_| OperationError::new("UNSUPPORTED_PATH_ENCODING"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::repository::{open_repository, query, read_repository_state, tests::Fixture};
    use std::{
        fs,
        time::{Duration, Instant},
    };

    /// 等待协调器终态，不重复发起写入。
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
            if let Some(result) = coordinator
                .read_operation(Some(&handle.operation_id))
                .unwrap()
                .and_then(|record| record.result)
            {
                return result;
            }
            assert!(Instant::now() < deadline, "分支操作未及时完成");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    /// 分支列表保留同 OID 不同名称，远端 symbolic HEAD 不作为分支目标。
    #[test]
    fn branch_list_resolves_current_upstream_and_linked_worktree() {
        let f = Fixture::new();
        f.write("a", b"base");
        f.command(&["add", "."]);
        f.command(&["commit", "-m", "base"]);
        f.command(&["branch", "feature"]);
        f.command(&["update-ref", "refs/remotes/origin/main", "HEAD"]);
        f.command(&[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/main",
        ]);
        f.command(&[
            "config",
            "remote.origin.url",
            "https://example.invalid/repo.git",
        ]);
        f.command(&[
            "config",
            "remote.origin.fetch",
            "+refs/heads/*:refs/remotes/origin/*",
        ]);
        f.command(&["branch", "--set-upstream-to=origin/main", "main"]);
        let worktree = f.root.join("linked worktree");
        f.command(&["worktree", "add", worktree.to_str().unwrap(), "feature"]);
        let (repo, _) = open_repository(&f.git, &f.root).unwrap();
        let session = BranchSession::new(&f.git, &repo).unwrap();
        let list = session.list();
        assert_eq!(list.branches.len(), 3);
        let main = list
            .branches
            .iter()
            .find(|branch| branch.name == "main")
            .unwrap();
        let feature = list
            .branches
            .iter()
            .find(|branch| branch.name == "feature")
            .unwrap();
        let remote = list
            .branches
            .iter()
            .find(|branch| branch.name == "origin/main")
            .unwrap();
        assert!(main.current);
        assert!(!main.occupied_by_other_worktree);
        assert!(feature.occupied_by_other_worktree);
        assert_eq!(main.upstream_ref_id.as_ref(), Some(&remote.branch_id));
        assert_eq!(main.oid, feature.oid);
        let (linked, _) = open_repository(&f.git, &worktree).unwrap();
        let linked = BranchSession::new(&f.git, &linked).unwrap().list();
        assert!(
            linked
                .branches
                .iter()
                .find(|branch| branch.name == "feature")
                .unwrap()
                .current
        );
        assert!(
            linked
                .branches
                .iter()
                .find(|branch| branch.name == "main")
                .unwrap()
                .occupied_by_other_worktree
        );
    }

    /// detached 上创建分支仅新增引用，保留 HEAD、索引和未提交文件。
    #[test]
    fn detached_create_is_readonly_until_execution_and_does_not_switch() {
        let f = Fixture::new();
        f.write("a", b"base");
        f.command(&["add", "."]);
        f.command(&["commit", "-m", "base"]);
        f.command(&["checkout", "--detach"]);
        f.write("a", b"dirty");
        let (repo, state) = open_repository(&f.git, &f.root).unwrap();
        let before = [
            repo.git_dir.join("HEAD"),
            repo.git_dir.join("index"),
            f.root.join("a"),
        ]
        .map(|path| fs::read(path).unwrap());
        let refs = query(&f.git, &f.root, &["show-ref"], 4096).unwrap();
        let coordinator = RepositoryCoordinator::new();
        let preview =
            prepare_create_branch(&coordinator, &f.git, &repo, &state, "feature/中文").unwrap();
        assert_eq!(query(&f.git, &f.root, &["show-ref"], 4096).unwrap(), refs);
        assert!(
            matches!(finish(&coordinator, &repo, &preview), OperationResult::Succeeded { branch_name: Some(name), .. } if name == "feature/中文")
        );
        assert_eq!(
            [
                repo.git_dir.join("HEAD"),
                repo.git_dir.join("index"),
                f.root.join("a")
            ]
            .map(|path| fs::read(path).unwrap()),
            before
        );
        let HeadState::Detached { oid } = state.head else {
            panic!("应保留 detached")
        };
        assert_eq!(
            query(
                &f.git,
                &f.root,
                &["rev-parse", "refs/heads/feature/中文"],
                4096
            )
            .unwrap(),
            format!("{oid}\n").as_bytes()
        );
    }

    /// unborn 没有创建来源，非法名称及已有引用命名空间均在准备阶段拒绝。
    #[test]
    fn create_rejects_unborn_invalid_names_and_existing_namespaces() {
        let f = Fixture::new();
        let (repo, state) = open_repository(&f.git, &f.root).unwrap();
        assert!(BranchSession::new(&f.git, &repo)
            .unwrap()
            .list()
            .branches
            .is_empty());
        let coordinator = RepositoryCoordinator::new();
        assert_eq!(
            prepare_create_branch(&coordinator, &f.git, &repo, &state, "feature")
                .unwrap_err()
                .code,
            "HEAD_REQUIRED"
        );
        f.write("a", b"base");
        f.command(&["add", "."]);
        f.command(&["commit", "-m", "base"]);
        f.command(&["branch", "nested/child"]);
        let state = read_repository_state(&f.git, &repo).unwrap();
        let before = query(&f.git, &f.root, &["show-ref"], 4096).unwrap();
        for name in [
            "", "HEAD", "-bad", "@{-1}", "a..b", "a b", "a.lock", "a/", "a\0b",
        ] {
            assert_eq!(
                prepare_create_branch(&coordinator, &f.git, &repo, &state, name)
                    .unwrap_err()
                    .code,
                "INVALID_BRANCH_NAME",
                "名称：{name:?}"
            );
        }
        for name in ["main", "main/child", "nested"] {
            assert_eq!(
                prepare_create_branch(&coordinator, &f.git, &repo, &state, name)
                    .unwrap_err()
                    .code,
                "BRANCH_EXISTS"
            );
        }
        assert_eq!(query(&f.git, &f.root, &["show-ref"], 4096).unwrap(), before);
    }

    /// 外部移动来源提交使准备失效；外部抢先创建同名分支绝不被覆盖。
    #[test]
    fn stale_source_or_existing_target_never_overwrites_reference() {
        let f = Fixture::new();
        f.write("a", b"base");
        f.command(&["add", "."]);
        f.command(&["commit", "-m", "base"]);
        let (repo, state) = open_repository(&f.git, &f.root).unwrap();
        let coordinator = RepositoryCoordinator::new();
        let preview = prepare_create_branch(&coordinator, &f.git, &repo, &state, "new").unwrap();
        f.command(&["commit", "--allow-empty", "-m", "external"]);
        assert!(
            matches!(finish(&coordinator, &repo, &preview), OperationResult::Failed { error, .. } if error.code == "STALE_WRITE_PLAN")
        );
        assert!(query(
            &f.git,
            &f.root,
            &["show-ref", "--verify", "refs/heads/new"],
            4096
        )
        .is_err());
        let state = read_repository_state(&f.git, &repo).unwrap();
        let preview = prepare_create_branch(&coordinator, &f.git, &repo, &state, "new").unwrap();
        f.command(&["branch", "new", "HEAD~1"]);
        let actual = query(
            &f.git,
            &f.root,
            &["show-ref", "--verify", "refs/heads/new"],
            4096,
        )
        .unwrap();
        assert!(
            matches!(finish(&coordinator, &repo, &preview), OperationResult::Failed { error, .. } if error.code == "STALE_WRITE_PLAN")
        );
        assert_eq!(
            query(
                &f.git,
                &f.root,
                &["show-ref", "--verify", "refs/heads/new"],
                4096
            )
            .unwrap(),
            actual
        );
    }

    /// 原子创建使用与来源相同长度的零 OID，兼容 SHA-256 引用。
    #[test]
    fn create_supports_sha256_repository() {
        let f = Fixture::new();
        f.command(&["init", "--object-format=sha256", "-b", "main", "sha256"]);
        for (key, value) in [
            ("user.name", "测试"),
            ("user.email", "test@example.invalid"),
            ("commit.gpgsign", "false"),
        ] {
            f.command(&["-C", "sha256", "config", key, value]);
        }
        f.command(&["-C", "sha256", "commit", "--allow-empty", "-m", "base"]);
        let (repo, state) = open_repository(&f.git, &f.root.join("sha256")).unwrap();
        let coordinator = RepositoryCoordinator::new();
        let preview =
            prepare_create_branch(&coordinator, &f.git, &repo, &state, "sha-feature").unwrap();
        assert!(matches!(
            finish(&coordinator, &repo, &preview),
            OperationResult::Succeeded { .. }
        ));
        let oid = query(
            &f.git,
            &repo.root,
            &["rev-parse", "refs/heads/sha-feature"],
            4096,
        )
        .unwrap();
        assert_eq!(oid.len(), 65);
        assert_eq!(
            oid,
            query(&f.git, &repo.root, &["rev-parse", "HEAD"], 4096).unwrap()
        );
    }

    /// 准备切换不写仓库，执行后具名 HEAD 和工作文件都来自确认目标。
    #[test]
    fn switch_is_readonly_until_execution_and_checks_actual_target() {
        let f = Fixture::new();
        f.write("a", b"main\n");
        f.write("deleted", b"main only\n");
        f.command(&["add", "."]);
        f.command(&["commit", "-m", "main"]);
        f.command(&["checkout", "-b", "feature"]);
        f.write("a", b"feature\n");
        fs::remove_file(f.root.join("deleted")).unwrap();
        f.write("new", b"new file\n");
        f.command(&["add", "--all"]);
        f.command(&["commit", "-am", "feature"]);
        f.command(&["checkout", "main"]);
        let (repo, state) = open_repository(&f.git, &f.root).unwrap();
        let branches = BranchSession::new(&f.git, &repo).unwrap();
        let branch = branches
            .list
            .branches
            .iter()
            .find(|branch| branch.name == "feature")
            .unwrap();
        let before = [
            repo.git_dir.join("HEAD"),
            repo.git_dir.join("index"),
            f.root.join("a"),
        ]
        .map(|path| fs::read(path).unwrap());
        let objects = query(&f.git, &f.root, &["count-objects", "-v"], 4096).unwrap();
        let coordinator = RepositoryCoordinator::new();
        let preview = prepare_switch_branch(
            &coordinator,
            &f.git,
            &repo,
            &state,
            &branches,
            &branch.branch_id,
        )
        .unwrap();
        assert_eq!(preview.paths, ["a", "deleted", "new"]);
        assert_eq!(
            [
                repo.git_dir.join("HEAD"),
                repo.git_dir.join("index"),
                f.root.join("a")
            ]
            .map(|path| fs::read(path).unwrap()),
            before
        );
        assert_eq!(
            query(&f.git, &f.root, &["count-objects", "-v"], 4096).unwrap(),
            objects
        );
        assert!(
            matches!(finish(&coordinator, &repo, &preview), OperationResult::Succeeded { branch_name: Some(name), .. } if name == "feature")
        );
        assert!(
            matches!(read_repository_state(&f.git, &repo).unwrap().head, HeadState::Branch { name, oid } if name == "feature" && oid == branch.oid)
        );
        assert_eq!(fs::read(f.root.join("a")).unwrap(), b"feature\n");
    }

    /// 未暂存、已暂存或未跟踪内容都阻止切换；准备后新增内容同样拒绝。
    #[test]
    fn switch_rejects_dirty_states_and_late_worktree_changes() {
        for mode in ["unstaged", "staged", "untracked", "late"] {
            let f = Fixture::new();
            f.write("a", b"base");
            f.command(&["add", "."]);
            f.command(&["commit", "-m", "base"]);
            f.command(&["branch", "feature"]);
            let (repo, state) = open_repository(&f.git, &f.root).unwrap();
            let branches = BranchSession::new(&f.git, &repo).unwrap();
            let id = branches
                .list
                .branches
                .iter()
                .find(|branch| branch.name == "feature")
                .unwrap()
                .branch_id
                .clone();
            let coordinator = RepositoryCoordinator::new();
            let preview = if mode == "late" {
                Some(
                    prepare_switch_branch(&coordinator, &f.git, &repo, &state, &branches, &id)
                        .unwrap(),
                )
            } else {
                None
            };
            let changed = if mode == "untracked" { "new" } else { "a" };
            f.write(changed, b"user content");
            if mode == "staged" {
                f.command(&["add", "a"]);
            }
            if let Some(preview) = preview {
                assert!(
                    matches!(finish(&coordinator, &repo, &preview), OperationResult::Failed { error, .. } if error.code == "WORKTREE_DIRTY" || error.code == "STALE_WRITE_PLAN")
                );
            } else {
                let state = read_repository_state(&f.git, &repo).unwrap();
                assert_eq!(
                    prepare_switch_branch(&coordinator, &f.git, &repo, &state, &branches, &id)
                        .unwrap_err()
                        .code,
                    "WORKTREE_DIRTY"
                );
            }
            assert_eq!(fs::read(f.root.join(changed)).unwrap(), b"user content");
            assert!(
                matches!(read_repository_state(&f.git, &repo).unwrap().head, HeadState::Branch { name, .. } if name == "main")
            );
        }
    }

    /// 占用分支、远端分支和其他列表签发的 ID 都不能作为切换目标。
    #[test]
    fn switch_rejects_occupied_remote_and_foreign_ids() {
        let f = Fixture::new();
        f.command(&["commit", "--allow-empty", "-m", "base"]);
        f.command(&["branch", "feature"]);
        f.command(&["update-ref", "refs/remotes/origin/main", "HEAD"]);
        let owner = Fixture::new();
        f.command(&[
            "worktree",
            "add",
            owner.root.join("linked").to_str().unwrap(),
            "feature",
        ]);
        let (repo, state) = open_repository(&f.git, &f.root).unwrap();
        let branches = BranchSession::new(&f.git, &repo).unwrap();
        let id = &branches
            .list
            .branches
            .iter()
            .find(|branch| branch.name == "feature")
            .unwrap()
            .branch_id;
        let coordinator = RepositoryCoordinator::new();
        assert_eq!(
            prepare_switch_branch(&coordinator, &f.git, &repo, &state, &branches, id)
                .unwrap_err()
                .code,
            "BRANCH_IN_USE"
        );
        let remote = &branches
            .list
            .branches
            .iter()
            .find(|branch| branch.kind == ReferenceKind::Remote)
            .unwrap()
            .branch_id;
        assert_eq!(
            prepare_switch_branch(&coordinator, &f.git, &repo, &state, &branches, remote)
                .unwrap_err()
                .code,
            "INVALID_INPUT"
        );
        let newer = BranchSession::new(&f.git, &repo).unwrap();
        assert_eq!(
            prepare_switch_branch(&coordinator, &f.git, &repo, &state, &newer, id)
                .unwrap_err()
                .code,
            "INVALID_INPUT"
        );
    }

    /// Git 忽略规则不构成覆盖授权，冲突的被忽略文件保留且 HEAD 不变。
    #[test]
    fn switch_preserves_ignored_collision() {
        let f = Fixture::new();
        f.write(".gitignore", b"ignored\n");
        f.command(&["add", "."]);
        f.command(&["commit", "-m", "main"]);
        f.command(&["checkout", "-b", "feature"]);
        f.write("ignored", b"target");
        f.command(&["add", "-f", "ignored"]);
        f.command(&["commit", "-m", "feature"]);
        f.command(&["checkout", "main"]);
        f.write("ignored", b"private user content");
        let (repo, state) = open_repository(&f.git, &f.root).unwrap();
        assert!(state.changes.is_empty());
        let branches = BranchSession::new(&f.git, &repo).unwrap();
        let id = &branches
            .list
            .branches
            .iter()
            .find(|branch| branch.name == "feature")
            .unwrap()
            .branch_id;
        let coordinator = RepositoryCoordinator::new();
        let preview =
            prepare_switch_branch(&coordinator, &f.git, &repo, &state, &branches, id).unwrap();
        assert!(matches!(
            finish(&coordinator, &repo, &preview),
            OperationResult::Failed { .. }
        ));
        assert_eq!(
            fs::read(f.root.join("ignored")).unwrap(),
            b"private user content"
        );
        assert_eq!(
            read_repository_state(&f.git, &repo).unwrap().head,
            state.head
        );
    }

    /// 当前树安全也必须检查目标属性与文件模式，不运行目标过滤器。
    #[cfg(unix)]
    #[test]
    fn switch_rejects_unsafe_target_tree_and_external_ref_move() {
        for mode in ["filter", "symlink", "ref-change"] {
            let f = Fixture::new();
            f.write("a", b"base");
            f.command(&["add", "."]);
            f.command(&["commit", "-m", "base"]);
            f.command(&["checkout", "-b", "feature"]);
            if mode == "filter" {
                f.write(".gitattributes", b"*.txt filter=evil\n");
                f.write("file.txt", b"target");
            } else if mode == "symlink" {
                std::os::unix::fs::symlink("/etc/passwd", f.root.join("link")).unwrap();
            } else {
                f.write("a", b"feature");
            }
            f.command(&["add", "."]);
            f.command(&["commit", "-m", "feature"]);
            f.command(&["checkout", "main"]);
            if mode == "filter" {
                f.command(&["config", "filter.evil.smudge", "touch executed"]);
            }
            let (repo, state) = open_repository(&f.git, &f.root).unwrap();
            let branches = BranchSession::new(&f.git, &repo).unwrap();
            let id = &branches
                .list
                .branches
                .iter()
                .find(|branch| branch.name == "feature")
                .unwrap()
                .branch_id;
            let coordinator = RepositoryCoordinator::new();
            let result = prepare_switch_branch(&coordinator, &f.git, &repo, &state, &branches, id);
            if mode == "ref-change" {
                let preview = result.unwrap();
                f.command(&["update-ref", "refs/heads/feature", "HEAD"]);
                assert!(
                    matches!(finish(&coordinator, &repo, &preview), OperationResult::Failed { error, .. } if error.code == "STALE_WRITE_PLAN")
                );
            } else {
                assert_eq!(result.unwrap_err().code, "UNSUPPORTED_WRITE_CONFIGURATION");
            }
            assert!(!f.root.join("executed").exists());
            assert_eq!(fs::read(f.root.join("a")).unwrap(), b"base");
            assert_eq!(
                read_repository_state(&f.git, &repo).unwrap().head,
                state.head
            );
        }
    }
}
