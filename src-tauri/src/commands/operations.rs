//! 写入只接受当前已交付的确认计划；后台任务由核心独立持有。
use super::{
    blocking,
    readonly::{Context, HistoryCache},
    DesktopState, Session,
};
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
    RevertStart(String, Arc<Mutex<HistoryCache>>),
    RevertConflict(RevertConflictWriteRequest, Arc<git::revert::RevertSession>),
    Restore(Vec<String>, Arc<git::restore::RestoreSession>),
    File(String, String, String, Arc<git::files::ProjectFilesSession>),
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
    /// 文件保存使用当前列表签发 ID，完整版本由核心再次验证。
    fn file(
        shared: &DesktopState,
        repository_id: &str,
        snapshot_id: &str,
        file_id: String,
        version: String,
        content: String,
    ) -> Result<Self, OperationError> {
        let mut s = shared.lock()?;
        let ctx = Context::capture(&s, repository_id)?;
        ctx.check_snapshot(snapshot_id)?;
        let cache = s
            .project_files
            .clone()
            .ok_or_else(|| OperationError::new("FILE_UNAVAILABLE"))?;
        let ticket = Preparation::begin(&mut s)?;
        Ok(Self {
            ticket,
            ctx,
            input: Input::File(file_id, version, content, cache),
        })
    }
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
        let input = if let LocalWriteRequest::RevertCommit { commit_oid } = request {
            let cache = s
                .history
                .clone()
                .ok_or_else(|| OperationError::new("STALE_GRAPH"))?;
            Input::RevertStart(commit_oid, cache)
        } else if let LocalWriteRequest::RestoreFiles { source, file_ids } = request {
            let cache = s
                .restore_source
                .clone()
                .ok_or_else(|| OperationError::new("STALE_REQUEST"))?;
            if cache.page(None)?.source != source {
                return Err(OperationError::new("STALE_REQUEST"));
            }
            Input::Restore(file_ids, cache)
        } else {
            Input::Local(request, branches)
        };
        let ticket = Preparation::begin(&mut s)?;
        Ok(Self { ticket, ctx, input })
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
    /// 撤销保存和完成只接受独立撤销会话签发的 ID。
    fn revert_conflict(
        shared: &DesktopState,
        repository_id: &str,
        revert_session_id: &str,
        request: RevertConflictWriteRequest,
    ) -> Result<Self, OperationError> {
        let mut s = shared.lock()?;
        let ctx = Context::capture(&s, repository_id)?;
        let cache = s
            .revert
            .clone()
            .filter(|cache| cache.state().revert_session_id == revert_session_id)
            .ok_or_else(|| OperationError::new("STALE_CONFLICT"))?;
        let ticket = Preparation::begin(&mut s)?;
        Ok(Self {
            ticket,
            ctx,
            input: Input::RevertConflict(request, cache),
        })
    }

    /// 在准备前后检查捕获的映射仍是当前资源。
    fn check(&self, s: &Session) -> Result<(), OperationError> {
        self.ticket.check(s)?;
        let valid = match &self.input {
            Input::RevertStart(_, cache) => s
                .history
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, cache)),
            Input::RevertConflict(_, cache) => s
                .revert
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, cache)),
            Input::Restore(_, cache) => s
                .restore_source
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, cache)),
            Input::File(_, _, _, cache) => s
                .project_files
                .as_ref()
                .is_some_and(|c| Arc::ptr_eq(c, cache)),
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
            Input::RevertStart(oid, cache) => {
                let authorized = cache
                    .lock()
                    .map_err(|_| OperationError::new("GIT_EXECUTION_FAILED"))?
                    .authorizes_commit(oid);
                if !authorized {
                    return Err(OperationError::new("STALE_GRAPH"));
                }
                git::revert::prepare_revert(c, g, r, s, oid)
            }
            Input::RevertConflict(request, cache) => match request {
                RevertConflictWriteRequest::SaveRevertConflict {
                    conflict_id,
                    fingerprint,
                    content,
                } => cache.prepare_save(c, conflict_id, fingerprint, content),
                RevertConflictWriteRequest::FinishRevert { message } => {
                    cache.prepare_finish(c, message)
                }
            },
            Input::Restore(ids, cache) => cache.prepare(c, s, ids),
            Input::File(id, version, content, cache) => {
                cache.prepare_save_with_state(c, s, id, version, content)
            }
            Input::Local(request, branches) => match request {
                LocalWriteRequest::RevertCommit { .. } => Err(OperationError::new("STALE_GRAPH")),
                LocalWriteRequest::AbortRevert {} => git::revert::prepare_abort(c, g, r, s),
                LocalWriteRequest::CleanUntracked { change_ids } => {
                    git::clean::prepare(c, g, r, s, change_ids)
                }
                LocalWriteRequest::RestoreFiles { .. } => Err(OperationError::new("STALE_REQUEST")),
                LocalWriteRequest::Stage { change_ids } => {
                    git::write::prepare_index_change(c, g, r, s, change_ids, true)
                }
                LocalWriteRequest::Unstage { change_ids } => {
                    git::write::prepare_index_change(c, g, r, s, change_ids, false)
                }
                LocalWriteRequest::Commit {
                    message,
                    stage_change_ids,
                } => git::write::prepare_commit_selected(c, g, r, s, message, stage_change_ids),
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
                RemoteWriteRequest::PublishBranch {
                    remote_id,
                    target_branch_name,
                } => git::remote::prepare_publish_branch(
                    c,
                    g,
                    r,
                    s,
                    cache,
                    remote_id,
                    target_branch_name,
                ),
                RemoteWriteRequest::SetUpstream {
                    remote_id,
                    target_branch_name,
                } => git::remote::prepare_set_upstream(
                    c,
                    g,
                    r,
                    s,
                    cache,
                    remote_id,
                    target_branch_name,
                ),
                RemoteWriteRequest::SyncPush => git::remote::prepare_sync_push(c, g, r, s, cache),
                RemoteWriteRequest::SyncFastForward => {
                    git::remote::prepare_sync_fast_forward(c, g, r, s, cache)
                }
                RemoteWriteRequest::FetchAll { remote_id } => {
                    git::remote::prepare_fetch_all(c, g, r, s, cache, remote_id)
                }
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
    let epoch = s.epoch;
    let handle = s.coordinator.execute(repository_id, &preview.plan_id)?;
    let kind = s
        .coordinator
        .read_operation(Some(&handle.operation_id))?
        .map(|record| record.progress.kind);
    s.repository_request += 1;
    let accepted_token = s.repository_request;
    let start_versions = s.monitor.as_ref().map(|m| m.versions()).unwrap_or_default();
    s.accepted_operations.push_back((
        handle.operation_id.clone(),
        epoch,
        accepted_token,
        start_versions,
    ));
    while s.accepted_operations.len() > 17 {
        s.accepted_operations.pop_front();
    }
    if matches!(
        kind,
        Some(OperationKind::SaveFile | OperationKind::Stage | OperationKind::Unstage)
    ) {
        s.clear_readonly_domains(false, false, true);
    } else {
        s.clear_readonly();
    }
    s.executed.push_back((plan_id.to_owned(), handle.clone()));
    while s.executed.len() > 17 {
        s.executed.pop_front();
    }
    Ok(handle)
}

