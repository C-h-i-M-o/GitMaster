//! 将分支检出输入固定在私有公共目录，真实工作区由 Git 保护和发布。
use super::{
    process::{inspect_local_config, run_isolated_git, run_worktree_git, ProcessOutput},
    write_guard::{self as guard, WriteFingerprint},
    *,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs,
    io::Read,
    path::Path,
    time::Instant,
};

/// 已确认的分支检出输入，不向前端暴露路径或配置原文。
pub(crate) struct CapturedCheckout {
    name: String,
    target_oid: String,
    source: HeadState,
    tree: CapturedTree,
}

/// 分支切换和整合共同使用的固定配置与逐路径属性。
pub(crate) struct CapturedTree {
    directory: tempfile::TempDir,
    settings: Vec<(String, String)>,
    global_attributes: Option<String>,
}

impl CapturedCheckout {
    /// 捕获源与目标，准备阶段只写应用私有目录。
    pub(crate) fn capture(
        git: &GitExecutable,
        repo: &RepositoryHandle,
        expected: &WriteFingerprint,
        target: &BranchSummary,
        deadline: Instant,
    ) -> Result<Self, OperationError> {
        Ok(Self {
            name: target.name.clone(),
            target_oid: target.oid.clone(),
            source: expected.head.clone(),
            tree: CapturedTree::capture(git, repo, expected, &target.oid, deadline)?,
        })
    }

    /// 由原生 switch 执行实际工作区、索引和 HEAD 更新。
    pub(crate) fn execute_with(
        &self,
        git: &GitExecutable,
        repo: &RepositoryHandle,
        deadline: Instant,
        before_write: impl FnOnce() -> Result<(), OperationError>,
    ) -> Result<ProcessOutput, OperationError> {
        let mut references = BTreeMap::from([(
            format!("refs/heads/{}", self.name),
            Some(self.target_oid.clone()),
        )]);
        match &self.source {
            HeadState::Branch { name, oid } => {
                references.insert(format!("refs/heads/{name}"), Some(oid.clone()));
            }
            HeadState::Unborn { name } => {
                references.insert(format!("refs/heads/{name}"), None);
            }
            HeadState::Detached { .. } => (),
        }
        let locks = references
            .keys()
            .map(|reference| ReferenceLock::acquire(repo, reference))
            .collect::<Result<Vec<_>, _>>()?;
        for (reference, oid) in &references {
            let current = guard::local_query(
                git,
                repo,
                &[
                    "for-each-ref",
                    "--format=%(refname)%00%(objectname)%00%(symref)",
                    reference,
                ],
                &[],
                None,
                deadline,
            )?;
            let expected = oid
                .as_ref()
                .map(|oid| format!("{reference}\0{oid}\0\n"))
                .unwrap_or_default();
            if current != expected.as_bytes() {
                return Err(OperationError::new("STALE_WRITE_PLAN"));
            }
        }
        let current = super::repository::read_repository_state_until(git, repo, deadline)?;
        if current.head != self.source {
            return Err(OperationError::new("STALE_WRITE_PLAN"));
        }
        if !current.changes.is_empty() || !current.operations.is_empty() {
            return Err(OperationError::new("WORKTREE_DIRTY"));
        }
        let branches = super::branches::BranchSession::new_until(git, repo, deadline)?.list();
        if branches.branches.iter().any(|branch| {
            branch.kind == ReferenceKind::Local
                && branch.name == self.name
                && branch.occupied_by_other_worktree
        }) {
            return Err(OperationError::new("BRANCH_IN_USE"));
        }
        if locks.iter().any(|lock| !lock.owns_lock()) {
            return Err(OperationError::new("STALE_WRITE_PLAN"));
        }
        before_write()?;
        let args = [
            "switch",
            "--no-guess",
            "--no-recurse-submodules",
            "--no-overwrite-ignore",
            "--",
            &self.name,
        ]
        .map(OsString::from);
        self.tree.run(git, repo, &args, deadline)
    }

    /// 定向测试从最后检出入口验证引用锁、原生发布和配置隔离。
    #[cfg(test)]
    fn execute(
        &self,
        git: &GitExecutable,
        repo: &RepositoryHandle,
        deadline: Instant,
    ) -> Result<ProcessOutput, OperationError> {
        self.execute_with(git, repo, deadline, || Ok(()))
    }
}

