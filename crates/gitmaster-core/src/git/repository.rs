use super::*;
use super::{process::run_git, status::parse_status};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// 生成进程内唯一身份；界面不得把路径当作授权令牌。
pub(crate) fn next_id() -> String {
    NEXT_ID.fetch_add(1, Ordering::Relaxed).to_string()
}

/// 运行限定的查询并将内部错误映射为稳定代码。
pub(crate) fn query(
    git: &GitExecutable,
    cwd: &Path,
    args: &[&str],
    limit: usize,
) -> Result<Vec<u8>, OperationError> {
    query_until(git, cwd, args, limit, None)
}

/// 同一次业务查询链共享期限；M1 的普通入口继续使用单进程读取上限。
pub(crate) fn query_until(
    git: &GitExecutable,
    cwd: &Path,
    args: &[&str],
    limit: usize,
    deadline: Option<Instant>,
) -> Result<Vec<u8>, OperationError> {
    let mut command_index = 0;
    while args.get(command_index) == Some(&"-c") {
        command_index += 2;
    }
    let command = args.get(command_index).copied();
    let args: Vec<OsString> = args.iter().map(OsString::from).collect();
    let out = match deadline {
        Some(deadline) => {
            super::process::run_git_read_until(git, cwd, &args, limit, false, deadline)
        }
        None => run_git(git, cwd, &args, limit, false),
    }
    .map_err(|error| {
        let os_code = error.diagnostic.as_ref().and_then(|info| info.os_code);
        error.with_diagnostic(query_stage(command), os_code, None)
    })?;
    if !out.success {
        return Err(command_error_with_context(
            &out.stderr,
            command,
            out.exit_code,
        ));
    }
    Ok(out.stdout)
}

/// 脱敏底层失败；这些映射只用于提示，仓库身份始终来自成功的机器查询。
/// 将查询命令映射到稳定阶段，并附加退出码而不透传 stderr。
pub(crate) fn command_error(stderr: &[u8]) -> OperationError {
    command_error_with_context(stderr, None, None)
}

/// 分类查询失败并附加退出码；stderr 仅用于本地错误分类。
fn command_error_with_context(
    stderr: &[u8],
    command: Option<&str>,
    exit_code: Option<i32>,
) -> OperationError {
    let text = String::from_utf8_lossy(stderr);
    OperationError::new(if text.contains("dubious ownership") {
        "UNSAFE_REPOSITORY"
    } else if text.contains("Permission denied") {
        "ACCESS_DENIED"
    } else if text.contains("not a git repository") {
        "NOT_REPOSITORY"
    } else {
        "GIT_EXECUTION_FAILED"
    })
    .with_diagnostic(query_stage(command), None, exit_code)
}

/// 只允许查询入口使用的 Git 子命令进入诊断阶段。
fn query_stage(command: Option<&str>) -> &'static str {
    match command {
        Some("rev-parse") => "revParse",
        Some("status") => "status",
        Some("log") => "log",
        Some("rev-list") => "revList",
        Some("for-each-ref") => "forEachRef",
        _ => "gitQuery",
    }
}

/// 无损解码机器路径，移除 Git 添加的唯一换行而非路径中的空白。
fn git_path(bytes: Vec<u8>) -> Result<PathBuf, OperationError> {
    let bytes = bytes.strip_suffix(b"\n").unwrap_or(&bytes);
    let path =
        std::str::from_utf8(bytes).map_err(|_| OperationError::new("UNSUPPORTED_PATH_ENCODING"))?;
    std::fs::canonicalize(path).map_err(|_| OperationError::new("ACCESS_DENIED"))
}

/// 无损转换显示路径；不可表示的文件名不通过有损字符串执行。
pub(crate) fn path_text(path: &Path) -> Result<String, OperationError> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| OperationError::new("UNSUPPORTED_PATH_ENCODING"))
}

/// 识别本地工作树并返回当前只读快照。
pub fn open_repository(
    git: &GitExecutable,
    path: &Path,
) -> Result<(RepositoryHandle, RepositoryState), OperationError> {
    let repository = identify_repository(git, path)?;
    let state = read_repository_state(git, &repository)?;
    Ok((repository, state))
}

