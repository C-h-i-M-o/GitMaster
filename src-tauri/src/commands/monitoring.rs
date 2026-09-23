use super::*;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::{
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
use tauri::Emitter;

/// 只传递失效版本，不把路径、文件内容或 Git 命令暴露到事件中。
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchState {
    repository_id: String,
    revision: u64,
    reliable: bool,
}

/// 监听生命周期绑定仓库会话；回调不持有会话锁、不运行 Git。
pub(super) struct RepositoryMonitor {
    repository_id: String,
    revision: Arc<AtomicU64>,
    reliable: Arc<AtomicBool>,
    _watcher: Option<git::watcher::RepositoryWatcher>,
}

impl RepositoryMonitor {
    /// 监听失败不阻止打开项目，前端收到不可靠状态后使用低频核实。
    pub(super) fn new(handle: &RepositoryHandle, app: tauri::AppHandle) -> Self {
        let revision = Arc::new(AtomicU64::new(0));
        let reliable = Arc::new(AtomicBool::new(true));
        let event_revision = Arc::clone(&revision);
        let event_reliable = Arc::clone(&reliable);
        let repository_id = handle.id.clone();
        let event_id = repository_id.clone();
        let (sender, receiver) = mpsc::sync_channel(1);
        // 容量为一的通知只表示“有变化”；版本存在原子计数器中，不因事件洪泛丢失。
        let worker_revision = Arc::clone(&revision);
        let worker_reliable = Arc::clone(&reliable);
        let worker = thread::Builder::new()
            .name("repository-watch".into())
            .spawn(move || {
                while receiver.recv().is_ok() {
                    let deadline = Instant::now() + Duration::from_millis(500);
                    while Instant::now() < deadline {
                        let wait = Duration::from_millis(100)
                            .min(deadline.saturating_duration_since(Instant::now()));
                        if receiver.recv_timeout(wait).is_err() {
                            break;
                        }
                    }
                    let _ = app.emit(
                        "repository-invalidated",
                        WatchState {
                            repository_id: event_id.clone(),
                            revision: worker_revision.load(Ordering::Acquire),
                            reliable: worker_reliable.load(Ordering::Acquire),
                        },
                    );
                }
            });
        if worker.is_err() {
            reliable.store(false, Ordering::Release);
        }
        let watcher = git::watcher::RepositoryWatcher::new(
            &handle.root,
            &handle.git_dir,
            &handle.common_dir,
            move |is_reliable| {
                if !is_reliable {
                    if event_reliable.swap(false, Ordering::AcqRel) {
                        log::warn!(target: "gitmaster::diagnostic", "文件监听失效，启用低频核实");
                    }
                }
                event_revision.fetch_add(1, Ordering::AcqRel);
                let _ = sender.try_send(());
            },
        )
        .ok();
        if watcher.is_none() {
            log::warn!(target: "gitmaster::diagnostic", "文件监听初始化失败，启用低频核实");
            reliable.store(false, Ordering::Release);
        }
        Self {
            repository_id,
            revision,
            reliable,
            _watcher: watcher,
        }
    }

    /// 焦点核实只读取原子版本，不启动 Git 或扫描文件系统。
    fn state(&self) -> WatchState {
        WatchState {
            repository_id: self.repository_id.clone(),
            revision: self.revision.load(Ordering::Acquire),
            reliable: self.reliable.load(Ordering::Acquire),
        }
    }
}

/// 读取当前项目监听状态，拒绝已切换项目的迟到请求。
#[tauri::command]
pub fn read_repository_watch(
    state: State<'_, DesktopState>,
    repository_id: String,
) -> Result<WatchState, OperationError> {
    let session = state.lock()?;
    session
        .monitor
        .as_ref()
        .filter(|monitor| monitor.repository_id == repository_id)
        .filter(|_| {
            session
                .active
                .as_ref()
                .is_some_and(|(handle, _)| handle.id == repository_id)
        })
        .map(RepositoryMonitor::state)
        .ok_or_else(|| OperationError::new("STALE_REQUEST"))
}
