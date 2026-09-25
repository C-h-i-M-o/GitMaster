//! 从真实 merge 元数据、固定索引和三方 blob 恢复只读冲突会话。
use super::{
    process::read_captured_index,
    repository::{query_until, read_repository_state_until},
    write_guard as guard, *,
};
use cap_std::fs::{Dir, Metadata};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    path::Path,
    time::{Duration, Instant, UNIX_EPOCH},
};
const MAX: usize = 1024 * 1024;
const MAX_LINES: usize = 5000;
const BUDGET: Duration = Duration::from_secs(120);

/// 索引中的普通或未合并 stage；不以工作区显示状态猜测三方内容。
#[derive(Clone, Debug, PartialEq, Eq)]
struct Entry {
    mode: String,
    oid: String,
}
/// 一次合并状态和固定索引副本的内容绑定。
struct Snapshot {
    id: String,
    head: String,
    merge_heads: Vec<String>,
    entries: BTreeMap<String, BTreeMap<u8, Entry>>,
}
/// 无损读取的普通文件字节及目录项身份。
#[derive(PartialEq, Eq)]
pub(crate) struct FileBytes {
    pub(crate) bytes: Vec<u8>,
    pub(crate) identity: String,
}
/// 可编辑 UTF-8 文本与输出字节策略，BOM 不混入编辑草稿。
struct Text {
    value: String,
    bom: bool,
    ending: &'static str,
}
/// 三方和结果通过同一能力检查后才生成可编辑文档。
struct Contents {
    base: Text,
    local: Text,
    incoming: Text,
    result: Text,
    fingerprint: String,
}

/// 绑定当前真实 merge 状态的冲突会话，重启可以重新读取。
pub struct ConflictSession {
    git: GitExecutable,
    repo: RepositoryHandle,
    snapshot: Snapshot,
    state: ConflictState,
}

impl ConflictSession {
    /// 以统一预算读取实际合并状态，不依赖应用曾经发起合并。
    pub fn new(git: &GitExecutable, repo: &RepositoryHandle) -> Result<Self, OperationError> {
        Self::new_until(git, repo, Instant::now() + BUDGET)
    }

    /// 整合结果核验复用原操作的剩余期限。
    pub(crate) fn new_until(
        git: &GitExecutable,
        repo: &RepositoryHandle,
        deadline: Instant,
    ) -> Result<Self, OperationError> {
        let snapshot = snapshot(git, repo, deadline)?;
        let mut files = Vec::new();
        for (path, entries) in &snapshot.entries {
            guard::check_time(deadline)?;
            if entries.keys().all(|stage| *stage == 0) {
                continue;
            }
            let support = match contents(git, repo, &snapshot.id, path, entries, deadline) {
                Ok(_) => ConflictEditorSupport::Supported,
                Err(error) if matches!(error.code.as_str(), "TIMEOUT" | "STALE_CONFLICT") => {
                    return Err(error)
                }
                Err(error) => ConflictEditorSupport::Unsupported {
                    reason: unsupported_reason(&error).into(),
                },
            };
            let oid = |stage| entries.get(&stage).map(|entry| entry.oid.clone());
            files.push(ConflictFile {
                conflict_id: digest(&[snapshot.id.as_bytes(), path.as_bytes()]),
                path: path.clone(),
                stage_oids: ConflictStageOids {
                    base: oid(1),
                    local: oid(2),
                    incoming: oid(3),
                },
                editor_support: support,
            });
        }
        validate_snapshot(git, repo, &snapshot.id, deadline)?;
        let state = ConflictState {
            repository_id: repo.id.clone(),
            merge_session_id: snapshot.id.clone(),
            head_oid: snapshot.head.clone(),
            merge_head_oids: snapshot.merge_heads.clone(),
            files,
        };
        Ok(Self {
            git: git.clone(),
            repo: repo.clone(),
            snapshot,
            state,
        })
    }

    /// 返回按文件能力分类的真实冲突列表；干净合并仍可返回空列表。
    pub fn state(&self) -> ConflictState {
        self.state.clone()
    }