/// 先识别工作树身份，允许桌面在第一次状态读取前安装文件监听。
pub fn identify_repository(
    git: &GitExecutable,
    path: &Path,
) -> Result<RepositoryHandle, OperationError> {
    if !path.is_dir() {
        return Err(OperationError::new("NOT_REPOSITORY"));
    }
    let bare = query(git, path, &["rev-parse", "--is-bare-repository"], 4096)?;
    if bare == b"true\n" {
        return Err(OperationError::new("BARE_REPOSITORY"));
    }
    if query(git, path, &["rev-parse", "--is-inside-work-tree"], 4096)? != b"true\n" {
        return Err(OperationError::new("NOT_REPOSITORY"));
    }
    let root = git_path(query(
        git,
        path,
        &["rev-parse", "--path-format=absolute", "--show-toplevel"],
        65536,
    )?)?;
    let git_dir = git_path(query(
        git,
        path,
        &["rev-parse", "--absolute-git-dir"],
        65536,
    )?)?;
    let common_dir = git_path(query(
        git,
        path,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        65536,
    )?)?;
    Ok(RepositoryHandle {
        id: next_id(),
        root,
        git_dir,
        common_dir,
    })
}

/// 刷新已识别工作树的完整快照，不执行网络或写入操作。
pub fn read_repository_state(
    git: &GitExecutable,
    repository: &RepositoryHandle,
) -> Result<RepositoryState, OperationError> {
    read_state_until(git, repository, None)
}

/// 写业务中的状态核验与调用方共享剩余时间预算。
pub(crate) fn read_repository_state_until(
    git: &GitExecutable,
    repository: &RepositoryHandle,
    deadline: Instant,
) -> Result<RepositoryState, OperationError> {
    read_state_until(git, repository, Some(deadline))
}

