//! 恢复候选只接受当前仓库快照，旧请求不能重新安装过期来源。
use super::{blocking, readonly::Context, DesktopState};
use gitmaster_core::git::{
    coordinator::CoordinationKey,
    restore::{RestoreFilePage, RestoreSession, RestoreSource},
    OperationError,
};
use std::sync::Arc;
use tauri::State;

/// 返回有界来源页；分页必须沿用同一来源，新来源立即废弃旧文件身份。
#[tauri::command]
pub async fn prepare_restore_source(
    state: State<'_, DesktopState>,
    repository_id: String,
    snapshot_id: String,
    source: RestoreSource,
    cursor: Option<String>,
) -> Result<RestoreFilePage, OperationError> {
    let shared = state.inner().clone();
    let (ctx, generation, existing) = {
        let mut desktop = shared.lock()?;
        let ctx = Context::capture(&desktop, &repository_id)?;
        ctx.check_snapshot(&snapshot_id)?;
        if cursor.is_none() {
            desktop.restore_request += 1;
            desktop.restore_source = None;
        }
        (ctx, desktop.restore_request, desktop.restore_source.clone())
    };
    blocking("prepare_restore_source", move || {
        let key = CoordinationKey::repository(&ctx.repository)?;
        let session = match existing {
            Some(session) => session,
            None if cursor.is_none() => ctx.coordinator.read(&key, || {
                RestoreSession::new(&ctx.git, &ctx.repository, &ctx.state, source.clone())
                    .map(Arc::new)
            })?,
            None => return Err(OperationError::new("STALE_REQUEST")),
        };
        let page = session.page(cursor.as_deref())?;
        if page.source != source {
            return Err(OperationError::new("STALE_REQUEST"));
        }
        let mut desktop = shared.lock()?;
        ctx.check(&desktop)?;
        if desktop.restore_request != generation {
            return Err(OperationError::new("STALE_REQUEST"));
        }
        desktop.restore_source = Some(session);
        Ok(page)
    })
    .await
}
