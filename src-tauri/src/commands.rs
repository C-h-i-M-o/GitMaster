use crate::settings;
use gitmaster_core::git::{
    self,
    environment::{describe_git, resolve_git},
    *,
};
mod diff;
mod external;
mod monitoring;
mod operations;
mod preferences;
mod readonly;
mod resources;
mod terminal;

pub use diff::*;
pub use monitoring::*;
pub use operations::*;
pub use preferences::*;
pub use readonly::*;
pub use resources::*;
pub use terminal::*;

use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
};
use tauri::{Manager, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

/// 仓库状态与各读取域的监听版本，供前端按域复用缓存。
#[derive(Debug, Clone, Copy, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryVersions {
    pub history: u64,
    pub files: u64,
    pub documents: u64,
}

/// 桌面层返回的仓库快照，兼容核心状态字段并补充变化域版本。
#[derive(Debug, Clone, serde::Serialize)]
pub struct RepositorySnapshot {
    #[serde(flatten)]
    pub state: RepositoryState,
    pub versions: RepositoryVersions,
}

/// 当前环境和工作树会话，不持久化仓库文件或凭据。
#[derive(Default)]
pub struct Session {
    environment_request: u64,
    repository_request: u64,
    /// 只读能力生命周期；普通状态刷新不递增，避免无关读取失效。
    readonly_request: u64,
    /// 上一次已消费的监听域版本。
    watch_seen: RepositoryVersions,
    /// 对外签发的域版本。
    versions: RepositoryVersions,
    force_refresh: bool,
    epoch: u64,
    git: Option<GitExecutable>,
    active: Option<(RepositoryHandle, RepositoryState)>,
    coordinator: git::coordinator::RepositoryCoordinator,
    history: Option<Arc<Mutex<readonly::HistoryCache>>>,
    branches: Option<Arc<git::branches::BranchSession>>,
    project_files: Option<Arc<git::files::ProjectFilesSession>>,
    /// 编辑文档能力不依赖目录刷新，仅保留单文件映射及最近使用顺序。
    editor_files: VecDeque<(String, Arc<git::files::ProjectFilesSession>)>,
    history_request: u64,
    branches_request: u64,
    project_files_request: u64,
    diff_request: u64,
    diff: Option<Arc<diff::DiffCache>>,
    prepare_gate: Arc<Mutex<()>>,
    write_request: u64,
    preview: Option<operations::PreviewBinding>,
    executed: VecDeque<(String, OperationHandle)>,
    /// 已执行写任务与其可消费的仓库代次，防止旧终态覆盖新状态。
    accepted_operations: VecDeque<(String, u64, u64, RepositoryVersions)>,
    remotes: Option<Arc<git::remote::RemoteSession>>,
    previous_remotes: Option<Arc<git::remote::RemoteSession>>,
    conflicts: Option<Arc<git::conflicts::ConflictSession>>,
    remotes_request: u64,
    conflicts_request: u64,
    clone_parent: Option<(String, PathBuf)>,
    picker_request: u64,
    monitor: Option<monitoring::RepositoryMonitor>,
}
/// 在异步阻塞任务间共享会话；Git 子进程期间不持有锁。
#[derive(Clone, Default)]
pub struct DesktopState(pub Arc<Mutex<Session>>);
impl DesktopState {
    /// 将中毒锁转为结构化错误，避免 IPC panic。
    fn lock(&self) -> Result<MutexGuard<'_, Session>, OperationError> {
        self.0
            .lock()
            .map_err(|_| OperationError::new("GIT_EXECUTION_FAILED"))
    }
}
impl Session {
    /// 为新的环境请求分配代次。
    fn begin_environment(&mut self) -> u64 {
        self.environment_request += 1;
        self.environment_request
    }
    /// 确认环境请求未被后来的请求取代。
    fn check_environment(&self, token: u64) -> Result<(), OperationError> {
        if self.environment_request != token {
            Err(OperationError::new("STALE_REQUEST"))
        } else {
            Ok(())
        }
    }
    /// 安装已验证环境；实际改变时使旧仓库和在途请求失效。
    fn install(&mut self, git: Option<GitExecutable>, force: bool) -> Result<(), OperationError> {
        let changed = force
            || self.git.as_ref().map(|g| (&g.path, &g.version))
                != git.as_ref().map(|g| (&g.path, &g.version));
        if changed {
            self.coordinator.invalidate()?;
            self.epoch += 1;
            self.repository_request += 1;
            self.active = None;
            self.editor_files.clear();
            self.monitor = None;
            self.clear_readonly();
            self.previous_remotes = None;
            self.clone_parent = None;
            self.picker_request += 1;
        }
        self.git = git;
        Ok(())
    }
    /// 环境或仓库刷新使全部模块缓存与在途初始化失效。
    fn clear_readonly(&mut self) {
        self.clear_readonly_full(true);
    }
    /// 清理全部只读槽；发布强制刷新时可跳过重复的对外版本递增。
    fn clear_readonly_full(&mut self, bump_versions: bool) {
        self.readonly_request += 1;
        self.diff_request += 1;
        self.diff = None;
        self.history_request += 1;
        self.branches_request += 1;
        self.project_files_request += 1;
        self.history = None;
        self.branches = None;
        self.project_files = None;
        self.remotes_request += 1;
        self.conflicts_request += 1;
        if let Some(remote) = self.remotes.take() {
            self.previous_remotes = Some(remote);
        }
        self.conflicts = None;
        self.write_request += 1;
        self.preview = None;
        if bump_versions {
            self.versions.history += 1;
            self.versions.files += 1;
            self.versions.documents += 1;
        }
    }
    /// 开始打开或刷新仓库；活动写任务期间不能作废正在使用的会话。
    fn begin_repository_request(&mut self) -> Result<u64, OperationError> {
        self.begin_repository_request_with(true)
    }
    /// 开始仓库请求；强制请求才清理历史、文件和差异等只读能力。
    fn begin_repository_request_with(&mut self, force: bool) -> Result<u64, OperationError> {
        self.coordinator.invalidate()?;
        self.repository_request += 1;
        if force {
            self.clear_readonly();
        }
        self.force_refresh = force;
        Ok(self.repository_request)
    }
    /// 发布完整仓库状态，同时拒绝刷新期间捕获旧快照的后台结果。
    fn publish_repository(
        &mut self,
        epoch: u64,
        token: u64,
        handle: RepositoryHandle,
        snapshot: RepositoryState,
    ) -> Result<(), OperationError> {
        self.check_repository(epoch, token)?;
        self.editor_files.clear();
        if self.force_refresh {
            self.clear_readonly_full(false);
            self.force_refresh = false;
        }
        self.repository_request += 1;
        self.active = Some((handle, snapshot));
        Ok(())
    }
    /// 发布普通刷新结果，并按监听版本只清理已发生变化的读取域。
    fn publish_repository_with_domains(
        &mut self,
        epoch: u64,
        token: u64,
        handle: RepositoryHandle,
        snapshot: RepositoryState,
        start: RepositoryVersions,
    ) -> Result<(), OperationError> {
        self.check_repository(epoch, token)?;
        if self.force_refresh {
            self.clear_readonly_full(false);
            self.force_refresh = false;
            self.watch_seen = start;
        }
        let history = start.history != self.watch_seen.history;
        let files = start.files != self.watch_seen.files;
        let documents = start.documents != self.watch_seen.documents;
        let head_changed = self
            .active
            .as_ref()
            .is_some_and(|(_, previous)| previous.head != snapshot.head);
        let state_changed = self.active.as_ref().is_some_and(|(_, previous)| {
            previous.operations != snapshot.operations || previous.changes != snapshot.changes
        });
        self.watch_seen = start;
        if history || files || documents || head_changed || state_changed {
            self.clear_readonly_domains(
                history || head_changed,
                files || head_changed,
                documents || head_changed || state_changed,
            );
        }
        self.repository_request += 1;
        self.active = Some((handle, snapshot));
        Ok(())
    }
    /// 按变化域失效缓存；文档变化同时清理差异等依赖正文的能力。
    fn clear_readonly_domains(&mut self, history: bool, files: bool, documents: bool) {
        if history {
            self.history_request += 1;
            self.branches_request += 1;
            self.remotes_request += 1;
            self.history = None;
            self.branches = None;
            if let Some(remote) = self.remotes.take() {
                self.previous_remotes = Some(remote);
            }
        }
        if files {
            self.project_files_request += 1;
            self.project_files = None;
        }
        if documents {
            self.diff_request += 1;
            self.diff = None;
            self.conflicts_request += 1;
            self.conflicts = None;
            self.write_request += 1;
            self.preview = None;
        }
        if history {
            self.versions.history += 1;
        }
        if files {
            self.versions.files += 1;
        }
        if documents {
            self.versions.documents += 1;
        }
    }
    /// 仅接受当前环境下最后一次仓库读取的结果。
    fn check_repository(&self, epoch: u64, token: u64) -> Result<(), OperationError> {
        if self.epoch != epoch || self.repository_request != token {
            Err(OperationError::new("STALE_REQUEST"))
        } else {
            Ok(())
        }
    }

