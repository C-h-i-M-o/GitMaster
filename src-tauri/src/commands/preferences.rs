//! 统一设置的读写适配，Git 验证发生在保存前，普通偏好不清空仓库。
use super::{settings_path, DesktopState};
use crate::settings::{self, Settings, SettingsError, SettingsSnapshot};
use gitmaster_core::git::{environment::resolve_git, OperationError};
use std::path::Path;
use tauri::State;

/// 返回完整配置及并发修改标识，不执行任何终端程序。
#[tauri::command]
pub async fn read_app_settings(app: tauri::AppHandle) -> Result<SettingsSnapshot, OperationError> {
    let path = settings_path(&app)?;
    super::blocking("read_app_settings", move || settings::read_snapshot(&path)).await
}

/// 合并提交设置草稿；可执行 Git 先验证，参数和 shell 绝不在保存时运行。
#[tauri::command]
pub async fn apply_app_settings(
    app: tauri::AppHandle,
    state: State<'_, DesktopState>,
    expected_revision: String,
    draft: Settings,
) -> Result<SettingsSnapshot, SettingsError> {
    let path = settings_path(&app)?;
    let shared = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let current = settings::read_snapshot(&path)?;
        if current.revision != expected_revision {
            return Err(SettingsError {
                code: "STALE_SETTINGS".into(),
                field_errors: Vec::new(),
            });
        }
        let changed_git = current.settings.git_path != draft.git_path;
        let resolved = if changed_git {
            let result = resolve_git(draft.git_path.as_deref().map(Path::new));
            if draft.git_path.is_some() {
                if let Err(error) = &result {
                    return Err(SettingsError {
                        code: error.code.clone(),
                        field_errors: vec![settings::FieldError {
                            field: "gitPath".into(),
                            message: "所选 Git 未通过验证，请检查可执行文件。".into(),
                        }],
                    });
                }
            }
            Some(result.ok())
        } else {
            None
        };
        let mut session = shared.lock()?;
        if changed_git {
            session.coordinator.invalidate()?;
        }
        let level = draft.log_level;
        let saved = crate::logging::save_with_level(level, || {
            settings::save_all(&path, &expected_revision, draft)
        })?;
        if let Some(git) = resolved {
            session.install(git, true)?;
        }
        Ok(saved)
    })
    .await
    .map_err(|_| SettingsError {
        code: "SETTINGS_IO".into(),
        field_errors: Vec::new(),
    })?
}
