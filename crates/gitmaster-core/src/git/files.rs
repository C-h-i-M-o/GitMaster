//! 当前工作树项目文件的只读列表与按需内容读取。
use super::{
    diff::preview,
    repository::{next_id, query, read_repository_state},
    FileDiff, GitExecutable, OperationError, ProjectFile, ProjectFileKind, ProjectFileList,
    RepositoryHandle, RepositoryState,
};
use std::collections::BTreeMap;

const MAX_FILES: usize = 10_000;

/// 绑定仓库快照的项目文件会话；文件 ID 只在本会话内有效。
pub struct ProjectFilesSession {
    git: GitExecutable,
    repository: RepositoryHandle,
    state: RepositoryState,
    files: Vec<(String, String, ProjectFileKind)>,
}

impl ProjectFilesSession {
    /// 从已验证的仓库状态建立文件列表，不扫描忽略目录。
    pub fn new(
        git: GitExecutable,
        repository: RepositoryHandle,
        state: RepositoryState,
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
        let mut entries = BTreeMap::new();
        for (output, kind) in [
            (tracked, ProjectFileKind::Tracked),
            (untracked, ProjectFileKind::Untracked),
        ] {
            for raw in output.split(|b| *b == 0).filter(|raw| !raw.is_empty()) {
                let path = std::str::from_utf8(raw)
                    .map_err(|_| OperationError::new("UNSUPPORTED_PATH_ENCODING"))?
                    .to_owned();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::repository::{open_repository, tests::Fixture};

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
        let session = ProjectFilesSession::new(f.git.clone(), repo, state).unwrap();
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
