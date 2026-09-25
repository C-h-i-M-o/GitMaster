//! 当前工作树项目文件的只读列表与按需内容读取。
pub mod editor_text;
pub mod reader;
pub mod tree;
use super::{
    diff::preview,
    repository::{next_id, query, read_repository_state},
    FileDiff, GitExecutable, OperationError, ProjectFile, ProjectFileKind, ProjectFileList,
    RepositoryHandle, RepositoryState,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

const MAX_FILES: usize = 10_000;

/// 完整编辑文档绑定文件身份，不能使用只读截断结果构造。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditableFile {
    pub file_id: String,
    pub path: String,
    pub version: String,
    pub text: editor_text::EditorText,
}

/// 绑定仓库快照的项目文件会话；文件 ID 只在本会话内有效。
pub struct ProjectFilesSession {
    git: GitExecutable,
    repository: RepositoryHandle,
    state: RepositoryState,
    files: Vec<(String, String, ProjectFileKind)>,
    tree: std::sync::OnceLock<tree::TreeIndex>,
    readers: std::sync::Mutex<BTreeMap<String, reader::PagedDocument>>,
}

impl ProjectFilesSession {
    /// 生成一次性保存计划，执行由原协调器排队并发布标准任务结果。
    pub fn prepare_save(
        self: &std::sync::Arc<Self>,
        coordinator: &super::coordinator::RepositoryCoordinator,
        file_id: &str,
        expected_version: &str,
        content: &str,
    ) -> Result<super::WritePreview, OperationError> {
        let generation = coordinator.begin_prepare()?;
        let key = super::coordinator::CoordinationKey::repository(&self.repository)?;
        let deadline = Instant::now() + Duration::from_secs(120);
        let document = coordinator.read_until(&key, deadline, || self.read_editable(file_id))?;
        if document.version != expected_version {
            return Err(OperationError::new("FILE_CHANGED"));
        }
        editor_text::encode(&document.text, content)?;
        let session = self.clone();
        let id = file_id.to_owned();
        let version = expected_version.to_owned();
        let draft = content.to_owned();
        let plan = coordinator.prepare(
            generation,
            Some(self.repository.id.clone()),
            key,
            super::OperationKind::SaveFile,
            move |reporter| {
                let deadline = reporter.deadline(Duration::from_secs(120));
                let result = session.save_editable(&id, &version, &draft);
                super::write::operation_result(
                    reporter,
                    &session.git,
                    &session.repository,
                    result,
                    false,
                    None,
                    None,
                    deadline,
                )
            },
        )?;
        Ok(super::WritePreview {
            plan_id: plan.plan_id,
            repository_id: self.repository.id.clone(),
            snapshot_id: self.state.snapshot_id.clone(),
            kind: super::OperationKind::SaveFile,
            head: self.state.head.clone(),
            parent_oids: Vec::new(),
            paths: vec![document.path],
            author: None,
            message: None,
            target: None,
            warnings: Vec::new(),
            expires_at: plan.expires_at,
        })
    }
    /// 保存完整草稿前复核页面版本；调用方必须持有仓库协调队列。
    pub fn save_editable(
        &self,
        file_id: &str,
        expected_version: &str,
        content: &str,
    ) -> Result<(), OperationError> {
        let deadline = Instant::now() + Duration::from_secs(120);
        super::write_guard::validate_snapshot(&self.git, &self.repository, &self.state, deadline)?;
        let (_, path, _) = self
            .files
            .iter()
            .find(|(id, _, _)| id == file_id)
            .ok_or_else(|| OperationError::new("FILE_UNAVAILABLE"))?;
        let original = super::conflicts::read_regular(
            &self.repository.root,
            path,
            editor_text::MAX_EDIT_BYTES,
            deadline,
        )
        .map_err(editor_read_error)?;
        if file_version(&self.repository.id, file_id, path, &original) != expected_version {
            return Err(OperationError::new("FILE_CHANGED"));
        }
        let document = editor_text::decode(&original.bytes)?;
        let bytes = editor_text::encode(&document, content)?;
        if bytes == original.bytes {
            return Ok(());
        }
        super::conflicts::replace_regular(
            &self.repository.root,
            path,
            &original,
            &bytes,
            editor_text::MAX_EDIT_BYTES,
            deadline,
        )
        .map_err(editor_read_error)
    }
    /// 复用经过符号链接和文件身份检查的有界读取，返回完整 UTF-8 编辑文档。
    pub fn read_editable(&self, file_id: &str) -> Result<EditableFile, OperationError> {
        let deadline = Instant::now() + Duration::from_secs(120);
        super::write_guard::validate_snapshot(&self.git, &self.repository, &self.state, deadline)?;
        let (_, path, _) = self
            .files
            .iter()
            .find(|(id, _, _)| id == file_id)
            .ok_or_else(|| OperationError::new("FILE_UNAVAILABLE"))?;
        let file = super::conflicts::read_regular(
            &self.repository.root,
            path,
            editor_text::MAX_EDIT_BYTES,
            deadline,
        )
        .map_err(editor_read_error)?;
        let text = editor_text::decode(&file.bytes)?;
        Ok(EditableFile {
            file_id: file_id.to_owned(),
            path: path.clone(),
            version: file_version(&self.repository.id, file_id, path, &file),
            text,
        })
    }
    /// 从已验证的仓库状态建立文件列表，不扫描忽略目录。
    pub fn new(
        git: GitExecutable,
        repository: RepositoryHandle,
        state: RepositoryState,
    ) -> Result<Self, OperationError> {
        Self::new_with_ignored(git, repository, state, false)
    }

