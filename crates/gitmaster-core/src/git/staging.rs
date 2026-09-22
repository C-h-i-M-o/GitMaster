//! 用固定文件副本与内建转换配置生成暂存条目。
use super::{
    process::run_isolated_git,
    write_guard::{self as guard, IndexEntry, WriteFingerprint},
    *,
};
use std::{collections::BTreeMap, ffi::OsString, fs, time::Instant};

/// 只保留 Git 内建内容转换选项，不包含可执行命令、include 或路径配置。
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct ConversionPolicy {
    settings: Vec<(String, String)>,
    attributes: BTreeMap<String, Vec<(String, String)>>,
    object_format: String,
}

impl ConversionPolicy {
    /// 从同一次有效配置与属性查询结果固定选中文件的转换规则。
    pub(crate) fn capture(
        config: &BTreeMap<String, String>,
        attributes: &[u8],
        paths: &[String],
    ) -> Result<Self, OperationError> {
        let defaults = [
            ("core.autocrlf", "false"),
            ("core.eol", "native"),
            ("core.safecrlf", "false"),
            ("core.filemode", "true"),
            ("core.bigfilethreshold", "512m"),
        ];
        let settings = defaults
            .into_iter()
            .map(|(key, fallback)| {
                (
                    key.to_owned(),
                    config
                        .get(key)
                        .cloned()
                        .unwrap_or_else(|| fallback.to_owned()),
                )
            })
            .collect();
        let object_format = config
            .get("extensions.objectformat")
            .map(String::as_str)
            .unwrap_or("sha1");
        if !matches!(object_format, "sha1" | "sha256") {
            return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
        }
        let mut captured: BTreeMap<String, Vec<(String, String)>> = paths
            .iter()
            .map(|path| (path.clone(), Vec::new()))
            .collect();
        let fields = attributes.split(|byte| *byte == 0).collect::<Vec<_>>();
        if fields.last() != Some(&&b""[..]) || (fields.len() - 1) % 3 != 0 {
            return Err(OperationError::new("PARSE_FAILED"));
        }
        for fields in fields[..fields.len() - 1].chunks_exact(3) {
            let path = std::str::from_utf8(fields[0])
                .map_err(|_| OperationError::new("UNSUPPORTED_PATH_ENCODING"))?;
            let Some(values) = captured.get_mut(path) else {
                continue;
            };
            let key =
                std::str::from_utf8(fields[1]).map_err(|_| OperationError::new("PARSE_FAILED"))?;
            let value =
                std::str::from_utf8(fields[2]).map_err(|_| OperationError::new("PARSE_FAILED"))?;
            if !matches!(key, "text" | "eol" | "ident" | "crlf") {
                continue;
            }
            let allowed = matches!(value, "set" | "unset" | "unspecified")
                || (key == "text" && value == "auto")
                || (key == "eol" && matches!(value, "lf" | "crlf"))
                || (key == "crlf" && value == "input");
            if !allowed {
                return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
            }
            values.push((key.to_owned(), value.to_owned()));
        }
        if captured.values().any(|values| values.len() != 4) {
            return Err(OperationError::new("PARSE_FAILED"));
        }
        Ok(Self {
            settings,
            attributes: captured,
            object_format: object_format.into(),
        })
    }
}

/// 冻结选择文件后只读取私有副本，返回可发布到事务索引的已转换条目。
pub(crate) fn prepare_entries(
    git: &GitExecutable,
    repo: &RepositoryHandle,
    expected: &WriteFingerprint,
    paths: &[String],
    deadline: Instant,
) -> Result<Vec<IndexEntry>, OperationError> {
    CapturedInputs::new(repo, expected, paths, deadline)?.convert(git, repo, deadline)
}

