//! 用户引用与上下文预览复用后端工作区签发和只读工具，不建立额外工作区。
use super::*;
use gitmaster_core::agent::{
    references::{verify_references, UserReferenceSpec, VerifiedReference},
    runtime::{ContextGuard, ContextPreviewResult},
};

/// 为发送和预览构建相同工具范围，并返回会在项目切换后失效的代次检查。
pub(super) async fn workspace_context(
    app: &tauri::AppHandle,
    services: &Arc<AgentServices>,
    shared: &DesktopState,
    workspace_id: &str,
) -> AgentResult<(AgentWorkspace, Arc<Toolbox>, String, ContextGuard)> {
    let (workspace, context, system_context) = {
        let session = shared.0.lock().map_err(|_| storage())?;
        let workspace = workspaces::resolve_workspace(services, &session, workspace_id)?;
        let context = match (&session.git, &session.active) {
            (Some(git), Some((repository, _))) if workspace.git_available => Some(GitContext {
                git: git.clone(),
                repository: repository.clone(),
                coordinator: session.coordinator.clone(),
            }),
            _ => None,
        };
        let git = context
            .as_ref()
            .map(|value| format!("当前仓库 ID：{}。", value.repository.id))
            .unwrap_or_else(|| "此工作区没有可用 Git 仓库工具。".into());
        let system_context = format!(
            "当前工作区：{}；根目录：{}；唯一允许写入的目录：{}。其他宿主路径不可访问。{}",
            workspace.label, workspace.root_path, workspace.write_path, git
        );
        (workspace, context, system_context)
    };
    let mut protected = vec![services.root.clone()];
    for path in [
        app.path().app_config_dir(),
        app.path().app_log_dir(),
        app.path().app_cache_dir(),
    ]
    .into_iter()
    .flatten()
    {
        protected.push(path);
    }
    let scope = workspace.clone();
    let tools = work(move || {
        Ok(Arc::new(Toolbox::scoped(
            context,
            PathBuf::from(scope.root_path),
            PathBuf::from(scope.write_path),
            protected,
        )?))
    })
    .await?;
    let scope = workspace.clone();
    let owner = services.clone();
    let shared = shared.clone();
    let guard: ContextGuard = Arc::new(move || {
        let session = shared.0.lock().map_err(|_| storage())?;
        let current = workspaces::resolve_workspace(&owner, &session, &scope.id)?;
        if current.generation != scope.generation || current.root_path != scope.root_path {
            return Err(AgentError::new(
                "AGENT_REPOSITORY_CHANGED",
                "工作区已变化，请重新选择引用或发送消息。",
            ));
        }
        Ok(())
    });
    guard()?;
    Ok((workspace, tools, system_context, guard))
}

/// 预览用户明确选择的资料，读取范围由当前工作区确定；不保存、不发模型请求。
#[tauri::command]
pub async fn agent_preview_references(
    app: tauri::AppHandle,
    state: State<'_, AgentState>,
    desktop: State<'_, DesktopState>,
    workspace_id: String,
    references: Vec<UserReferenceSpec>,
) -> AgentResult<Vec<VerifiedReference>> {
    let services = state.services()?;
    let (_, tools, _, guard) =
        workspace_context(&app, &services, desktop.inner(), &workspace_id).await?;
    let verified = verify_references(&tools, &references, &Cancellation::new()).await?;
    guard()?;
    Ok(verified)
}

/// 在当前聊天配置下预览预算和记忆；迟到结果由工作区代次和前端草稿版本共同丢弃。
#[tauri::command]
pub async fn agent_preview_context(
    app: tauri::AppHandle,
    state: State<'_, AgentState>,
    desktop: State<'_, DesktopState>,
    id: String,
    text: String,
    references: Option<Vec<UserReferenceSpec>>,
) -> AgentResult<ContextPreviewResult> {
    let services = state.services()?;
    let chat = work({
        let services = services.clone();
        let id = id.clone();
        move || {
            idle(&services, &id)?;
            services.store.read_chat(&id)
        }
    })
    .await?;
    let (_, tools, system, guard) =
        workspace_context(&app, &services, desktop.inner(), &chat.session.workspace_id).await?;
    let references = verify_references(
        &tools,
        &references.unwrap_or_default(),
        &Cancellation::new(),
    )
    .await?;
    guard()?;
    work(move || {
        guard()?;
        services
            .runtime
            .preview_context(&id, &text, &tools, &system, references)
    })
    .await
}