impl CapturedTree {
    /// 解析目标树并捕获内建设置，所有中间文件位于应用私有目录。
    pub(crate) fn capture(
        git: &GitExecutable,
        repo: &RepositoryHandle,
        expected: &WriteFingerprint,
        target_oid: &str,
        deadline: Instant,
    ) -> Result<Self, OperationError> {
        let config = guard::parse_config(&inspect_local_config(git, &repo.root, deadline)?)?;
        if config
            .get("extensions.refstorage")
            .is_some_and(|value| value != "files")
        {
            return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
        }
        let object_format = config
            .get("extensions.objectformat")
            .map(String::as_str)
            .unwrap_or("sha1");
        if !matches!(object_format, "sha1" | "sha256") {
            return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
        }
        let directory = tempfile::Builder::new()
            .prefix("gitmaster-switch-")
            .tempdir()
            .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
        let settings = [
            "core.autocrlf",
            "core.eol",
            "core.safecrlf",
            "core.filemode",
            "core.bigfilethreshold",
            "core.ignorecase",
            "core.precomposeunicode",
            "core.protecthfs",
            "core.protectntfs",
            "user.name",
            "user.email",
            "merge.default",
            "merge.renames",
            "merge.renamelimit",
            "merge.renormalize",
            "merge.conflictstyle",
            "merge.directoryrenames",
        ]
        .into_iter()
        .filter_map(|key| config.get(key).map(|value| (key.to_owned(), value.clone())))
        .collect::<Vec<_>>();
        let captured = Self {
            directory,
            settings,
            global_attributes: None,
        };
        captured.private_query(
            git,
            repo,
            &[
                "init",
                "--template=",
                &format!("--object-format={object_format}"),
                "--initial-branch=gitmaster-private",
            ],
            deadline,
        )?;
        let entries = guard::checkout_tree(git, repo, target_oid, deadline)?;
        let paths = expected
            .index
            .iter()
            .chain(entries.iter())
            .map(|entry| entry.path.clone())
            .collect::<BTreeSet<_>>();
        if paths.len() > 10_000 {
            return Err(OperationError::new("OUTPUT_LIMIT"));
        }
        let index = captured.directory.path().join("capture.index");
        guard::local_query(
            git,
            repo,
            &["--no-replace-objects", "read-tree", target_oid],
            &[],
            Some(&index),
            deadline,
        )?;
        let input = paths
            .iter()
            .flat_map(|path| path.as_bytes().iter().copied().chain([0]))
            .collect::<Vec<_>>();
        let attributes = guard::local_query(
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
                "text",
                "eol",
                "ident",
                "crlf",
            ],
            &input,
            Some(&index),
            deadline,
        )?;
        guard::validate_attributes(&attributes)?;
        let rules = attribute_rules(&attributes, &paths)?;
        fs::create_dir_all(captured.directory.path().join(".git/info"))
            .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
        fs::write(
            captured.directory.path().join(".git/info/attributes"),
            rules,
        )
        .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
        Ok(captured)
    }

    /// 固定环境中执行核心生成的参数数组，不接受前端任意命令。
    pub(crate) fn run(
        &self,
        git: &GitExecutable,
        repo: &RepositoryHandle,
        args: &[OsString],
        deadline: Instant,
    ) -> Result<ProcessOutput, OperationError> {
        let mut fixed = Vec::new();
        if let Some(path) = &self.global_attributes {
            fixed.extend([
                OsString::from("-c"),
                OsString::from(format!("core.attributesFile={path}")),
            ]);
        }
        fixed.extend_from_slice(args);
        run_worktree_git(
            git,
            repo,
            &self.directory.path().join(".git"),
            &self.settings,
            &fixed,
            deadline,
        )
    }

    /// 普通 merge 保留原生工作树属性演进，只冻结外部属性源和执行配置。
    pub(crate) fn for_merge(
        mut self,
        git: &GitExecutable,
        repo: &RepositoryHandle,
        deadline: Instant,
    ) -> Result<Self, OperationError> {
        let config = guard::parse_config(&inspect_local_config(git, &repo.root, deadline)?)?;
        let global = if config.contains_key("core.attributesfile") {
            let bytes = guard::local_query(
                git,
                repo,
                &["config", "--path", "--get", "core.attributesFile"],
                &[],
                None,
                deadline,
            )?;
            let bytes = bytes.strip_suffix(b"\n").unwrap_or(&bytes);
            let path = std::str::from_utf8(bytes)
                .map_err(|_| OperationError::new("UNSUPPORTED_PATH_ENCODING"))?;
            if path.is_empty() {
                None
            } else {
                Some(repo.root.join(path))
            }
        } else {
            std::env::var_os("XDG_CONFIG_HOME")
                .filter(|value| !value.is_empty())
                .map(std::path::PathBuf::from)
                .or_else(|| {
                    std::env::var_os("HOME")
                        .map(|home| std::path::PathBuf::from(home).join(".config"))
                })
                .map(|base| base.join("git/attributes"))
        };
        let global_bytes = match global {
            Some(path) => capture_attribute_file(&path, deadline)?,
            None => Vec::new(),
        };
        let global_path = self.directory.path().join("global-attributes");
        fs::write(&global_path, global_bytes).map_err(|_| OperationError::new("ACCESS_DENIED"))?;
        self.global_attributes = Some(
            global_path
                .to_str()
                .ok_or_else(|| OperationError::new("UNSUPPORTED_PATH_ENCODING"))?
                .into(),
        );
        let mut info = capture_attribute_file(&repo.common_dir.join("info/attributes"), deadline)?;
        info.extend_from_slice(b"\n* -filter -working-tree-encoding\n");
        fs::write(self.directory.path().join(".git/info/attributes"), info)
            .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
        Ok(self)
    }

    /// 私有初始化不接触原仓库的配置、索引或 refs。
    fn private_query(
        &self,
        git: &GitExecutable,
        repo: &RepositoryHandle,
        args: &[&str],
        deadline: Instant,
    ) -> Result<(), OperationError> {
        let args = args.iter().map(OsString::from).collect::<Vec<_>>();
        let output = run_isolated_git(
            git,
            self.directory.path(),
            &args,
            &[],
            Some(&repo.common_dir.join("objects")),
            &self.settings,
            deadline,
        )?;
        if output.success {
            Ok(())
        } else {
            Err(OperationError::new("GIT_EXECUTION_FAILED"))
        }
    }
}

