//! 项目清单的目录能力与有界分页，不在展开目录时重复签发文件身份。
use super::{next_id, OperationError, ProjectFileKind, ProjectFilesSession};
use serde::Serialize;
use std::collections::BTreeMap;

const PAGE_SIZE: usize = 200;

/// 目录与文件使用不同标签，文件身份沿用现有读取与保存接口。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ProjectTreeEntry {
    Directory {
        id: String,
        path: String,
        name: String,
    },
    File {
        id: String,
        path: String,
        name: String,
        status: ProjectFileKind,
    },
}
impl ProjectTreeEntry {
    /// 返回仅在本会话内有效的能力标识。
    pub fn id(&self) -> &str {
        match self {
            Self::Directory { id, .. } | Self::File { id, .. } => id,
        }
    }
    /// 排序只处理可信结构中的展示名称，不把名称当文件系统输入。
    fn name(&self) -> &str {
        match self {
            Self::Directory { name, .. } | Self::File { name, .. } => name,
        }
    }
}

/// 每次响应只包含当前目录或搜索结果中的一页。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectTreePage {
    pub repository_id: String,
    pub snapshot_id: String,
    pub tree_id: String,
    pub directory_id: String,
    pub entries: Vec<ProjectTreeEntry>,
    pub next_offset: Option<usize>,
    pub total: usize,
}

/// 元数据索引首次使用树接口时建立，之后只在当前文件会话内复用。
pub(super) struct TreeIndex {
    id: String,
    children: BTreeMap<String, Vec<ProjectTreeEntry>>,
}
impl TreeIndex {
    /// 从已核验的文件清单构造层级，不访问仓库外路径或读取正文。
    fn new(files: &[(String, String, ProjectFileKind)]) -> Self {
        let id = next_id();
        let mut directories = BTreeMap::from([(String::new(), id.clone())]);
        let mut children = BTreeMap::from([(id.clone(), Vec::new())]);
        for (file_id, path, status) in files {
            let mut parts = path.split('/').peekable();
            let mut parent = id.clone();
            let mut prefix = String::new();
            while let Some(name) = parts.next() {
                if !prefix.is_empty() {
                    prefix.push('/');
                }
                prefix.push_str(name);
                if parts.peek().is_none() {
                    children
                        .entry(parent.clone())
                        .or_default()
                        .push(ProjectTreeEntry::File {
                            id: file_id.clone(),
                            path: path.clone(),
                            name: name.to_owned(),
                            status: *status,
                        });
                } else {
                    let directory = directories.entry(prefix.clone()).or_insert_with(|| {
                        let child = next_id();
                        children.entry(parent.clone()).or_default().push(
                            ProjectTreeEntry::Directory {
                                id: child.clone(),
                                path: prefix.clone(),
                                name: name.to_owned(),
                            },
                        );
                        child
                    });
                    parent = directory.clone();
                    children.entry(parent.clone()).or_default();
                }
            }
        }
        for entries in children.values_mut() {
            entries.sort_by(|left, right| {
                let left_file = matches!(left, ProjectTreeEntry::File { .. });
                let right_file = matches!(right, ProjectTreeEntry::File { .. });
                left_file
                    .cmp(&right_file)
                    .then_with(|| left.name().cmp(right.name()))
            });
        }
        Self { id, children }
    }
}

impl ProjectFilesSession {
    /// 读取目录能力索引；旧平面接口不承担未使用树的构造成本。
    fn tree_index(&self) -> &TreeIndex {
        self.tree.get_or_init(|| TreeIndex::new(&self.files))
    }

    /// 建立树后只返回根目录第一页，不携带后代文件列表。
    pub fn tree_root(&self) -> ProjectTreePage {
        let tree = self.tree_index();
        self.make_tree_page(&tree.id, &tree.children[&tree.id], 0)
    }

    /// 仅接受当前会话签发目录能力；伪造路径和跨会话 ID 均不能寻址。
    pub fn tree_page(
        &self,
        tree_id: &str,
        directory_id: &str,
        offset: usize,
    ) -> Result<ProjectTreePage, OperationError> {
        let tree = self.tree_index();
        if tree_id != tree.id {
            return Err(OperationError::new("STALE_REQUEST"));
        }
        let entries = tree
            .children
            .get(directory_id)
            .ok_or_else(|| OperationError::new("FILE_UNAVAILABLE"))?;
        if offset > entries.len() {
            return Err(OperationError::new("INVALID_INPUT"));
        }
        Ok(self.make_tree_page(directory_id, entries, offset))
    }

    /// 按完整相对路径搜索，结果复用文件 ID 且响应大小保持分页上限。
    pub fn tree_search(
        &self,
        tree_id: &str,
        query: &str,
        offset: usize,
    ) -> Result<ProjectTreePage, OperationError> {
        let tree = self.tree_index();
        if tree_id != tree.id {
            return Err(OperationError::new("STALE_REQUEST"));
        }
        if query.chars().count() > 256 {
            return Err(OperationError::new("INVALID_INPUT"));
        }
        let query = query.trim().to_lowercase();
        let entries: Vec<_> = self
            .files
            .iter()
            .filter(|(_, path, _)| path.to_lowercase().contains(&query))
            .map(|(id, path, status)| ProjectTreeEntry::File {
                id: id.clone(),
                path: path.clone(),
                name: path.rsplit('/').next().unwrap_or(path).to_owned(),
                status: *status,
            })
            .collect();
        if offset > entries.len() {
            return Err(OperationError::new("INVALID_INPUT"));
        }
        Ok(self.make_tree_page(&tree.id, &entries, offset))
    }

    /// 统一分页元数据，nextOffset 只在仍有条目时给出。
    fn make_tree_page(
        &self,
        directory_id: &str,
        entries: &[ProjectTreeEntry],
        offset: usize,
    ) -> ProjectTreePage {
        let end = (offset + PAGE_SIZE).min(entries.len());
        ProjectTreePage {
            repository_id: self.repository.id.clone(),
            snapshot_id: self.state.snapshot_id.clone(),
            tree_id: self.tree_index().id.clone(),
            directory_id: directory_id.to_owned(),
            entries: entries[offset..end].to_vec(),
            next_offset: (end < entries.len()).then_some(end),
            total: entries.len(),
        }
    }
}
