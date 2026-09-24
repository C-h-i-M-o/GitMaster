//! 本地写入的只读能力检查和内容指纹。
use super::{
    process::{inspect_local_config, run_local_git},
    repository::read_repository_state_until,
    *,
};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs,
    io::Read,
    path::{Component, Path},
    time::Instant,
};

/// 完整索引条目，路径不作为模式解释。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct IndexEntry {
    pub path: String,
    pub mode: String,
    pub oid: String,
}

/// 后端保存的确认前提；不把配置原文或文件内容发送到前端。
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct WriteFingerprint {
    pub head: HeadState,
    pub index: Vec<IndexEntry>,
    pub index_bytes: Option<Vec<u8>>,
    pub conversion: super::staging::ConversionPolicy,
    config: [u8; 32],
    refs: [u8; 32],
    attributes: [u8; 32],
    executable: [u8; 32],
    files: BTreeMap<String, Option<[u8; 32]>>,
}

impl WriteFingerprint {
    /// 冲突结果可能正是 .gitattributes；发布工作文件后仅核对仍应固定的仓库输入。
    pub(crate) fn same_repository_inputs(&self, other: &Self) -> bool {
        self.head == other.head
            && self.index_bytes == other.index_bytes
            && self.config == other.config
            && self.refs == other.refs
            && self.executable == other.executable
    }
}

/// 将固定参数送入本地执行器，不透传底层输出到 UI。
pub(crate) fn local_query(
    git: &GitExecutable,
    repo: &RepositoryHandle,
    args: &[&str],
    input: &[u8],
    index: Option<&Path>,
    deadline: Instant,
) -> Result<Vec<u8>, OperationError> {
    let args: Vec<OsString> = args.iter().map(OsString::from).collect();
    let out = run_local_git(git, &repo.root, &args, input, index, deadline)?;
    if out.success {
        Ok(out.stdout)
    } else {
        Err(OperationError::new("GIT_EXECUTION_FAILED"))
    }
}

/// 整项预算也覆盖文件循环，避免大量小查询叠加越过期限。
pub(crate) fn check_time(deadline: Instant) -> Result<(), OperationError> {
    if Instant::now() >= deadline {
        Err(OperationError::new("TIMEOUT"))
    } else {
        Ok(())
    }
}

/// 用户输入必须能无损作为单个仓库相对路径使用。
pub(crate) fn validate_path(path: &str) -> Result<(), OperationError> {
    if path.is_empty()
        || path.contains('\0')
        || Path::new(path).components().any(
            |part| !matches!(part, Component::Normal(name) if !name.eq_ignore_ascii_case(".git")),
        )
    {
        return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
    }
    Ok(())
}

/// 同一快照的选择只能用于原工作树和未改变的 M1 状态。
pub(crate) fn validate_snapshot(
    git: &GitExecutable,
    repo: &RepositoryHandle,
    state: &RepositoryState,
    deadline: Instant,
) -> Result<(), OperationError> {
    if state.repository_id != repo.id {
        return Err(OperationError::new("STALE_REQUEST"));
    }
    let fresh = read_repository_state_until(git, repo, deadline)?;
    if fresh.head != state.head
        || fresh.index_identity != state.index_identity
        || fresh.changes != state.changes
        || fresh.operations != state.operations
    {
        return Err(OperationError::new("STALE_REQUEST"));
    }
    Ok(())
}

/// 查询真实状态、全部索引和配置，再捕获影响本次写入的文件内容。
pub(crate) fn fingerprint(
    git: &GitExecutable,
    repo: &RepositoryHandle,
    paths: &[String],
    own_lock: bool,
    deadline: Instant,
) -> Result<WriteFingerprint, OperationError> {
    fingerprint_with_head_policy(git, repo, paths, own_lock, false, false, deadline)
}

/// 创建分支可从 detached HEAD 开始，其余配置与内容校验保持一致。
pub(crate) fn branch_fingerprint(
    git: &GitExecutable,
    repo: &RepositoryHandle,
    deadline: Instant,
) -> Result<WriteFingerprint, OperationError> {
    fingerprint_with_head_policy(git, repo, &[], false, true, false, deadline)
}