/// 准备当前签发文件的版本化保存计划，执行复用后台任务接口。
#[tauri::command]
pub async fn prepare_file_save(
    state: State<'_, DesktopState>,
    repository_id: String,
    snapshot_id: String,
    file_id: String,
    version: String,
    content: String,
) -> Result<WritePreview, OperationError> {
    let shared = state.inner().clone();
    let prepare = WritePreparation::file(
        &shared,
        &repository_id,
        &snapshot_id,
        file_id,
        version,
        content,
    )?;
    blocking("prepare_file_save", move || prepare.run(&shared)).await
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
    blocking("read_write_context", move || {
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
    let prepare = WritePreparation::local(&shared, &repository_id, &snapshot_id, request);
    blocking("prepare_local_write", move || prepare?.run(&shared)).await
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
    blocking("prepare_remote_write", move || prepare.run(&shared)).await
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
    blocking("prepare_conflict_write", move || prepare.run(&shared)).await
}

/// 撤销文本保存和最终提交使用独立请求类型与会话身份。
#[tauri::command]
pub async fn prepare_revert_conflict_write(
    state: State<'_, DesktopState>,
    repository_id: String,
    revert_session_id: String,
    request: RevertConflictWriteRequest,
) -> Result<WritePreview, OperationError> {
    let shared = state.inner().clone();
    let prepare =
        WritePreparation::revert_conflict(&shared, &repository_id, &revert_session_id, request)?;
    blocking("prepare_revert_conflict_write", move || {
        prepare.run(&shared)
    })
    .await
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

/// 消费写任务已核实的仓库快照；代次变化或重复消费均安全降级为刷新。
#[tauri::command]
pub fn accept_operation_state(
    state: State<'_, DesktopState>,
    repository_id: String,
    operation_id: String,
) -> Result<Option<super::RepositorySnapshot>, OperationError> {
    let mut s = state.lock()?;
    let record = s.coordinator.read_operation(Some(&operation_id))?;
    let Some(record) = record else {
        return Ok(None);
    };
    let Some(gitmaster_core::git::types::OperationResult::Succeeded {
        refresh: gitmaster_core::git::types::WriteRefresh::Ready { state: snapshot },
        ..
    }) = record.result
    else {
        return Ok(None);
    };
    let Some((epoch, token)) = s
        .accepted_operations
        .iter()
        .find(|(id, _, _, _)| id == &operation_id)
        .map(|(_, epoch, token, _)| (*epoch, *token))
    else {
        return Ok(None);
    };
    match s.accept_operation_state(&repository_id, &operation_id, snapshot, epoch, token) {
        Ok(snapshot) => Ok(Some(snapshot)),
        Err(_) => Ok(None),
    }
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
    blocking("prepare_clone", move || prepare.run(&shared)).await
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