    /// 读取已签发文件 ID 的三方原文与最新草稿，读取前后核对合并代次。
    pub fn read_document(&self, conflict_id: &str) -> Result<ConflictDocument, OperationError> {
        let deadline = Instant::now() + BUDGET;
        validate_snapshot(&self.git, &self.repo, &self.snapshot.id, deadline)?;
        let file = self
            .state
            .files
            .iter()
            .find(|file| file.conflict_id == conflict_id)
            .ok_or_else(|| OperationError::new("STALE_CONFLICT"))?;
        if !matches!(file.editor_support, ConflictEditorSupport::Supported) {
            return Err(OperationError::new("UNSUPPORTED_CONFLICT"));
        }
        let entries = self
            .snapshot
            .entries
            .get(&file.path)
            .ok_or_else(|| OperationError::new("STALE_CONFLICT"))?;
        let value = contents(
            &self.git,
            &self.repo,
            &self.snapshot.id,
            &file.path,
            entries,
            deadline,
        )
        .map_err(|error| {
            if matches!(error.code.as_str(), "TIMEOUT" | "STALE_CONFLICT") {
                error
            } else {
                OperationError::new("UNSUPPORTED_CONFLICT")
            }
        })?;
        validate_snapshot(&self.git, &self.repo, &self.snapshot.id, deadline)?;
        Ok(ConflictDocument {
            merge_session_id: self.snapshot.id.clone(),
            conflict_id: conflict_id.into(),
            base: Some(value.base.value),
            local: Some(value.local.value),
            incoming: Some(value.incoming.value),
            result: value.result.value,
            encoding: if value.result.bom { "utf8-bom" } else { "utf8" }.into(),
            line_ending: value.result.ending.into(),
            fingerprint: value.fingerprint,
        })
    }
}

/// 当前 HEAD 必须具名且只在 merge 中，MERGE_HEAD 与索引均按普通文件捕获。
fn snapshot(
    git: &GitExecutable,
    repo: &RepositoryHandle,
    deadline: Instant,
) -> Result<Snapshot, OperationError> {
    guard::check_time(deadline)?;
    // 先排除 FIFO/链接索引，再让 Git 读取状态，避免把特殊文件交给子进程。
    let merge = read_regular(&repo.git_dir, "MERGE_HEAD", MAX, deadline)?;
    let index = read_regular(&repo.git_dir, "index", 8 * MAX, deadline)?;
    let state = read_repository_state_until(git, repo, deadline)?;
    if state.operations != ["merge"] {
        return Err(OperationError::new("UNSUPPORTED_CONFLICT"));
    }
    let (name, head) = match state.head {
        HeadState::Branch { name, oid } => (name, oid),
        _ => return Err(OperationError::new("UNSUPPORTED_CONFLICT")),
    };
    let text = std::str::from_utf8(&merge.bytes)
        .map_err(|_| OperationError::new("UNSUPPORTED_CONFLICT"))?;
    let merge_heads = text.lines().map(str::to_owned).collect::<Vec<_>>();
    if merge_heads.is_empty()
        || merge_heads.len() > 1000
        || merge_heads
            .iter()
            .any(|oid| !is_oid(oid) || oid.len() != head.len())
    {
        return Err(OperationError::new("UNSUPPORTED_CONFLICT"));
    }
    for oid in &merge_heads {
        query_until(
            git,
            &repo.root,
            &[
                "--no-replace-objects",
                "cat-file",
                "-e",
                &format!("{oid}^{{commit}}"),
            ],
            1024,
            Some(deadline),
        )?;
    }
    let mut temporary =
        tempfile::NamedTempFile::new().map_err(|_| OperationError::new("ACCESS_DENIED"))?;
    temporary
        .write_all(&index.bytes)
        .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
    let output = read_captured_index(git, &repo.root, temporary.path(), deadline)?;
    if !output.success {
        return Err(OperationError::new("UNSUPPORTED_CONFLICT"));
    }
    let entries = parse_index(&output.stdout)?;
    let id = digest(&[
        repo.id.as_bytes(),
        name.as_bytes(),
        head.as_bytes(),
        &merge.bytes,
        merge.identity.as_bytes(),
        &index.bytes,
    ]);
    Ok(Snapshot {
        id,
        head,
        merge_heads,
        entries,
    })
}