/// 冲突结果没有既有 stage 0；只将确认字节送入相同的隔离内建转换器。
pub(crate) fn prepare_conflict_entry(
    git: &GitExecutable,
    repo: &RepositoryHandle,
    policy: &ConversionPolicy,
    path: &str,
    bytes: &[u8],
    permissions: cap_std::fs::Permissions,
    local_mode: &str,
    deadline: Instant,
) -> Result<IndexEntry, OperationError> {
    guard::check_time(deadline)?;
    let directory = tempfile::Builder::new()
        .prefix("gitmaster-resolution-")
        .tempdir()
        .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
    let file = directory.path().join("input-0");
    fs::write(&file, bytes).map_err(|_| OperationError::new("ACCESS_DENIED"))?;
    let handle = fs::File::open(&file).map_err(|_| OperationError::new("ACCESS_DENIED"))?;
    cap_std::fs::File::from_std(handle)
        .set_permissions(permissions)
        .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
    let captured = CapturedInputs {
        directory,
        policy: policy.clone(),
        paths: vec![CapturedPath {
            original: path.into(),
            synthetic: "input-0".into(),
            present: true,
            prior: None,
        }],
    };
    let mut entries = captured.convert(git, repo, deadline)?;
    if entries.len() != 1 {
        return Err(OperationError::new("PARSE_FAILED"));
    }
    let mut entry = entries.remove(0);
    let filemode = captured.query(
        git,
        repo,
        &["config", "--bool", "--get", "core.filemode"],
        &[],
        false,
        deadline,
    )?;
    if filemode == b"false\n" {
        if !matches!(local_mode, "100644" | "100755") {
            return Err(OperationError::new("UNSUPPORTED_CONFLICT"));
        }
        entry.mode = local_mode.into();
    }
    Ok(entry)
}

/// 私有目录中一个合成路径与真实索引项的映射。
struct CapturedPath {
    original: String,
    synthetic: String,
    present: bool,
    prior: Option<IndexEntry>,
}

/// 副本创建结束后不再持有读取原工作路径的业务入口。
struct CapturedInputs {
    directory: tempfile::TempDir,
    policy: ConversionPolicy,
    paths: Vec<CapturedPath>,
}

impl CapturedInputs {
    /// 复制时一并验证摘要与执行位；超时或内容变化使全部副本作废。
    fn new(
        repo: &RepositoryHandle,
        expected: &WriteFingerprint,
        paths: &[String],
        deadline: Instant,
    ) -> Result<Self, OperationError> {
        let directory = tempfile::Builder::new()
            .prefix("gitmaster-stage-")
            .tempdir()
            .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
        let indexed: BTreeMap<_, _> = expected
            .index
            .iter()
            .map(|entry| (entry.path.as_str(), entry))
            .collect();
        let mut captures = Vec::new();
        for (number, path) in paths.iter().enumerate() {
            guard::check_time(deadline)?;
            let synthetic = format!("input-{number}");
            let present = guard::capture_file(
                repo,
                expected,
                path,
                &directory.path().join(&synthetic),
                deadline,
            )?;
            captures.push(CapturedPath {
                original: path.clone(),
                synthetic,
                present,
                prior: indexed.get(path.as_str()).map(|entry| (*entry).clone()),
            });
        }
        Ok(Self {
            directory,
            policy: expected.conversion.clone(),
            paths: captures,
        })
    }