    /// 安装写任务返回的核实快照，并同步推进桌面仓库代次。
    fn accept_operation_state(
        &mut self,
        repository_id: &str,
        operation_id: &str,
        snapshot: RepositoryState,
        epoch: u64,
        token: u64,
    ) -> Result<RepositorySnapshot, OperationError> {
        if self.epoch != epoch
            || self.repository_request != token
            || self
                .active
                .as_ref()
                .is_none_or(|(handle, _)| handle.id != repository_id)
            || snapshot.repository_id != repository_id
        {
            return Err(OperationError::new("STALE_REQUEST"));
        }
        let Some((_, _, _, start_versions)) = self
            .accepted_operations
            .iter()
            .find(|(id, e, t, _)| id == operation_id && *e == epoch && *t == token)
            .cloned()
        else {
            return Err(OperationError::new("STALE_REQUEST"));
        };
        self.accepted_operations
            .retain(|(id, _, _, _)| id != operation_id);
        let (handle, _) = self
            .active
            .clone()
            .ok_or_else(|| OperationError::new("STALE_REQUEST"))?;
        self.publish_repository_with_domains(
            epoch,
            token,
            handle,
            snapshot.clone(),
            start_versions,
        )?;
        Ok(RepositorySnapshot {
            state: snapshot,
            versions: self.versions,
        })
    }
}