    /// 按用户开关包含忽略文件；文件身份和内容读取仍受原会话约束。
    pub fn new_with_ignored(
        git: GitExecutable,
        repository: RepositoryHandle,
        state: RepositoryState,
        include_ignored: bool,
    ) -> Result<Self, OperationError> {
        if state.repository_id != repository.id {
            return Err(OperationError::new("STALE_REQUEST"));
        }
        let fresh = read_repository_state(&git, &repository)?;
        if fresh.head != state.head
            || fresh.operations != state.operations
            || fresh.changes != state.changes
        {
            return Err(OperationError::new("STALE_REQUEST"));
        }
        let tracked = query(
            &git,
            &repository.root,
            &["ls-files", "-z", "--cached"],
            8 * 1024 * 1024,
        )?;
        let untracked = query(
            &git,
            &repository.root,
            &["ls-files", "-z", "--others", "--exclude-standard"],
            8 * 1024 * 1024,
        )?;
        let ignored = if include_ignored {
            query(
                &git,
                &repository.root,
                &[
                    "ls-files",
                    "-z",
                    "--others",
                    "--ignored",
                    "--exclude-standard",
                ],
                8 * 1024 * 1024,
            )?
        } else {
            Vec::new()
        };
        let mut entries = BTreeMap::new();
        for (output, kind) in [
            (tracked, ProjectFileKind::Tracked),
            (untracked, ProjectFileKind::Untracked),
            (ignored, ProjectFileKind::Ignored),
        ] {
            for raw in output.split(|b| *b == 0).filter(|raw| !raw.is_empty()) {
                let path = std::str::from_utf8(raw)
                    .map_err(|_| OperationError::new("UNSUPPORTED_PATH_ENCODING"))?
                    .to_owned();
                // Git 元数据不进入文件访问能力，即使用户开启忽略文件。
                if path
                    .split('/')
                    .any(|part| part.eq_ignore_ascii_case(".git"))
                {
                    continue;
                }
                entries.entry(path).or_insert(kind);
            }
        }
        if entries.len() > MAX_FILES {
            return Err(OperationError::new("OUTPUT_LIMIT"));
        }
        let after = read_repository_state(&git, &repository)?;
        if after.head != state.head
            || after.operations != state.operations
            || after.changes != state.changes
        {
            return Err(OperationError::new("STALE_REQUEST"));
        }
        let files = entries
            .into_iter()
            .map(|(path, kind)| (next_id(), path, kind))
            .collect();
        Ok(Self {
            git,
            repository,
            state,
            files,
            tree: std::sync::OnceLock::new(),
            readers: std::sync::Mutex::new(BTreeMap::new()),
        })
    }