/// 冲突专用入口允许真实 merge 和未合并 stage，原始索引仍完整参与指纹。
pub(crate) fn merge_fingerprint(
    git: &GitExecutable,
    repo: &RepositoryHandle,
    own_lock: bool,
    deadline: Instant,
) -> Result<WriteFingerprint, OperationError> {
    fingerprint_with_head_policy(git, repo, &[], own_lock, false, true, deadline)
}

/// 按操作所需 HEAD 能力捕获相同的仓库前提。
fn fingerprint_with_head_policy(
    git: &GitExecutable,
    repo: &RepositoryHandle,
    paths: &[String],
    own_lock: bool,
    allow_detached: bool,
    allow_merge: bool,
    deadline: Instant,
) -> Result<WriteFingerprint, OperationError> {
    check_time(deadline)?;
    if !own_lock
        && repo
            .git_dir
            .join("index.lock")
            .try_exists()
            .map_err(|_| OperationError::new("ACCESS_DENIED"))?
    {
        return Err(OperationError::new("INDEX_LOCKED"));
    }
    let state = read_repository_state_until(git, repo, deadline)?;
    if if allow_merge {
        state.operations != ["merge"]
    } else {
        !state.operations.is_empty()
    } {
        return Err(OperationError::new("REPOSITORY_OPERATION_ACTIVE"));
    }
    if !allow_detached && matches!(state.head, HeadState::Detached { .. }) {
        return Err(OperationError::new("DETACHED_HEAD_WRITE_BLOCKED"));
    }
    let config_bytes = inspect_local_config(git, &repo.root, deadline)?;
    let config = parse_config(&config_bytes)?;
    validate_config(repo, &config)?;
    let index = read_index_with_conflicts(git, repo, None, allow_merge, deadline)?;
    let flags = local_query(git, repo, &["ls-files", "-v", "-z"], &[], None, deadline)?;
    if flags
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
        .any(|record| !record.starts_with(b"H ") && !(allow_merge && record.starts_with(b"M ")))
    {
        return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
    }
    let all_paths: BTreeSet<String> = index
        .iter()
        .map(|entry| entry.path.clone())
        .chain(state.changes.iter().map(|change| change.path.clone()))
        .chain(paths.iter().cloned())
        .collect();
    if all_paths.len() > 10_000 {
        return Err(OperationError::new("OUTPUT_LIMIT"));
    }
    let dir = cap_std::fs::Dir::open_ambient_dir(&repo.root, cap_std::ambient_authority())
        .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
    let mut path_input = Vec::new();
    crate::diagnostics::measure("write_path_check", || {
        for path in &all_paths {
            check_time(deadline)?;
            validate_path(path)?;
            check_file_path(&dir, path)?;
            path_input.extend_from_slice(path.as_bytes());
            path_input.push(0);
        }
        Ok(())
    })?;
    let (attributes, work_attributes) =
        crate::diagnostics::measure("write_attribute_read", || {
            let mut attributes = Vec::new();
            let mut work_attributes = Vec::new();
            for cached in [false, true] {
                let mut args = vec!["check-attr", "-z"];
                if cached {
                    args.push("--cached");
                }
                args.extend([
                    "--stdin",
                    "filter",
                    "merge",
                    "working-tree-encoding",
                    "text",
                    "eol",
                    "ident",
                    "crlf",
                    "conflict-marker-size",
                ]);
                let bytes = local_query(git, repo, &args, &path_input, None, deadline)?;
                validate_attributes(&bytes)?;
                if !cached {
                    work_attributes = bytes.clone();
                }
                attributes.extend(bytes);
            }
            Ok((attributes, work_attributes))
        })?;
    let conversion = super::staging::ConversionPolicy::capture(&config, &work_attributes, paths)?;
    let mut files = BTreeMap::new();
    for path in paths {
        files.insert(path.clone(), file_digest(&dir, path, deadline)?);
    }
    let refs = local_query(
        git,
        repo,
        &[
            "for-each-ref",
            "--format=%(refname) %(objectname) %(symref)",
        ],
        &[],
        None,
        deadline,
    )?;
    let index_bytes = read_index_bytes(repo)?;
    let executable =
        fs::File::open(&git.path).map_err(|_| OperationError::new("GIT_PATH_INVALID"))?;
    Ok(WriteFingerprint {
        head: state.head,
        index,
        index_bytes,
        conversion,
        config: Sha256::digest(config_bytes).into(),
        refs: Sha256::digest(refs).into(),
        attributes: Sha256::digest(attributes).into(),
        executable: digest(executable, deadline)?,
        files,
    })
}

