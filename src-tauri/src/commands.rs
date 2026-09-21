use crate::settings;
use gitmaster_core::git::{
    self,
    environment::{describe_git, resolve_git},
    *,
};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
};
use tauri::{Manager, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

/// 当前环境和工作树会话，不持久化仓库文件或凭据。
#[derive(Default)]
pub struct Session {
    environment_request: u64,
    repository_request: u64,
    epoch: u64,
    git: Option<GitExecutable>,
    active: Option<(RepositoryHandle, RepositoryState)>,
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
    fn install(&mut self, git: Option<GitExecutable>, force: bool) {
        let changed = force
            || self.git.as_ref().map(|g| (&g.path, &g.version))
                != git.as_ref().map(|g| (&g.path, &g.version));
        if changed {
            self.epoch += 1;
            self.repository_request += 1;
            self.active = None;
        }
        self.git = git;
    }
    /// 仅接受当前环境下最后一次仓库读取的结果。
    fn check_repository(&self, epoch: u64, token: u64) -> Result<(), OperationError> {
        if self.epoch != epoch || self.repository_request != token {
            Err(OperationError::new("STALE_REQUEST"))
        } else {
            Ok(())
        }
    }
}

/// 在工作线程执行阻塞核心调用，避免阻塞桌面事件循环。
async fn blocking<T: Send + 'static>(
    job: impl FnOnce() -> Result<T, OperationError> + Send + 'static,
) -> Result<T, OperationError> {
    tauri::async_runtime::spawn_blocking(job)
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
    blocking(move || {
        let result =
            settings::load(&path).and_then(|s| resolve_git(s.git_path.as_deref().map(Path::new)));
        let mut session = shared.lock()?;
        session.check_environment(token)?;
        match result {
            Ok(git) => {
                let environment = describe_git(&git)?;
                session.install(Some(git), false);
                Ok(environment)
            }
            Err(error) => {
                session.install(None, false);
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
    blocking(move || {
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
        settings::save(&config, saved)?;
        session.install(result.ok(), true);
        Ok(environment)
    })
    .await
}

/// 识别并打开已选择工作区，只在完整成功后替换活动仓库。
#[tauri::command]
pub async fn open_repository(
    state: State<'_, DesktopState>,
    path: String,
) -> Result<RepositoryState, OperationError> {
    let shared = state.inner().clone();
    let (git, epoch, token) = {
        let mut s = shared.lock()?;
        let git = s
            .git
            .clone()
            .ok_or_else(|| OperationError::new("GIT_NOT_FOUND"))?;
        s.repository_request += 1;
        (git, s.epoch, s.repository_request)
    };
    blocking(move || {
        let (handle, snapshot) = git::repository::open_repository(&git, Path::new(&path))?;
        let mut s = shared.lock()?;
        s.check_repository(epoch, token)?;
        s.active = Some((handle, snapshot.clone()));
        Ok(snapshot)
    })
    .await
}

/// 刷新当前活动工作树；旧请求不得覆盖新项目。
#[tauri::command]
pub async fn read_repository_state(
    state: State<'_, DesktopState>,
    repository_id: String,
) -> Result<RepositoryState, OperationError> {
    let shared = state.inner().clone();
    let (git, handle, epoch, token) = {
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
        s.repository_request += 1;
        (git, handle, s.epoch, s.repository_request)
    };
    blocking(move || {
        let snapshot = git::repository::read_repository_state(&git, &handle)?;
        let mut s = shared.lock()?;
        s.check_repository(epoch, token)?;
        s.active = Some((handle, snapshot.clone()));
        Ok(snapshot)
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
) -> Result<FileDiff, OperationError> {
    let shared = state.inner().clone();
    let (git, handle, snapshot, epoch, token) = {
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
        )
    };
    blocking(move || {
        let diff = git::diff::read_file_diff(&git, &handle, &snapshot, &change_id, side)?;
        shared.lock()?.check_repository(epoch, token)?;
        Ok(diff)
    })
    .await
}

/// 打开原生 Git 文件选择器，取消返回 null。
#[tauri::command]
pub async fn choose_git_path(app: tauri::AppHandle) -> Result<Option<String>, OperationError> {
    blocking(move || {
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
    blocking(move || {
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
    blocking(move || {
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
        s.install(None, true);
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
        s.install(Some(g.clone()), false);
        let epoch = s.epoch;
        s.install(Some(g.clone()), false);
        assert_eq!(s.epoch, epoch);
        s.install(Some(g), true);
        assert!(s.epoch > epoch);
    }
}
