//! 桌面 Agent 适配；密钥仅在此进程内使用，IPC 读取从不返回秘密。
use super::DesktopState;
use crate::agent_credentials::SystemCredentialVault;
use gitmaster_core::agent::{
    provider::{Cancellation, ModelClient},
    runtime::{AgentRuntime, RunNotice, RunSnapshot, StartRun},
    tools::{git::GitContext, registry::Toolbox},
    *,
};
use serde::Serialize;
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tauri::{Emitter, Manager, State};

/// 初始化失败只禁用 AI，不影响普通 Git 功能。
pub struct AgentState(Result<Arc<AgentServices>, AgentError>);
struct AgentServices {
    store: Arc<AgentStore>,
    profiles: ProfileService<SystemCredentialVault>,
    runtime: Arc<AgentRuntime>,
    root: PathBuf,
    gate: Mutex<()>,
}
impl AgentState {
    /// 为每次 IPC 克隆服务句柄，不长期持有 Tauri 状态引用。
    fn services(&self) -> AgentResult<Arc<AgentServices>> {
        self.0.clone()
    }
}
/// 应用启动时恢复本地会话；事件只携带安全运行记录。
pub fn initialize_agent(app: &tauri::AppHandle) {
    let result = (|| {
        let root = app
            .path()
            .app_data_dir()
            .map_err(|_| storage())?
            .join("agent-v1");
        let store = Arc::new(AgentStore::open(root.clone())?);
        let handle = app.clone();
        let runtime = AgentRuntime::new(
            store.clone(),
            Arc::new(move |notice| {
                if let RunNotice::Record { event } = &notice {
                    if let EventKind::ToolResult { name, .. } = &event.payload {
                        if matches!(
                            name.as_str(),
                            "write_file"
                                | "git_stage"
                                | "git_unstage"
                                | "git_commit"
                                | "git_create_branch"
                                | "git_switch_branch"
                                | "git_fetch"
                                | "git_push"
                                | "git_pull"
                        ) {
                            if let Ok(mut session) = handle.state::<DesktopState>().0.lock() {
                                session.force_refresh = true;
                            }
                            let _ = handle.emit("agent:repository-changed", ());
                        }
                    }
                }
                let _ = handle.emit("agent:event", notice);
            }),
        )?;
        Ok(Arc::new(AgentServices {
            profiles: ProfileService::new(store.clone(), SystemCredentialVault),
            store,
            runtime,
            root,
            gate: Mutex::new(()),
        }))
    })();
    app.manage(AgentState(result));
}
/// 窗口销毁后不允许后台继续发起新工具；在途 Git 写正常收尾。
pub fn stop_agent(app: &tauri::AppHandle) {
    if let Some(state) = app.try_state::<AgentState>() {
        if let Ok(services) = state.services() {
            services.runtime.cancel_all();
        }
    }
}
/// 设置和 Keychain IO 移到阻塞池，错误不附带请求参数。
async fn work<T: Send + 'static>(
    task: impl FnOnce() -> AgentResult<T> + Send + 'static,
) -> AgentResult<T> {
    tauri::async_runtime::spawn_blocking(task)
        .await
        .map_err(|_| AgentError::new("AGENT_DESKTOP", "AI 桌面任务异常，请重试。"))?
}
/// 读取普通配置和密钥存在状态。
#[tauri::command]
pub async fn agent_read_settings(state: State<'_, AgentState>) -> AgentResult<AgentSettingsView> {
    let services = state.services()?;
    work(move || {
        let mut view = services.profiles.view()?;
        if !services.runtime.recovery_warnings().is_empty() {
            view.warnings
                .push("部分聊天无法恢复，原文件已保留，请检查此设备的数据目录。".into());
        }
        Ok(view)
    })
    .await
}
/// 保存 API 配置，明文 Key 仅传给系统密钥库。
#[tauri::command]
pub async fn agent_save_profile(
    state: State<'_, AgentState>,
    request: SaveProfile,
) -> AgentResult<AgentSettingsView> {
    let services = state.services()?;
    work(move || services.profiles.save(request)).await
}
/// 删除配置与其系统凭据，不删除聊天。
#[tauri::command]
pub async fn agent_delete_profile(
    state: State<'_, AgentState>,
    id: String,
) -> AgentResult<AgentSettingsView> {
    let services = state.services()?;
    work(move || services.profiles.delete(&id)).await
}
/// 为后续运行选择已保存配置。
#[tauri::command]
pub async fn agent_select_profile(
    state: State<'_, AgentState>,
    id: String,
) -> AgentResult<AgentSettingsView> {
    let services = state.services()?;
    work(move || {
        services.store.select_profile(&id)?;
        services.profiles.view()
    })
    .await
}
/// 权限调整仅作用于下一轮，正在运行的操作保留原快照。
#[tauri::command]
pub async fn agent_set_permission(
    state: State<'_, AgentState>,
    mode: PermissionMode,
) -> AgentResult<AgentSettingsView> {
    let services = state.services()?;
    work(move || {
        services.store.set_permission(mode)?;
        services.profiles.view()
    })
    .await
}
/// 使用已保存密钥发送最小文本请求，不创建聊天或启用工具。
#[tauri::command]
pub async fn agent_test_connection(state: State<'_, AgentState>, id: String) -> AgentResult<()> {
    let services = state.services()?;
    let (profile, key) = work(move || {
        let profile = services
            .store
            .settings()?
            .profiles
            .into_iter()
            .find(|profile| profile.id == id)
            .ok_or_else(profile_missing)?;
        let key = services.profiles.key_for(&profile)?;
        Ok((profile, key))
    })
    .await?;
    let events = [AgentEvent {
        version: 1,
        sequence: 1,
        session_id: new_id(),
        run_id: new_id(),
        timestamp: timestamp(),
        payload: EventKind::UserMessage {
            text: "请回复：连接成功".into(),
        },
    }];
    ModelClient::new()?
        .complete(
            &profile,
            &key,
            &events,
            &[],
            "仅验证连接，不调用工具。",
            &Cancellation::new(),
            &|_| {},
        )
        .await?;
    Ok(())
}
/// 列出全部本地聊天，单独报告损坏聊天。
#[tauri::command]
pub async fn agent_list_chats(state: State<'_, AgentState>) -> AgentResult<ChatList> {
    let services = state.services()?;
    work(move || services.store.list_chats()).await
}
/// 聊天关联后端当前项目，不接受前端任意项目路径。
#[tauri::command]
pub async fn agent_create_chat(
    state: State<'_, AgentState>,
    desktop: State<'_, DesktopState>,
) -> AgentResult<ChatSession> {
    let services = state.services()?;
    let path = desktop
        .0
        .lock()
        .map_err(|_| storage())?
        .active
        .as_ref()
        .map(|(handle, _)| handle.root.to_string_lossy().into_owned());
    work(move || {
        let settings = services.store.settings()?;
        services.store.create_chat(
            settings
                .active_profile_id
                .as_deref()
                .ok_or_else(profile_missing)?,
            path,
        )
    })
    .await
}
/// 返回持久记录和活动运行快照，供重启/切换聊天恢复。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatView {
    record: ChatRecord,
    run: Option<RunSnapshot>,
}
/// 只按应用签发的聊天 ID 读取，不接受路径。
#[tauri::command]
pub async fn agent_read_chat(state: State<'_, AgentState>, id: String) -> AgentResult<ChatView> {
    let services = state.services()?;
    work(move || {
        Ok(ChatView {
            record: services.store.read_chat(&id)?,
            run: services.runtime.snapshot(&id)?,
        })
    })
    .await
}
/// 修改标题或归档；活动运行必须先结束。
#[tauri::command]
pub async fn agent_update_chat(
    state: State<'_, AgentState>,
    id: String,
    title: String,
    archived: bool,
) -> AgentResult<ChatSession> {
    let services = state.services()?;
    work(move || {
        let _gate = services.gate.lock().map_err(|_| storage())?;
        idle(&services, &id)?;
        privacy::ensure_safe_text(&title, None)?;
        services.store.update_chat(&id, &title, archived)
    })
    .await
}
/// 删除指定本地聊天，运行中拒绝删除日志。
#[tauri::command]
pub async fn agent_delete_chat(state: State<'_, AgentState>, id: String) -> AgentResult<()> {
    let services = state.services()?;
    work(move || {
        let _gate = services.gate.lock().map_err(|_| storage())?;
        idle(&services, &id)?;
        services.store.delete_chat(&id)
    })
    .await
}
/// 后端快照绑定仓库代次，切换项目后不能批准旧工具。
#[tauri::command]
pub async fn agent_send_message(
    app: tauri::AppHandle,
    state: State<'_, AgentState>,
    desktop: State<'_, DesktopState>,
    id: String,
    text: String,
) -> AgentResult<String> {
    let services = state.services()?;
    let shared = desktop.inner().clone();
    let (context, epoch, token, path) = {
        let session = shared.0.lock().map_err(|_| storage())?;
        let context = match (&session.git, &session.active) {
            (Some(git), Some((repository, _))) => Some(GitContext {
                git: git.clone(),
                repository: repository.clone(),
                coordinator: session.coordinator.clone(),
            }),
            _ => None,
        };
        let path = context
            .as_ref()
            .map(|value| value.repository.root.to_string_lossy().into_owned());
        (
            context,
            session.epoch,
            session.agent_repository_request,
            path,
        )
    };
    let mut protected = vec![services.root.clone()];
    for directory in [
        app.path().app_config_dir(),
        app.path().app_log_dir(),
        app.path().app_cache_dir(),
    ] {
        if let Ok(path) = directory {
            protected.push(path);
        }
    }
    let (profile, key, mode, tools) = work({
        let services = services.clone();
        move || {
            let settings = services.store.settings()?;
            let profile = settings
                .profiles
                .into_iter()
                .find(|value| Some(&value.id) == settings.active_profile_id.as_ref())
                .ok_or_else(profile_missing)?;
            let key = services.profiles.key_for(&profile)?;
            Ok((
                profile,
                key,
                settings.default_permission_mode,
                Arc::new(Toolbox::new(context, protected)?),
            ))
        }
    })
    .await?;
    let system_context = {
        let session = shared.0.lock().map_err(|_| storage())?;
        match &session.active {
            Some((handle, _)) => format!(
                "当前仓库 ID：{}；当前项目：{}。",
                handle.id,
                handle.root.display()
            ),
            None => "当前未打开项目，只能使用用户明确指定的绝对文件路径。".into(),
        }
    };
    let guard = Arc::new(move || {
        let session = shared.0.lock().map_err(|_| storage())?;
        if session.epoch != epoch || session.agent_repository_request != token {
            Err(AgentError::new(
                "AGENT_REPOSITORY_CHANGED",
                "项目环境已切换，旧运行已停止，请重新发送消息。",
            ))
        } else {
            Ok(())
        }
    });
    let _gate = services.gate.lock().map_err(|_| storage())?;
    services.runtime.start(StartRun {
        chat_id: id,
        text,
        profile,
        key,
        mode,
        tools,
        repository_path: path,
        system_context,
        guard,
    })
}
/// 取消指定运行，不声称回滚已经完成的写入。
#[tauri::command]
pub fn agent_cancel_run(
    state: State<'_, AgentState>,
    id: String,
    run_id: String,
) -> AgentResult<()> {
    state.services()?.runtime.cancel(&id, &run_id)
}
/// 只提交布尔决定，工具参数始终由后端保存。
#[tauri::command]
pub fn agent_resolve_approval(
    state: State<'_, AgentState>,
    id: String,
    run_id: String,
    approval_id: String,
    approved: bool,
) -> AgentResult<()> {
    state
        .services()?
        .runtime
        .resolve(&id, &run_id, &approval_id, approved)
}
/// 此设备分类统计；相同或嵌套目录不重复累计。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceUsage {
    total_bytes: u64,
    categories: Vec<UsageCategory>,
    locations: Vec<String>,
}
#[derive(Serialize)]
struct UsageCategory {
    id: String,
    label: String,
    bytes: u64,
}
/// 读取真实文件容量，读取错误明确返回，不能把失败当成零。
#[tauri::command]
pub async fn agent_read_device_usage(
    app: tauri::AppHandle,
    state: State<'_, AgentState>,
) -> AgentResult<DeviceUsage> {
    let services = state.services()?;
    let paths = vec![
        ("cache", "内容缓存", app.path().app_cache_dir()),
        ("logs", "日志", app.path().app_log_dir()),
        ("settings", "应用设置", app.path().app_config_dir()),
        ("other", "其他应用数据", app.path().app_data_dir()),
    ];
    let paths = paths
        .into_iter()
        .map(|(id, label, path)| Ok((id, label, path.map_err(|_| storage())?)))
        .collect::<AgentResult<Vec<_>>>()?;
    work(move || {
        let usage = services.store.usage()?;
        let mut categories = vec![
            UsageCategory {
                id: "chats".into(),
                label: "聊天记录".into(),
                bytes: usage.chat_bytes,
            },
            UsageCategory {
                id: "archived".into(),
                label: "已归档聊天".into(),
                bytes: usage.archived_bytes,
            },
            UsageCategory {
                id: "agent".into(),
                label: "AI 配置与其他数据".into(),
                bytes: usage.other_bytes,
            },
        ];
        let mut excluded = vec![services.root.clone()];
        let mut locations = vec![usage.location];
        for (id, label, path) in paths {
            let bytes = count_excluding(&path, &excluded)?;
            categories.push(UsageCategory {
                id: id.into(),
                label: label.into(),
                bytes,
            });
            if !locations.contains(&path.to_string_lossy().into_owned()) {
                locations.push(path.to_string_lossy().into_owned());
            }
            excluded.push(path);
        }
        Ok(DeviceUsage {
            total_bytes: categories.iter().map(|value| value.bytes).sum(),
            categories,
            locations,
        })
    })
    .await
}
/// 遍历目录但不跟随符号链接；已统计的子树直接跳过。
fn count_excluding(path: &Path, excluded: &[PathBuf]) -> AgentResult<u64> {
    if excluded.iter().any(|prefix| path.starts_with(prefix)) {
        return Ok(0);
    }
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(_) => return Err(storage()),
    };
    if metadata.file_type().is_symlink() {
        return Ok(0);
    }
    if metadata.is_file() {
        return Ok(metadata.len());
    }
    let mut total = 0;
    for entry in std::fs::read_dir(path).map_err(|_| storage())? {
        total += count_excluding(&entry.map_err(|_| storage())?.path(), excluded)?;
    }
    Ok(total)
}
/// 所有聊天修改与启动共享短锁，避免检查后被新运行抢占。
fn idle(services: &AgentServices, id: &str) -> AgentResult<()> {
    if services.runtime.snapshot(id)?.is_some() {
        Err(AgentError::new("AGENT_RUNNING", "请先停止该聊天的运行。"))
    } else {
        Ok(())
    }
}
/// 平台错误不透传路径、凭据或底层响应。
fn storage() -> AgentError {
    AgentError::new(
        "AGENT_STORAGE",
        "无法读取 AI 本地数据，请检查应用目录权限。",
    )
}
/// 未配置时引导进入首配，不猜测模型或服务商。
fn profile_missing() -> AgentError {
    AgentError::new("AGENT_PROFILE_MISSING", "请先配置并选择模型 API。")
}

#[cfg(test)]
mod tests {
    use super::*;
    /// 重叠应用目录不重复统计，子目录单独分类后从父目录排除。
    #[test]
    fn device_usage_excludes_already_counted_subtrees() {
        let root = std::env::temp_dir().join(format!("gitmaster-agent-usage-{}", new_id()));
        let nested = root.join("agent");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("history"), b"12345").unwrap();
        std::fs::write(root.join("settings"), b"123").unwrap();
        assert_eq!(
            count_excluding(&root, std::slice::from_ref(&nested)).unwrap(),
            3
        );
        assert_eq!(
            count_excluding(&root, std::slice::from_ref(&root)).unwrap(),
            0
        );
        assert_eq!(count_excluding(&nested, &[]).unwrap(), 5);
        std::fs::remove_dir_all(root).unwrap();
    }
    /// 普通状态刷新改变 Git 读取代次，但不得使 Agent 项目身份失效。
    #[test]
    fn ordinary_refresh_keeps_agent_project_generation() {
        let mut session = super::super::Session::default();
        let generation = session.agent_repository_request;
        let request = session.repository_request;
        session.begin_repository_request_with(false).unwrap();
        assert_ne!(session.repository_request, request);
        assert_eq!(session.agent_repository_request, generation);
    }
}