/// 读取机器格式索引，拒绝 gitlink、符号链接、冲突及无法表示的路径。
pub(crate) fn read_index(
    git: &GitExecutable,
    repo: &RepositoryHandle,
    index: Option<&Path>,
    deadline: Instant,
) -> Result<Vec<IndexEntry>, OperationError> {
    read_index_with_conflicts(git, repo, index, false, deadline)
}

/// merge 指纹保留未合并原始字节，只将 stage 0 转成可复用的普通索引条目。
fn read_index_with_conflicts(
    git: &GitExecutable,
    repo: &RepositoryHandle,
    index: Option<&Path>,
    allow_conflicts: bool,
    deadline: Instant,
) -> Result<Vec<IndexEntry>, OperationError> {
    let bytes = local_query(
        git,
        repo,
        &["ls-files", "--stage", "-z"],
        &[],
        index,
        deadline,
    )?;
    let mut entries = Vec::new();
    for row in bytes.split(|byte| *byte == 0).filter(|row| !row.is_empty()) {
        let tab = row
            .iter()
            .position(|byte| *byte == b'\t')
            .ok_or_else(|| OperationError::new("PARSE_FAILED"))?;
        let fields = std::str::from_utf8(&row[..tab])
            .map_err(|_| OperationError::new("PARSE_FAILED"))?
            .split(' ')
            .collect::<Vec<_>>();
        if fields.len() != 3 {
            return Err(OperationError::new("PARSE_FAILED"));
        }
        if fields[2] != "0" {
            if allow_conflicts && matches!(fields[2], "1" | "2" | "3") {
                continue;
            }
            return Err(OperationError::new("CONFLICT_PRESENT"));
        }
        if !matches!(fields[0], "100644" | "100755") {
            return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
        }
        let path = std::str::from_utf8(&row[tab + 1..])
            .map_err(|_| OperationError::new("UNSUPPORTED_PATH_ENCODING"))?
            .to_owned();
        validate_path(&path)?;
        entries.push(IndexEntry {
            path,
            mode: fields[0].to_owned(),
            oid: fields[1].to_owned(),
        });
    }
    Ok(entries)
}

/// 保留确认时的原始索引，特殊文件和超过状态预算的索引不进入写流程。
pub(crate) fn read_index_bytes(repo: &RepositoryHandle) -> Result<Option<Vec<u8>>, OperationError> {
    let dir = cap_std::fs::Dir::open_ambient_dir(&repo.git_dir, cap_std::ambient_authority())
        .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
    read_index_file(&dir, "index")
}

/// 从已打开的目录能力中有界读取索引，不跟随末级链接或等待特殊设备。
pub(crate) fn read_index_file(
    dir: &cap_std::fs::Dir,
    path: &str,
) -> Result<Option<Vec<u8>>, OperationError> {
    let mut options = cap_std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use cap_std::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    crate::git::windows_fs::nofollow(&mut options);
    let file = match dir.open_with(path, &options) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(OperationError::new("ACCESS_DENIED")),
    };
    let metadata = file
        .metadata()
        .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
    }
    #[cfg(windows)]
    if crate::git::windows_fs::is_reparse(&metadata) {
        return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
    }
    if metadata.len() > 8 * 1024 * 1024 {
        return Err(OperationError::new("OUTPUT_LIMIT"));
    }
    let mut bytes = Vec::new();
    (&file)
        .take(8 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err(OperationError::new("OUTPUT_LIMIT"));
    }
    let after = file
        .metadata()
        .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
    if metadata.len() != after.len() || metadata.modified().ok() != after.modified().ok() {
        return Err(OperationError::new("STALE_WRITE_PLAN"));
    }
    Ok(Some(bytes))
}