/// 状态消失、分支变化或索引变化均使旧冲突会话失效。
fn validate_snapshot(
    git: &GitExecutable,
    repo: &RepositoryHandle,
    expected: &str,
    deadline: Instant,
) -> Result<(), OperationError> {
    match snapshot(git, repo, deadline) {
        Ok(actual) if actual.id == expected => Ok(()),
        Err(error) if error.code == "TIMEOUT" => Err(error),
        _ => Err(OperationError::new("STALE_CONFLICT")),
    }
}

/// NUL 路径允许中文和换行，stage 重复/损坏或路径越界不进入会话。
fn parse_index(bytes: &[u8]) -> Result<BTreeMap<String, BTreeMap<u8, Entry>>, OperationError> {
    if !bytes.is_empty() && !bytes.ends_with(&[0]) {
        return Err(OperationError::new("PARSE_FAILED"));
    }
    let mut result: BTreeMap<String, BTreeMap<u8, Entry>> = BTreeMap::new();
    for row in bytes.split(|byte| *byte == 0).filter(|row| !row.is_empty()) {
        let text = std::str::from_utf8(row)
            .map_err(|_| OperationError::new("UNSUPPORTED_PATH_ENCODING"))?;
        let (header, path) = text
            .split_once('\t')
            .ok_or_else(|| OperationError::new("PARSE_FAILED"))?;
        guard::validate_path(path)?;
        let fields = header.split(' ').collect::<Vec<_>>();
        if fields.len() != 3 || !is_oid(fields[1]) {
            return Err(OperationError::new("PARSE_FAILED"));
        }
        let stage: u8 = fields[2]
            .parse()
            .map_err(|_| OperationError::new("PARSE_FAILED"))?;
        if stage > 3 {
            return Err(OperationError::new("PARSE_FAILED"));
        }
        let previous = result.entry(path.to_owned()).or_default().insert(
            stage,
            Entry {
                mode: fields[0].into(),
                oid: fields[1].into(),
            },
        );
        if previous.is_some() || result.len() > 10_000 {
            return Err(OperationError::new("OUTPUT_LIMIT"));
        }
    }
    Ok(result)
}

/// 每侧和工作文件均通过限额与文本能力检查，不把缺失或二进制转换成空字符串。
fn contents(
    git: &GitExecutable,
    repo: &RepositoryHandle,
    session: &str,
    path: &str,
    entries: &BTreeMap<u8, Entry>,
    deadline: Instant,
) -> Result<Contents, OperationError> {
    if entries.len() != 3
        || !(1..=3).all(|stage| {
            entries
                .get(&stage)
                .is_some_and(|entry| matches!(entry.mode.as_str(), "100644" | "100755"))
        })
    {
        return Err(OperationError::new("UNSUPPORTED_CONFLICT"));
    }
    let mut values = Vec::new();
    for stage in 1..=3 {
        let oid = &entries[&stage].oid;
        let size = query_until(
            git,
            &repo.root,
            &["--no-replace-objects", "cat-file", "-s", oid],
            128,
            Some(deadline),
        )?;
        let size: usize = std::str::from_utf8(&size)
            .ok()
            .and_then(|size| size.trim().parse().ok())
            .ok_or_else(|| OperationError::new("PARSE_FAILED"))?;
        if size > MAX {
            return Err(OperationError::new("OUTPUT_LIMIT"));
        }
        let bytes = query_until(
            git,
            &repo.root,
            &["--no-replace-objects", "cat-file", "blob", oid],
            MAX + 1,
            Some(deadline),
        )?;
        if bytes.len() != size {
            return Err(OperationError::new("STALE_CONFLICT"));
        }
        values.push(decode(&bytes)?);
    }
    let work = read_regular(&repo.root, path, MAX, deadline)?;
    let result = decode(&work.bytes)?;
    if values.iter().any(|text| text.bom != result.bom) {
        return Err(OperationError::new("UNSUPPORTED_CONFLICT"));
    }
    let mut values = values.into_iter();
    Ok(Contents {
        base: values.next().expect("三个 stage"),
        local: values.next().expect("三个 stage"),
        incoming: values.next().expect("三个 stage"),
        result,
        fingerprint: digest(&[
            session.as_bytes(),
            path.as_bytes(),
            &work.bytes,
            work.identity.as_bytes(),
        ]),
    })
}

