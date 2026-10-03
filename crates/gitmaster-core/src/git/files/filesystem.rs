//! 真实工作目录的懒加载索引，目录能力独立于 Git 跟踪状态。
use super::{next_id, tree::ProjectTreeEntry, OperationError, ProjectFileKind};
use cap_std::fs::Dir;
use std::{
    collections::{BTreeMap, VecDeque},
    path::Path,
    time::{Duration, Instant},
};

/// 每个搜索游标绑定当前查询，只保存目录队列和已扫描位置。
struct Search {
    query: String,
    directories: VecDeque<String>,
    position: usize,
    cursor: usize,
    matches: usize,
}
/// 已发现条目的身份在会话内稳定；不会在构造时递归读取后代。
pub(super) struct FilesystemTree {
    pub id: String,
    root: Dir,
    pub files: BTreeMap<String, (String, ProjectFileKind)>,
    paths: BTreeMap<String, String>,
    directories: BTreeMap<String, String>,
    children: BTreeMap<String, Vec<ProjectTreeEntry>>,
    search: Option<Search>,
}
impl FilesystemTree {
    /// 首屏只访问根目录，空目录也有独立目录能力。
    pub fn new(root: &Path) -> Result<Self, OperationError> {
        let id = next_id();
        let mut tree = Self {
            id: id.clone(),
            root: open_root(root)?,
            files: BTreeMap::new(),
            paths: BTreeMap::new(),
            directories: BTreeMap::from([(id.clone(), String::new())]),
            children: BTreeMap::new(),
            search: None,
        };
        tree.load(&id)?;
        Ok(tree)
    }
    /// 通过不跟随链接的逐级目录句柄枚举单层，不读取文件正文。
    fn load(&mut self, id: &str) -> Result<(), OperationError> {
        if self.children.contains_key(id) {
            return Ok(());
        }
        let path = self
            .directories
            .get(id)
            .ok_or_else(|| OperationError::new("FILE_UNAVAILABLE"))?
            .clone();
        let dir = open_directory(&self.root, &path)?;
        let mut entries = Vec::new();
        for item in dir
            .entries()
            .map_err(|_| OperationError::new("ACCESS_DENIED"))?
        {
            let item = item.map_err(|_| OperationError::new("ACCESS_DENIED"))?;
            let name = item
                .file_name()
                .into_string()
                .map_err(|_| OperationError::new("UNSUPPORTED_PATH_ENCODING"))?;
            if name.eq_ignore_ascii_case(".git") {
                continue;
            }
            let child_path = if path.is_empty() {
                name.clone()
            } else {
                format!("{path}/{name}")
            };
            let kind = item
                .file_type()
                .map_err(|_| OperationError::new("FILE_UNAVAILABLE"))?;
            let child = self
                .paths
                .entry(child_path.clone())
                .or_insert_with(next_id)
                .clone();
            if kind.is_dir() && !kind.is_symlink() {
                self.directories.insert(child.clone(), child_path.clone());
                entries.push(ProjectTreeEntry::Directory {
                    id: child,
                    path: child_path,
                    name,
                });
            } else if kind.is_file() || kind.is_symlink() {
                // 链接只可展示，文件读取原语仍拒绝链接及任何链接祖先。
                self.files.insert(
                    child.clone(),
                    (child_path.clone(), ProjectFileKind::Untracked),
                );
                entries.push(ProjectTreeEntry::File {
                    id: child,
                    path: child_path,
                    name,
                    status: ProjectFileKind::Untracked,
                });
            }
        }
        entries.sort_by(|a, b| {
            matches!(a, ProjectTreeEntry::File { .. })
                .cmp(&matches!(b, ProjectTreeEntry::File { .. }))
                .then_with(|| a.name().cmp(b.name()))
        });
        self.children.insert(id.to_owned(), entries);
        Ok(())
    }
    /// 已签发目录按需读取，分页不会递归到后代。
    pub fn page(
        &mut self,
        _root: &Path,
        id: &str,
        offset: usize,
    ) -> Result<(Vec<ProjectTreeEntry>, usize), OperationError> {
        self.load(id)?;
        let entries = &self.children[id];
        if offset > entries.len() {
            return Err(OperationError::new("INVALID_INPUT"));
        }
        Ok((
            entries[offset..(offset.saturating_add(200)).min(entries.len())].to_vec(),
            entries.len(),
        ))
    }
    /// 根已经在构造时核实，返回缓存不引入新的失败通道。
    pub fn root(&self) -> (&[ProjectTreeEntry], usize) {
        let entries = &self.children[&self.id];
        (&entries[..entries.len().min(200)], entries.len())
    }
    /// 分批扫描路径，最多扫描 2000 条、返回 200 条匹配；游标不能跨查询复用。
    pub fn search(
        &mut self,
        _root: &Path,
        query: &str,
        offset: usize,
    ) -> Result<(Vec<ProjectTreeEntry>, Option<usize>, usize, bool), OperationError> {
        if query.chars().count() > 256 {
            return Err(OperationError::new("INVALID_INPUT"));
        }
        let query = query.trim().to_lowercase();
        if offset == 0 {
            self.search = Some(Search {
                query: query.clone(),
                directories: VecDeque::from([self.id.clone()]),
                position: 0,
                cursor: 0,
                matches: 0,
            });
        }
        let mut search = self
            .search
            .take()
            .ok_or_else(|| OperationError::new("STALE_REQUEST"))?;
        if search.query != query || search.cursor != offset {
            self.search = Some(search);
            return Err(OperationError::new("STALE_REQUEST"));
        }
        let mut output = Vec::new();
        let deadline = Instant::now() + Duration::from_millis(75);
        let mut visited = 0;
        let result = (|| {
            while visited < 2000 && output.len() < 200 && Instant::now() < deadline {
                let Some(id) = search.directories.front().cloned() else {
                    break;
                };
                self.load(&id)?;
                let entries = &self.children[&id];
                let Some(entry) = entries.get(search.position) else {
                    search.directories.pop_front();
                    search.position = 0;
                    // 空目录也推进游标，避免零结果批次无法继续。
                    visited += 1;
                    search.cursor += 1;
                    continue;
                };
                search.position += 1;
                search.cursor += 1;
                visited += 1;
                match entry {
                    ProjectTreeEntry::Directory { id, .. } => {
                        search.directories.push_back(id.clone())
                    }
                    ProjectTreeEntry::File { path, .. } if path.to_lowercase().contains(&query) => {
                        output.push(entry.clone());
                        search.matches += 1;
                    }
                    _ => {}
                }
            }
            let incomplete = !search.directories.is_empty();
            Ok((
                output,
                incomplete.then_some(search.cursor),
                search.matches,
                incomplete,
            ))
        })();
        self.search = Some(search);
        result
    }
}