/// 从捕获索引与固定父树得出完整提交路径，不依赖后续实时索引。
pub(crate) fn commit_paths(
    git: &GitExecutable,
    repo: &RepositoryHandle,
    captured: &WriteFingerprint,
    deadline: Instant,
) -> Result<Vec<String>, OperationError> {
    let mut parent = BTreeMap::new();
    if let HeadState::Branch { oid, .. } = &captured.head {
        let bytes = local_query(
            git,
            repo,
            &["ls-tree", "-r", "-z", "--full-tree", oid],
            &[],
            None,
            deadline,
        )?;
        for row in bytes.split(|byte| *byte == 0).filter(|row| !row.is_empty()) {
            let text = std::str::from_utf8(row)
                .map_err(|_| OperationError::new("UNSUPPORTED_PATH_ENCODING"))?;
            let (header, path) = text
                .split_once('\t')
                .ok_or_else(|| OperationError::new("PARSE_FAILED"))?;
            let fields = header.split(' ').collect::<Vec<_>>();
            if fields.len() != 3 {
                return Err(OperationError::new("PARSE_FAILED"));
            }
            validate_path(path)?;
            parent.insert(
                path.to_owned(),
                (fields[0].to_owned(), fields[2].to_owned()),
            );
        }
    }
    let index: BTreeMap<_, _> = captured
        .index
        .iter()
        .map(|entry| (entry.path.clone(), (entry.mode.clone(), entry.oid.clone())))
        .collect();
    let paths: BTreeSet<_> = parent.keys().chain(index.keys()).cloned().collect();
    Ok(paths
        .into_iter()
        .filter(|path| parent.get(path) != index.get(path))
        .collect())
}

/// 在系统临时索引检查目标树与属性，不改变真实索引或创建对象。
pub(crate) fn checkout_tree(
    git: &GitExecutable,
    repo: &RepositoryHandle,
    oid: &str,
    deadline: Instant,
) -> Result<Vec<IndexEntry>, OperationError> {
    let temporary = tempfile::Builder::new()
        .prefix("gitmaster-checkout-")
        .tempdir()
        .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
    let index = temporary.path().join("index");
    local_query(
        git,
        repo,
        &["--no-replace-objects", "read-tree", oid],
        &[],
        Some(&index),
        deadline,
    )?;
    let entries = read_index(git, repo, Some(&index), deadline)?;
    if entries.len() > 10_000 {
        return Err(OperationError::new("OUTPUT_LIMIT"));
    }
    let dir = cap_std::fs::Dir::open_ambient_dir(&repo.root, cap_std::ambient_authority())
        .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
    let mut input = Vec::new();
    for entry in &entries {
        check_time(deadline)?;
        check_checkout_path(&dir, &entry.path)?;
        input.extend_from_slice(entry.path.as_bytes());
        input.push(0);
    }
    if !input.is_empty() {
        let attributes = local_query(
            git,
            repo,
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
            Some(&index),
            deadline,
        )?;
        validate_attributes(&attributes)?;
    }
    Ok(entries)
}

/// 检出可在普通文件与目录之间转换，但不可穿过链接、特殊文件或嵌套仓库。
fn check_checkout_path(dir: &cap_std::fs::Dir, path: &str) -> Result<(), OperationError> {
    let mut relative = std::path::PathBuf::new();
    for component in Path::new(path).components() {
        relative.push(component);
        let metadata = match dir.symlink_metadata(&relative) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(_) => return Err(OperationError::new("ACCESS_DENIED")),
        };
        if metadata.file_type().is_symlink() || (!metadata.is_dir() && !metadata.is_file()) {
            return Err(OperationError::new("CHECKOUT_UNSUPPORTED"));
        }
        if metadata.is_file() {
            return Ok(());
        }
        if dir.symlink_metadata(relative.join(".git")).is_ok() {
            return Err(OperationError::new("CHECKOUT_UNSUPPORTED"));
        }
    }
    Ok(())
}

/// 属性值只允许内建文本行为，过滤器和自定义合并驱动转为明确拒绝。
pub(crate) fn validate_attributes(bytes: &[u8]) -> Result<(), OperationError> {
    let fields = bytes.split(|byte| *byte == 0).collect::<Vec<_>>();
    if fields.last() != Some(&&b""[..]) || (fields.len() - 1) % 3 != 0 {
        return Err(OperationError::new("PARSE_FAILED"));
    }
    for fields in fields[..fields.len() - 1].chunks_exact(3) {
        if !matches!(fields[1], b"filter" | b"merge" | b"working-tree-encoding") {
            continue;
        }
        let allowed = fields[2] == b"unspecified"
            || fields[2] == b"unset"
            || (fields[1] == b"merge"
                && matches!(fields[2], b"text" | b"binary" | b"union" | b"set"));
        if !allowed {
            return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
        }
    }
    Ok(())
}

