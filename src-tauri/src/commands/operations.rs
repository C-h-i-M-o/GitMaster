//! 写入只接受当前已交付的确认计划；后台任务由核心独立持有。
use super::{blocking, readonly::Context, DesktopState, Session};
use gitmaster_core::git::{self, coordinator::RepositoryCoordinator, *};
use std::sync::{Arc, Mutex};
use tauri::State;

/// 当前界面收到的计划与当时桌面上下文绑定。
pub(super) struct PreviewBinding {
    plan_id: String,
    repository_id: Option<String>,
    epoch: u64,
    token: u64,
}

/// 准备请求的同步代次，阻塞工作不持有桌面主锁。
struct Preparation {
    epoch: u64,
    token: u64,
    generation: u64,
    gate: Arc<Mutex<()>>,
    coordinator: RepositoryCoordinator,
}
impl Preparation {
    /// 新准备作废前一预览，活动写任务期间拒绝准备。
    fn begin(s: &mut Session) -> Result<Self, OperationError> {
        s.coordinator.invalidate()?;
        s.write_request += 1;
        s.preview = None;
        Ok(Self {
            epoch: s.epoch,
            token: s.repository_request,
            generation: s.write_request,
            gate: s.prepare_gate.clone(),
            coordinator: s.coordinator.clone(),
        })
    }
    /// 旧工作线程不得在新计划之后重新准备或发布预览。
    fn check(&self, s: &Session) -> Result<(), OperationError> {
        if self.epoch != s.epoch
            || self.token != s.repository_request
            || self.generation != s.write_request
        {
            return Err(OperationError::new("STALE_WRITE_PLAN"));
        }
        Ok(())
    }
    /// 只在当前代次交付核心签发的计划 ID。
    fn publish(
        &self,
        s: &mut Session,
        plan_id: String,
        repository_id: Option<String>,
    ) -> Result<(), OperationError> {
        self.check(s)?;
        s.preview = Some(PreviewBinding {
            plan_id,
            repository_id,
            epoch: self.epoch,
            token: self.token,
        });
        Ok(())
    }
}