/// 对能力签发的相对目录逐级打开，拒绝链接替换和仓库外访问。
fn open_directory(root: &Dir, path: &str) -> Result<Dir, OperationError> {
    let mut dir = root
        .try_clone()
        .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
    for part in path.split('/').filter(|part| !part.is_empty()) {
        if part == "." || part == ".." || part.eq_ignore_ascii_case(".git") {
            return Err(OperationError::new("FILE_UNAVAILABLE"));
        }
        #[cfg(unix)]
        {
            use cap_std::fs::OpenOptionsExt;
            let mut options = cap_std::fs::OpenOptions::new();
            options
                .read(true)
                .custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY | libc::O_NONBLOCK);
            let file = dir
                .open_with(part, &options)
                .map_err(|_| OperationError::new("FILE_UNAVAILABLE"))?;
            dir = Dir::from_std_file(file.into_std());
        }
        #[cfg(windows)]
        {
            dir = crate::git::windows_fs::open_directory(&dir, Path::new(part))?;
        }
        #[cfg(not(any(unix, windows)))]
        {
            return Err(OperationError::new("FILE_UNAVAILABLE"));
        }
    }
    Ok(dir)
}

/// 打开时拒绝根链接，后续分页复用句柄，不再次解析可能已被替换的根路径。
fn open_root(root: &Path) -> Result<Dir, OperationError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY | libc::O_NONBLOCK)
            .open(root)
            .map_err(|_| OperationError::new("FILE_UNAVAILABLE"))?;
        Ok(Dir::from_std_file(file))
    }
    #[cfg(windows)]
    {
        if let (Some(parent), Some(name)) = (root.parent(), root.file_name()) {
            let dir = Dir::open_ambient_dir(parent, cap_std::ambient_authority())
                .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
            crate::git::windows_fs::open_directory(&dir, Path::new(name))
        } else {
            Dir::open_ambient_dir(root, cap_std::ambient_authority())
                .map_err(|_| OperationError::new("ACCESS_DENIED"))
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        Err(OperationError::new("FILE_UNAVAILABLE"))
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::symlink};

    #[test]
    /// 根路径被替换为链接后仍只能访问会话最初打开的目录。
    fn replaced_root_cannot_redirect_tree_outside_repository() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("repo");
        let outside = temp.path().join("outside");
        fs::create_dir_all(root.join("child")).unwrap();
        fs::create_dir_all(outside.join("child")).unwrap();
        fs::write(root.join("child/local.txt"), b"local").unwrap();
        fs::write(outside.join("child/private.txt"), b"private").unwrap();
        let mut tree = FilesystemTree::new(&root).unwrap();
        let child = tree.paths["child"].clone();
        fs::rename(&root, temp.path().join("old-repo")).unwrap();
        symlink(&outside, &root).unwrap();
        let (entries, _) = tree.page(&root, &child, 0).unwrap();
        assert!(entries.iter().any(|entry| entry.name() == "local.txt"));
        assert!(!entries.iter().any(|entry| entry.name() == "private.txt"));
        assert!(FilesystemTree::new(&root).is_err());
    }
}
