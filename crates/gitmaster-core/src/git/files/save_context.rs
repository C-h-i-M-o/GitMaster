use super::super::{OperationError, RepositoryHandle};
use cap_std::fs::Dir;
use sha2::{Digest, Sha256};
use std::{path::Path, time::Instant};

/// 保存上下文的纯文件指纹，绑定仓库身份、HEAD/ref 和工作区操作标记。
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct SaveContext(pub(crate) String);

/// 有界读取保存所需的 Git 元数据，不调用 Git 子进程。
pub(crate) fn capture(
    repo: &RepositoryHandle,
    deadline: Instant,
) -> Result<SaveContext, OperationError> {
    let started = Instant::now();
    if Instant::now() >= deadline {
        return Err(OperationError::new("TIMEOUT"));
    }
    // 一次捕获内复用目录能力；下一次捕获仍从当前路径重新核对实体身份。
    let root = open_directory(&repo.root)?;
    let git_dir = open_directory(&repo.git_dir)?;
    let common_directory = if repo.common_dir == repo.git_dir {
        None
    } else {
        Some(open_directory(&repo.common_dir)?)
    };
    let common_dir = common_directory.as_ref().unwrap_or(&git_dir);
    let mut h = Sha256::new();
    add(&mut h, identity_from_directory(&root)?.as_bytes());
    let git_identity = identity_from_directory(&git_dir)?;
    add(&mut h, git_identity.as_bytes());
    let common_identity = if repo.common_dir == repo.git_dir {
        git_identity
    } else {
        identity_from_directory(common_dir)?
    };
    add(&mut h, common_identity.as_bytes());
    crate::diagnostics::log_elapsed("file_context_directories", started.elapsed());
    let started = Instant::now();
    let head = read_optional(&repo.git_dir, &git_dir, "HEAD", 4096, deadline)?
        .ok_or_else(|| OperationError::new("STALE_REQUEST"))?;
    add(&mut h, &head);
    crate::diagnostics::log_elapsed("file_context_head", started.elapsed());
    let markers_started = Instant::now();
    for relative in [
        "MERGE_HEAD",
        "CHERRY_PICK_HEAD",
        "REVERT_HEAD",
        "BISECT_HEAD",
    ] {
        add(
            &mut h,
            read_optional(&repo.git_dir, &git_dir, relative, 65536, deadline)?
                .as_deref()
                .unwrap_or(b""),
        );
    }
    let marker_reads = markers_started.elapsed();
    let started = Instant::now();
    if let Some(reference) = std::str::from_utf8(&head)
        .ok()
        .and_then(|h| h.trim().strip_prefix("ref: "))
    {
        if !reference.starts_with("refs/")
            || reference
                .split('/')
                .any(|p| p.is_empty() || p == "." || p == "..")
            || reference.contains(['\\', '\0', '\r', '\n', ':'])
        {
            return Err(OperationError::new("STALE_REQUEST"));
        }
        if let Some(value) = read_optional(&repo.common_dir, common_dir, reference, 4096, deadline)?
        {
            add(&mut h, &value);
        } else if let Some(packed) = read_optional(
            &repo.common_dir,
            common_dir,
            "packed-refs",
            4 * 1024 * 1024,
            deadline,
        )? {
            let packed =
                std::str::from_utf8(&packed).map_err(|_| OperationError::new("STALE_REQUEST"))?;
            let oid = packed
                .lines()
                .find_map(|line| {
                    line.split_once(' ')
                        .filter(|(_, name)| *name == reference)
                        .map(|(oid, _)| oid)
                })
                .unwrap_or("");
            add(&mut h, oid.as_bytes());
        } else {
            add(&mut h, b"");
        }
    }
    crate::diagnostics::log_elapsed("file_context_reference", started.elapsed());
    let started = Instant::now();
    for relative in ["rebase-merge", "rebase-apply"] {
        let path = repo.git_dir.join(relative);
        match std::fs::symlink_metadata(&path) {
            Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => {
                add(&mut h, directory_identity(&path)?.as_bytes())
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => add(&mut h, b""),
            _ => return Err(OperationError::new("STALE_REQUEST")),
        }
    }
    // 外部 Git 正在改变索引或 HEAD 时不抢写；应用内另有共享写屏障。
    for name in ["index.lock", "HEAD.lock"] {
        match std::fs::symlink_metadata(repo.git_dir.join(name)) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            _ => return Err(OperationError::new("OPERATION_IN_PROGRESS")),
        }
    }
    crate::diagnostics::log_elapsed("file_context_markers", marker_reads + started.elapsed());
    Ok(SaveContext(format!("{:x}", h.finalize())))
}

