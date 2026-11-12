//! AI command 只暴露具体业务动作，所有目录都从已打开项目取得。
use super::DesktopState;
use crate::{
    ai_runtime::{self, AiState, Binding, Runtime},
    ai_settings::{self, CredentialAction, ModelRef, Permission, Provider, Snapshot},
};
use gitmaster_core::git::{coordinator::CoordinationKey, OperationError};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::Path;
use std::sync::{Arc, Mutex};
use tauri::{ipc::Channel, Manager, State};

static MODEL_GATE: Mutex<()> = Mutex::new(());

/// 用户主动检查当前项目的沙箱条件，检查本身不改变目录权限。
#[tauri::command]
pub async fn read_ai_sandbox_status(
    app: tauri::AppHandle,
    window: tauri::Window,
    state: State<'_, DesktopState>,
    repository_id: String,
) -> Result<Value, OperationError> {
    let scope = binding(&state, &repository_id)?;
    super::blocking("read_ai_sandbox_status", move || {
        if let Ok(runtime) = app.state::<AiState>().existing(window.label()) {
            let _ = runtime.request("invalidateSandbox", json!({}));
        }
        serde_json::to_value(crate::ai_sandbox::inspect(&scope.root)?)
            .map_err(|_| OperationError::new("AI_PROTOCOL"))
    })
    .await
}
/// 准备权限修复预览，不执行脚本或修改项目。
#[tauri::command]
pub async fn prepare_ai_sandbox_repair(
    app: tauri::AppHandle,
    window: tauri::Window,
    state: State<'_, DesktopState>,
    repository_id: String,
) -> Result<Value, OperationError> {
    let scope = binding(&state, &repository_id)?;
    app.state::<crate::ai_sandbox::SandboxRepairs>().prepare(
        window.label(),
        &scope.root,
        &app.path()
            .app_data_dir()
            .map_err(|_| OperationError::new("AI_SETTINGS_IO"))?,
    )
}
/// 仅接受用户确认后的原生票据，不接受任意目录、脚本或 Shell 参数。
#[tauri::command]
pub async fn execute_ai_sandbox_repair(
    app: tauri::AppHandle,
    window: tauri::Window,
    state: State<'_, DesktopState>,
    repository_id: String,
    confirmation_id: String,
) -> Result<Value, OperationError> {
    let scope = binding(&state, &repository_id)?;
    super::blocking("execute_ai_sandbox_repair", move || {
        let _guard = model_gate()?;
        if app
            .state::<AiState>()
            .activity(window.label())?
            .get("busy")
            .and_then(Value::as_bool)
            == Some(true)
        {
            return Err(OperationError::new("AI_BUSY"));
        }
        let result = app.state::<crate::ai_sandbox::SandboxRepairs>().execute(
            window.label(),
            &scope.root,
            &confirmation_id,
        )?;
        if let Ok(runtime) = app.state::<AiState>().existing(window.label()) {
            let _ = runtime.request("invalidateSandbox", json!({}));
        }
        Ok(result)
    })
    .await
}
/// 会话归属同时核验真实工作树，路径失效时不能把两个失败结果当成相同目录。
fn same_workspace(left: &Path, right: &Path) -> bool {
    match (std::fs::canonicalize(left), std::fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}
/// 配置发布与模型任务接纳共用短锁，避免空闲核验后突然接纳旧凭据任务。
fn model_gate() -> Result<std::sync::MutexGuard<'static, ()>, OperationError> {
    MODEL_GATE
        .lock()
        .map_err(|_| OperationError::new("AI_RUNTIME_FAILED"))
}