/// 写请求保存后端当前签发的资源映射，前端不能自行提供路径或引用。
enum Input {
    Local(LocalWriteRequest, Option<Arc<git::branches::BranchSession>>),
    Remote(RemoteWriteRequest, Arc<git::remote::RemoteSession>),
    Conflict(ConflictWriteRequest, Arc<git::conflicts::ConflictSession>),
}
/// 一项仓库写准备及所需的只读上下文。
struct WritePreparation {
    ticket: Preparation,
    ctx: Context,
    input: Input,
}
impl WritePreparation {
    /// 先检查快照和分支映射，再登记本地写准备。
    fn local(
        shared: &DesktopState,
        repository_id: &str,
        snapshot_id: &str,
        request: LocalWriteRequest,
    ) -> Result<Self, OperationError> {
        let mut s = shared.lock()?;
        let ctx = Context::capture(&s, repository_id)?;
        ctx.check_snapshot(snapshot_id)?;
        let branches = if matches!(request, LocalWriteRequest::SwitchBranch { .. }) {
            Some(
                s.branches
                    .clone()
                    .ok_or_else(|| OperationError::new("STALE_REQUEST"))?,
            )
        } else {
            None
        };
        let ticket = Preparation::begin(&mut s)?;
        Ok(Self {
            ticket,
            ctx,
            input: Input::Local(request, branches),
        })
    }
    /// 远端操作必须使用当前远端列表签发的 ID。
    fn remote(
        shared: &DesktopState,
        repository_id: &str,
        snapshot_id: &str,
        request: RemoteWriteRequest,
    ) -> Result<Self, OperationError> {
        let mut s = shared.lock()?;
        let ctx = Context::capture(&s, repository_id)?;
        ctx.check_snapshot(snapshot_id)?;
        let cache = s
            .remotes
            .clone()
            .ok_or_else(|| OperationError::new("STALE_REQUEST"))?;
        let ticket = Preparation::begin(&mut s)?;
        Ok(Self {
            ticket,
            ctx,
            input: Input::Remote(request, cache),
        })
    }
    /// 冲突请求绑定当前 mergeSessionId，内容指纹继续由核心复核。
    fn conflict(
        shared: &DesktopState,
        repository_id: &str,
        merge_session_id: &str,
        request: ConflictWriteRequest,
    ) -> Result<Self, OperationError> {
        let mut s = shared.lock()?;
        let ctx = Context::capture(&s, repository_id)?;
        let cache = s
            .conflicts
            .clone()
            .filter(|c| c.state().merge_session_id == merge_session_id)
            .ok_or_else(|| OperationError::new("STALE_CONFLICT"))?;
        let ticket = Preparation::begin(&mut s)?;
        Ok(Self {
            ticket,
            ctx,
            input: Input::Conflict(request, cache),
        })
    }
    /// 在准备前后检查捕获的映射仍是当前资源。
    fn check(&self, s: &Session) -> Result<(), OperationError> {
        self.ticket.check(s)?;
        let valid = match &self.input {
            Input::Local(_, Some(cache)) => {
                s.branches.as_ref().is_some_and(|c| Arc::ptr_eq(c, cache))
            }
            Input::Local(_, None) => true,
            Input::Remote(_, cache) => s.remotes.as_ref().is_some_and(|c| Arc::ptr_eq(c, cache)),
            Input::Conflict(_, cache) => {
                s.conflicts.as_ref().is_some_and(|c| Arc::ptr_eq(c, cache))
            }
        };
        if valid {
            Ok(())
        } else {
            Err(OperationError::new("STALE_WRITE_PLAN"))
        }
    }
    /// 串行进入核心 prepare；核心自己排队，不重复获取同仓库队列锁。
    fn run(self, shared: &DesktopState) -> Result<WritePreview, OperationError> {
        let _gate = self
            .ticket
            .gate
            .lock()
            .map_err(|_| OperationError::new("GIT_EXECUTION_FAILED"))?;
        self.check(&*shared.lock()?)?;
        let c = &self.ticket.coordinator;
        let g = &self.ctx.git;
        let r = &self.ctx.repository;
        let s = &self.ctx.state;
        let preview = match &self.input {
            Input::Local(request, branches) => match request {
                LocalWriteRequest::Stage { change_ids } => {
                    git::write::prepare_index_change(c, g, r, s, change_ids, true)
                }
                LocalWriteRequest::Unstage { change_ids } => {
                    git::write::prepare_index_change(c, g, r, s, change_ids, false)
                }
                LocalWriteRequest::Commit { message } => {
                    git::write::prepare_commit(c, g, r, s, message)
                }
                LocalWriteRequest::CreateBranch { name } => {
                    git::branches::prepare_create_branch(c, g, r, s, name)
                }
                LocalWriteRequest::SwitchBranch { branch_id } => {
                    git::branches::prepare_switch_branch(
                        c,
                        g,
                        r,
                        s,
                        branches
                            .as_ref()
                            .ok_or_else(|| OperationError::new("STALE_REQUEST"))?,
                        branch_id,
                    )
                }
            },
            Input::Remote(request, cache) => match request {
                RemoteWriteRequest::Fetch {
                    remote_id,
                    remote_branch_id,
                } => git::remote::prepare_fetch(c, g, r, s, cache, remote_id, remote_branch_id),
                RemoteWriteRequest::Push {
                    remote_id,
                    target_branch_name,
                } => git::remote::prepare_push(c, g, r, s, cache, remote_id, target_branch_name),
                RemoteWriteRequest::Integrate {
                    remote_branch_id,
                    mode,
                } => git::merge::prepare_integrate(c, g, r, s, cache, remote_branch_id, *mode),
            },
            Input::Conflict(request, cache) => match request {
                ConflictWriteRequest::SaveConflict {
                    conflict_id,
                    fingerprint,
                    content,
                } => git::conflicts::prepare_save(c, cache, conflict_id, fingerprint, content),
                ConflictWriteRequest::FinishMerge { message } => {
                    git::conflicts::prepare_finish(c, cache, message)
                }
            },
        }?;
        let mut desktop = shared.lock()?;
        self.check(&desktop)?;
        self.ticket
            .publish(&mut desktop, preview.plan_id.clone(), Some(r.id.clone()))?;
        Ok(preview)
    }
}

/// 确认仅消费当前已交付计划，重复确认返回原任务且不再次使读取失效。
fn execute(
    shared: &DesktopState,
    repository_id: Option<&str>,
    plan_id: &str,
) -> Result<OperationHandle, OperationError> {
    let mut s = shared.lock()?;
    if let Some((_, handle)) = s
        .executed
        .iter()
        .find(|(plan, handle)| plan == plan_id && handle.repository_id.as_deref() == repository_id)
    {
        return Ok(handle.clone());
    }
    let preview = s
        .preview
        .as_ref()
        .filter(|p| {
            p.plan_id == plan_id
                && p.repository_id.as_deref() == repository_id
                && p.epoch == s.epoch
                && p.token == s.repository_request
        })
        .ok_or_else(|| OperationError::new("STALE_WRITE_PLAN"))?;
    let handle = s.coordinator.execute(repository_id, &preview.plan_id)?;
    s.repository_request += 1;
    s.clear_readonly();
    s.executed.push_back((plan_id.to_owned(), handle.clone()));
    while s.executed.len() > 17 {
        s.executed.pop_front();
    }
    Ok(handle)
}

