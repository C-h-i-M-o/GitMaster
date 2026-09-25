//! 差异正文只存在单个后台槽，所有分页绑定仓库快照及请求代次。
use super::{blocking, readonly::Context, DesktopState, Session};
use gitmaster_core::git::{
    self,
    coordinator::CoordinationKey,
    diff::document::{DiffDocument, DiffDocumentResult, DiffPage, DiffSummary},
    DiffSide, OperationError,
};
use serde::Serialize;
use std::sync::Arc;
use tauri::State;

/// 桌面签发的文档摘要不含正文。
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum OpenedDiff {
    #[serde(rename_all = "camelCase")]
    Paged {
        document_id: String,
        #[serde(flatten)]
        summary: DiffSummary,
    },
    Binary,
    Unsupported {
        reason: String,
    },
}
/// 不可变差异快照，替换槽立即使旧能力失效。
pub(super) struct DiffCache {
    id: String,
    document: DiffDocument,
}
/// 打开时捕获比较侧和文档代次，不允许迟到请求覆盖后发请求。
pub(super) struct DiffRequest {
    ctx: Context,
    generation: u64,
    change_id: String,
    side: DiffSide,
    context_lines: u16,
}
impl DiffRequest {
    /// 先验证仓库快照及参数，再为有效打开分配代次。
    pub(super) fn begin(
        shared: &DesktopState,
        repository_id: &str,
        snapshot_id: &str,
        change_id: String,
        side: DiffSide,
        context_lines: u16,
    ) -> Result<Self, OperationError> {
        let mut desktop = shared.lock()?;
        let ctx = Context::capture(&desktop, repository_id)?;
        ctx.check_snapshot(snapshot_id)?;
        if context_lines > 2000 || !ctx.state.changes.iter().any(|c| c.change_id == change_id) {
            return Err(OperationError::new("INVALID_INPUT"));
        }
        desktop.diff_request += 1;
        desktop.diff = None;
        Ok(Self {
            ctx,
            generation: desktop.diff_request,
            change_id,
            side,
            context_lines,
        })
    }
    /// 队列开始和发布时都核对代次；读取过程不持有主会话锁。
    pub(super) fn run(self, shared: &DesktopState) -> Result<OpenedDiff, OperationError> {
        self.run_with(shared, |ctx, change, side, context| {
            git::diff::document::open_diff_document(
                &ctx.git,
                &ctx.repository,
                &ctx.state,
                change,
                side,
                context,
            )
        })
    }

    /// 将核心读取与发布分离，便于验证生成期间被新请求替换的竞态。
    pub(super) fn run_with(
        self,
        shared: &DesktopState,
        read: impl FnOnce(&Context, &str, DiffSide, u16) -> Result<DiffDocumentResult, OperationError>,
    ) -> Result<OpenedDiff, OperationError> {
        let key = CoordinationKey::repository(&self.ctx.repository)?;
        let result = self.ctx.coordinator.read(&key, || {
            self.check(&*shared.lock()?)?;
            read(&self.ctx, &self.change_id, self.side, self.context_lines)
        })?;
        let mut desktop = shared.lock()?;
        self.check(&desktop)?;
        match result {
            DiffDocumentResult::Text(document) => {
                let id = format!(
                    "diff-{}-{}-{}",
                    self.ctx.epoch, self.ctx.token, self.generation
                );
                let summary = document.summary();
                desktop.diff = Some(Arc::new(DiffCache {
                    id: id.clone(),
                    document,
                }));
                Ok(OpenedDiff::Paged {
                    document_id: id,
                    summary,
                })
            }
            DiffDocumentResult::Binary => Ok(OpenedDiff::Binary),
            DiffDocumentResult::Unsupported { reason } => Ok(OpenedDiff::Unsupported { reason }),
        }
    }
    /// 环境或仓库变化、更新的文档请求都会拒绝当前打开。
    fn check(&self, desktop: &Session) -> Result<(), OperationError> {
        self.ctx.check(desktop)?;
        if desktop.diff_request != self.generation {
            return Err(OperationError::new("STALE_REQUEST"));
        }
        Ok(())
    }
}

