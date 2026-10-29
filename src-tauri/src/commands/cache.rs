//! 应用缓存的目录和 IPC 适配；前端不能选择缓存路径。
use super::{blocking, settings_path, DesktopState, OperationError};
use gitmaster_core::cache::{CacheUsage, ContentCache};
use std::sync::Arc;
use tauri::{Manager, State};

/// 启动时共享唯一缓存实例；目录或配置异常时仍允许真实 Git 查询。
pub fn initialize_content_cache(app: &tauri::AppHandle) {
    let cache = app
        .path()
        .app_cache_dir()
        .ok()
        .and_then(|path| ContentCache::new(&path.join("content-v1")).ok());
    if let Some(cache) = cache {
        let enabled = settings_path(app)
            .and_then(|path| crate::settings::load(&path))
            .map(|settings| settings.disk_cache_enabled)
            .unwrap_or(false);
        cache.set_disk_enabled(enabled);
        if let Ok(mut session) = app.state::<DesktopState>().lock() {
            session.content_cache = Some(Arc::new(cache));
        }
    }
}

/// 获取已初始化的全局缓存；初始化失败不伪造零占用。
fn content_cache(state: &DesktopState) -> Result<Arc<ContentCache>, OperationError> {
    state
        .lock()?
        .content_cache
        .clone()
        .ok_or_else(|| OperationError::new("CACHE_UNAVAILABLE"))
}

/// 从实际目录及内存读取用量，不持有桌面会话锁进行文件操作。
#[tauri::command]
pub async fn read_cache_usage(
    state: State<'_, DesktopState>,
) -> Result<CacheUsage, OperationError> {
    let cache = content_cache(state.inner())?;
    blocking("read_cache_usage", move || {
        cache
            .usage()
            .map_err(|_| OperationError::new("CACHE_UNAVAILABLE"))
    })
    .await
}

/// 清除可重新生成的内容缓存，不接受仓库、文档或配置路径。
#[tauri::command]
pub async fn clear_content_cache(
    state: State<'_, DesktopState>,
) -> Result<CacheUsage, OperationError> {
    let cache = content_cache(state.inner())?;
    blocking("clear_content_cache", move || {
        cache
            .clear()
            .map_err(|_| OperationError::new("CACHE_UNAVAILABLE"))?;
        cache
            .usage()
            .map_err(|_| OperationError::new("CACHE_UNAVAILABLE"))
    })
    .await
}