/// 从已打开项目捕获真实路径、公共目录队列键及协调器。
fn binding(state: &DesktopState, id: &str) -> Result<Binding, OperationError> {
    let shared = state.lock()?;
    let repository = &shared
        .active
        .as_ref()
        .filter(|(repo, _)| repo.id == id)
        .ok_or_else(|| OperationError::new("STALE_REQUEST"))?
        .0;
    Ok(Binding {
        git: shared
            .git
            .clone()
            .ok_or_else(|| OperationError::new("GIT_NOT_FOUND"))?,
        repository: repository.clone(),
        permission: Permission::ReadOnly,
        root: repository.root.clone(),
        repository_key: std::fs::canonicalize(&repository.common_dir)
            .map_err(|_| OperationError::new("AI_SESSION_PROJECT"))?
            .to_string_lossy()
            .into_owned(),
        key: CoordinationKey::repository(repository)?,
        coordinator: shared.coordinator.clone(),
        protected: false,
    })
}
/// 所有阻塞进程/凭据调用都离开桌面事件线程。
async fn runtime_call<T: Send + 'static>(
    app: tauri::AppHandle,
    owner: String,
    job: impl FnOnce(Arc<Runtime>) -> Result<T, OperationError> + Send + 'static,
) -> Result<T, OperationError> {
    super::blocking("ai_command", move || {
        let runtime = app.state::<AiState>().get(&app, &owner)?;
        job(runtime)
    })
    .await
}
/// 验证宿主返回的会话仍属于捕获项目，再签发写入绑定。
fn bind_session(
    runtime: &Runtime,
    value: &Value,
    mut scope: Binding,
) -> Result<(), OperationError> {
    scope.permission = serde_json::from_value(
        value
            .get("permission")
            .cloned()
            .ok_or_else(|| OperationError::new("AI_PROTOCOL"))?,
    )
    .map_err(|_| OperationError::new("AI_PROTOCOL"))?;
    let id = value
        .get("sessionId")
        .and_then(Value::as_str)
        .filter(|id| id.len() == 36)
        .ok_or_else(|| OperationError::new("AI_PROTOCOL"))?;
    let root = value
        .get("workspaceRoot")
        .and_then(Value::as_str)
        .ok_or_else(|| OperationError::new("AI_PROTOCOL"))?;
    if !same_workspace(Path::new(root), &scope.root)
        || value.get("repositoryKey").and_then(Value::as_str) != Some(scope.repository_key.as_str())
    {
        return Err(OperationError::new("AI_SESSION_PROJECT"));
    }
    runtime
        .bindings
        .lock()
        .map_err(|_| OperationError::new("AI_RUNTIME_FAILED"))?
        .entry(id.to_owned())
        .or_insert(scope);
    Ok(())
}
/// 只返回脱敏配置，不启动 Agent 或发起 API 请求。
#[tauri::command]
pub async fn read_ai_settings(app: tauri::AppHandle) -> Result<Snapshot, OperationError> {
    let path = ai_runtime::settings_path(&app)?;
    super::blocking("read_ai_settings", move || {
        Ok(ai_settings::snapshot(ai_settings::read(&path)?))
    })
    .await
}
/// 保存单个提供商及明确的凭据动作，然后同步已启动的运行时。
#[tauri::command]
pub async fn save_ai_provider(
    app: tauri::AppHandle,
    expected_revision: u64,
    provider: Provider,
    credential_action: CredentialAction,
) -> Result<Snapshot, OperationError> {
    let path = ai_runtime::settings_path(&app)?;
    super::blocking("save_ai_provider", move || {
        let _guard = model_gate()?;
        if !matches!(&credential_action, CredentialAction::Keep) {
            app.state::<AiState>().ensure_credentials_idle()?;
        }
        let mut snapshot =
            ai_settings::save_provider(&path, expected_revision, provider, credential_action)?;
        if app.state::<AiState>().sync(&snapshot.settings).is_err() {
            snapshot.warning = Some("配置已保存；运行时同步失败，下次发送前将重试。".into());
        }
        Ok(snapshot)
    })
    .await
}
/// 删除服务商不会删除历史会话或自动替换历史模型。
#[tauri::command]
pub async fn delete_ai_provider(
    app: tauri::AppHandle,
    expected_revision: u64,
    provider_id: String,
) -> Result<Snapshot, OperationError> {
    let path = ai_runtime::settings_path(&app)?;
    super::blocking("delete_ai_provider", move || {
        let _guard = model_gate()?;
        app.state::<AiState>().ensure_credentials_idle()?;
        let mut snapshot = ai_settings::delete_provider(&path, expected_revision, &provider_id)?;
        if app.state::<AiState>().sync(&snapshot.settings).is_err() {
            snapshot.warning = Some("配置已保存；运行时同步失败，下次发送前将重试。".into());
        }
        Ok(snapshot)
    })
    .await
}
/// 新会话默认选择与历史会话模型分开保存。
#[tauri::command]
pub async fn update_ai_defaults(
    app: tauri::AppHandle,
    expected_revision: u64,
    model: Option<ModelRef>,
    permission: Permission,
) -> Result<Snapshot, OperationError> {
    let path = ai_runtime::settings_path(&app)?;
    super::blocking("update_ai_defaults", move || {
        ai_settings::update_defaults(&path, expected_revision, model, permission)
    })
    .await
}
/// 队列复用公共目录，列表仍按当前真实工作树隔离。
#[tauri::command]
pub async fn list_ai_sessions(
    app: tauri::AppHandle,
    window: tauri::Window,
    state: State<'_, DesktopState>,
    repository_id: String,
    offset: Option<u64>,
) -> Result<Value, OperationError> {
    let scope = binding(&state, &repository_id)?;
    runtime_call(app, window.label().to_owned(), move |runtime| {
        runtime.request(
            "list",
            json!({"repositoryKey":scope.repository_key,"workspaceRoot":scope.root,"offset":offset.unwrap_or(0)}),
        )
    })
    .await
}
/// 创建时绑定真实项目；前端不能直接传入工作目录。
#[tauri::command]
pub async fn create_ai_session(
    app: tauri::AppHandle,
    window: tauri::Window,
    state: State<'_, DesktopState>,
    repository_id: String,
    model: ModelRef,
    permission: Permission,
) -> Result<Value, OperationError> {
    let scope = binding(&state, &repository_id)?;
    runtime_call(app, window.label().to_owned(), move |runtime| { runtime.refresh_settings()?; let session = runtime.request("create", json!({"workspaceRoot":scope.root,"repositoryKey":scope.repository_key,"model":model,"permission":permission}))?; bind_session(&runtime, &session, scope)?; Ok(session) }).await
}
/// 历史恢复不发送用户输入，也不自动重放命令。
#[tauri::command]
pub async fn read_ai_session(
    app: tauri::AppHandle,
    window: tauri::Window,
    state: State<'_, DesktopState>,
    repository_id: String,
    session_id: String,
    after: u64,
) -> Result<Value, OperationError> {
    let scope = binding(&state, &repository_id)?;
    runtime_call(app, window.label().to_owned(), move |runtime| {
        let page = runtime.request("read", json!({"sessionId":session_id,"after":after}))?;
        bind_session(
            &runtime,
            page.get("session")
                .ok_or_else(|| OperationError::new("AI_PROTOCOL"))?,
            scope,
        )?;
        Ok(page)
    })
    .await
}
/// 只在已有会话上安装事件通道，防止读取另一个窗口的数据。
#[tauri::command]
pub async fn subscribe_ai_events(
    app: tauri::AppHandle,
    window: tauri::Window,
    session_id: String,
    channel: Channel<Value>,
) -> Result<(), OperationError> {
    let owner = window.label().to_owned();
    runtime_call(app, owner.clone(), move |runtime| {
        runtime.subscribe(&owner, session_id, channel)
    })
    .await
}
/// 消息只在会话项目仍然打开时接纳；草稿标记用于执行前的写保护。
#[tauri::command]
pub async fn send_ai_message(
    app: tauri::AppHandle,
    window: tauri::Window,
    state: State<'_, DesktopState>,
    repository_id: String,
    session_id: String,
    client_message_id: String,
    text: String,
    has_unsaved_files: bool,
) -> Result<Value, OperationError> {
    let mut scope = binding(&state, &repository_id)?;
    scope.protected = has_unsaved_files;
    runtime_call(app, window.label().to_owned(), move |runtime| {
        let _guard = model_gate()?;
        runtime.refresh_settings()?;
        let mut bindings = runtime
            .bindings
            .lock()
            .map_err(|_| OperationError::new("AI_RUNTIME_FAILED"))?;
        let old = bindings
            .get(&session_id)
            .ok_or_else(|| OperationError::new("AI_SESSION_NOT_FOUND"))?;
        if old.repository_key != scope.repository_key || !same_workspace(&old.root, &scope.root) {
            return Err(OperationError::new("AI_SESSION_PROJECT"));
        }
        let previous_protected = old.protected;
        scope.permission = old.permission.clone();
        bindings.insert(session_id.clone(), scope);
        drop(bindings);
        let result = runtime.request(
            "send",
            json!({"sessionId":session_id.clone(),"clientMessageId":client_message_id,"text":text}),
        );
        if result.is_err() {
            // 拒绝或结果不明时不能撤掉原任务的草稿保护；只保留更严格的保护。
            if let Ok(mut bindings) = runtime.bindings.lock() {
                if let Some(binding) = bindings.get_mut(&session_id) {
                    binding.protected |= previous_protected;
                }
            }
        }
        result
    })
    .await
}
/// 取消只指向当前运行 ID，不终止其他会话或手动终端。
#[tauri::command]
pub async fn cancel_ai_run(
    app: tauri::AppHandle,
    window: tauri::Window,
    session_id: String,
    run_id: String,
) -> Result<Value, OperationError> {
    runtime_call(app, window.label().to_owned(), move |runtime| {
        runtime.request("cancel", json!({"sessionId":session_id,"runId":run_id}))
    })
    .await
}
/// 会话编辑只允许标题、确切模型与权限三个字段。
#[derive(Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<ModelRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    permission: Option<Permission>,
}
/// 更新会话与上游持久选择，忙碌时由宿主拒绝。
#[tauri::command]
pub async fn update_ai_session(
    app: tauri::AppHandle,
    window: tauri::Window,
    session_id: String,
    patch: SessionPatch,
) -> Result<Value, OperationError> {
    runtime_call(app, window.label().to_owned(), move |runtime| {
        let mut params =
            serde_json::to_value(patch).map_err(|_| OperationError::new("AI_INVALID_INPUT"))?;
        params["sessionId"] = session_id.into();
        let result = runtime.request("update", params)?;
        let id = result
            .get("sessionId")
            .and_then(Value::as_str)
            .ok_or_else(|| OperationError::new("AI_PROTOCOL"))?;
        if let Some(binding) = runtime
            .bindings
            .lock()
            .map_err(|_| OperationError::new("AI_RUNTIME_FAILED"))?
            .get_mut(id)
        {
            binding.permission = serde_json::from_value(result["permission"].clone())
                .map_err(|_| OperationError::new("AI_PROTOCOL"))?;
        }
        Ok(result)
    })
    .await
}
/// 删除前由 UI 处理未完成任务确认，宿主再次核验状态。
#[tauri::command]
pub async fn delete_ai_session(
    app: tauri::AppHandle,
    window: tauri::Window,
    session_id: String,
    confirmed: bool,
) -> Result<Value, OperationError> {
    runtime_call(app, window.label().to_owned(), move |runtime| {
        runtime.request(
            "delete",
            json!({"sessionId":session_id,"confirmed":confirmed}),
        )
    })
    .await
}
/// 一次性批准与拒绝共用过期校验；空回复不会转换为许可。
#[tauri::command]
pub async fn resolve_ai_approval(
    app: tauri::AppHandle,
    window: tauri::Window,
    session_id: String,
    approval_id: String,
    decision: String,
) -> Result<Value, OperationError> {
    if !matches!(decision.as_str(), "allowed-once" | "rejected") {
        return Err(OperationError::new("AI_INVALID_INPUT"));
    }
    runtime_call(app, window.label().to_owned(), move |runtime| {
        runtime.request(
            "decision",
            json!({"sessionId":session_id,"decisionId":approval_id,"value":decision}),
        )
    })
    .await
}
/// 问题回答仍需通过上游结构验证，不会产生审批授权。
#[tauri::command]
pub async fn answer_ai_question(
    app: tauri::AppHandle,
    window: tauri::Window,
    session_id: String,
    question_id: String,
    answer: Value,
) -> Result<Value, OperationError> {
    runtime_call(app, window.label().to_owned(), move |runtime| {
        runtime.request(
            "decision",
            json!({"sessionId":session_id,"decisionId":question_id,"value":answer}),
        )
    })
    .await
}
/// 读取 UI 独立输出游标，不消耗模型尚未领取的结果。
#[tauri::command]
pub async fn read_ai_jobs(
    app: tauri::AppHandle,
    window: tauri::Window,
    session_id: String,
) -> Result<Value, OperationError> {
    let owner = window.label().to_owned();
    super::blocking("ai_jobs", move || {
        let runtime = app.state::<AiState>().existing(&owner)?;
        runtime.request("jobs", json!({"sessionId":session_id}))
    })
    .await
}
/// 全局活动只包含布尔状态，用于跨项目的编辑和退出保护。
#[tauri::command]
pub async fn read_ai_activity(
    app: tauri::AppHandle,
    window: tauri::Window,
) -> Result<Value, OperationError> {
    let owner = window.label().to_owned();
    super::blocking("ai_activity", move || {
        app.state::<AiState>().activity(&owner)
    })
    .await
}
/// 测试用户已保存的模型，明确点击后才发起一个小额真实请求。
#[tauri::command]
pub async fn test_ai_provider(
    app: tauri::AppHandle,
    window: tauri::Window,
    provider_id: String,
    model_id: String,
) -> Result<Value, OperationError> {
    runtime_call(app, window.label().to_owned(), move |runtime| {
        let _guard = model_gate()?;
        runtime.refresh_settings()?;
        runtime.request(
            "testModel",
            json!({"providerId":provider_id,"modelId":model_id}),
        )
    })
    .await
}
/// 停止仅限目标会话可见的后台任务。
#[tauri::command]
pub async fn stop_ai_job(
    app: tauri::AppHandle,
    window: tauri::Window,
    session_id: String,
    job_id: String,
) -> Result<Value, OperationError> {
    runtime_call(app, window.label().to_owned(), move |runtime| {
        runtime.request("stopJob", json!({"sessionId":session_id,"jobId":job_id}))
    })
    .await
}
/// 手动压缩只对空闲会话执行，复用上游摘要与持久化逻辑。
#[tauri::command]
pub async fn compact_ai_session(
    app: tauri::AppHandle,
    window: tauri::Window,
    session_id: String,
) -> Result<Value, OperationError> {
    runtime_call(app, window.label().to_owned(), move |runtime| {
        let _guard = model_gate()?;
        runtime.refresh_settings()?;
        runtime.request("compact", json!({"sessionId":session_id}))
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    /// 共享仓库元数据不代表工作树相同，失效路径必须拒绝。
    fn workspace_scope_rejects_siblings_and_missing_paths() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        assert!(same_workspace(first.path(), &first.path().join(".")));
        assert!(!same_workspace(first.path(), second.path()));
        assert!(!same_workspace(
            &first.path().join("missing"),
            &first.path().join("missing")
        ));
    }
}