/// 每次读页持有当前文档引用，但不允许旧引用绕过发布检查。
pub(super) struct DiffAccess {
    ctx: Context,
    cache: Arc<DiffCache>,
}
impl DiffAccess {
    /// 原子核对文档身份和仓库快照，不接收前端路径。
    pub(super) fn capture(
        shared: &DesktopState,
        repository_id: &str,
        snapshot_id: &str,
        document_id: &str,
    ) -> Result<Self, OperationError> {
        let desktop = shared.lock()?;
        let ctx = Context::capture(&desktop, repository_id)?;
        ctx.check_snapshot(snapshot_id)?;
        let cache = desktop
            .diff
            .as_ref()
            .filter(|cache| cache.id == document_id)
            .cloned()
            .ok_or_else(|| OperationError::new("STALE_REQUEST"))?;
        Ok(Self { ctx, cache })
    }
    /// 后台按需复制有界文本段，返回前确认槽未被刷新或关闭。
    pub(super) fn run(
        &self,
        shared: &DesktopState,
        start_row: usize,
        count: usize,
        byte_offset: usize,
    ) -> Result<DiffPage, OperationError> {
        let key = CoordinationKey::repository(&self.ctx.repository)?;
        let page = self.ctx.coordinator.read(&key, || {
            self.check(&*shared.lock()?)?;
            self.cache.document.page(start_row, count, byte_offset)
        })?;
        self.check(&*shared.lock()?)?;
        Ok(page)
    }
    /// Arc 身份检查与环境代次共同防止旧页回填。
    fn check(&self, desktop: &Session) -> Result<(), OperationError> {
        self.ctx.check(desktop)?;
        if !desktop
            .diff
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, &self.cache))
        {
            return Err(OperationError::new("STALE_REQUEST"));
        }
        Ok(())
    }
}

/// 只关闭精确匹配的当前文档，迟到清理不能误伤新文档。
pub(super) fn close_document(
    shared: &DesktopState,
    repository_id: &str,
    snapshot_id: &str,
    document_id: &str,
) -> Result<(), OperationError> {
    let mut desktop = shared.lock()?;
    let ctx = Context::capture(&desktop, repository_id)?;
    ctx.check_snapshot(snapshot_id)?;
    if !desktop
        .diff
        .as_ref()
        .is_some_and(|cache| cache.id == document_id)
    {
        return Err(OperationError::new("STALE_REQUEST"));
    }
    desktop.diff = None;
    Ok(())
}

/// 新的分块差异入口，保留旧 read_file_diff 兼容预览使用者。
#[tauri::command]
pub async fn open_diff_document(
    state: State<'_, DesktopState>,
    repository_id: String,
    snapshot_id: String,
    change_id: String,
    side: DiffSide,
    context_lines: Option<u16>,
) -> Result<OpenedDiff, OperationError> {
    let shared = state.inner().clone();
    let request = DiffRequest::begin(
        &shared,
        &repository_id,
        &snapshot_id,
        change_id,
        side,
        context_lines.unwrap_or(3),
    )?;
    blocking("open_diff_document", move || request.run(&shared)).await
}
/// 读取当前快照的一页或一条长行续段。
#[tauri::command]
pub async fn read_diff_page(
    state: State<'_, DesktopState>,
    repository_id: String,
    snapshot_id: String,
    document_id: String,
    start_row: usize,
    count: usize,
    byte_offset: Option<usize>,
) -> Result<DiffPage, OperationError> {
    let shared = state.inner().clone();
    let access = DiffAccess::capture(&shared, &repository_id, &snapshot_id, &document_id)?;
    blocking("read_diff_page", move || {
        access.run(&shared, start_row, count, byte_offset.unwrap_or(0))
    })
    .await
}
/// 释放活动差异，不修改仓库或磁盘文件。
#[tauri::command]
pub async fn close_diff_document(
    state: State<'_, DesktopState>,
    repository_id: String,
    snapshot_id: String,
    document_id: String,
) -> Result<(), OperationError> {
    close_document(state.inner(), &repository_id, &snapshot_id, &document_id)
}