    /// 用官方 Git 创建隔离索引、执行内建转换并读取生成的 blob OID。
    fn convert(
        &self,
        git: &GitExecutable,
        repo: &RepositoryHandle,
        deadline: Instant,
    ) -> Result<Vec<IndexEntry>, OperationError> {
        self.query(
            git,
            repo,
            &[
                "init",
                "--quiet",
                "--template=",
                &format!("--object-format={}", self.policy.object_format),
                ".",
            ],
            &[],
            false,
            deadline,
        )?;
        fs::create_dir_all(self.directory.path().join(".git/info"))
            .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
        let mut attributes = String::new();
        let mut index_input = Vec::new();
        let mut path_input = Vec::new();
        for path in &self.paths {
            guard::check_time(deadline)?;
            let values = self
                .policy
                .attributes
                .get(&path.original)
                .ok_or_else(|| OperationError::new("INVALID_INPUT"))?;
            attributes.push_str(&path.synthetic);
            for (key, value) in values {
                attributes.push(' ');
                match value.as_str() {
                    "unspecified" => {
                        attributes.push('!');
                        attributes.push_str(key);
                    }
                    "unset" => {
                        attributes.push('-');
                        attributes.push_str(key);
                    }
                    "set" => attributes.push_str(key),
                    _ => {
                        attributes.push_str(key);
                        attributes.push('=');
                        attributes.push_str(value);
                    }
                }
            }
            attributes.push('\n');
            if let Some(prior) = &path.prior {
                index_input.extend_from_slice(
                    format!("{} {}\t{}", prior.mode, prior.oid, path.synthetic).as_bytes(),
                );
                index_input.push(0);
            }
            if path.present || path.prior.is_some() {
                path_input.extend_from_slice(path.synthetic.as_bytes());
                path_input.push(0);
            }
        }
        fs::write(
            self.directory.path().join(".git/info/attributes"),
            attributes,
        )
        .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
        if !index_input.is_empty() {
            self.query(
                git,
                repo,
                &["update-index", "-z", "--index-info"],
                &index_input,
                true,
                deadline,
            )?;
        }
        if !path_input.is_empty() {
            self.query(
                git,
                repo,
                &[
                    "--literal-pathspecs",
                    "add",
                    "--all",
                    "--pathspec-from-file=-",
                    "--pathspec-file-nul",
                ],
                &path_input,
                true,
                deadline,
            )?;
        }
        let bytes = self.query(
            git,
            repo,
            &["ls-files", "--stage", "-z"],
            &[],
            true,
            deadline,
        )?;
        let mapped: BTreeMap<_, _> = self
            .paths
            .iter()
            .map(|path| (path.synthetic.as_str(), path))
            .collect();
        let mut entries = Vec::new();
        for row in bytes.split(|byte| *byte == 0).filter(|row| !row.is_empty()) {
            let text = std::str::from_utf8(row).map_err(|_| OperationError::new("PARSE_FAILED"))?;
            let (header, synthetic) = text
                .split_once('\t')
                .ok_or_else(|| OperationError::new("PARSE_FAILED"))?;
            let fields = header.split(' ').collect::<Vec<_>>();
            if fields.len() != 3 || fields[2] != "0" || !matches!(fields[0], "100644" | "100755") {
                return Err(OperationError::new("PARSE_FAILED"));
            }
            let path = mapped
                .get(synthetic)
                .ok_or_else(|| OperationError::new("PARSE_FAILED"))?;
            if !path.present {
                return Err(OperationError::new("PARSE_FAILED"));
            }
            entries.push(IndexEntry {
                path: path.original.clone(),
                mode: fields[0].to_owned(),
                oid: fields[1].to_owned(),
            });
        }
        Ok(entries)
    }