/// 解码有效配置，最后出现的值与 Git 的标量选项覆盖规则保持一致。
pub(crate) fn parse_config(bytes: &[u8]) -> Result<BTreeMap<String, String>, OperationError> {
    let mut config = BTreeMap::new();
    for entry in bytes
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
    {
        let text = std::str::from_utf8(entry)
            .map_err(|_| OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"))?;
        let (key, value) = text.split_once('\n').unwrap_or((text, ""));
        config.insert(key.to_lowercase(), value.to_owned());
    }
    Ok(config)
}

/// 拒绝会启用额外执行程序或不完整工作树语义的有效配置和 hooks。
fn validate_config(
    repo: &RepositoryHandle,
    config: &BTreeMap<String, String>,
) -> Result<(), OperationError> {
    for key in [
        "commit.gpgsign",
        "core.sparsecheckout",
        "core.sparsecheckoutcone",
        "index.sparse",
    ] {
        if config.get(key).is_some_and(|value| {
            !matches!(value.to_lowercase().as_str(), "false" | "no" | "off" | "0")
        }) {
            return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
        }
    }
    let hooks = match config.get("core.hookspath") {
        Some(path) if path.is_empty() => return Ok(()),
        Some(path) if path.starts_with('~') => {
            return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"))
        }
        Some(path) => repo.root.join(path),
        None => repo.common_dir.join("hooks"),
    };
    for hook in [
        "pre-commit",
        "prepare-commit-msg",
        "commit-msg",
        "post-commit",
        "post-index-change",
        "reference-transaction",
        "pre-merge-commit",
        "post-checkout",
        "post-merge",
        "pre-push",
    ] {
        if let Ok(metadata) = fs::metadata(hooks.join(hook)) {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if metadata.permissions().mode() & 0o111 != 0 {
                    return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
                }
            }
            #[cfg(not(unix))]
            if metadata.is_file() {
                return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
            }
        }
    }
    Ok(())
}

/// 每个路径组成部分均不得是链接，祖先目录不得是嵌套仓库。
fn check_file_path(dir: &cap_std::fs::Dir, path: &str) -> Result<(), OperationError> {
    let mut relative = std::path::PathBuf::new();
    let components = Path::new(path).components().collect::<Vec<_>>();
    for (index, component) in components.iter().enumerate() {
        relative.push(component.as_os_str());
        let metadata = match dir.symlink_metadata(&relative) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(_) => return Err(OperationError::new("ACCESS_DENIED")),
        };
        if metadata.file_type().is_symlink() {
            return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
        }
        #[cfg(windows)]
        if crate::git::windows_fs::is_reparse(&metadata) {
            return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
        }
        if index + 1 == components.len() {
            if !metadata.is_file() {
                return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
            }
        } else if !metadata.is_dir() || dir.symlink_metadata(relative.join(".git")).is_ok() {
            return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
        }
    }
    Ok(())
}

/// 在目录能力内按普通文件读取内容及执行位，缺失表示已删除。
fn file_digest(
    dir: &cap_std::fs::Dir,
    path: &str,
    deadline: Instant,
) -> Result<Option<[u8; 32]>, OperationError> {
    digest_file_copy(dir, path, None, deadline)
}

/// 在目录能力内复制已确认的内容，摘要不匹配时不得使用该副本暂存。
pub(crate) fn capture_file(
    repo: &RepositoryHandle,
    expected: &WriteFingerprint,
    path: &str,
    destination: &Path,
    deadline: Instant,
) -> Result<bool, OperationError> {
    let dir = cap_std::fs::Dir::open_ambient_dir(&repo.root, cap_std::ambient_authority())
        .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
    validate_path(path)?;
    check_file_path(&dir, path)?;
    let wanted = expected
        .files
        .get(path)
        .ok_or_else(|| OperationError::new("INVALID_INPUT"))?;
    let actual = digest_file_copy(&dir, path, Some(destination), deadline)?;
    if &actual != wanted {
        return Err(OperationError::new("STALE_WRITE_PLAN"));
    }
    Ok(actual.is_some())
}