/// 读取真实能力提示；不会创建写计划，allowed 仍需独立确认。
#[tauri::command]
pub async fn read_write_context(
    state: State<'_, DesktopState>,
    repository_id: String,
    snapshot_id: String,
) -> Result<WriteContext, OperationError> {
    let shared = state.inner().clone();
    let ctx = Context::capture(&*shared.lock()?, &repository_id)?;
    ctx.check_snapshot(&snapshot_id)?;
    blocking(move || {
        let key = git::coordinator::CoordinationKey::repository(&ctx.repository)?;
        let result = ctx.coordinator.read(&key, || {
            git::capabilities::read_write_context(&ctx.git, &ctx.repository, &ctx.state)
        })?;
        ctx.check(&*shared.lock()?)?;
        Ok(result)
    })
    .await
}
/// 准备暂存、提交或分支写入，前端只提交业务请求。
#[tauri::command]
pub async fn prepare_local_write(
    state: State<'_, DesktopState>,
    repository_id: String,
    snapshot_id: String,
    request: LocalWriteRequest,
) -> Result<WritePreview, OperationError> {
    let shared = state.inner().clone();
    let prepare = WritePreparation::local(&shared, &repository_id, &snapshot_id, request)?;
    blocking(move || prepare.run(&shared)).await
}
/// 准备当前远端映射对应的获取、推送或整合。
#[tauri::command]
pub async fn prepare_remote_write(
    state: State<'_, DesktopState>,
    repository_id: String,
    snapshot_id: String,
    request: RemoteWriteRequest,
) -> Result<WritePreview, OperationError> {
    let shared = state.inner().clone();
    let prepare = WritePreparation::remote(&shared, &repository_id, &snapshot_id, request)?;
    blocking(move || prepare.run(&shared)).await
}
/// 准备当前合并会话的保存或完成操作。
#[tauri::command]
pub async fn prepare_conflict_write(
    state: State<'_, DesktopState>,
    repository_id: String,
    merge_session_id: String,
    request: ConflictWriteRequest,
) -> Result<WritePreview, OperationError> {
    let shared = state.inner().clone();
    let prepare = WritePreparation::conflict(&shared, &repository_id, &merge_session_id, request)?;
    blocking(move || prepare.run(&shared)).await
}
/// 启动确认的仓库任务，函数返回后核心继续执行。
#[tauri::command]
pub fn execute_write(
    state: State<'_, DesktopState>,
    repository_id: String,
    plan_id: String,
) -> Result<OperationHandle, OperationError> {
    execute(state.inner(), Some(&repository_id), &plan_id)
}
/// 查询任务不会安装其旧快照，也不会切换当前仓库。
#[tauri::command]
pub fn read_operation(
    state: State<'_, DesktopState>,
    operation_id: Option<String>,
) -> Result<Option<OperationRecord>, OperationError> {
    state
        .lock()?
        .coordinator
        .read_operation(operation_id.as_deref())
}

/// clone 准备仅持有原生选择器保存的父目录能力。
struct ClonePreparation {
    ticket: Preparation,
    git: GitExecutable,
    parent: std::path::PathBuf,
    request: CloneRequest,
}
impl ClonePreparation {
    /// 在主锁内解析父目录 ID；拒绝前端自行提供路径。
    fn begin(shared: &DesktopState, request: CloneRequest) -> Result<Self, OperationError> {
        let mut s = shared.lock()?;
        let parent = s
            .clone_parent
            .as_ref()
            .filter(|(id, _)| id == &request.parent_directory_id)
            .map(|(_, p)| p.clone())
            .ok_or_else(|| OperationError::new("STALE_REQUEST"))?;
        let git = s
            .git
            .clone()
            .ok_or_else(|| OperationError::new("GIT_NOT_FOUND"))?;
        let ticket = Preparation::begin(&mut s)?;
        Ok(Self {
            ticket,
            git,
            parent,
            request,
        })
    }
    /// 核心复核目录身份和目标不存在，准备不连接网络。
    fn run(self, shared: &DesktopState) -> Result<ClonePreview, OperationError> {
        let _gate = self
            .ticket
            .gate
            .lock()
            .map_err(|_| OperationError::new("GIT_EXECUTION_FAILED"))?;
        self.ticket.check(&*shared.lock()?)?;
        let preview = git::remote::prepare_clone(
            &self.ticket.coordinator,
            &self.git,
            &self.parent,
            &self.request.url,
            &self.request.directory_name,
        )?;
        self.ticket
            .publish(&mut *shared.lock()?, preview.plan_id.clone(), None)?;
        Ok(preview)
    }
}
/// 只读生成 clone 目标和脱敏地址预览。
#[tauri::command]
pub async fn prepare_clone(
    state: State<'_, DesktopState>,
    request: CloneRequest,
) -> Result<ClonePreview, OperationError> {
    let shared = state.inner().clone();
    let prepare = ClonePreparation::begin(&shared, request)?;
    blocking(move || prepare.run(&shared)).await
}
/// 启动已确认 clone，成功路径由前端显式打开，不隐式切换会话。
#[tauri::command]
pub fn execute_clone(
    state: State<'_, DesktopState>,
    plan_id: String,
) -> Result<OperationHandle, OperationError> {
    execute(state.inner(), None, &plan_id)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod conflict_tests;