/// 使用长度分隔，避免不同元数据组合产生相同拼接。
fn add(hash: &mut Sha256, bytes: &[u8]) {
    hash.update((bytes.len() as u64).to_le_bytes());
    hash.update(bytes);
}

/// 复用拒绝符号链接、校验文件身份的有界读取，分配前检查文件大小。
fn read_optional(
    root: &Path,
    directory: &Dir,
    name: &str,
    limit: usize,
    deadline: Instant,
) -> Result<Option<Vec<u8>>, OperationError> {
    match std::fs::symlink_metadata(root.join(name)) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(OperationError::new("STALE_REQUEST")),
        Ok(_) => {
            #[cfg(windows)]
            {
                read_metadata_windows(directory, name, limit, deadline).map(Some)
            }
            #[cfg(not(windows))]
            {
                let _ = directory;
                super::super::conflicts::read_regular(root, name, limit, deadline)
                    .map(|f| Some(f.bytes))
            }
        }
    }
}

/// 固定全部祖先句柄后核对最终目录项，避免每次读取 Git 元数据重复打开祖先链。
#[cfg(windows)]
fn read_metadata_windows(
    root: &Dir,
    name: &str,
    limit: usize,
    deadline: Instant,
) -> Result<Vec<u8>, OperationError> {
    use super::super::windows_fs;
    use std::io::Read;
    let started = Instant::now();
    if Instant::now() >= deadline {
        return Err(OperationError::new("TIMEOUT"));
    }
    let mut directories = Vec::new();
    let parts: Vec<_> = name.split('/').collect();
    // 输入来自固定元数据名或已验证的 refs 路径，仍拒绝空组件和父路径。
    if parts
        .iter()
        .any(|part| part.is_empty() || *part == "." || *part == ".." || part.contains(['\\', ':']))
    {
        return Err(OperationError::new("STALE_REQUEST"));
    }
    for part in &parts[..parts.len() - 1] {
        let parent = directories.last().unwrap_or(root);
        directories.push(windows_fs::open_directory(parent, Path::new(part))?);
    }
    let dir = directories.last().unwrap_or(root);
    let name = parts
        .last()
        .ok_or_else(|| OperationError::new("STALE_REQUEST"))?;
    let mut options = cap_std::fs::OpenOptions::new();
    options.read(true);
    windows_fs::nofollow(&mut options);
    let mut file = dir
        .open_with(name, &options)
        .map_err(|_| OperationError::new("STALE_REQUEST"))?;
    crate::diagnostics::log_elapsed("file_metadata_open", started.elapsed());
    let started = Instant::now();
    let before = file
        .metadata()
        .map_err(|_| OperationError::new("STALE_REQUEST"))?;
    if !before.is_file()
        || windows_fs::is_reparse(&before)
        || windows_fs::identity(&before).is_none()
    {
        return Err(OperationError::new("STALE_REQUEST"));
    }
    if before.len() > limit as u64 {
        return Err(OperationError::new("OUTPUT_LIMIT"));
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| OperationError::new("STALE_REQUEST"))?;
    let after = file
        .metadata()
        .map_err(|_| OperationError::new("STALE_REQUEST"))?;
    crate::diagnostics::log_elapsed("file_metadata_read", started.elapsed());
    let started = Instant::now();
    let current = dir
        .symlink_metadata(name)
        .map_err(|_| OperationError::new("STALE_REQUEST"))?;
    if bytes.len() > limit {
        return Err(OperationError::new("OUTPUT_LIMIT"));
    }
    if !windows_fs::same(&before, &after)
        || !windows_fs::same(&before, &current)
        || windows_fs::is_reparse(&current)
        || before.len() != after.len()
        || before.len() != current.len()
        || before.modified().ok() != after.modified().ok()
        || before.modified().ok() != current.modified().ok()
    {
        return Err(OperationError::new("STALE_REQUEST"));
    }
    if Instant::now() >= deadline {
        return Err(OperationError::new("TIMEOUT"));
    }
    crate::diagnostics::log_elapsed("file_metadata_verify", started.elapsed());
    Ok(bytes)
}

/// 打开已验证仓库路径的目录能力，交给当前捕获作用域管理生命周期。
fn open_directory(path: &Path) -> Result<Dir, OperationError> {
    Dir::open_ambient_dir(path, cap_std::ambient_authority())
        .map_err(|_| OperationError::new("STALE_REQUEST"))
}

