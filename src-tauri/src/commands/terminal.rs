//! 终端能力按窗口归属，启动参数只来自已保存 profile。
use super::{settings_path, DesktopState};
use crate::{
    settings::{self, TerminalDirectory},
    terminal::{TerminalChunk, TerminalRegistry, TerminalSession},
};
use gitmaster_core::git::OperationError;
use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};
use tauri::{Manager, State};

/// 全部会话由桌面应用管理，Git 核心不依赖其生命周期。
#[derive(Default)]
pub struct TerminalState(pub Mutex<TerminalRegistry>);

/// 校验已有绝对程序路径，不通过启动程序进行检测。
fn executable(path: &Path) -> bool {
    if !path.is_absolute() || !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        return path
            .metadata()
            .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false);
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// 默认 shell 只在用户创建终端时解析，检测过程不运行命令。
fn default_shell() -> Result<PathBuf, OperationError> {
    let mut candidates = Vec::new();
    #[cfg(unix)]
    {
        if let Some(shell) = std::env::var_os("SHELL") {
            candidates.push(PathBuf::from(shell));
        }
        candidates.extend([
            PathBuf::from("/bin/zsh"),
            PathBuf::from("/bin/bash"),
            PathBuf::from("/bin/sh"),
        ]);
    }
    #[cfg(windows)]
    {
        if let Some(path) = std::env::var_os("PATH") {
            candidates.extend(
                std::env::split_paths(&path)
                    .filter(|path| path.is_absolute())
                    .map(|path| path.join("pwsh.exe")),
            );
        }
        if let Some(root) = std::env::var_os("SystemRoot") {
            candidates
                .push(PathBuf::from(root).join("System32/WindowsPowerShell/v1.0/powershell.exe"));
        }
    }
    candidates
        .into_iter()
        .find(|path| executable(path))
        .ok_or_else(|| OperationError::new("TERMINAL_SHELL"))
}

/// 读取已保存配置并绑定创建时项目，前端不能提交程序、参数或目录。
#[tauri::command]
pub async fn create_terminal(
    app: tauri::AppHandle,
    window: tauri::Window,
    state: State<'_, DesktopState>,
    repository_id: Option<String>,
    profile_id: String,
    cols: u16,
    rows: u16,
) -> Result<TerminalSession, OperationError> {
    let path = settings_path(&app)?;
    let owner = window.label().to_owned();
    let root = if let Some(id) = &repository_id {
        let shared = state.lock()?;
        Some(
            shared
                .active
                .as_ref()
                .filter(|(repo, _)| &repo.id == id)
                .ok_or_else(|| OperationError::new("STALE_REQUEST"))?
                .0
                .root
                .clone(),
        )
    } else {
        None
    };
    super::blocking("create_terminal", move || {
        let settings = settings::read_snapshot(&path)?.settings;
        let profile = settings
            .terminal
            .profiles
            .iter()
            .find(|profile| profile.profile_id == profile_id)
            .ok_or_else(|| OperationError::new("TERMINAL_PROFILE"))?;
        let shell = profile
            .executable_path
            .as_ref()
            .map(PathBuf::from)
            .map(Ok)
            .unwrap_or_else(default_shell)?;
        if !executable(&shell) {
            return Err(OperationError::new("TERMINAL_SHELL"));
        }
        let home = || {
            app.path()
                .home_dir()
                .map_err(|_| OperationError::new("TERMINAL_DIRECTORY"))
        };
        let cwd = match &profile.cwd {
            TerminalDirectory::Project => root.map(Ok).unwrap_or_else(home)?,
            TerminalDirectory::Home => home()?,
            TerminalDirectory::Fixed { path } => PathBuf::from(path),
        };
        if !cwd.is_absolute() || !cwd.is_dir() {
            return Err(OperationError::new("TERMINAL_DIRECTORY"));
        }
        let terminal = app.state::<TerminalState>();
        let mut registry = terminal
            .0
            .lock()
            .map_err(|_| OperationError::new("TERMINAL_IO"))?;
        if app.get_webview_window(&owner).is_none() {
            return Err(OperationError::new("TERMINAL_UNAVAILABLE"));
        }
        registry.create(
            &owner,
            repository_id,
            shell
                .to_str()
                .ok_or_else(|| OperationError::new("UNSUPPORTED_PATH_ENCODING"))?,
            &profile.args,
            &cwd,
            cols,
            rows,
        )
    })
    .await
}

/// 只有所属窗口可读取已确认游标之后的输出。
#[tauri::command]
pub fn read_terminal(
    window: tauri::Window,
    state: State<'_, TerminalState>,
    session_id: String,
    acknowledged_sequence: u64,
) -> Result<TerminalChunk, OperationError> {
    state
        .0
        .lock()
        .map_err(|_| OperationError::new("TERMINAL_IO"))?
        .read(window.label(), &session_id, acknowledged_sequence)
}

/// 用户输入按原始字节有界排队，不记录命令正文。
#[tauri::command]
pub fn write_terminal(
    window: tauri::Window,
    state: State<'_, TerminalState>,
    session_id: String,
    bytes: Vec<u8>,
) -> Result<(), OperationError> {
    state
        .0
        .lock()
        .map_err(|_| OperationError::new("TERMINAL_IO"))?
        .write(window.label(), &session_id, bytes)
}

/// 布局尺寸只影响当前 PTY，不改变启动配置。
#[tauri::command]
pub fn resize_terminal(
    window: tauri::Window,
    state: State<'_, TerminalState>,
    session_id: String,
    cols: u16,
    rows: u16,
) -> Result<(), OperationError> {
    state
        .0
        .lock()
        .map_err(|_| OperationError::new("TERMINAL_IO"))?
        .resize(window.label(), &session_id, cols, rows)
}

/// 终端关闭在后台回收，避免 ConPTY 清理阻塞桌面事件循环。
#[tauri::command]
pub async fn close_terminal(
    app: tauri::AppHandle,
    window: tauri::Window,
    session_id: String,
) -> Result<(), OperationError> {
    let owner = window.label().to_owned();
    super::blocking("close_terminal", move || {
        app.state::<TerminalState>()
            .0
            .lock()
            .map_err(|_| OperationError::new("TERMINAL_IO"))?
            .close(&owner, &session_id)
    })
    .await
}