/// 捕获已有属性文件，缺失为空；特殊文件和超过 1 MiB 的规则不进入写环境。
fn capture_attribute_file(path: &Path, deadline: Instant) -> Result<Vec<u8>, OperationError> {
    guard::check_time(deadline)?;
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let mut file = match options.open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err(OperationError::new("ACCESS_DENIED")),
    };
    let metadata = file
        .metadata()
        .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
    if !metadata.is_file() {
        return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
    }
    if metadata.len() > 1024 * 1024 {
        return Err(OperationError::new("OUTPUT_LIMIT"));
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
    guard::check_time(deadline)?;
    let after = file
        .metadata()
        .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
    if bytes.len() > 1024 * 1024 {
        return Err(OperationError::new("OUTPUT_LIMIT"));
    }
    if metadata.len() != after.len() || metadata.modified().ok() != after.modified().ok() {
        return Err(OperationError::new("STALE_WRITE_PLAN"));
    }
    Ok(bytes)
}

/// 只锁住已确认的源/目标引用，原生 switch 不更新这两个已有分支的 OID。
struct ReferenceLock {
    directory: cap_std::fs::Dir,
    name: String,
    file: cap_std::fs::File,
}

impl ReferenceLock {
    /// 沿普通目录打开引用父目录，以 create_new 遵守 Git 已有锁协议。
    fn acquire(repo: &RepositoryHandle, reference: &str) -> Result<Self, OperationError> {
        let mut directory =
            cap_std::fs::Dir::open_ambient_dir(&repo.common_dir, cap_std::ambient_authority())
                .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
        guard::validate_path(reference)?;
        let components = reference.split('/').collect::<Vec<_>>();
        for component in &components[..components.len() - 1] {
            match directory.create_dir(component) {
                Ok(()) => (),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (),
                Err(_) => return Err(OperationError::new("ACCESS_DENIED")),
            }
            let metadata = directory
                .symlink_metadata(component)
                .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
            if !metadata.is_dir() || metadata.file_type().is_symlink() {
                return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
            }
            directory = directory
                .open_dir(component)
                .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
        }
        let name = format!(
            "{}.lock",
            components
                .last()
                .ok_or_else(|| OperationError::new("INVALID_INPUT"))?
        );
        let mut options = cap_std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        let file = directory.open_with(&name, &options).map_err(|error| {
            OperationError::new(if error.kind() == std::io::ErrorKind::AlreadyExists {
                "STALE_WRITE_PLAN"
            } else {
                "ACCESS_DENIED"
            })
        })?;
        Ok(Self {
            directory,
            name,
            file,
        })
    }