    /// 隔离进程仅共享对象存储，配置、属性、工作文件与索引均来自私有目录。
    fn query(
        &self,
        git: &GitExecutable,
        repo: &RepositoryHandle,
        args: &[&str],
        input: &[u8],
        share_objects: bool,
        deadline: Instant,
    ) -> Result<Vec<u8>, OperationError> {
        let objects = repo.common_dir.join("objects");
        let args = args.iter().map(OsString::from).collect::<Vec<_>>();
        let out = run_isolated_git(
            git,
            self.directory.path(),
            &args,
            input,
            share_objects.then_some(objects.as_path()),
            &self.policy.settings,
            deadline,
        )?;
        if out.success {
            Ok(out.stdout)
        } else {
            Err(OperationError::new("GIT_EXECUTION_FAILED"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::{
        repository::{open_repository, query, tests::Fixture},
        write_guard,
    };
    use std::time::Duration;

    /// Git 内建 CRLF 和 ident 转换应在隔离目录执行，真实索引不参与写入。
    #[test]
    fn isolated_conversion_preserves_git_text_rules() {
        let f = Fixture::new();
        f.write(".gitattributes", b"a text eol=lf ident\n");
        f.write("a", b"line\r\n$Id: old hash $\r\n");
        let (repo, _) = open_repository(&f.git, &f.root).unwrap();
        let paths = vec!["a".to_owned()];
        let deadline = Instant::now() + Duration::from_secs(20);
        let expected = write_guard::fingerprint(&f.git, &repo, &paths, false, deadline).unwrap();
        let entries = prepare_entries(&f.git, &repo, &expected, &paths, deadline).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "a");
        assert_eq!(
            query(
                &f.git,
                &f.root,
                &["cat-file", "blob", &entries[0].oid],
                4096
            )
            .unwrap(),
            b"line\n$Id$\n"
        );
        assert!(!repo.git_dir.join("index").exists());
        assert_eq!(
            std::fs::read(f.root.join("a")).unwrap(),
            b"line\r\n$Id: old hash $\r\n"
        );
    }

    /// 复制完成后原文件、属性和过滤器配置变化，不影响已确认副本或执行外部程序。
    #[test]
    fn frozen_input_ignores_later_source_and_conversion_changes() {
        let f = Fixture::new();
        f.write("a", b"base\n");
        f.command(&["add", "a"]);
        f.command(&["config", "core.autocrlf", "true"]);
        f.write(".gitattributes", b"a ident\n");
        f.write("a", b"confirmed\r\n$Id: data $\r\n");
        let (repo, _) = open_repository(&f.git, &f.root).unwrap();
        let paths = vec!["a".to_owned()];
        let deadline = Instant::now() + Duration::from_secs(20);
        let expected = write_guard::fingerprint(&f.git, &repo, &paths, false, deadline).unwrap();
        let index = fs::read(repo.git_dir.join("index")).unwrap();
        let inputs = CapturedInputs::new(&repo, &expected, &paths, deadline).unwrap();
        f.write("a", b"changed after capture");
        f.write(".gitattributes", b"a -text filter=evil\n");
        let marker = f
            .root
            .join("executed")
            .to_string_lossy()
            .replace('\'', "'\\''");
        f.command(&["config", "filter.evil.clean", &format!("touch '{marker}'")]);
        f.command(&["config", "core.autocrlf", "false"]);
        let entries = inputs.convert(&f.git, &repo, deadline).unwrap();
        assert_eq!(
            query(
                &f.git,
                &f.root,
                &["cat-file", "blob", &entries[0].oid],
                4096
            )
            .unwrap(),
            b"confirmed\n$Id$\n"
        );
        assert!(!f.root.join("executed").exists());
        assert_eq!(fs::read(repo.git_dir.join("index")).unwrap(), index);
        assert_eq!(
            fs::read(f.root.join("a")).unwrap(),
            b"changed after capture"
        );
    }

    /// 复制开始前的同状态内容变化必须拒绝，不创建用户仓库对象或索引。
    #[test]
    fn copy_rejects_changed_content_before_creating_objects() {
        let f = Fixture::new();
        f.write("a", b"first");
        let (repo, _) = open_repository(&f.git, &f.root).unwrap();
        let paths = vec!["a".to_owned()];
        let deadline = Instant::now() + Duration::from_secs(20);
        let expected = write_guard::fingerprint(&f.git, &repo, &paths, false, deadline).unwrap();
        let before = query(&f.git, &f.root, &["count-objects", "-v"], 4096).unwrap();
        f.write("a", b"other");
        assert!(
            matches!(prepare_entries(&f.git, &repo, &expected, &paths, deadline), Err(error) if error.code == "STALE_WRITE_PLAN")
        );
        assert_eq!(
            query(&f.git, &f.root, &["count-objects", "-v"], 4096).unwrap(),
            before
        );
        assert!(!repo.git_dir.join("index").exists());
    }

    /// 既有索引的 CRLF 会影响 Git 自动转换，合成路径索引必须保留这种行为。
    #[test]
    fn existing_crlf_blob_matches_native_add() {
        let f = Fixture::new();
        f.command(&["config", "core.autocrlf", "false"]);
        f.write("a", b"base\r\n");
        f.command(&["add", "a"]);
        f.command(&["config", "core.autocrlf", "true"]);
        f.write("a", b"changed\r\n");
        let (repo, _) = open_repository(&f.git, &f.root).unwrap();
        let paths = vec!["a".to_owned()];
        let deadline = Instant::now() + Duration::from_secs(20);
        let expected = write_guard::fingerprint(&f.git, &repo, &paths, false, deadline).unwrap();
        let isolated = prepare_entries(&f.git, &repo, &expected, &paths, deadline).unwrap();
        f.command(&["add", "a"]);
        let native = write_guard::read_index(&f.git, &repo, None, deadline).unwrap();
        assert_eq!(isolated, native);
        assert_eq!(
            query(&f.git, &f.root, &["show", ":a"], 4096).unwrap(),
            b"changed\r\n"
        );
    }

    /// 大二进制经过文件副本处理，不通过受 8 MiB 限制的进程 stdin 传输。
    #[test]
    fn binary_larger_than_stdin_budget_keeps_exact_bytes() {
        let f = Fixture::new();
        let mut content = vec![0xa5; 9 * 1024 * 1024];
        content[0] = 0;
        f.write("large", &content);
        let (repo, _) = open_repository(&f.git, &f.root).unwrap();
        let paths = vec!["large".to_owned()];
        let deadline = Instant::now() + Duration::from_secs(20);
        let expected = write_guard::fingerprint(&f.git, &repo, &paths, false, deadline).unwrap();
        let entries = prepare_entries(&f.git, &repo, &expected, &paths, deadline).unwrap();
        let native = query(
            &f.git,
            &f.root,
            &["hash-object", "--no-filters", "--", "large"],
            4096,
        )
        .unwrap();
        assert_eq!(format!("{}\n", entries[0].oid).as_bytes(), native);
        assert_eq!(
            query(&f.git, &f.root, &["cat-file", "-s", &entries[0].oid], 4096).unwrap(),
            b"9437184\n"
        );
    }

    /// filemode=false 保留已有可执行位；新文件的可执行位按 Git 配置处理。
    #[cfg(unix)]
    #[test]
    fn filemode_policy_matches_native_add_for_old_and_new_files() {
        use std::os::unix::fs::PermissionsExt;
        let f = Fixture::new();
        f.write("old", b"old");
        fs::set_permissions(f.root.join("old"), fs::Permissions::from_mode(0o755)).unwrap();
        f.command(&["add", "old"]);
        f.command(&["config", "core.filemode", "false"]);
        f.write("old", b"changed");
        fs::set_permissions(f.root.join("old"), fs::Permissions::from_mode(0o644)).unwrap();
        f.write("new", b"new");
        fs::set_permissions(f.root.join("new"), fs::Permissions::from_mode(0o755)).unwrap();
        let (repo, _) = open_repository(&f.git, &f.root).unwrap();
        let paths = vec!["new".to_owned(), "old".to_owned()];
        let deadline = Instant::now() + Duration::from_secs(20);
        let expected = write_guard::fingerprint(&f.git, &repo, &paths, false, deadline).unwrap();
        let isolated = prepare_entries(&f.git, &repo, &expected, &paths, deadline).unwrap();
        f.command(&["add", "new", "old"]);
        assert_eq!(
            isolated,
            write_guard::read_index(&f.git, &repo, None, deadline).unwrap()
        );
    }
}