/// 严格 UTF-8，保留单一 LF/CRLF 策略，拒绝 NUL、混合或旧式 CR 换行。
fn decode(bytes: &[u8]) -> Result<Text, OperationError> {
    if bytes.len() > MAX {
        return Err(OperationError::new("OUTPUT_LIMIT"));
    }
    if bytes.contains(&0) {
        return Err(OperationError::new("UNSUPPORTED_CONFLICT"));
    }
    let bom = bytes.starts_with(&[0xef, 0xbb, 0xbf]);
    let body = if bom { &bytes[3..] } else { bytes };
    let value =
        std::str::from_utf8(body).map_err(|_| OperationError::new("UNSUPPORTED_CONFLICT"))?;
    let mut lf = 0;
    let mut crlf = 0;
    for (index, byte) in body.iter().enumerate() {
        if *byte == b'\r' && body.get(index + 1) != Some(&b'\n') {
            return Err(OperationError::new("UNSUPPORTED_CONFLICT"));
        }
        if *byte == b'\n' {
            if index > 0 && body[index - 1] == b'\r' {
                crlf += 1;
            } else {
                lf += 1;
            }
        }
    }
    if lf > 0 && crlf > 0 {
        return Err(OperationError::new("UNSUPPORTED_CONFLICT"));
    }
    if lf + crlf + usize::from(!body.is_empty() && !body.ends_with(b"\n")) > MAX_LINES {
        return Err(OperationError::new("OUTPUT_LIMIT"));
    }
    Ok(Text {
        value: value.into(),
        bom,
        ending: if crlf > 0 {
            "crlf"
        } else if lf > 0 {
            "lf"
        } else {
            "none"
        },
    })
}

/// 沿用目录能力边界打开普通文件，供完整读取与流式只读索引共用。
pub(crate) fn open_regular(
    root: &Path,
    path: &str,
    deadline: Instant,
) -> Result<RegularFile, OperationError> {
    guard::validate_path(path)?;
    guard::check_time(deadline)?;
    let mut dir = Dir::open_ambient_dir(root, cap_std::ambient_authority())
        .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
    let parts = path.split('/').collect::<Vec<_>>();
    for part in &parts[..parts.len() - 1] {
        let metadata = dir
            .symlink_metadata(part)
            .map_err(|_| OperationError::new("FILE_UNAVAILABLE"))?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(OperationError::new("UNSUPPORTED_CONFLICT"));
        }
        #[cfg(windows)]
        {
            dir = crate::git::windows_fs::open_directory(&dir, Path::new(part))?;
        }
        #[cfg(not(windows))]
        {
            dir = dir
                .open_dir(part)
                .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
        }
        if dir.symlink_metadata(".git").is_ok() {
            return Err(OperationError::new("UNSUPPORTED_CONFLICT"));
        }
    }
    let name = parts
        .last()
        .ok_or_else(|| OperationError::new("FILE_UNAVAILABLE"))?;
    let mut options = cap_std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use cap_std::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    crate::git::windows_fs::nofollow(&mut options);
    let file = dir
        .open_with(name, &options)
        .map_err(|_| OperationError::new("FILE_UNAVAILABLE"))?;
    let before = file
        .metadata()
        .map_err(|_| OperationError::new("FILE_UNAVAILABLE"))?;
    if !before.is_file() || before.file_type().is_symlink() {
        return Err(OperationError::new("UNSUPPORTED_CONFLICT"));
    }
    #[cfg(windows)]
    if crate::git::windows_fs::is_reparse(&before)
        || crate::git::windows_fs::identity(&before).is_none()
    {
        return Err(OperationError::new("UNSUPPORTED_CONFLICT"));
    }
    Ok(RegularFile {
        file,
        identity: metadata_identity(&before),
        len: before.len(),
        root: root.to_owned(),
        path: path.to_owned(),
    })
}