    /// 外部替换锁路径后不能把别人的文件当作自己仍持有的锁。
    fn owns_lock(&self) -> bool {
        #[cfg(unix)]
        {
            use cap_std::fs::MetadataExt;
            match (
                self.file.metadata(),
                self.directory.symlink_metadata(&self.name),
            ) {
                (Ok(opened), Ok(current)) => {
                    current.is_file()
                        && opened.dev() == current.dev()
                        && opened.ino() == current.ino()
                }
                _ => false,
            }
        }
        #[cfg(windows)]
        {
            match (
                self.file.metadata(),
                self.directory.symlink_metadata(&self.name),
            ) {
                (Ok(opened), Ok(current)) => {
                    current.is_file()
                        && !super::windows_fs::is_reparse(&current)
                        && super::windows_fs::same(&opened, &current)
                }
                _ => false,
            }
        }
        #[cfg(not(any(unix, windows)))]
        false
    }
}

impl Drop for ReferenceLock {
    /// 仅释放本次创建且身份仍相同的锁，不清理外部已有或替换后的锁。
    fn drop(&mut self) {
        if self.owns_lock() {
            let _ = self.directory.remove_file(&self.name);
        }
    }
}

/// 将已解析属性写成完整路径规则，不把文件名中的通配符作为属性表达式。
fn attribute_rules(bytes: &[u8], paths: &BTreeSet<String>) -> Result<Vec<u8>, OperationError> {
    let fields = bytes.split(|byte| *byte == 0).collect::<Vec<_>>();
    if fields.last() != Some(&&b""[..]) || (fields.len() - 1) % 3 != 0 {
        return Err(OperationError::new("PARSE_FAILED"));
    }
    let mut rules: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    for row in fields[..fields.len() - 1].chunks_exact(3) {
        let values = row
            .iter()
            .map(|field| {
                std::str::from_utf8(field).map_err(|_| OperationError::new("PARSE_FAILED"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let (path, key, value) = (values[0], values[1], values[2]);
        if !paths.contains(path)
            || !matches!(
                key,
                "filter" | "merge" | "working-tree-encoding" | "text" | "eol" | "ident" | "crlf"
            )
        {
            return Err(OperationError::new("PARSE_FAILED"));
        }
        let encoded = match value {
            "set" => key.to_owned(),
            "unset" => format!("-{key}"),
            "unspecified" => format!("!{key}"),
            "auto" if key == "text" => format!("{key}={value}"),
            "lf" | "crlf" if key == "eol" => format!("{key}={value}"),
            "input" if key == "crlf" => format!("{key}={value}"),
            "text" | "binary" | "union" if key == "merge" => format!("{key}={value}"),
            _ => return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION")),
        };
        if rules
            .entry(path.to_owned())
            .or_default()
            .insert(key.to_owned(), encoded)
            .is_some()
        {
            return Err(OperationError::new("PARSE_FAILED"));
        }
    }
    if rules.len() != paths.len() || rules.values().any(|rule| rule.len() != 7) {
        return Err(OperationError::new("PARSE_FAILED"));
    }
    let mut output = Vec::new();
    for (path, values) in rules {
        output.extend_from_slice(quoted_pattern(&path).as_bytes());
        for value in values.values() {
            output.push(b' ');
            output.extend_from_slice(value.as_bytes());
        }
        output.push(b'\n');
    }
    Ok(output)
}

/// 先转义 glob 元字符，再按 Git 的 C 风格引号表示任意 UTF-8 文件名。
fn quoted_pattern(path: &str) -> String {
    let mut pattern = String::from("/");
    for character in path.chars() {
        if matches!(character, '\\' | '*' | '?' | '[' | ']') {
            pattern.push('\\');
        }
        pattern.push(character);
    }
    let mut quoted = String::from("\"");
    for byte in pattern.bytes() {
        match byte {
            b'\\' => quoted.push_str("\\\\"),
            b'"' => quoted.push_str("\\\""),
            32..=126 => quoted.push(char::from(byte)),
            _ => quoted.push_str(&format!("\\{byte:03o}")),
        }
    }
    quoted.push('"');
    quoted
}

#[cfg(test)]
mod tests;