    /// 返回后端生成且与本会话绑定的文件 ID。
    pub fn list(&self) -> ProjectFileList {
        ProjectFileList {
            repository_id: self.repository.id.clone(),
            snapshot_id: self.state.snapshot_id.clone(),
            files: self
                .files
                .iter()
                .map(|(file_id, path, kind)| ProjectFile {
                    file_id: file_id.clone(),
                    path: path.clone(),
                    kind: *kind,
                })
                .collect(),
        }
    }

    /// 重新核对状态后，安全读取列表中的普通文件内容。
    pub fn read(&self, file_id: &str) -> Result<FileDiff, OperationError> {
        let fresh = read_repository_state(&self.git, &self.repository)?;
        if fresh.head != self.state.head
            || fresh.operations != self.state.operations
            || fresh.changes != self.state.changes
        {
            return Err(OperationError::new("STALE_REQUEST"));
        }
        let (_, path, _) = self
            .files
            .iter()
            .find(|(id, _, _)| id == file_id)
            .ok_or_else(|| OperationError::new("FILE_UNAVAILABLE"))?;
        preview(&self.repository, path)
    }
}

/// 共用安全读取原语的历史错误码在编辑入口转换为对应文件错误。
fn editor_read_error(error: OperationError) -> OperationError {
    match error.code.as_str() {
        "OUTPUT_LIMIT" => OperationError::new("FILE_EDIT_TOO_LARGE"),
        "UNSUPPORTED_CONFLICT" => OperationError::new("FILE_EDIT_UNSUPPORTED"),
        "STALE_CONFLICT" => OperationError::new("FILE_CHANGED"),
        _ => error,
    }
}