/// 已安全打开的句柄，后续读取需核对句柄和当前路径仍对应相同身份。
pub(crate) struct RegularFile {
    pub(crate) file: cap_std::fs::File,
    pub(crate) identity: String,
    pub(crate) len: u64,
    root: std::path::PathBuf,
    path: String,
}
impl RegularFile {
    /// 每次重走原始祖先能力路径，防止旧父目录句柄掩盖路径替换。
    pub(crate) fn verify(&self, deadline: Instant) -> Result<(), OperationError> {
        guard::check_time(deadline)?;
        let after = self
            .file
            .metadata()
            .map_err(|_| OperationError::new("FILE_UNAVAILABLE"))?;
        let current = open_regular(&self.root, &self.path, deadline)
            .map_err(|_| OperationError::new("STALE_CONFLICT"))?;
        if self.identity != metadata_identity(&after) || self.identity != current.identity {
            return Err(OperationError::new("STALE_CONFLICT"));
        }
        Ok(())
    }
}

/// 普通文件完整读取保持原上限及读取前后身份核对，不改变保存合同。
pub(crate) fn read_regular(
    root: &Path,
    path: &str,
    limit: usize,
    deadline: Instant,
) -> Result<FileBytes, OperationError> {
    let mut opened = open_regular(root, path, deadline)?;
    if opened.len > limit as u64 {
        return Err(OperationError::new("OUTPUT_LIMIT"));
    }
    let mut bytes = Vec::new();
    let mut buffer = vec![0; 64 * 1024];
    loop {
        guard::check_time(deadline)?;
        let count = opened
            .file
            .read(&mut buffer)
            .map_err(|_| OperationError::new("FILE_UNAVAILABLE"))?;
        if count == 0 {
            break;
        }
        if bytes.len() + count > limit {
            return Err(OperationError::new("OUTPUT_LIMIT"));
        }
        bytes.extend_from_slice(&buffer[..count]);
    }
    opened.verify(deadline)?;
    Ok(FileBytes {
        bytes,
        identity: opened.identity,
    })
}

/// 元数据身份区分相同内容的新一轮 merge 和被替换的工作文件。
fn metadata_identity(metadata: &Metadata) -> String {
    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.into_std().duration_since(UNIX_EPOCH).ok())
        .map(|time| time.as_nanos())
        .unwrap_or(0);
    #[cfg(unix)]
    {
        use cap_std::fs::{MetadataExt, PermissionsExt};
        format!(
            "{}:{}:{}:{}:{modified}",
            metadata.dev(),
            metadata.ino(),
            metadata.len(),
            metadata.permissions().mode()
        )
    }
    #[cfg(windows)]
    {
        format!(
            "{:?}:{}:{}:{modified}",
            crate::git::windows_fs::identity(metadata),
            metadata.len(),
            metadata.permissions().readonly()
        )
    }
    #[cfg(not(any(unix, windows)))]
    {
        format!("{}:{modified}", metadata.len())
    }
}

/// SHA-1 和 SHA-256 完整 OID 均可读取，不接受简称或任意 revspec。
fn is_oid(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
/// 每段写入长度，避免不同字段拼接得到相同会话摘要。
fn digest(parts: &[&[u8]]) -> String {
    let mut hash = Sha256::new();
    for part in parts {
        hash.update((part.len() as u64).to_le_bytes());
        hash.update(part);
    }
    format!("{:x}", hash.finalize())
}
/// 文件级限制以可理解原因展示，不透传 Git 或系统错误原文。
fn unsupported_reason(error: &OperationError) -> &'static str {
    match error.code.as_str() {
        "OUTPUT_LIMIT" => "内容超过 1 MiB 或 5000 行，请在外部处理",
        "ACCESS_DENIED" | "FILE_UNAVAILABLE" => "当前工作文件不可安全读取，请在外部处理",
        _ => "仅支持完整三方的普通 UTF-8 文本冲突，请在外部处理",
    }
}

#[cfg(test)]
mod tests;

mod save;
pub use save::prepare_save;
pub(crate) use save::replace_regular;
mod finish;
pub use finish::prepare_finish;
#[cfg(test)]
mod finish_tests;
#[cfg(test)]
mod save_tests;
