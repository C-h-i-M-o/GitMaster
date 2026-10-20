use crate::git::OperationError;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

/// 持有文件系统监听器；释放对象时由 notify 自动停止后台监听。
pub struct RepositoryWatcher {
    _watcher: RecommendedWatcher,
}

/// 事件只携带变化域；上层使用单调版本合并，不把工作区路径发送给前端。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RepositoryChange {
    pub reliable: bool,
    pub history: bool,
    pub files: bool,
    pub documents: bool,
}

/// 未知事件保守覆盖全部读取域，普通正文修改不污染历史和目录版本。
fn classify_change(event: &Event, git_dir: &Path, common_dir: &Path) -> Option<RepositoryChange> {
    let reliable = classify_event(event)?;
    if !reliable || matches!(event.kind, EventKind::Any | EventKind::Other) {
        return Some(RepositoryChange {
            reliable: false,
            history: true,
            files: true,
            documents: true,
        });
    }
    let mut change = RepositoryChange {
        reliable: true,
        ..Default::default()
    };
    let mut relevant = false;
    for path in &event.paths {
        let path = dunce::simplified(path);
        if let Some(relative) = path
            .strip_prefix(dunce::simplified(git_dir))
            .ok()
            .or_else(|| path.strip_prefix(dunce::simplified(common_dir)).ok())
        {
            // 锁文件与对象写入本身不改变展示；实际引用和索引事件负责通知。
            let name = relative.to_string_lossy().replace('\\', "/");
            if name.ends_with(".lock") || name.starts_with("objects/") || name.starts_with("logs/")
            {
                continue;
            }
            relevant = true;
            if name == "HEAD"
                || name == "packed-refs"
                || name.starts_with("refs/")
                || name.starts_with("worktrees/")
                || name == "config"
                || name == "config.worktree"
                || name == "shallow"
            {
                change.history = true;
            }
            // HEAD 变化可能切换工作树，保守使目录和正文版本前进。
            if name == "HEAD" {
                change.files = true;
                change.documents = true;
            }
        } else {
            relevant = true;
            change.documents = true;
            change.files |= matches!(
                event.kind,
                EventKind::Create(_)
                    | EventKind::Remove(_)
                    | EventKind::Modify(notify::event::ModifyKind::Name(_))
            );
            // Windows 将正文/属性修改统一报为 Any；名称变化另有独立事件。
            change.files |= !cfg!(windows)
                && matches!(
                    event.kind,
                    EventKind::Modify(notify::event::ModifyKind::Any)
                );
            if path.file_name().is_some_and(|name| name == ".git") {
                change.history = true;
                change.files = true;
            }
        }
    }
    relevant.then_some(change)
}

/// 将 notify 事件归类为可靠变更、需要完整核实或可忽略事件。
fn classify_event(event: &Event) -> Option<bool> {
    if event.need_rescan() {
        return Some(false);
    }
    if matches!(event.kind, EventKind::Access(_)) {
        return None;
    }
    Some(!event.paths.is_empty())
}

impl RepositoryWatcher {
    /// 监听工作树、Git 目录和公共 Git 目录，并把可靠性变化交给调用方。
    pub fn new<F>(
        root: &Path,
        git_dir: &Path,
        common_dir: &Path,
        callback: F,
    ) -> Result<Self, OperationError>
    where
        F: Fn(bool) + Send + Sync + 'static,
    {
        Self::new_scoped(root, git_dir, common_dir, move |change| {
            callback(change.reliable)
        })
    }

    /// 按变化域监听同一组真实目录，保持旧可靠性接口的兼容性。
    pub fn new_scoped<F>(
        root: &Path,
        git_dir: &Path,
        common_dir: &Path,
        callback: F,
    ) -> Result<Self, OperationError>
    where
        F: Fn(RepositoryChange) + Send + Sync + 'static,
    {
        let callback: Arc<dyn Fn(RepositoryChange) + Send + Sync> = Arc::new(callback);
        let event_callback = Arc::clone(&callback);
        let event_git =
            std::fs::canonicalize(git_dir).map_err(|_| OperationError::new("FILE_UNAVAILABLE"))?;
        let event_common = std::fs::canonicalize(common_dir)
            .map_err(|_| OperationError::new("FILE_UNAVAILABLE"))?;
        let mut watcher =
            notify::recommended_watcher(move |result: notify::Result<Event>| match result {
                Err(_) => event_callback(RepositoryChange {
                    reliable: false,
                    history: true,
                    files: true,
                    documents: true,
                }),
                Ok(event) => {
                    if let Some(change) = classify_change(&event, &event_git, &event_common) {
                        event_callback(change);
                    }
                }
            })
            .map_err(|_| OperationError::new("FILE_UNAVAILABLE"))?;

        let mut watched = Vec::new();
        for path in [root, git_dir, common_dir] {
            let path =
                std::fs::canonicalize(path).map_err(|_| OperationError::new("FILE_UNAVAILABLE"))?;
            if !watched
                .iter()
                .any(|parent: &PathBuf| path.starts_with(parent))
            {
                watcher
                    .watch(&path, RecursiveMode::Recursive)
                    .map_err(|_| OperationError::new("FILE_UNAVAILABLE"))?;
                watched.push(path);
            }
        }
        Ok(Self { _watcher: watcher })
    }
}

