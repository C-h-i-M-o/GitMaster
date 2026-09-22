//! clone 使用私有下载和检出目录，最后向选定父目录发布新仓库。
use super::{auth::AuthPolicy, url};
use crate::git::{
    coordinator::{CoordinationKey, OperationReporter, RepositoryCoordinator},
    process::{run_isolated_git, run_network_git, ProcessOutput},
    repository::{next_id, read_repository_state_until},
    write_guard, *,
};
use cap_std::fs::Dir;
use std::{
    ffi::OsString,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const BUDGET: Duration = Duration::from_secs(15 * 60);

/// 测试只替换传输协议，计划、目标保护和检出均执行真实实现。
type NetworkRunner = fn(
    &GitExecutable,
    &Path,
    &[OsString],
    Option<&Path>,
    &[(String, String)],
    Instant,
    &mut dyn FnMut(u64, u64),
) -> Result<ProcessOutput, OperationError>;

/// 固定已选择父目录的能力与身份，私有下载目录由一次性计划管理。
struct ClonePlan {
    git: GitExecutable,
    parent: PathBuf,
    parent_dir: Dir,
    name: String,
    url: String,
    auth: AuthPolicy,
    scratch: tempfile::TempDir,
    conversion: Vec<(String, String)>,
}

/// 准备 clone，不连接远端或创建用户目标。
pub fn prepare_clone(
    coordinator: &RepositoryCoordinator,
    git: &GitExecutable,
    parent: &Path,
    raw_url: &str,
    name: &str,
) -> Result<ClonePreview, OperationError> {
    prepare_with_runner(coordinator, git, parent, raw_url, name, run_network_git)
}

/// 共同准备入口，真实下载通过固定生产执行器完成。
fn prepare_with_runner(
    coordinator: &RepositoryCoordinator,
    git: &GitExecutable,
    parent: &Path,
    raw_url: &str,
    name: &str,
    runner: NetworkRunner,
) -> Result<ClonePreview, OperationError> {
    let generation = coordinator.begin_prepare()?;
    if !cfg!(any(unix, windows)) {
        return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
    }
    url::validate(raw_url)?;
    let key = CoordinationKey::clone_target(parent, name)?;
    if name.starts_with('-') || name.eq_ignore_ascii_case(".git") || name.contains('\0') {
        return Err(OperationError::new("INVALID_INPUT"));
    }
    let parent = std::fs::canonicalize(parent).map_err(|_| OperationError::new("ACCESS_DENIED"))?;
    let target = parent.join(name);
    let target_path = target
        .to_str()
        .ok_or_else(|| OperationError::new("UNSUPPORTED_PATH_ENCODING"))?
        .to_owned();
    let parent_dir = Dir::open_ambient_dir(&parent, cap_std::ambient_authority())
        .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
    let scratch = tempfile::Builder::new()
        .prefix("gitmaster-clone-")
        .tempdir()
        .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
    if scratch.path().to_str().is_none() {
        return Err(OperationError::new("UNSUPPORTED_PATH_ENCODING"));
    }
    let deadline = Instant::now() + super::BUDGET;
    let auth = AuthPolicy::capture(git, scratch.path(), deadline)?;
    // 捕获内建换行设置；不会把用户级可执行程序或 include 写入新仓库。
    let conversion = ["core.autocrlf", "core.eol", "core.safecrlf"]
        .into_iter()
        .filter_map(|key| {
            auth.values(key)
                .last()
                .map(|value| (key.to_owned(), (*value).to_owned()))
        })
        .collect();
    capture_attributes(&auth, scratch.path())?;
    let plan = ClonePlan {
        git: git.clone(),
        parent,
        parent_dir,
        name: name.into(),
        url: raw_url.into(),
        auth,
        scratch,
        conversion,
    };
    plan.verify(deadline)?;
    let prepared = coordinator.prepare(
        generation,
        None,
        key,
        OperationKind::Clone,
        move |reporter| execute_clone(reporter, plan, runner),
    )?;
    Ok(ClonePreview {
        plan_id: prepared.plan_id,
        display_url: url::display(raw_url),
        target_path,
        expires_at: prepared.expires_at,
    })
}

impl ClonePlan {
    /// 父目录可以被改名或替换；身份不一致时不在替换后的路径创建目标。
    fn verify(&self, deadline: Instant) -> Result<(), OperationError> {
        write_guard::check_time(deadline)?;
        self.auth.verify(&self.git, self.scratch.path(), deadline)?;
        self.verify_parent()?;
        match self.parent_dir.symlink_metadata(&self.name) {
            Ok(_) => Err(OperationError::new("TARGET_EXISTS")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(OperationError::new("ACCESS_DENIED")),
        }
    }
    /// 通过目录句柄和实际显示路径核对同一目录，不跟随替换后的父目录。
    fn verify_parent(&self) -> Result<(), OperationError> {
        let current = Dir::open_ambient_dir(&self.parent, cap_std::ambient_authority())
            .map_err(|_| OperationError::new("STALE_WRITE_PLAN"))?;
        if !same_directory(&current, &self.parent_dir)? {
            return Err(OperationError::new("STALE_WRITE_PLAN"));
        }
        Ok(())
    }
    /// 在私有下载仓库读取或检出固定对象，仅追加已捕获的属性文件路径。
    fn local(
        &self,
        args: &[&str],
        input: &[u8],
        deadline: Instant,
    ) -> Result<ProcessOutput, OperationError> {
        let mut all = vec![OsString::from("-c")];
        let mut attribute = OsString::from("core.attributesFile=");
        attribute.push(self.scratch.path().join("attributes"));
        all.push(attribute);
        all.extend(args.iter().map(OsString::from));
        run_isolated_git(
            &self.git,
            &self.scratch.path().join("repository"),
            &all,
            input,
            None,
            &self.conversion,
            deadline,
        )
    }
    /// 下载不检出，不使用本地对象共享、模板或子模块递归。
    fn download(
        &self,
        reporter: &OperationReporter,
        runner: NetworkRunner,
        deadline: Instant,
    ) -> Result<(), OperationError> {
        reporter.report(OperationPhase::Transferring, None)?;
        let mut args: Vec<OsString> = [
            "clone",
            "--no-checkout",
            "--no-local",
            "--no-recurse-submodules",
            "--template=",
            "--progress",
        ]
        .map(OsString::from)
        .into();
        for (key, value) in &self.conversion {
            args.extend(["--config".into(), format!("{key}={value}").into()]);
        }
        args.extend(["--".into(), self.url.clone().into(), "repository".into()]);
        let result = runner(
            &self.git,
            self.scratch.path(),
            &args,
            None,
            &self.auth.settings,
            deadline,
            &mut |completed, total| {
                let _ = reporter.report(
                    OperationPhase::Transferring,
                    Some(OperationCounts {
                        completed,
                        total: Some(total),
                    }),
                );
            },
        )?;
        if !result.success {
            return Err(OperationError::new("NETWORK_FAILED"));
        }
        Ok(())
    }
    /// 先读取固定目标树和属性再检出；空远端可以成功，失效远端 HEAD 不能误报为空。
    fn checkout(
        &self,
        reporter: &OperationReporter,
        deadline: Instant,
    ) -> Result<HeadState, OperationError> {
        reporter.report(OperationPhase::Checking, None)?;
        let root = self.scratch.path().join("repository");
        let head = self.local(&["rev-parse", "--verify", "HEAD^{commit}"], &[], deadline)?;
        if !head.success {
            let refs = self.local(&["for-each-ref", "--format=%(refname)"], &[], deadline)?;
            if refs.success
                && refs.stdout.is_empty()
                && self
                    .local(&["symbolic-ref", "HEAD"], &[], deadline)?
                    .success
            {
                return self.checked_head(deadline);
            }
            return Err(OperationError::new("CHECKOUT_UNSUPPORTED"));
        }
        let oid = super::text(&head.stdout)?.trim().to_owned();
        if !matches!(oid.len(), 40 | 64) || !oid.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(OperationError::new("PARSE_FAILED"));
        }
        let read = self.local(&["read-tree", &oid], &[], deadline)?;
        if !read.success {
            return Err(OperationError::new("CHECKOUT_UNSUPPORTED"));
        }
        let repo = RepositoryHandle {
            id: String::new(),
            root: root.clone(),
            git_dir: root.join(".git"),
            common_dir: root.join(".git"),
        };
        let entries = write_guard::read_index(&self.git, &repo, None, deadline)?;
        if entries.len() > 10000 {
            return Err(OperationError::new("OUTPUT_LIMIT"));
        }
        let mut input = Vec::new();
        for entry in &entries {
            input.extend_from_slice(entry.path.as_bytes());
            input.push(0);
        }
        if !input.is_empty() {
            let attrs = self.local(
                &[
                    "check-attr",
                    "--cached",
                    "-z",
                    "--stdin",
                    "filter",
                    "merge",
                    "working-tree-encoding",
                ],
                &input,
                deadline,
            )?;
            if !attrs.success {
                return Err(OperationError::new("CHECKOUT_UNSUPPORTED"));
            }
            write_guard::validate_attributes(&attrs.stdout)?;
        }
        reporter.report(OperationPhase::Writing, None)?;
        let checked = self.local(&["checkout-index", "--all"], &[], deadline)?;
        if !checked.success {
            return Err(OperationError::new("CHECKOUT_UNSUPPORTED"));
        }
        self.checked_head(deadline)
    }
    /// 捕获私有检出实际 HEAD，发布后不能把另一个干净仓库当作本次成功。
    fn checked_head(&self, deadline: Instant) -> Result<HeadState, OperationError> {
        let root =
            std::fs::canonicalize(self.scratch.path().join("repository")).map_err(io_error)?;
        let repo = RepositoryHandle {
            id: next_id(),
            git_dir: root.join(".git"),
            common_dir: root.join(".git"),
            root,
        };
        let state = read_repository_state_until(&self.git, &repo, deadline)?;
        if !state.changes.is_empty() {
            return Err(OperationError::new("CHECKOUT_UNSUPPORTED"));
        }
        Ok(state.head)
    }
    /// 发布时只创建新目录和新文件，跨卷同样有效；任何已有条目都不覆盖。
    fn publish(&self, deadline: Instant) -> Result<(PathBuf, Dir), OperationError> {
        self.verify(deadline)?;
        self.parent_dir.create_dir(&self.name).map_err(io_error)?;
        let destination = open_child(&self.parent_dir, Path::new(&self.name))?;
        let source_path = self.scratch.path().join("repository");
        if !source_path.exists() {
            std::fs::create_dir(&source_path).map_err(io_error)?;
        }
        let source =
            Dir::open_ambient_dir(&source_path, cap_std::ambient_authority()).map_err(io_error)?;
        copy_tree(&source, &destination, 0, &mut 0, deadline)?;
        self.verify_parent()?;
        let live = open_child(&self.parent_dir, Path::new(&self.name))?;
        if !same_directory(&live, &destination)? {
            return Err(OperationError::new("STALE_WRITE_PLAN"));
        }
        Ok((self.parent.join(&self.name), destination))
    }
}

/// 冻结用户属性文件，默认 XDG/HOME 路径也遵循 Git 规则；不保存外部执行配置。
fn capture_attributes(auth: &AuthPolicy, scratch: &Path) -> Result<(), OperationError> {
    let configured = auth.values("core.attributesfile");
    let source = match configured.last().copied() {
        Some("") => None,
        Some(path) if path.starts_with("~/") => Some(
            PathBuf::from(
                std::env::var_os("HOME")
                    .ok_or_else(|| OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"))?,
            )
            .join(&path[2..]),
        ),
        Some(path) if Path::new(path).is_absolute() => Some(PathBuf::from(path)),
        Some(_) => return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION")),
        None => std::env::var_os("XDG_CONFIG_HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
            .map(|base| base.join("git/attributes")),
    };
    let bytes = match source
        .map(|path| {
            let mut options = std::fs::OpenOptions::new();
            options.read(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.custom_flags(libc::O_NONBLOCK);
            }
            options.open(path)
        })
        .transpose()
    {
        Ok(Some(file)) => {
            if !file.metadata().map_err(io_error)?.is_file() {
                return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
            }
            let mut bytes = Vec::new();
            file.take(1024 * 1024 + 1)
                .read_to_end(&mut bytes)
                .map_err(io_error)?;
            if bytes.len() > 1024 * 1024 {
                return Err(OperationError::new("OUTPUT_LIMIT"));
            }
            bytes
        }
        Ok(None) => Vec::new(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(io_error(error)),
    };
    std::fs::write(scratch.join("attributes"), bytes).map_err(io_error)
}

/// 同一设备上的 inode 身份用于识别被外部替换的目录；未实现平台不能进入写路径。
fn same_directory(first: &Dir, second: &Dir) -> Result<bool, OperationError> {
    #[cfg(unix)]
    {
        use cap_std::fs::MetadataExt;
        let a = first.dir_metadata().map_err(io_error)?;
        let b = second.dir_metadata().map_err(io_error)?;
        Ok(a.dev() == b.dev() && a.ino() == b.ino())
    }
    #[cfg(windows)]
    {
        Ok(crate::git::windows_fs::same(
            &first.dir_metadata().map_err(io_error)?,
            &second.dir_metadata().map_err(io_error)?,
        ))
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (first, second);
        Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"))
    }
}

/// 使用目录句柄和 O_NOFOLLOW 打开单一子目录，不能把链接当作新目标继续写入。
fn open_child(parent: &Dir, name: &Path) -> Result<Dir, OperationError> {
    #[cfg(unix)]
    {
        use std::os::{
            fd::{AsRawFd, FromRawFd},
            unix::ffi::OsStrExt,
        };
        let name = std::ffi::CString::new(name.as_os_str().as_bytes())
            .map_err(|_| OperationError::new("INVALID_INPUT"))?;
        // 参数均为存活的目录句柄及单组件名称，成功后立即移交 fd 所有权。
        let fd = unsafe {
            libc::openat(
                parent.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(io_error(std::io::Error::last_os_error()));
        }
        Ok(Dir::from_std_file(unsafe {
            std::fs::File::from_raw_fd(fd)
        }))
    }
    #[cfg(windows)]
    {
        crate::git::windows_fs::open_directory(parent, name)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (parent, name);
        Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"))
    }
}

/// 只复制普通文件和目录，缓冲固定为 64 KiB；每次写入都受总期限及 create_new 保护。
fn copy_tree(
    source: &Dir,
    destination: &Dir,
    depth: usize,
    count: &mut usize,
    deadline: Instant,
) -> Result<(), OperationError> {
    if depth > 128 {
        return Err(OperationError::new("OUTPUT_LIMIT"));
    }
    for entry in source.entries().map_err(io_error)? {
        write_guard::check_time(deadline)?;
        *count += 1;
        if *count > 100000 {
            return Err(OperationError::new("OUTPUT_LIMIT"));
        }
        let entry = entry.map_err(io_error)?;
        let name = entry.file_name();
        let kind = entry.file_type().map_err(io_error)?;
        if kind.is_dir() {
            destination.create_dir(&name).map_err(io_error)?;
            copy_tree(
                &open_child(source, Path::new(&name))?,
                &open_child(destination, Path::new(&name))?,
                depth + 1,
                count,
                deadline,
            )?;
        } else if kind.is_file() {
            let mut options = cap_std::fs::OpenOptions::new();
            options.read(true);
            #[cfg(unix)]
            {
                use cap_std::fs::OpenOptionsExt;
                options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
            }
            #[cfg(windows)]
            crate::git::windows_fs::nofollow(&mut options);
            let mut input = source.open_with(&name, &options).map_err(io_error)?;
            let metadata = input.metadata().map_err(io_error)?;
            #[cfg(windows)]
            if crate::git::windows_fs::is_reparse(&metadata) {
                return Err(OperationError::new("CHECKOUT_UNSUPPORTED"));
            }
            if !metadata.is_file() {
                return Err(OperationError::new("CHECKOUT_UNSUPPORTED"));
            }
            let mut options = cap_std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            let mut output = destination.open_with(&name, &options).map_err(io_error)?;
            let mut buffer = vec![0u8; 65536];
            loop {
                write_guard::check_time(deadline)?;
                let size = input.read(&mut buffer).map_err(io_error)?;
                if size == 0 {
                    break;
                }
                output.write_all(&buffer[..size]).map_err(io_error)?;
            }
            output
                .set_permissions(metadata.permissions())
                .map_err(io_error)?;
        } else {
            return Err(OperationError::new("CHECKOUT_UNSUPPORTED"));
        }
    }
    Ok(())
}

/// 文件系统失败只返回稳定码，不把本地路径或操作系统原文作为错误日志。
fn io_error(error: std::io::Error) -> OperationError {
    OperationError::new(if error.kind() == std::io::ErrorKind::AlreadyExists {
        "TARGET_EXISTS"
    } else {
        "ACCESS_DENIED"
    })
}

/// 下载与检出失败仍保留可定位内容；仅完整发布并核验的仓库报告成功。
fn execute_clone(
    reporter: &OperationReporter,
    plan: ClonePlan,
    runner: NetworkRunner,
) -> OperationResult {
    let deadline = reporter.deadline(BUDGET);
    let operation_id = reporter.handle.operation_id.clone();
    if let Err(error) = plan.verify(deadline) {
        return OperationResult::Failed {
            operation_id,
            kind: OperationKind::Clone,
            error,
            clone_recovery: None,
            refresh: WriteRefresh::NotApplicable,
        };
    }
    let mut stage = CloneStage::Download;
    let work = (|| {
        plan.download(reporter, runner, deadline)?;
        stage = CloneStage::Checkout;
        plan.checkout(reporter, deadline)
    })();
    let _ = reporter.report(OperationPhase::Writing, None);
    let published = plan.publish(deadline);
    let (target, destination) = match published {
        Ok(target) => target,
        Err(error) => {
            let (error, recovery_stage) = match work {
                Err(cause) => (cause, stage),
                Ok(_) => (error, CloneStage::Publish),
            };
            let root = plan.scratch.keep();
            return OperationResult::Failed {
                operation_id,
                kind: OperationKind::Clone,
                error,
                clone_recovery: Some(CloneRecovery {
                    path: root.to_string_lossy().into_owned(),
                    stage: recovery_stage,
                }),
                refresh: WriteRefresh::NotApplicable,
            };
        }
    };
    let expected = match work {
        Ok(head) => head,
        Err(error) => {
            return OperationResult::Failed {
                operation_id,
                kind: OperationKind::Clone,
                error,
                clone_recovery: Some(CloneRecovery {
                    path: target.to_string_lossy().into_owned(),
                    stage,
                }),
                refresh: WriteRefresh::NotApplicable,
            }
        }
    };
    let _ = reporter.report(OperationPhase::Verifying, None);
    let refreshed = (|| {
        plan.verify_parent()?;
        if !same_directory(
            &open_child(&plan.parent_dir, Path::new(&plan.name))?,
            &destination,
        )? {
            return Err(OperationError::new("STALE_WRITE_PLAN"));
        }
        let repo = RepositoryHandle {
            id: next_id(),
            root: target.clone(),
            git_dir: target.join(".git"),
            common_dir: target.join(".git"),
        };
        let state = read_repository_state_until(&plan.git, &repo, deadline)?;
        plan.verify_parent()?;
        if !same_directory(
            &open_child(&plan.parent_dir, Path::new(&plan.name))?,
            &destination,
        )? || state.head != expected
            || !state.changes.is_empty()
        {
            return Err(OperationError::new("STALE_WRITE_PLAN"));
        }
        Ok::<_, OperationError>(state)
    })();
    match refreshed {
        Ok(state) => {
            let (commit_oid, branch_name) = match &state.head {
                HeadState::Branch { name, oid } => (Some(oid.clone()), Some(name.clone())),
                HeadState::Detached { oid } => (Some(oid.clone()), None),
                HeadState::Unborn { name } => (None, Some(name.clone())),
            };
            OperationResult::Succeeded {
                operation_id,
                kind: OperationKind::Clone,
                commit_oid,
                branch_name,
                clone_path: Some(target.to_string_lossy().into_owned()),
                refresh: WriteRefresh::Ready { state },
            }
        }
        _ => OperationResult::Unknown {
            operation_id,
            kind: OperationKind::Clone,
            error: OperationError::new("WRITE_OUTCOME_UNKNOWN"),
            clone_recovery: Some(CloneRecovery {
                path: target.to_string_lossy().into_owned(),
                stage: CloneStage::Publish,
            }),
            refresh: WriteRefresh::NotApplicable,
        },
    }
}

#[cfg(test)]
mod tests;