/// 同一次流式读取同时生成摘要和可选副本，避免先校验后重新打开原文件。
fn digest_file_copy(
    dir: &cap_std::fs::Dir,
    path: &str,
    destination: Option<&Path>,
    deadline: Instant,
) -> Result<Option<[u8; 32]>, OperationError> {
    let mut options = cap_std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use cap_std::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    crate::git::windows_fs::nofollow(&mut options);
    let file = match dir.open_with(path, &options) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(OperationError::new("ACCESS_DENIED")),
    };
    let before = file
        .metadata()
        .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
    #[cfg(windows)]
    if crate::git::windows_fs::is_reparse(&before) {
        return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
    }
    if !before.is_file() {
        return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
    }
    let mut copy = destination
        .map(|path| {
            fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(path)
        })
        .transpose()
        .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
    #[cfg(unix)]
    if let Some(copy) = &copy {
        use cap_std::fs::PermissionsExt;
        use std::os::unix::fs::PermissionsExt as StdPermissionsExt;
        copy.set_permissions(fs::Permissions::from_mode(
            0o600 | (before.permissions().mode() & 0o111),
        ))
        .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
    }
    let mut hasher = Sha256::new();
    #[cfg(unix)]
    {
        use cap_std::fs::PermissionsExt;
        hasher.update((before.permissions().mode() & 0o111).to_le_bytes());
    }
    let mut reader = &file;
    let mut buffer = [0; 65536];
    loop {
        check_time(deadline)?;
        let size = reader
            .read(&mut buffer)
            .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
        if size == 0 {
            break;
        }
        hasher.update(&buffer[..size]);
        if let Some(copy) = &mut copy {
            use std::io::Write;
            copy.write_all(&buffer[..size])
                .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
        }
    }
    let after = file
        .metadata()
        .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
    if before.len() != after.len() || before.modified().ok() != after.modified().ok() {
        return Err(OperationError::new("STALE_WRITE_PLAN"));
    }
    Ok(Some(hasher.finalize().into()))
}

/// 对已有可执行文件流式计算指纹，避免路径不变而程序被替换。
fn digest(mut reader: impl Read, deadline: Instant) -> Result<[u8; 32], OperationError> {
    let mut hash = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        check_time(deadline)?;
        let size = reader
            .read(&mut buffer)
            .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
        if size == 0 {
            break;
        }
        hash.update(&buffer[..size]);
    }
    Ok(hash.finalize().into())
}

/// 身份只接受有效 Git 配置，禁止从系统用户名猜测邮件身份。
#[derive(Clone)]
pub(crate) struct CommitIdentity {
    author_name: String,
    author_email: String,
    committer_name: String,
    committer_email: String,
}

impl CommitIdentity {
    /// 作者展示与实际写入参数使用同一个捕获值。
    pub(crate) fn author_display(&self) -> String {
        format!("{} <{}>", self.author_name, self.author_email)
    }

    /// 固定角色身份与 UTF-8 编码，不让 commit-tree 再次依赖当前身份配置。
    pub(crate) fn arguments(&self) -> Vec<String> {
        [
            ("author.name", self.author_name.as_str()),
            ("author.email", self.author_email.as_str()),
            ("committer.name", self.committer_name.as_str()),
            ("committer.email", self.committer_email.as_str()),
            ("i18n.commitEncoding", "UTF-8"),
            ("commit.gpgsign", "false"),
        ]
        .into_iter()
        .flat_map(|(key, value)| ["-c".to_owned(), format!("{key}={value}")])
        .collect()
    }
}

/// 身份只接受有效 Git 配置，禁止从系统用户名猜测邮件身份。
pub(crate) fn identity(
    git: &GitExecutable,
    repo: &RepositoryHandle,
    deadline: Instant,
) -> Result<CommitIdentity, OperationError> {
    let config = parse_config(&inspect_local_config(git, &repo.root, deadline)?)?;
    for role in ["author", "committer"] {
        for field in ["name", "email"] {
            if !config
                .get(&format!("{role}.{field}"))
                .or_else(|| config.get(&format!("user.{field}")))
                .is_some_and(|value| !value.trim().is_empty())
            {
                return Err(OperationError::new("IDENTITY_REQUIRED"));
            }
        }
    }
    let (author_name, author_email) = read_identity(git, repo, "GIT_AUTHOR_IDENT", deadline)?;
    let (committer_name, committer_email) =
        read_identity(git, repo, "GIT_COMMITTER_IDENT", deadline)?;
    Ok(CommitIdentity {
        author_name,
        author_email,
        committer_name,
        committer_email,
    })
}