impl std::fmt::Debug for RepositoryWatcher {
    /// 只显示监听器类型，避免暴露路径或平台句柄。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("RepositoryWatcher").finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::mpsc, time::Duration};

    /// 正文变化只影响文档，引用与目录拓扑使用独立版本。
    #[test]
    fn content_changes_preserve_history_and_directory_domains() {
        let root = Path::new("repo");
        let git = root.join(".git");
        let event = Event::new(EventKind::Modify(notify::event::ModifyKind::Data(
            notify::event::DataChange::Content,
        )))
        .add_path(root.join("file.txt"));
        let change = classify_change(&event, &git, &git).unwrap();
        assert!(change.documents && change.reliable);
        assert!(!change.history && !change.files);
        let modified = Event::new(EventKind::Modify(notify::event::ModifyKind::Any))
            .add_path(root.join("file.txt"));
        let changed = classify_change(&modified, &git, &git).unwrap();
        assert!(changed.documents && !changed.history);
        assert_eq!(changed.files, !cfg!(windows));
        let created = Event::new(EventKind::Create(notify::event::CreateKind::File))
            .add_path(root.join("new.txt"));
        assert!(classify_change(&created, &git, &git).unwrap().files);
        let head = Event::new(EventKind::Modify(notify::event::ModifyKind::Any))
            .add_path(git.join("HEAD"));
        assert!(classify_change(&head, &git, &git).unwrap().history);
        let index = Event::new(EventKind::Modify(notify::event::ModifyKind::Any))
            .add_path(git.join("index"));
        assert_eq!(
            classify_change(&index, &git, &git),
            Some(RepositoryChange {
                reliable: true,
                ..Default::default()
            })
        );
        let lock = Event::new(EventKind::Create(notify::event::CreateKind::File))
            .add_path(git.join("index.lock"));
        assert_eq!(classify_change(&lock, &git, &git), None);
    }

    /// 工作树文件变化应触发可靠变更通知。
    #[test]
    fn file_change_triggers_callback() {
        let directory = tempfile::tempdir().unwrap();
        let git = directory.path().join(".git");
        std::fs::create_dir_all(&git).unwrap();
        let (sender, receiver) = mpsc::channel();
        let _watcher = RepositoryWatcher::new(directory.path(), &git, &git, move |reliable| {
            let _ = sender.send(reliable);
        })
        .unwrap();
        std::fs::write(directory.path().join("tracked.txt"), b"changed").unwrap();
        assert!(receiver.recv_timeout(Duration::from_secs(2)).unwrap());
    }

    /// Access 事件不应被当成仓库内容变化通知。
    #[test]
    fn access_event_is_ignored() {
        let event = Event::new(EventKind::Access(notify::event::AccessKind::Read));
        assert_eq!(classify_event(&event), None);
    }

    /// rescan 事件要求调用方完整核实仓库状态。
    #[test]
    fn rescan_event_is_unreliable() {
        let mut event = Event::new(EventKind::Other);
        event.attrs.set_flag(notify::event::Flag::Rescan);
        assert_eq!(classify_event(&event), Some(false));
    }

    /// 没有路径的普通事件不能被视为已可靠定位的变更。
    #[test]
    fn empty_event_is_unreliable() {
        let event = Event::new(EventKind::Modify(notify::event::ModifyKind::Any));
        assert_eq!(classify_event(&event), Some(false));
    }

    /// 无效监听目录应在构造阶段返回稳定错误。
    #[test]
    fn invalid_directory_returns_error() {
        let missing = Path::new("this-directory-does-not-exist");
        let result = RepositoryWatcher::new(missing, missing, missing, |_| {});
        assert_eq!(result.unwrap_err().code, "FILE_UNAVAILABLE");
    }
}
