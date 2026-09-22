//! 固定提交对象的文件列表与差异，不访问工作文件内容。
use super::*;
use crate::git::{
    diff::text_diff, process::run_git, repository::command_error, CommitFile, CommitFileList,
    FileDiff,
};
use std::ffi::OsString;

/// 最近一次签发的文件选择，生命周期受图快照约束。
pub(super) struct FileSelection {
    oid: String,
    parent: Option<String>,
    entries: Vec<FileEntry>,
}

/// 映射只含 Git 返回的字面路径与模式，不接受前端路径。
struct FileEntry {
    file: CommitFile,
    unsupported: Option<&'static str>,
}

impl HistorySession {
    /// 列出已加载提交相对指定直接父或第一父的文件变化。
    pub fn commit_files(
        &mut self,
        oid: &str,
        parent: Option<&str>,
    ) -> Result<CommitFileList, OperationError> {
        self.file_selection = None;
        let detail = self.commit_detail(oid)?;
        let parent = match parent {
            Some(value) if detail.parent_oids.iter().any(|oid| oid == value) => {
                Some(value.to_owned())
            }
            Some(_) => return Err(OperationError::new("INVALID_INPUT")),
            None => detail.parent_oids.first().cloned(),
        };
        let args = diff_args(oid, parent.as_deref(), "--raw", &[]);
        let raw = run_git(&self.git, &self.repository.root, &args, OUTPUT_LIMIT, false)?;
        if !raw.success {
            return Err(command_error(&raw.stderr));
        }
        let mut entries = parse_raw(&raw.stdout)?;
        let args = diff_args(oid, parent.as_deref(), "--numstat", &[]);
        let stats = run_git(&self.git, &self.repository.root, &args, OUTPUT_LIMIT, false)?;
        if !stats.success {
            return Err(command_error(&stats.stderr));
        }
        apply_stats(&mut entries, &stats.stdout)?;
        let list_id = next_id();
        for (index, entry) in entries.iter_mut().enumerate() {
            entry.file.file_id = format!("{}:{list_id}:{index}", self.snapshot_id);
        }
        let result = CommitFileList {
            repository_id: self.repository.id.clone(),
            graph_snapshot_id: self.snapshot_id.clone(),
            oid: oid.to_owned(),
            parent_oid: parent.clone(),
            files: entries.iter().map(|entry| entry.file.clone()).collect(),
        };
        self.file_selection = Some(FileSelection {
            oid: oid.to_owned(),
            parent,
            entries,
        });
        Ok(result)
    }

    /// 仅通过本次列表签发的文件 ID 读取差异。
    pub fn commit_file_diff(&self, file_id: &str) -> Result<FileDiff, OperationError> {
        let selection = self
            .file_selection
            .as_ref()
            .ok_or_else(|| OperationError::new("FILE_UNAVAILABLE"))?;
        let entry = selection
            .entries
            .iter()
            .find(|entry| entry.file.file_id == file_id)
            .ok_or_else(|| OperationError::new("FILE_UNAVAILABLE"))?;
        if let Some(reason) = entry.unsupported {
            return Ok(FileDiff::Unsupported {
                reason: reason.into(),
            });
        }
        if entry.file.additions.is_none() {
            return Ok(FileDiff::Binary);
        }
        let mut paths = vec![entry.file.path.as_str()];
        if let Some(path) = &entry.file.original_path {
            paths.push(path);
        }
        let args = diff_args(
            &selection.oid,
            selection.parent.as_deref(),
            "--patch",
            &paths,
        );
        let out = run_git(&self.git, &self.repository.root, &args, 1024 * 1024, true)?;
        if !out.success && !out.truncated {
            return Err(command_error(&out.stderr));
        }
        text_diff(out.stdout, out.truncated)
    }
}