/// 解析 Git 已规范化的 ident，时间戳留给实际提交时生成。
fn read_identity(
    git: &GitExecutable,
    repo: &RepositoryHandle,
    role: &str,
    deadline: Instant,
) -> Result<(String, String), OperationError> {
    let bytes = local_query(git, repo, &["var", role], &[], None, deadline)
        .map_err(|_| OperationError::new("IDENTITY_REQUIRED"))?;
    let text = String::from_utf8(bytes).map_err(|_| OperationError::new("IDENTITY_REQUIRED"))?;
    let end = text
        .rfind('>')
        .ok_or_else(|| OperationError::new("IDENTITY_REQUIRED"))?;
    let start = text[..end]
        .rfind(" <")
        .ok_or_else(|| OperationError::new("IDENTITY_REQUIRED"))?;
    let name = &text[..start];
    let email = &text[start + 2..end];
    if name.is_empty()
        || email.is_empty()
        || name.contains(['\0', '\n', '\r'])
        || email.contains(['\0', '\n', '\r'])
    {
        return Err(OperationError::new("IDENTITY_REQUIRED"));
    }
    Ok((name.to_owned(), email.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::repository::{open_repository, tests::Fixture};
    use std::time::Duration;

    /// 外部恢复索引和移动 HEAD 后，预览仍只比较捕获索引与捕获父树。
    #[test]
    fn commit_paths_use_captured_index_and_parent() {
        let f = Fixture::new();
        let new_name = if cfg!(windows) {
            "new 文件"
        } else {
            "new\n文件"
        };
        f.write("same", b"base");
        f.write("deleted", b"base");
        f.write("modified", b"base");
        f.command(&["add", "."]);
        f.command(&["commit", "-m", "base"]);
        f.write("modified", b"confirmed");
        f.write(new_name, b"new");
        fs::remove_file(f.root.join("deleted")).unwrap();
        f.command(&["add", "--all"]);
        let (repo, _) = open_repository(&f.git, &f.root).unwrap();
        let deadline = Instant::now() + Duration::from_secs(20);
        let captured = fingerprint(&f.git, &repo, &[], false, deadline).unwrap();
        f.command(&["commit", "-m", "external"]);
        assert_eq!(
            commit_paths(&f.git, &repo, &captured, deadline).unwrap(),
            ["deleted", "modified", new_name]
        );
    }

    /// 索引的链接、FIFO 和超出预算的普通文件均拒绝，缺失返回空索引。
    #[cfg(unix)]
    #[test]
    fn index_input_rejects_special_files_and_limits_bytes() {
        use std::os::unix::{ffi::OsStrExt, fs::symlink};
        let f = Fixture::new();
        let (repo, _) = open_repository(&f.git, &f.root).unwrap();
        let path = repo.git_dir.join("index");
        assert_eq!(read_index_bytes(&repo).unwrap(), None);
        f.write("outside", b"secret");
        symlink(f.root.join("outside"), &path).unwrap();
        assert!(read_index_bytes(&repo).is_err());
        fs::remove_file(&path).unwrap();
        let c_path = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
        // 测试只在独有临时仓库创建 FIFO，确认读取不会阻塞等待写端。
        assert_eq!(unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) }, 0);
        assert!(read_index_bytes(&repo).is_err());
        fs::remove_file(&path).unwrap();
        let file = fs::File::create(&path).unwrap();
        file.set_len(8 * 1024 * 1024 + 1).unwrap();
        assert_eq!(read_index_bytes(&repo).unwrap_err().code, "OUTPUT_LIMIT");
        fs::write(&path, b"captured").unwrap();
        assert_eq!(read_index_bytes(&repo).unwrap(), Some(b"captured".to_vec()));
    }
}