/// 在工作线程执行阻塞核心调用，避免阻塞桌面事件循环。
async fn blocking<T: Send + 'static>(
    stage: &'static str,
    job: impl FnOnce() -> Result<T, OperationError> + Send + 'static,
) -> Result<T, OperationError> {
    tauri::async_runtime::spawn_blocking(move || gitmaster_core::diagnostics::measure(stage, job))
        .await
        .map_err(|_| OperationError::new("GIT_EXECUTION_FAILED"))?
}

/// 定位应用自己的配置文件，不读取前端提供的设置路径。
fn settings_path(app: &tauri::AppHandle) -> Result<PathBuf, OperationError> {
    app.path()
        .app_config_dir()
        .map(|p| p.join("settings.json"))
        .map_err(|_| OperationError::new("SETTINGS_IO"))
}

/// 按保存设置重新检测 Git；检测失败显示不可用而不伪造路径。
#[tauri::command]
pub async fn detect_git(
    app: tauri::AppHandle,
    state: State<'_, DesktopState>,
) -> Result<GitEnvironment, OperationError> {
    let path = settings_path(&app)?;
    let shared = state.inner().clone();
    let token = shared.lock()?.begin_environment();
    blocking("detect_git", move || {
        let result =
            settings::load(&path).and_then(|s| resolve_git(s.git_path.as_deref().map(Path::new)));
        let mut session = shared.lock()?;
        session.check_environment(token)?;
        match result {
            Ok(git) => {
                let environment = describe_git(&git)?;
                session.install(Some(git), false)?;
                Ok(environment)
            }
            Err(error) => {
                session.install(None, false)?;
                Ok(GitEnvironment::Unavailable { error })
            }
        }
    })
    .await
}

