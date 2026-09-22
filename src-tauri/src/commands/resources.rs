//! 远端、冲突和原生目录选择的桌面能力映射。
use super::{blocking, readonly::Context, settings_path, DesktopState, Session};
use crate::settings::{self, UiPreferences};
use gitmaster_core::git::{self, coordinator::CoordinationKey, *};
use serde::Serialize;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};
use tauri::State;
use tauri_plugin_dialog::DialogExt;

/// 远端初始化捕获请求代次，同仓库刷新继承真实获取时间。
pub(super) struct RemoteRequest {
    ctx: Context,
    generation: u64,
    previous: Option<Arc<git::remote::RemoteSession>>,
}
impl RemoteRequest {
    /// 立即使旧 ID 失效，后来开始的请求优先发布。
    pub(super) fn begin(
        shared: &DesktopState,
        repository_id: &str,
    ) -> Result<Self, OperationError> {
        let mut s = shared.lock()?;
        let ctx = Context::capture(&s, repository_id)?;
        s.remotes_request += 1;
        if let Some(cache) = s.remotes.take() {
            s.previous_remotes = Some(cache);
        }
        let previous = s
            .previous_remotes
            .clone()
            .filter(|c| c.state().repository_id == repository_id);
        Ok(Self {
            ctx,
            generation: s.remotes_request,
            previous,
        })
    }
    /// 在仓库队列读取新映射，完成后核对当前请求身份。
    pub(super) fn run(self, shared: &DesktopState) -> Result<RemoteState, OperationError> {
        let key = CoordinationKey::repository(&self.ctx.repository)?;
        let cache = self.ctx.coordinator.read(&key, || match &self.previous {
            Some(c) => c.refreshed(),
            None => git::remote::RemoteSession::new(&self.ctx.git, &self.ctx.repository),
        })?;
        let result = cache.state();
        let mut s = shared.lock()?;
        self.ctx.check(&s)?;
        if s.remotes_request != self.generation {
            return Err(OperationError::new("STALE_REQUEST"));
        }
        s.remotes = Some(Arc::new(cache));
        Ok(result)
    }
}
/// 读取配置地址和跟踪引用，不连接网络。
#[tauri::command]
pub async fn read_remotes(
    state: State<'_, DesktopState>,
    repository_id: String,
) -> Result<RemoteState, OperationError> {
    let shared = state.inner().clone();
    let request = RemoteRequest::begin(&shared, &repository_id)?;
    blocking(move || request.run(&shared)).await
}
/// 评估已签发跟踪分支与当前 HEAD，迟到结果不能用于新仓库。
#[tauri::command]
pub async fn assess_remote(
    state: State<'_, DesktopState>,
    repository_id: String,
    snapshot_id: String,
    remote_branch_id: String,
) -> Result<RemoteAssessment, OperationError> {
    let shared = state.inner().clone();
    let (ctx, cache) = {
        let s = shared.lock()?;
        let ctx = Context::capture(&s, &repository_id)?;
        ctx.check_snapshot(&snapshot_id)?;
        (
            ctx,
            s.remotes
                .clone()
                .ok_or_else(|| OperationError::new("STALE_REQUEST"))?,
        )
    };
    blocking(move || {
        let key = CoordinationKey::repository(&ctx.repository)?;
        let result = ctx
            .coordinator
            .read(&key, || cache.assess(&ctx.state, &remote_branch_id))?;
        let s = shared.lock()?;
        ctx.check(&s)?;
        if !s.remotes.as_ref().is_some_and(|c| Arc::ptr_eq(c, &cache)) {
            return Err(OperationError::new("STALE_REQUEST"));
        }
        Ok(result)
    })
    .await
}

/// 冲突初始化绑定真实合并状态，可从外部 Git 或应用重启恢复。
pub(super) struct ConflictRequest {
    ctx: Context,
    generation: u64,
}
impl ConflictRequest {
    /// 先清除旧会话，防止列表刷新期间仍接受旧编辑器。
    pub(super) fn begin(
        shared: &DesktopState,
        repository_id: &str,
    ) -> Result<Self, OperationError> {
        let mut s = shared.lock()?;
        let ctx = Context::capture(&s, repository_id)?;
        s.conflicts_request += 1;
        s.conflicts = None;
        Ok(Self {
            ctx,
            generation: s.conflicts_request,
        })
    }
    /// 在共享队列读取实际 stage 并发布完整会话。
    pub(super) fn run(self, shared: &DesktopState) -> Result<ConflictState, OperationError> {
        let key = CoordinationKey::repository(&self.ctx.repository)?;
        let cache = self.ctx.coordinator.read(&key, || {
            git::conflicts::ConflictSession::new(&self.ctx.git, &self.ctx.repository)
        })?;
        let result = cache.state();
        let mut s = shared.lock()?;
        self.ctx.check(&s)?;
        if s.conflicts_request != self.generation {
            return Err(OperationError::new("STALE_CONFLICT"));
        }
        s.conflicts = Some(Arc::new(cache));
        Ok(result)
    }
}
/// 读取 merge 状态下的冲突清单及文件级支持能力。
#[tauri::command]
pub async fn read_conflicts(
    state: State<'_, DesktopState>,
    repository_id: String,
) -> Result<ConflictState, OperationError> {
    let shared = state.inner().clone();
    let request = ConflictRequest::begin(&shared, &repository_id)?;
    blocking(move || request.run(&shared)).await
}
/// 返回三方文本和工作文件指纹，拒绝旧合并会话或刷新前的缓存。
#[tauri::command]
pub async fn read_conflict_document(
    state: State<'_, DesktopState>,
    repository_id: String,
    merge_session_id: String,
    conflict_id: String,
) -> Result<ConflictDocument, OperationError> {
    let shared = state.inner().clone();
    let (ctx, cache) = {
        let s = shared.lock()?;
        let ctx = Context::capture(&s, &repository_id)?;
        let cache = s
            .conflicts
            .clone()
            .filter(|c| c.state().merge_session_id == merge_session_id)
            .ok_or_else(|| OperationError::new("STALE_CONFLICT"))?;
        (ctx, cache)
    };
    blocking(move || {
        let key = CoordinationKey::repository(&ctx.repository)?;
        let result = ctx
            .coordinator
            .read(&key, || cache.read_document(&conflict_id))?;
        let s = shared.lock()?;
        ctx.check(&s)?;
        if !s.conflicts.as_ref().is_some_and(|c| Arc::ptr_eq(c, &cache)) {
            return Err(OperationError::new("STALE_CONFLICT"));
        }
        Ok(result)
    })
    .await
}

