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
        let callback: Arc<dyn Fn(bool) + Send + Sync> = Arc::new(callback);
        let event_callback = Arc::clone(&callback);
        let mut watcher =
            notify::recommended_watcher(move |result: notify::Result<Event>| match result {
                Err(_) => event_callback(false),
                Ok(event) => {
                    if let Some(reliable) = classify_event(&event) {
                        event_callback(reliable);
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