/// 操作标记目录每次重新打开，不缓存其存在性或实体身份。
fn directory_identity(path: &Path) -> Result<String, OperationError> {
    identity_from_directory(&open_directory(path)?)
}

/// 使用句柄中的目录实体身份，不能用路径或目录修改时间替代。
fn identity_from_directory(dir: &Dir) -> Result<String, OperationError> {
    let metadata = dir
        .dir_metadata()
        .map_err(|_| OperationError::new("STALE_REQUEST"))?;
    #[cfg(windows)]
    {
        let id = super::super::windows_fs::identity(&metadata)
            .ok_or_else(|| OperationError::new("STALE_REQUEST"))?;
        Ok(format!("{}:{}", id.0, id.1))
    }
    #[cfg(unix)]
    {
        use cap_std::fs::MetadataExt;
        Ok(format!("{}:{}", metadata.dev(), metadata.ino()))
    }
    #[cfg(not(any(unix, windows)))]
    {
        Err(OperationError::new("FILE_EDIT_UNSUPPORTED"))
    }
}

/// 元数据快速通道仍限制读取字节、父路径和超时，不接受任意路径。
#[cfg(all(test, windows))]
#[test]
fn metadata_reader_preserves_bounds_and_components() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(directory.path().join("refs/heads")).unwrap();
    std::fs::write(directory.path().join("refs/heads/main"), b"12345678\n").unwrap();
    let deadline = Instant::now() + std::time::Duration::from_secs(10);
    let root = open_directory(directory.path()).unwrap();
    assert_eq!(
        read_metadata_windows(&root, "refs/heads/main", 64, deadline).unwrap(),
        b"12345678\n"
    );
    assert_eq!(
        read_metadata_windows(&root, "refs/heads/main", 2, deadline)
            .unwrap_err()
            .code,
        "OUTPUT_LIMIT"
    );
    assert_eq!(
        read_metadata_windows(&root, "refs/../HEAD", 64, deadline)
            .unwrap_err()
            .code,
        "STALE_REQUEST"
    );
    assert_eq!(
        read_metadata_windows(&root, "refs/heads/main", 64, Instant::now())
            .unwrap_err()
            .code,
        "TIMEOUT"
    );
}

/// 无 Git 夹具验证上下文每次读取真实元数据，同时记录文件系统步骤的诊断样本。
#[cfg(test)]
#[test]
fn capture_observes_metadata_changes_and_locks() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("project");
    let git_dir = root.join(".git");
    std::fs::create_dir_all(git_dir.join("refs/heads")).unwrap();
    std::fs::write(git_dir.join("HEAD"), b"ref: refs/heads/main\n").unwrap();
    std::fs::write(git_dir.join("refs/heads/main"), b"before\n").unwrap();
    std::fs::write(root.join("text"), vec![b'x'; 4096]).unwrap();
    let repo = RepositoryHandle {
        id: "probe".into(),
        root,
        git_dir: git_dir.clone(),
        common_dir: git_dir.clone(),
    };
    let deadline = Instant::now() + std::time::Duration::from_secs(60);
    let baseline = capture(&repo, deadline).unwrap();
    let mut samples = Vec::new();
    for _ in 0..20 {
        let started = Instant::now();
        assert!(capture(&repo, deadline).unwrap() == baseline);
        let metadata_ms = started.elapsed().as_secs_f64() * 1000.0;
        let started = Instant::now();
        super::super::conflicts::read_regular(&repo.root, "text", 8192, deadline).unwrap();
        samples.push((metadata_ms, started.elapsed().as_secs_f64() * 1000.0));
    }
    println!("上下文与工作文件读取毫秒样本: {samples:?}");
    std::fs::write(git_dir.join("refs/heads/main"), b"after\n").unwrap();
    assert!(capture(&repo, deadline).unwrap() != baseline);
    std::fs::write(git_dir.join("refs/heads/main"), b"before\n").unwrap();
    std::fs::write(git_dir.join("MERGE_HEAD"), b"merge\n").unwrap();
    assert!(capture(&repo, deadline).unwrap() != baseline);
    std::fs::remove_file(git_dir.join("MERGE_HEAD")).unwrap();
    assert!(capture(&repo, deadline).unwrap() == baseline);
    std::fs::write(git_dir.join("index.lock"), b"").unwrap();
    assert_eq!(
        capture(&repo, deadline).err().unwrap().code,
        "OPERATION_IN_PROGRESS"
    );
    std::fs::remove_file(git_dir.join("index.lock")).unwrap();
    assert_eq!(
        capture(&repo, Instant::now()).err().unwrap().code,
        "TIMEOUT"
    );
    // 分离的 common 目录读取共享引用，HEAD 与工作区锁仍属于本工作树目录。
    let common = directory.path().join("common");
    std::fs::create_dir_all(common.join("refs/heads")).unwrap();
    std::fs::write(common.join("refs/heads/main"), b"shared-before\n").unwrap();
    let worktree = RepositoryHandle {
        common_dir: common.clone(),
        ..repo.clone()
    };
    let shared = capture(&worktree, deadline).unwrap();
    std::fs::write(common.join("refs/heads/main"), b"shared-after\n").unwrap();
    assert!(capture(&worktree, deadline).unwrap() != shared);
    // 捕获结束释放句柄；同路径的新目录即使元数据正文相同也不能复用旧身份。
    std::fs::rename(&git_dir, repo.root.join("old-git")).unwrap();
    std::fs::create_dir_all(git_dir.join("refs/heads")).unwrap();
    std::fs::write(git_dir.join("HEAD"), b"ref: refs/heads/main\n").unwrap();
    std::fs::write(git_dir.join("refs/heads/main"), b"before\n").unwrap();
    assert!(capture(&repo, deadline).unwrap() != baseline);
}