/// 验证成功后才保存指定 Git；null 明确恢复自动检测。
#[tauri::command]
pub async fn set_git_path(
    app: tauri::AppHandle,
    state: State<'_, DesktopState>,
    path: Option<String>,
) -> Result<GitEnvironment, OperationError> {
    let config = settings_path(&app)?;
    let shared = state.inner().clone();
    let token = shared.lock()?.begin_environment();
    blocking("set_git_path", move || {
        let result = resolve_git(path.as_deref().map(Path::new));
        if path.is_some() {
            if let Err(error) = &result {
                return Err(error.clone());
            }
        }
        let environment = match &result {
            Ok(git) => describe_git(git)?,
            Err(error) => GitEnvironment::Unavailable {
                error: error.clone(),
            },
        };
        let mut session = shared.lock()?;
        session.check_environment(token)?;
        let saved = if path.is_some() {
            result
                .as_ref()
                .ok()
                .and_then(|g| g.path.to_str().map(str::to_owned))
        } else {
            None
        };
        session.coordinator.invalidate()?;
        settings::save(&config, saved)?;
        session.install(result.ok(), true)?;
        Ok(environment)
    })
    .await
}

/// 识别并打开已选择工作区，只在完整成功后替换活动仓库。
#[tauri::command]
pub async fn open_repository(
    app: tauri::AppHandle,
    state: State<'_, DesktopState>,
    path: String,
) -> Result<RepositorySnapshot, OperationError> {
    let shared = state.inner().clone();
    let (git, epoch, token) = {
        let mut s = shared.lock()?;
        let git = s
            .git
            .clone()
            .ok_or_else(|| OperationError::new("GIT_NOT_FOUND"))?;
        s.begin_repository_request()?;
        (git, s.epoch, s.repository_request)
    };
    blocking("open_repository", move || {
        let handle = git::repository::identify_repository(&git, Path::new(&path))?;
        let monitor = monitoring::RepositoryMonitor::new(&handle, app.clone());
        let watch_seen = monitor.versions();
        let snapshot = git::repository::read_repository_state(&git, &handle)?;
        let mut s = shared.lock()?;
        s.publish_repository(epoch, token, handle, snapshot.clone())?;
        s.monitor = Some(monitor);
        s.watch_seen = watch_seen;
        let versions = s.versions;
        drop(s);
        if let Err(error) =
            crate::recent_projects::record_opened_project(&app, Path::new(&snapshot.root_path))
        {
            crate::recent_projects::remember_error(Path::new(&snapshot.root_path), error);
        }
        Ok(RepositorySnapshot {
            state: snapshot,
            versions,
        })
    })
    .await
}

/// 刷新当前活动工作树；旧请求不得覆盖新项目。
#[tauri::command]
pub async fn read_repository_state(
    state: State<'_, DesktopState>,
    repository_id: String,
    force: Option<bool>,
) -> Result<RepositorySnapshot, OperationError> {
    let shared = state.inner().clone();
    let (git, handle, epoch, token, start_versions) = {
        let mut s = shared.lock()?;
        let git = s
            .git
            .clone()
            .ok_or_else(|| OperationError::new("GIT_NOT_FOUND"))?;
        let handle = s
            .active
            .as_ref()
            .filter(|(h, _)| h.id == repository_id)
            .map(|(h, _)| h.clone())
            .ok_or_else(|| OperationError::new("STALE_REQUEST"))?;
        let reliable = s.monitor.as_ref().is_some_and(|monitor| monitor.reliable());
        s.begin_repository_request_with(force.unwrap_or(true) || !reliable)?;
        let versions = s
            .monitor
            .as_ref()
            .map(|monitor| monitor.versions())
            .unwrap_or_default();
        (git, handle, s.epoch, s.repository_request, versions)
    };
    blocking("read_repository_state", move || {
        let coordinator = {
            let s = shared.lock()?;
            s.coordinator.clone()
        };
        let key = git::coordinator::CoordinationKey::repository(&handle)?;
        let snapshot = coordinator.read(&key, || {
            git::repository::read_repository_state(&git, &handle)
        })?;
        let mut s = shared.lock()?;
        s.publish_repository_with_domains(epoch, token, handle, snapshot.clone(), start_versions)?;
        let versions = s.versions;
        Ok(RepositorySnapshot {
            state: snapshot,
            versions,
        })
    })
    .await
}