/// 固定 OID 比较，根提交由 --root 表达；NUL 与字面路径消除路径解释歧义。
fn diff_args(oid: &str, parent: Option<&str>, format: &str, paths: &[&str]) -> Vec<OsString> {
    let mut args: Vec<OsString> = [
        "--literal-pathspecs",
        "diff-tree",
        "--root",
        "--no-commit-id",
        "-r",
        "-z",
        "-M",
        "--no-ext-diff",
        "--no-textconv",
        "--no-color",
        "--src-prefix=a/",
        "--dst-prefix=b/",
        format,
    ]
    .iter()
    .map(OsString::from)
    .collect();
    if format == "--patch" {
        args.push("--unified=3".into());
    }
    if let Some(parent) = parent {
        args.push(parent.into());
    }
    args.push(oid.into());
    args.push("--".into());
    args.extend(paths.iter().map(OsString::from));
    args
}

/// raw 的每个元数据头后跟一个路径，重命名和复制另带目标路径。
fn parse_raw(bytes: &[u8]) -> Result<Vec<FileEntry>, OperationError> {
    let mut fields = bytes.split(|byte| *byte == 0).peekable();
    let mut entries = Vec::new();
    while let Some(header) = fields.next() {
        if header.is_empty() && fields.peek().is_none() {
            break;
        }
        let header = text(header)?;
        let parts = header.split(' ').collect::<Vec<_>>();
        if parts.len() != 5 || !parts[0].starts_with(':') {
            return Err(OperationError::new("PARSE_FAILED"));
        }
        let status = parts[4]
            .chars()
            .next()
            .ok_or_else(|| OperationError::new("PARSE_FAILED"))?;
        if !matches!(status, 'A' | 'D' | 'M' | 'T' | 'R' | 'C') {
            return Err(OperationError::new("PARSE_FAILED"));
        }
        let first = next_path(&mut fields)?;
        let (path, original_path) = if matches!(status, 'R' | 'C') {
            (next_path(&mut fields)?, Some(first))
        } else {
            (first, None)
        };
        let modes = [&parts[0][1..], parts[1]];
        let unsupported = if modes.contains(&"160000") {
            Some("submodule")
        } else if modes.contains(&"120000") {
            Some("symlink")
        } else {
            None
        };
        entries.push(FileEntry {
            file: CommitFile {
                file_id: String::new(),
                path,
                original_path,
                status: status.to_string(),
                additions: None,
                deletions: None,
            },
            unsupported,
        });
        if entries.len() > 10_000 {
            return Err(OperationError::new("OUTPUT_LIMIT"));
        }
    }
    Ok(entries)
}

/// 不按空白拆分路径，保留制表符、换行和中文，拒绝无损编码失败。
fn next_path<'a>(fields: &mut impl Iterator<Item = &'a [u8]>) -> Result<String, OperationError> {
    let bytes = fields
        .next()
        .ok_or_else(|| OperationError::new("PARSE_FAILED"))?;
    if bytes.is_empty() {
        return Err(OperationError::new("PARSE_FAILED"));
    }
    std::str::from_utf8(bytes)
        .map(str::to_owned)
        .map_err(|_| OperationError::new("UNSUPPORTED_PATH_ENCODING"))
}