/// 手动交错测量文件系统原语，仅用于定位成本，不以微基准代替原生性能验收。
#[cfg(all(test, windows))]
#[test]
#[ignore = "手动文件系统成本诊断"]
fn measure_capture_primitives() {
    use std::time::Duration;
    let target = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
    let temporary = tempfile::Builder::new()
        .prefix("p19-primitives-")
        .tempdir_in(target)
        .unwrap();
    let root = temporary.path();
    std::fs::create_dir_all(root.join(".git/refs/heads")).unwrap();
    std::fs::write(root.join(".git/HEAD"), b"ref: refs/heads/main\n").unwrap();
    std::fs::write(root.join(".git/refs/heads/main"), b"1234567890\n").unwrap();
    std::fs::write(root.join("text"), vec![b'x'; 4096]).unwrap();
    let git_path = root.join(".git");
    let git = open_directory(&git_path).unwrap();
    let mut options = cap_std::fs::OpenOptions::new();
    options.read(true);
    super::super::windows_fs::nofollow(&mut options);
    let file = git.open_with("HEAD", &options).unwrap();
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut samples = std::collections::BTreeMap::<&str, Vec<f64>>::new();
    for _ in 0..20 {
        for name in [
            "missing_stat",
            "directory_identity",
            "head_read",
            "ref_read",
            "cap_open",
            "handle_metadata",
            "entry_metadata",
            "plain_read",
            "regular_read",
        ] {
            let started = Instant::now();
            match name {
                "missing_stat" => {
                    assert_eq!(
                        std::fs::symlink_metadata(git_path.join("MERGE_HEAD"))
                            .unwrap_err()
                            .kind(),
                        std::io::ErrorKind::NotFound
                    );
                }
                "directory_identity" => {
                    directory_identity(&git_path).unwrap();
                }
                "head_read" => {
                    read_optional(&git_path, &git, "HEAD", 4096, deadline).unwrap();
                }
                "ref_read" => {
                    read_optional(&git_path, &git, "refs/heads/main", 4096, deadline).unwrap();
                }
                "cap_open" => {
                    git.open_with("HEAD", &options).unwrap();
                }
                "handle_metadata" => {
                    file.metadata().unwrap();
                }
                "entry_metadata" => {
                    git.symlink_metadata("HEAD").unwrap();
                }
                "plain_read" => {
                    std::fs::read(git_path.join("HEAD")).unwrap();
                }
                "regular_read" => {
                    super::super::conflicts::read_regular(root, "text", 8192, deadline).unwrap();
                }
                _ => unreachable!(),
            }
            samples
                .entry(name)
                .or_default()
                .push(started.elapsed().as_secs_f64() * 1000.0);
        }
    }
    for (name, mut values) in samples {
        values.sort_by(f64::total_cmp);
        println!(
            "{name}: 最小={:.3} 中位={:.3} P95={:.3} 最大={:.3}",
            values[0],
            (values[9] + values[10]) / 2.0,
            values[18],
            values[19]
        );
    }
}