/// 根据活动快照内的文件 ID 读取差异，前端不传任意文件路径。
#[tauri::command]
pub async fn read_file_diff(
    state: State<'_, DesktopState>,
    repository_id: String,
    snapshot_id: String,
    change_id: String,
    side: DiffSide,
    context_lines: Option<u16>,
) -> Result<FileDiff, OperationError> {
    let shared = state.inner().clone();
    let (git, handle, snapshot, epoch, token, coordinator) = {
        let s = shared.lock()?;
        let git = s
            .git
            .clone()
            .ok_or_else(|| OperationError::new("GIT_NOT_FOUND"))?;
        let (handle, snapshot) = s
            .active
            .as_ref()
            .filter(|(h, s)| h.id == repository_id && s.snapshot_id == snapshot_id)
            .ok_or_else(|| OperationError::new("STALE_REQUEST"))?;
        (
            git,
            handle.clone(),
            snapshot.clone(),
            s.epoch,
            s.repository_request,
            s.coordinator.clone(),
        )
    };
    blocking("read_file_diff", move || {
        let key = git::coordinator::CoordinationKey::repository(&handle)?;
        let diff = coordinator.read(&key, || {
            git::diff::read_file_diff_with_context(
                &git,
                &handle,
                &snapshot,
                &change_id,
                side,
                u32::from(context_lines.unwrap_or(3)),
            )
        })?;
        shared.lock()?.check_repository(epoch, token)?;
        Ok(diff)
    })
    .await
}

/// 打开原生 Git 文件选择器，取消返回 null。
#[tauri::command]
pub async fn choose_git_path(app: tauri::AppHandle) -> Result<Option<String>, OperationError> {
    blocking("choose_git_path", move || {
        app.dialog()
            .file()
            .set_title("选择系统 Git 可执行文件")
            .blocking_pick_file()
            .map(|p| {
                p.into_path()
                    .map_err(|_| OperationError::new("GIT_PATH_INVALID"))
                    .and_then(|p| {
                        p.to_str()
                            .map(str::to_owned)
                            .ok_or_else(|| OperationError::new("UNSUPPORTED_PATH_ENCODING"))
                    })
            })
            .transpose()
    })
    .await
}

/// 打开原生已有工作树目录选择器，取消不改变会话。
#[tauri::command]
pub async fn choose_repository_path(
    app: tauri::AppHandle,
) -> Result<Option<String>, OperationError> {
    blocking("choose_repository_path", move || {
        app.dialog()
            .file()
            .set_title("打开已有 Git 仓库")
            .blocking_pick_folder()
            .map(|p| {
                p.into_path()
                    .map_err(|_| OperationError::new("NOT_REPOSITORY"))
                    .and_then(|p| {
                        p.to_str()
                            .map(str::to_owned)
                            .ok_or_else(|| OperationError::new("UNSUPPORTED_PATH_ENCODING"))
                    })
            })
            .transpose()
    })
    .await
}

/// 只打开固定的官方安装页面，不接受任意 URL。
#[tauri::command]
pub async fn open_git_install_page(app: tauri::AppHandle) -> Result<(), OperationError> {
    blocking("open_git_install_page", move || {
        app.opener()
            .open_url("https://git-scm.com/install/", None::<&str>)
            .map_err(|_| OperationError::new("GIT_EXECUTION_FAILED"))
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    /// 旧环境检测不能覆盖后来设置，过期仓库响应不能覆盖新项目。
    #[test]
    fn rejects_old_generations() {
        let mut s = Session::default();
        let old = s.begin_environment();
        s.begin_environment();
        assert_eq!(s.check_environment(old).unwrap_err().code, "STALE_REQUEST");
        s.repository_request = 2;
        assert!(s.check_repository(0, 1).is_err());
        assert!(s.check_repository(0, 2).is_ok());
        s.install(None, true).unwrap();
        assert!(s.check_repository(0, 2).is_err());
    }
    /// 相同路径重新检测不使当前仓库无故失效，手动设置会作废。
    #[test]
    fn redetection_preserves_epoch() {
        let mut s = Session::default();
        let g = GitExecutable {
            path: "/git".into(),
            version: "2.49.0".into(),
            source: "path".into(),
        };
        s.install(Some(g.clone()), false).unwrap();
        let epoch = s.epoch;
        s.install(Some(g.clone()), false).unwrap();
        assert_eq!(s.epoch, epoch);
        s.install(Some(g), true).unwrap();
        assert!(s.epoch > epoch);
    }
}