/// 对齐相同冻结提交的 NUL numstat；二进制以两个空统计表示。
fn apply_stats(entries: &mut [FileEntry], bytes: &[u8]) -> Result<(), OperationError> {
    let mut fields = bytes.split(|byte| *byte == 0).peekable();
    let mut found = HashMap::new();
    while let Some(record) = fields.next() {
        if record.is_empty() && fields.peek().is_none() {
            break;
        }
        let columns = record.splitn(3, |byte| *byte == b'\t').collect::<Vec<_>>();
        if columns.len() != 3 {
            return Err(OperationError::new("PARSE_FAILED"));
        }
        let (path, original) = if columns[2].is_empty() {
            let original = next_path(&mut fields)?;
            (next_path(&mut fields)?, Some(original))
        } else {
            (next_path(&mut std::iter::once(columns[2]))?, None)
        };
        let stats = if columns[0] == b"-" && columns[1] == b"-" {
            (None, None)
        } else {
            (
                Some(
                    text(columns[0])?
                        .parse::<u64>()
                        .map_err(|_| OperationError::new("PARSE_FAILED"))?,
                ),
                Some(
                    text(columns[1])?
                        .parse::<u64>()
                        .map_err(|_| OperationError::new("PARSE_FAILED"))?,
                ),
            )
        };
        if found.insert((path, original), stats).is_some() {
            return Err(OperationError::new("PARSE_FAILED"));
        }
    }
    for entry in entries {
        let (additions, deletions) = found
            .remove(&(entry.file.path.clone(), entry.file.original_path.clone()))
            .ok_or_else(|| OperationError::new("PARSE_FAILED"))?;
        entry.file.additions = additions;
        entry.file.deletions = deletions;
    }
    if !found.is_empty() {
        return Err(OperationError::new("PARSE_FAILED"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::repository::{open_repository, tests::Fixture};
    use std::fs;

    /// 根提交包含真实统计，恶意样式文件名按字面读取，查询保留仓库字节。
    #[test]
    fn root_files_are_literal_readonly_and_ids_expire() {
        let f = Fixture::new();
        let name = if cfg!(windows) {
            "-[target].txt"
        } else {
            ":(glob)*\n中文.txt"
        };
        f.write(name, b"selected\n");
        f.write("decoy", b"other\n");
        f.write("binary", b"\0binary");
        f.command(&["add", "."]);
        f.command(&["commit", "-m", "root"]);
        let (repo, _) = open_repository(&f.git, &f.root).unwrap();
        let tracked = [
            repo.git_dir.join("index"),
            repo.git_dir.join("HEAD"),
            repo.git_dir.join("refs/heads/main"),
            repo.git_dir.join("config"),
            f.root.join(name),
        ];
        let before: Vec<_> = tracked.iter().map(|path| fs::read(path).unwrap()).collect();
        let mut session = HistorySession::new(f.git.clone(), repo).unwrap();
        let page = session.read_page(None).unwrap();
        let oid = &page.commits[0].oid;
        let list = session.commit_files(oid, None).unwrap();
        assert_eq!(list.parent_oid, None);
        assert_eq!(list.files.len(), 3);
        let selected = list.files.iter().find(|file| file.path == name).unwrap();
        assert_eq!((selected.additions, selected.deletions), (Some(1), Some(0)));
        assert_eq!(selected.status, "A");
        let FileDiff::Text { content, truncated } =
            session.commit_file_diff(&selected.file_id).unwrap()
        else {
            panic!("应返回文本差异")
        };
        assert!(!truncated);
        assert!(content.contains("+selected"));
        assert!(!content.contains("+other"));
        let binary = list
            .files
            .iter()
            .find(|file| file.path == "binary")
            .unwrap();
        assert!(matches!(
            session.commit_file_diff(&binary.file_id).unwrap(),
            FileDiff::Binary
        ));
        assert!(session.commit_file_diff("../../index").is_err());
        let again = session.commit_files(oid, None).unwrap();
        assert_ne!(again.files[0].file_id, list.files[0].file_id);
        assert!(session.commit_file_diff(&selected.file_id).is_err());
        assert_eq!(
            tracked
                .iter()
                .map(|path| fs::read(path).unwrap())
                .collect::<Vec<_>>(),
            before
        );
    }

    /// 合并差异默认第一父，另一直接父可切换；非父与未返回提交拒绝。
    #[test]
    fn merge_selects_direct_parent_and_preserves_rename_paths() {
        let f = Fixture::new();
        let old_name = if cfg!(windows) {
            "old 中文"
        } else {
            "old\t中文"
        };
        let new_name = if cfg!(windows) {
            "new 中文"
        } else {
            "new\n中文"
        };
        f.write(old_name, b"base\n");
        f.command(&["add", "."]);
        f.command(&["commit", "-m", "base"]);
        f.command(&["branch", "other"]);
        f.command(&["mv", old_name, new_name]);
        f.command(&["commit", "-m", "rename"]);
        f.command(&["checkout", "other"]);
        f.write("other-file", b"incoming\n");
        f.command(&["add", "."]);
        f.command(&["commit", "-m", "other"]);
        f.command(&["checkout", "main"]);
        f.command(&["merge", "--no-ff", "other", "-m", "merge"]);
        let (repo, _) = open_repository(&f.git, &f.root).unwrap();
        let mut session = HistorySession::new(f.git.clone(), repo).unwrap();
        let page = session.read_page(None).unwrap();
        let merge = &page.commits[0];
        let first = session.commit_files(&merge.oid, None).unwrap();
        assert_eq!(first.parent_oid.as_ref(), merge.parent_oids.first());
        assert_eq!(first.files.len(), 1);
        assert_eq!(first.files[0].path, "other-file");
        let second = session
            .commit_files(&merge.oid, Some(&merge.parent_oids[1]))
            .unwrap();
        assert_eq!(second.files.len(), 1);
        assert_eq!(second.files[0].status, "R");
        assert_eq!(second.files[0].path, new_name);
        assert_eq!(second.files[0].original_path.as_deref(), Some(old_name));
        assert_eq!(
            (second.files[0].additions, second.files[0].deletions),
            (Some(0), Some(0))
        );
        let FileDiff::Text { content, .. } =
            session.commit_file_diff(&second.files[0].file_id).unwrap()
        else {
            panic!("重命名应返回文本差异")
        };
        assert!(content.contains("rename from"));
        let root = page
            .commits
            .iter()
            .find(|commit| commit.parent_oids.is_empty())
            .unwrap();
        assert_eq!(
            session
                .commit_files(&merge.oid, Some(&root.oid))
                .unwrap_err()
                .code,
            "INVALID_INPUT"
        );
        assert_eq!(
            session.commit_files("--all", None).unwrap_err().code,
            "INVALID_INPUT"
        );
    }

    /// 分页末端提交的父尚未加载时仍可作为经 Git 确认的差异基准。
    #[test]
    fn boundary_parent_is_valid_without_loading_its_card() {
        let f = Fixture::new();
        for value in 0..52 {
            f.write("count", format!("{value}\n").as_bytes());
            f.command(&["add", "."]);
            f.command(&["commit", "-m", &format!("{value}")]);
        }
        let (repo, _) = open_repository(&f.git, &f.root).unwrap();
        let mut session = HistorySession::new(f.git.clone(), repo).unwrap();
        let page = session.read_page(None).unwrap();
        let boundary = page.commits.last().unwrap();
        assert!(!session.returned.contains(&boundary.parent_oids[0]));
        let list = session.commit_files(&boundary.oid, None).unwrap();
        assert_eq!(
            list.parent_oid.as_deref(),
            Some(boundary.parent_oids[0].as_str())
        );
        assert_eq!(list.files[0].path, "count");
        assert_eq!(
            session
                .commit_files(&boundary.parent_oids[0], None)
                .unwrap_err()
                .code,
            "INVALID_INPUT"
        );
    }

    /// 历史文本复用既有行数和编码边界，符号链接不伪装为可编辑文本。
    #[test]
    fn historical_diff_reports_truncation_and_encoding() {
        let f = Fixture::new();
        f.write("large", "line\n".repeat(5100).as_bytes());
        f.write("encoded", &[0xff, b'\n']);
        #[cfg(unix)]
        std::os::unix::fs::symlink("large", f.root.join("link")).unwrap();
        f.command(&["add", "."]);
        f.command(&["commit", "-m", "content"]);
        let (repo, _) = open_repository(&f.git, &f.root).unwrap();
        let mut session = HistorySession::new(f.git.clone(), repo).unwrap();
        let page = session.read_page(None).unwrap();
        let list = session.commit_files(&page.commits[0].oid, None).unwrap();
        let large = list.files.iter().find(|file| file.path == "large").unwrap();
        let FileDiff::Text { content, truncated } =
            session.commit_file_diff(&large.file_id).unwrap()
        else {
            panic!("应返回截断文本")
        };
        assert!(truncated);
        assert_eq!(content.lines().count(), 5000);
        let encoded = list
            .files
            .iter()
            .find(|file| file.path == "encoded")
            .unwrap();
        assert!(
            matches!(session.commit_file_diff(&encoded.file_id).unwrap(), FileDiff::Unsupported { reason } if reason == "encoding")
        );
        #[cfg(unix)]
        {
            let link = list.files.iter().find(|file| file.path == "link").unwrap();
            assert!(
                matches!(session.commit_file_diff(&link.file_id).unwrap(), FileDiff::Unsupported { reason } if reason == "symlink")
            );
        }
    }
}