/// 内容和目录项身份共同构成版本，替换为同字节文件也不沿用旧授权。
fn file_version(
    repository_id: &str,
    file_id: &str,
    path: &str,
    file: &super::conflicts::FileBytes,
) -> String {
    let mut digest = Sha256::new();
    for part in [
        repository_id.as_bytes(),
        file_id.as_bytes(),
        path.as_bytes(),
        file.identity.as_bytes(),
        &file.bytes,
    ] {
        digest.update((part.len() as u64).to_le_bytes());
        digest.update(part);
    }
    format!("{:x}", digest.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::repository::{open_repository, tests::Fixture};

    /// 根目录不夹带后代文件，分页稳定且搜索复用文件身份。
    #[test]
    fn project_tree_pages_preserve_file_capabilities() {
        let f = Fixture::new();
        std::fs::create_dir_all(f.root.join("中文 目录/深层")).unwrap();
        f.write("中文 目录/深层/hello.ts", b"hello\n");
        for n in 0..205 {
            f.write(&format!("file-{n:03}"), b"value\n");
        }
        let (repo, state) = open_repository(&f.git, &f.root).unwrap();
        let session = ProjectFilesSession::new(f.git.clone(), repo.clone(), state.clone()).unwrap();
        let root = session.tree_root();
        assert_eq!(root.entries.len(), 200);
        assert_eq!(root.total, 206);
        assert_eq!(root.next_offset, Some(200));
        let directory = &root.entries[0];
        assert!(matches!(
            directory,
            tree::ProjectTreeEntry::Directory { .. }
        ));
        let second = session
            .tree_page(&root.tree_id, &root.directory_id, 200)
            .unwrap();
        assert_eq!(second.entries.len(), 6);
        assert_eq!(second.next_offset, None);
        let names: std::collections::BTreeSet<_> = root
            .entries
            .iter()
            .chain(&second.entries)
            .map(|entry| entry.id())
            .collect();
        assert_eq!(names.len(), 206);
        let child = session.tree_page(&root.tree_id, directory.id(), 0).unwrap();
        assert_eq!(child.entries.len(), 1);
        let leaf = session
            .tree_page(&root.tree_id, child.entries[0].id(), 0)
            .unwrap();
        assert_eq!(leaf.entries.len(), 1);
        let found = session.tree_search(&root.tree_id, "hello.TS", 0).unwrap();
        assert_eq!(found.entries[0].id(), leaf.entries[0].id());
        assert_eq!(
            session
                .read_editable(found.entries[0].id())
                .unwrap()
                .text
                .content,
            "hello\n"
        );
        assert_eq!(
            session
                .tree_page(&root.tree_id, "../../", 0)
                .unwrap_err()
                .code,
            "FILE_UNAVAILABLE"
        );
        assert_eq!(
            session
                .tree_page(&root.tree_id, &root.directory_id, 207)
                .unwrap_err()
                .code,
            "INVALID_INPUT"
        );
        let next = ProjectFilesSession::new(f.git.clone(), repo, state).unwrap();
        assert_eq!(
            next.tree_page(&root.tree_id, &root.directory_id, 0)
                .unwrap_err()
                .code,
            "STALE_REQUEST"
        );
    }

    /// 相同字节的新目录项不能复用旧文件的保存授权，删除后也不能隐式重建。
    #[test]
    fn editor_save_rejects_replacement_and_deleted_file() {
        let f = Fixture::new();
        f.write("text", b"original\n");
        f.command(&["add", "text"]);
        f.command(&["commit", "-m", "base"]);
        let (repo, state) = open_repository(&f.git, &f.root).unwrap();
        let index = std::fs::read(repo.git_dir.join("index")).unwrap();
        let session = ProjectFilesSession::new(f.git.clone(), repo.clone(), state).unwrap();
        let id = session.list().files[0].file_id.clone();
        let document = session.read_editable(&id).unwrap();
        // 旧文件保留在仓库之外，避免 inode 复用或新增工作文件影响快照断言。
        let backup = tempfile::tempdir_in(f.root.parent().unwrap()).unwrap();
        std::fs::rename(f.root.join("text"), backup.path().join("original")).unwrap();
        f.write("text", b"original\n");
        assert_eq!(
            session
                .save_editable(&id, &document.version, "overwrite")
                .unwrap_err()
                .code,
            "FILE_CHANGED"
        );
        assert_eq!(std::fs::read(f.root.join("text")).unwrap(), b"original\n");
        std::fs::remove_file(f.root.join("text")).unwrap();
        assert!(session
            .save_editable(&id, &document.version, "overwrite")
            .is_err());
        assert!(!f.root.join("text").exists());
        assert_eq!(std::fs::read(repo.git_dir.join("index")).unwrap(), index);
        assert_eq!(
            std::fs::read(backup.path().join("original")).unwrap(),
            b"original\n"
        );
    }

    /// 保存入口拒绝二进制及超限草稿，不能先截断或损坏工作文件。
    #[test]
    fn editor_save_rejects_invalid_drafts_without_writes() {
        let f = Fixture::new();
        f.write("text", b"\xef\xbb\xbforiginal\r\n");
        let (repo, state) = open_repository(&f.git, &f.root).unwrap();
        let session = ProjectFilesSession::new(f.git.clone(), repo, state).unwrap();
        let id = session.list().files[0].file_id.clone();
        let document = session.read_editable(&id).unwrap();
        for (content, expected) in [
            ("a\0b".to_owned(), "FILE_EDIT_BINARY"),
            (
                "a".repeat(editor_text::MAX_EDIT_BYTES),
                "FILE_EDIT_TOO_LARGE",
            ),
        ] {
            assert_eq!(
                session
                    .save_editable(&id, &document.version, &content)
                    .unwrap_err()
                    .code,
                expected
            );
            assert_eq!(
                std::fs::read(f.root.join("text")).unwrap(),
                b"\xef\xbb\xbforiginal\r\n"
            );
        }
    }

    /// 文档打开后替换为仓库外符号链接，保存必须拒绝且不改变链接目标。
    #[cfg(unix)]
    #[test]
    fn editor_save_rejects_late_symlink_escape() {
        let f = Fixture::new();
        f.write("text", b"original\n");
        let (repo, state) = open_repository(&f.git, &f.root).unwrap();
        let session = ProjectFilesSession::new(f.git.clone(), repo, state).unwrap();
        let id = session.list().files[0].file_id.clone();
        let document = session.read_editable(&id).unwrap();
        let outside = tempfile::tempdir().unwrap();
        let target = outside.path().join("target");
        std::fs::write(&target, b"outside\n").unwrap();
        std::fs::remove_file(f.root.join("text")).unwrap();
        std::os::unix::fs::symlink(&target, f.root.join("text")).unwrap();
        assert!(session
            .save_editable(&id, &document.version, "overwrite")
            .is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"outside\n");
        assert_eq!(std::fs::read_link(f.root.join("text")).unwrap(), target);
    }

    /// 编辑保存只替换工作文件，旧文档版本不能覆盖外部修改。
    #[test]
    fn editor_save_preserves_git_and_rejects_stale_bytes() {
        let f = Fixture::new();
        f.write("text", b"\xef\xbb\xbfline\r\n");
        let (repo, state) = open_repository(&f.git, &f.root).unwrap();
        let head = std::fs::read(repo.git_dir.join("HEAD")).unwrap();
        let session = std::sync::Arc::new(
            ProjectFilesSession::new(f.git.clone(), repo.clone(), state).unwrap(),
        );
        let id = session.list().files[0].file_id.clone();
        let document = session.read_editable(&id).unwrap();
        let coordinator = super::super::coordinator::RepositoryCoordinator::new();
        let plan = session
            .prepare_save(&coordinator, &id, &document.version, "changed\r\n")
            .unwrap();
        let handle = coordinator.execute(Some(&repo.id), &plan.plan_id).unwrap();
        let limit = Instant::now() + Duration::from_secs(120);
        loop {
            let record = coordinator
                .read_operation(Some(&handle.operation_id))
                .unwrap()
                .unwrap();
            if let Some(result) = record.result {
                assert!(
                    matches!(
                        result,
                        super::super::OperationResult::Succeeded {
                            kind: super::super::OperationKind::SaveFile,
                            ..
                        }
                    ),
                    "{result:?}"
                );
                break;
            }
            assert!(Instant::now() < limit, "文件保存任务超时");
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            std::fs::read(f.root.join("text")).unwrap(),
            b"\xef\xbb\xbfchanged\r\n"
        );
        assert_eq!(std::fs::read(repo.git_dir.join("HEAD")).unwrap(), head);
        assert!(query(&f.git, &f.root, &["ls-files", "--stage"], 4096)
            .unwrap()
            .is_empty());
        f.write("text", b"external");
        assert_eq!(
            session
                .save_editable(&id, &document.version, "overwrite")
                .unwrap_err()
                .code,
            "FILE_CHANGED"
        );
        assert_eq!(std::fs::read(f.root.join("text")).unwrap(), b"external");
    }

    /// 文档身份区分相同状态字母下的外部内容更新，正文保留 BOM 和换行。
    #[test]
    fn editable_document_is_complete_and_versioned() {
        let f = Fixture::new();
        f.write("text", b"\xef\xbb\xbfa\r\nb");
        let (repo, state) = open_repository(&f.git, &f.root).unwrap();
        let session = ProjectFilesSession::new(f.git.clone(), repo, state).unwrap();
        let id = &session.list().files[0].file_id;
        let before = session.read_editable(id).unwrap();
        assert_eq!(before.text.content, "a\r\nb");
        assert!(before.text.bom);
        f.write("text", b"new\n");
        let after = session.read_editable(id).unwrap();
        assert_ne!(before.version, after.version);
        assert_eq!(after.text.content, "new\n");
        assert_eq!(
            session.read_editable("unknown").unwrap_err().code,
            "FILE_UNAVAILABLE"
        );
    }

    /// 忽略文件不进入列表，tracked 与 untracked 路径各只出现一次。
    #[test]
    fn lists_tracked_and_non_ignored_untracked() {
        let f = Fixture::new();
        f.write("tracked", b"ok\n");
        f.write("forced", b"forced\n");
        f.write(".gitignore", b"ignored\nforced\n");
        f.write("ignored", b"no\n");
        f.write("new", b"new\n");
        f.command(&["add", "tracked", ".gitignore"]);
        f.command(&["add", "-f", "forced"]);
        let (repo, state) = open_repository(&f.git, &f.root).unwrap();
        let session = ProjectFilesSession::new(f.git.clone(), repo.clone(), state.clone()).unwrap();
        let list = session.list();
        assert!(list
            .files
            .iter()
            .any(|x| x.path == "tracked" && x.kind == ProjectFileKind::Tracked));
        assert!(list
            .files
            .iter()
            .any(|x| x.path == "forced" && x.kind == ProjectFileKind::Tracked));
        assert!(list
            .files
            .iter()
            .any(|x| x.path == "new" && x.kind == ProjectFileKind::Untracked));
        assert!(!list.files.iter().any(|x| x.path == "ignored"));
        let expanded =
            ProjectFilesSession::new_with_ignored(f.git.clone(), repo, state, true).unwrap();
        let expanded_list = expanded.list();
        let ignored = expanded_list
            .files
            .iter()
            .find(|file| file.path == "ignored")
            .unwrap();
        assert_eq!(ignored.kind, ProjectFileKind::Ignored);
        assert!(
            matches!(expanded.read(&ignored.file_id).unwrap(), FileDiff::Text { content, .. } if content == "no\n")
        );
        assert_eq!(
            expanded_list
                .files
                .iter()
                .filter(|file| file.path == "forced")
                .count(),
            1
        );
        assert!(expanded_list
            .files
            .iter()
            .any(|file| file.path == "forced" && file.kind == ProjectFileKind::Tracked));
        assert!(!expanded_list.files.iter().any(|file| file
            .path
            .split('/')
            .any(|part| part.eq_ignore_ascii_case(".git"))));
        assert_eq!(
            expanded.read(&list.files[0].file_id).unwrap_err().code,
            "FILE_UNAVAILABLE"
        );
    }

    /// 二进制内容可识别，文件 ID 不可跨会话复用，状态变化会使读取失效。
    #[test]
    fn rejects_old_id_and_stale_state_and_reads_binary() {
        let f = Fixture::new();
        f.write("bin", b"a\0b");
        let (repo, state) = open_repository(&f.git, &f.root).unwrap();
        let session = ProjectFilesSession::new(f.git.clone(), repo, state).unwrap();
        assert_eq!(session.read("old").unwrap_err().code, "FILE_UNAVAILABLE");
        let id = session.list().files[0].file_id.clone();
        let f2 = Fixture::new();
        f2.write("other", b"other\n");
        let (repo2, state2) = open_repository(&f2.git, &f2.root).unwrap();
        let session2 = ProjectFilesSession::new(f2.git.clone(), repo2, state2).unwrap();
        assert_eq!(session2.read(&id).unwrap_err().code, "FILE_UNAVAILABLE");
        assert!(matches!(session.read(&id).unwrap(), FileDiff::Binary));
        f.write("bin", b"latest content");
        assert!(
            matches!(session.read(&id).unwrap(), FileDiff::Text { content, .. } if content == "latest content")
        );
        f.command(&["add", "bin"]);
        assert_eq!(session.read(&id).unwrap_err().code, "STALE_REQUEST");
    }

    /// 空仓库没有 tracked 输出时仍能列出非忽略 untracked 文件。
    #[test]
    fn empty_repository_lists_untracked() {
        let f = Fixture::new();
        f.write("new", b"new\n");
        let (repo, state) = open_repository(&f.git, &f.root).unwrap();
        let session = ProjectFilesSession::new(f.git.clone(), repo, state).unwrap();
        assert_eq!(
            session
                .list()
                .files
                .iter()
                .filter(|file| file.path == "new")
                .count(),
            1
        );
    }

    /// 没有任何文件的 unborn 仓库返回空列表。
    #[test]
    fn empty_repository_lists_empty() {
        let f = Fixture::new();
        let (repo, state) = open_repository(&f.git, &f.root).unwrap();
        let session = ProjectFilesSession::new(f.git.clone(), repo, state).unwrap();
        assert!(session.list().files.is_empty());
    }

    /// 普通项目内容读取保留索引、HEAD、配置和工作文件的原始字节。
    #[test]
    fn ordinary_content_read_preserves_repository_bytes() {
        let f = Fixture::new();
        f.write("中文 空格", b"\xef\xbb\xbfline\r\n");
        f.command(&["add", "."]);
        let (repo, state) = open_repository(&f.git, &f.root).unwrap();
        let paths = [
            repo.git_dir.join("index"),
            repo.git_dir.join("HEAD"),
            repo.git_dir.join("config"),
            f.root.join("中文 空格"),
        ];
        let before = paths
            .iter()
            .map(|path| std::fs::read(path).unwrap())
            .collect::<Vec<_>>();
        let session = ProjectFilesSession::new(f.git.clone(), repo, state).unwrap();
        let file = session
            .list()
            .files
            .into_iter()
            .find(|file| file.path == "中文 空格")
            .unwrap();
        assert!(
            matches!(session.read(&file.file_id).unwrap(), FileDiff::Text { content, truncated: false } if content.as_bytes() == b"\xef\xbb\xbfline\r\n")
        );
        assert_eq!(
            paths
                .iter()
                .map(|path| std::fs::read(path).unwrap())
                .collect::<Vec<_>>(),
            before
        );
    }

    /// 多 stage 冲突条目只形成一个项目文件，列表创建拒绝已经失效的快照。
    #[test]
    fn conflict_stages_are_deduplicated_and_old_snapshot_is_rejected() {
        let f = Fixture::new();
        f.write("a", b"base\n");
        f.command(&["add", "."]);
        f.command(&["commit", "-m", "base"]);
        let (repo, old_state) = open_repository(&f.git, &f.root).unwrap();
        f.command(&["checkout", "-b", "other"]);
        f.write("a", b"other\n");
        f.command(&["commit", "-am", "other"]);
        f.command(&["checkout", "main"]);
        f.write("a", b"main\n");
        f.command(&["commit", "-am", "main"]);
        // merge 的冲突退出是测试目标，受限执行器不将其当作 fixture 准备成功。
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        assert!(crate::git::write_guard::local_query(
            &f.git,
            &repo,
            &["merge", "other"],
            &[],
            None,
            deadline
        )
        .is_err());
        assert!(
            matches!(ProjectFilesSession::new(f.git.clone(), repo.clone(), old_state), Err(error) if error.code == "STALE_REQUEST")
        );
        let state = read_repository_state(&f.git, &repo).unwrap();
        assert!(state
            .changes
            .iter()
            .any(|change| change.kind == "conflicted"));
        let session = ProjectFilesSession::new(f.git.clone(), repo, state).unwrap();
        assert_eq!(
            session
                .list()
                .files
                .iter()
                .filter(|file| file.path == "a")
                .count(),
            1
        );
    }

    /// 读取项目文件不会改写工作区字节；祖先符号链接不能越出仓库目录。
    #[cfg(unix)]
    #[test]
    fn read_is_readonly_and_rejects_ancestor_symlink() {
        use std::os::unix::fs::symlink;
        let f = Fixture::new();
        f.write("linkdir/file", b"unchanged\n");
        f.command(&["add", "linkdir/file"]);
        f.command(&["commit", "-m", "x"]);
        let f2 = Fixture::new();
        f2.write("file", b"secret\n");
        let outside = f2.root.clone();
        std::fs::remove_dir_all(f.root.join("linkdir")).unwrap();
        symlink(&outside, f.root.join("linkdir")).unwrap();
        let before_index = std::fs::read(f.root.join(".git/index")).unwrap();
        let before_head = std::fs::read(f.root.join(".git/HEAD")).unwrap();
        let before_config = std::fs::read(f.root.join(".git/config")).unwrap();
        let (repo, state) = open_repository(&f.git, &f.root).unwrap();
        let session = ProjectFilesSession::new(f.git.clone(), repo, state).unwrap();
        let listed = session.list();
        let real = listed
            .files
            .iter()
            .find(|file| file.path == "linkdir/file")
            .unwrap();
        assert!(session.read(&real.file_id).is_err());
        assert_eq!(
            std::fs::read(f.root.join("linkdir/file")).unwrap(),
            b"secret\n"
        );
        assert_eq!(
            std::fs::read(f.root.join(".git/HEAD")).unwrap(),
            before_head
        );
        assert_eq!(
            std::fs::read(f.root.join(".git/config")).unwrap(),
            before_config
        );
        assert_eq!(
            std::fs::read(f.root.join(".git/index")).unwrap(),
            before_index
        );
    }

    /// 删除 tracked 文件仍展示项目项，但内容读取拒绝不可读路径。
    #[test]
    fn deleted_tracked_file_is_listed_but_not_readable() {
        let f = Fixture::new();
        f.write("gone", b"x");
        f.command(&["add", "gone"]);
        f.command(&["commit", "-m", "x"]);
        std::fs::remove_file(f.root.join("gone")).unwrap();
        let (repo, state) = open_repository(&f.git, &f.root).unwrap();
        let session = ProjectFilesSession::new(f.git.clone(), repo, state).unwrap();
        let file = session
            .list()
            .files
            .into_iter()
            .find(|x| x.path == "gone")
            .unwrap();
        assert!(session.read(&file.file_id).is_err());
    }
}