/// 前端可展示父目录路径，但提交 clone 时只能使用选择器 ID。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneParent {
    pub parent_directory_id: String,
    pub display_path: String,
}
static PARENT_SEQUENCE: AtomicU64 = AtomicU64::new(0);
/// 生成进程内唯一的选择器身份，不把路径编码为 ID。
pub(super) fn next_parent_id() -> String {
    format!("parent-{}", PARENT_SEQUENCE.fetch_add(1, Ordering::Relaxed))
}
/// 选择器请求代次，慢窗口的返回不得替换后来选择。
struct ParentRequest {
    epoch: u64,
    generation: u64,
}
impl ParentRequest {
    /// 新选择清除旧目录能力及预览；活动操作期间拒绝变更。
    fn begin(s: &mut Session) -> Result<Self, OperationError> {
        s.coordinator.invalidate()?;
        s.picker_request += 1;
        s.clone_parent = None;
        s.preview = None;
        s.write_request += 1;
        Ok(Self {
            epoch: s.epoch,
            generation: s.picker_request,
        })
    }
    /// 只发布真实目录的规范化路径，取消保持空能力。
    fn publish(
        self,
        s: &mut Session,
        path: Option<PathBuf>,
    ) -> Result<Option<CloneParent>, OperationError> {
        if self.epoch != s.epoch || self.generation != s.picker_request {
            return Err(OperationError::new("STALE_REQUEST"));
        }
        let Some(path) = path else { return Ok(None) };
        if !path.is_dir() {
            return Err(OperationError::new("ACCESS_DENIED"));
        }
        let display_path = path
            .to_str()
            .ok_or_else(|| OperationError::new("UNSUPPORTED_PATH_ENCODING"))?
            .to_owned();
        let parent_directory_id = next_parent_id();
        s.clone_parent = Some((parent_directory_id.clone(), path));
        Ok(Some(CloneParent {
            parent_directory_id,
            display_path,
        }))
    }
}
/// 原生选择 clone 父目录，前端不能将任意字符串当作选择器结果。
#[tauri::command]
pub async fn choose_clone_parent(
    app: tauri::AppHandle,
    state: State<'_, DesktopState>,
) -> Result<Option<CloneParent>, OperationError> {
    let shared = state.inner().clone();
    let request = ParentRequest::begin(&mut *shared.lock()?)?;
    blocking(move || {
        let path = app
            .dialog()
            .file()
            .set_title("选择克隆仓库的父目录")
            .blocking_pick_folder()
            .map(|p| {
                p.into_path()
                    .map_err(|_| OperationError::new("ACCESS_DENIED"))
                    .and_then(|p| {
                        std::fs::canonicalize(p).map_err(|_| OperationError::new("ACCESS_DENIED"))
                    })
            })
            .transpose()?;
        request.publish(&mut *shared.lock()?, path)
    })
    .await
}
/// 读取应用偏好，不依赖仓库状态或浏览器 localStorage。
#[tauri::command]
pub async fn read_ui_preferences(app: tauri::AppHandle) -> Result<UiPreferences, OperationError> {
    let path = settings_path(&app)?;
    blocking(move || Ok(settings::load(&path)?.ui_preferences)).await
}
/// 校验并持久化两项界面偏好，同时保留 Git 路径设置。
#[tauri::command]
pub async fn set_ui_preferences(
    app: tauri::AppHandle,
    elasticity: u8,
    show_labels: bool,
) -> Result<UiPreferences, OperationError> {
    let path = settings_path(&app)?;
    blocking(move || {
        settings::save_preferences(
            &path,
            UiPreferences {
                elasticity,
                show_labels,
            },
        )
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    /// 迟到选择不能覆盖新目录，取消使旧能力失效。
    #[test]
    fn picker_generations_and_cancellation() {
        let mut s = Session::default();
        let old = ParentRequest::begin(&mut s).unwrap();
        let new = ParentRequest::begin(&mut s).unwrap();
        let root = std::fs::canonicalize(std::env::temp_dir()).unwrap();
        let parent = new.publish(&mut s, Some(root.clone())).unwrap().unwrap();
        assert_eq!(
            old.publish(&mut s, Some(root)).unwrap_err().code,
            "STALE_REQUEST"
        );
        assert_eq!(
            s.clone_parent.as_ref().unwrap().0,
            parent.parent_directory_id
        );
        ParentRequest::begin(&mut s)
            .unwrap()
            .publish(&mut s, None)
            .unwrap();
        assert!(s.clone_parent.is_none());
    }
}