/// 复用同一解析流程，避免限时读取与 M1 展示发生语义分歧。
fn read_state_until(
    git: &GitExecutable,
    repository: &RepositoryHandle,
    deadline: Option<Instant>,
) -> Result<RepositoryState, OperationError> {
    let current_dir = git_path(query_until(
        git,
        &repository.root,
        &["rev-parse", "--absolute-git-dir"],
        65536,
        deadline,
    )?)?;
    if current_dir != repository.git_dir {
        return Err(OperationError::new("STALE_REQUEST"));
    }
    let bytes = query_until(
        git,
        &repository.root,
        &[
            "-c",
            "status.renames=true",
            "status",
            "--porcelain=v2",
            "--branch",
            "-z",
            "--untracked-files=all",
            "--ignore-submodules=dirty",
        ],
        8 * 1024 * 1024,
        deadline,
    )?;
    let (head, changes) = parse_status(&bytes)?;
    let mut operations = Vec::new();
    for (name, markers) in [
        ("merge", &["MERGE_HEAD"][..]),
        ("rebase", &["rebase-merge", "rebase-apply"][..]),
        ("cherryPick", &["CHERRY_PICK_HEAD"][..]),
        ("revert", &["REVERT_HEAD"][..]),
        ("bisect", &["BISECT_LOG"][..]),
    ] {
        if markers
            .iter()
            .any(|marker| repository.git_dir.join(marker).exists())
        {
            operations.push(name.to_owned());
        }
    }
    Ok(RepositoryState {
        repository_id: repository.id.clone(),
        snapshot_id: next_id(),
        root_path: path_text(&repository.root)?,
        head,
        operations,
        changes,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::{
        fs,
        path::PathBuf,
        process::Command,
        sync::atomic::{AtomicU64, Ordering},
    };
    static NEXT: AtomicU64 = AtomicU64::new(0);

    /// 隔离的临时仓库，不读写用户仓库或全局 Git 配置。
    pub struct Fixture {
        pub root: PathBuf,
        pub git: GitExecutable,
    }
    impl Fixture {
        /// 创建独立测试目录并初始化仓库，关闭提交签名。
        pub fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "gitmaster-中文 测试-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&root).unwrap();
            let git = super::super::environment::resolve_git(None).unwrap();
            let fixture = Self { root, git };
            fixture.command(&["init", "-b", "main"]);
            fixture.command(&["config", "user.name", "测试"]);
            fixture.command(&["config", "user.email", "test@example.invalid"]);
            fixture.command(&["config", "commit.gpgsign", "false"]);
            fixture
        }
        /// 仅在 fixture 内执行测试准备命令，断言准备成功。
        pub fn command(&self, args: &[&str]) {
            let mut cmd = Command::new(&self.git.path);
            for (key, _) in
                std::env::vars_os().filter(|(k, _)| k.to_string_lossy().starts_with("GIT_"))
            {
                cmd.env_remove(key);
            }
            let output = cmd
                .current_dir(&self.root)
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env(
                    "GIT_CONFIG_GLOBAL",
                    if cfg!(windows) { "NUL" } else { "/dev/null" },
                )
                .args(args)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "测试准备失败：{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        /// 写入测试文件并创建父目录。
        pub fn write(&self, path: &str, content: &[u8]) {
            let target = self.root.join(path);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::write(target, content).unwrap();
        }
    }
    impl Drop for Fixture {
        /// 清理仅由本测试创建的目录。
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    /// 空仓库和子目录都能识别，bare/普通目录明确拒绝。
    #[test]
    fn repository_kinds() {
        let f = Fixture::new();
        fs::create_dir(f.root.join("子目录")).unwrap();
        let (repo, state) = open_repository(&f.git, &f.root.join("子目录")).unwrap();
        assert_eq!(repo.root, f.root.canonicalize().unwrap());
        assert_eq!(
            state.head,
            HeadState::Unborn {
                name: "main".into()
            }
        );
        let plain = f.root.join("plain");
        fs::create_dir(&plain).unwrap();
        // 嵌套普通目录仍归属外层工作树；独立 bare 目录须拒绝。
        f.command(&["init", "--bare", "bare.git"]);
        assert_eq!(
            open_repository(&f.git, &f.root.join("bare.git"))
                .unwrap_err()
                .code,
            "BARE_REPOSITORY"
        );
        assert!(open_repository(&f.git, Path::new("/gitmaster-nonexistent")).is_err());
    }

    /// 状态读取保留索引与用户配置的原始字节。
    #[test]
    fn real_status_is_read_only() {
        let f = Fixture::new();
        f.write("中文 空格.txt", b"base\n");
        f.command(&["add", "."]);
        f.command(&["commit", "-m", "base"]);
        f.write("中文 空格.txt", b"staged\n");
        f.command(&["add", "."]);
        f.write("中文 空格.txt", b"unstaged\n");
        f.write("新文件", b"new");
        let observed = [
            ".git/HEAD",
            ".git/refs/heads/main",
            "中文 空格.txt",
            "新文件",
        ];
        let originals: Vec<_> = observed
            .iter()
            .map(|p| fs::read(f.root.join(p)).unwrap())
            .collect();
        let index = fs::read(f.root.join(".git/index")).unwrap();
        let config = fs::read(f.root.join(".git/config")).unwrap();
        let (_, state) = open_repository(&f.git, &f.root).unwrap();
        assert_eq!(state.changes.len(), 2);
        let change = state
            .changes
            .iter()
            .find(|c| c.path == "中文 空格.txt")
            .unwrap();
        assert_eq!(
            (&change.index_status[..], &change.worktree_status[..]),
            ("M", "M")
        );
        assert_eq!(index, fs::read(f.root.join(".git/index")).unwrap());
        assert_eq!(config, fs::read(f.root.join(".git/config")).unwrap());
        for (path, original) in observed.iter().zip(originals) {
            assert_eq!(original, fs::read(f.root.join(path)).unwrap());
        }
    }

    /// linked worktree 使用实际 Git 目录，detached HEAD 不伪装分支。
    #[test]
    fn linked_worktree_and_detached() {
        let f = Fixture::new();
        f.write("a", b"base");
        f.command(&["add", "."]);
        f.command(&["commit", "-m", "base"]);
        f.command(&["worktree", "add", "--detach", "linked"]);
        let (repo, state) = open_repository(&f.git, &f.root.join("linked")).unwrap();
        assert!(matches!(state.head, HeadState::Detached { .. }));
        assert_ne!(repo.git_dir, repo.common_dir);
        fs::write(repo.git_dir.join("MERGE_HEAD"), "abc\n").unwrap();
        assert!(read_repository_state(&f.git, &repo)
            .unwrap()
            .operations
            .contains(&"merge".into()));
    }
}
