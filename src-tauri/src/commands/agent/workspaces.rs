use super::*;
use tauri_plugin_dialog::DialogExt;

/// 只展示桌面当前持有的两类工作区，不能通过 IPC 注册任意历史路径。
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentWorkspace {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub root_path: String,
    pub write_path: String,
    pub generation: u64,
    pub git_available: bool,
}

/// 根据应用固定路径及当前目录生成能力清单；默认项不绑定项目切换代次。
pub(super) fn workspaces(
    services: &AgentServices,
    session: &super::super::Session,
) -> Vec<AgentWorkspace> {
    let mut result = vec![AgentWorkspace {
        id: "default".into(),
        kind: "default".into(),
        label: "默认工作区".into(),
        root_path: services.application_root.to_string_lossy().into_owned(),
        write_path: services.output_root.to_string_lossy().into_owned(),
        generation: 0,
        git_available: false,
    }];
    if let Some(root) = session
        .agent_folder
        .as_ref()
        .or_else(|| session.active.as_ref().map(|(handle, _)| &handle.root))
    {
        let path = root.to_string_lossy().into_owned();
        result.push(AgentWorkspace {
            id: workspace_id(Some(&path)),
            kind: "folder".into(),
            label: root
                .file_name()
                .unwrap_or(root.as_os_str())
                .to_string_lossy()
                .into_owned(),
            root_path: path.clone(),
            write_path: path,
            generation: session.agent_repository_request,
            git_available: session
                .active
                .as_ref()
                .is_some_and(|(handle, _)| &handle.root == root)
                && session.git.is_some(),
        });
    }
    result
}

/// 历史路径仅供展示，执行前必须重新匹配当前目录能力。
pub(super) fn resolve_workspace(
    services: &AgentServices,
    session: &super::super::Session,
    id: &str,
) -> AgentResult<AgentWorkspace> {
    let workspace = workspaces(services, session)
        .into_iter()
        .find(|value| value.id == id)
        .ok_or_else(|| {
            AgentError::new(
                "AGENT_WORKSPACE_UNAVAILABLE",
                "请先打开此聊天所属的文件夹，历史记录仍可阅读。",
            )
        })?;
    for path in [&workspace.root_path, &workspace.write_path] {
        let current = std::fs::canonicalize(path).map_err(|_| {
            AgentError::new(
                "AGENT_WORKSPACE_UNAVAILABLE",
                "工作区目录已失联，请重新打开文件夹。",
            )
        })?;
        if current != PathBuf::from(path) || !current.is_dir() {
            return Err(AgentError::new(
                "AGENT_WORKSPACE_CHANGED",
                "工作区路径已改变，请重新打开文件夹。",
            ));
        }
    }
    Ok(workspace)
}

/// 读取清单不会自动创建用户工作区或恢复历史执行权限。
#[tauri::command]
pub fn agent_list_workspaces(
    state: State<'_, AgentState>,
    desktop: State<'_, DesktopState>,
) -> AgentResult<Vec<AgentWorkspace>> {
    let services = state.services()?;
    let session = desktop.0.lock().map_err(|_| storage())?;
    Ok(workspaces(&services, &session))
}

/// 通过原生选择器替换唯一当前文件夹；取消保留原目录，不接受前端路径。
#[tauri::command]
pub async fn agent_choose_folder(
    app: tauri::AppHandle,
    state: State<'_, AgentState>,
    desktop: State<'_, DesktopState>,
) -> AgentResult<Vec<AgentWorkspace>> {
    let services = state.services()?;
    let shared = desktop.inner().clone();
    let generation = shared
        .0
        .lock()
        .map_err(|_| storage())?
        .agent_repository_request;
    work(move || {
        let picked = app
            .dialog()
            .file()
            .set_title("打开 Agent 使用的文件夹")
            .blocking_pick_folder();
        let mut session = shared.0.lock().map_err(|_| storage())?;
        if session.agent_repository_request != generation {
            return Err(AgentError::new(
                "AGENT_WORKSPACE_CHANGED",
                "目录已被其他操作切换，请重试。",
            ));
        }
        if let Some(picked) = picked {
            let path = picked
                .into_path()
                .map_err(|_| storage())?
                .canonicalize()
                .map_err(|_| storage())?;
            if !path.is_dir() {
                return Err(AgentError::new(
                    "AGENT_WORKSPACE_UNAVAILABLE",
                    "请选择可用文件夹。",
                ));
            }
            session.agent_folder = Some(path);
            session.agent_repository_request += 1;
            let _ = app.emit("agent:workspaces-changed", ());
        }
        Ok(workspaces(&services, &session))
    })
    .await
}

/// 会话配置使用运行门禁，已有运行不会接受迟到参数变更。
#[tauri::command]
pub async fn agent_configure_chat(
    state: State<'_, AgentState>,
    id: String,
    model_config_id: String,
    permission_mode: PermissionMode,
    parameters: Option<ModelParameters>,
) -> AgentResult<ChatSession> {
    let services = state.services()?;
    work(move || {
        let _gate = services.gate.lock().map_err(|_| storage())?;
        idle(&services, &id)?;
        services
            .store
            .configure_chat(&id, &model_config_id, permission_mode, parameters)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 创建独占临时应用目录，不访问系统密钥库或用户项目。
    fn fixture() -> (PathBuf, AgentServices) {
        let root = std::env::temp_dir().join(format!("gitmaster-workspaces-{}", new_id()));
        std::fs::create_dir_all(root.join("application")).unwrap();
        std::fs::create_dir_all(root.join("output")).unwrap();
        let store = Arc::new(AgentStore::open(root.join("store")).unwrap());
        let services = AgentServices {
            profiles: ProfileService::new(store.clone(), SystemCredentialVault),
            runtime: AgentRuntime::new(store.clone(), Arc::new(|_| {})).unwrap(),
            store,
            root: root.join("store"),
            application_root: root.join("application").canonicalize().unwrap(),
            output_root: root.join("output").canonicalize().unwrap(),
            gate: Mutex::new(()),
            configuration_requests: Mutex::new(std::collections::HashMap::new()),
        };
        (root, services)
    }

    /// 默认工作区的执行身份不受当前文件夹切换影响，清单最多两项。
    #[test]
    fn default_workspace_survives_folder_changes_and_plain_folders_need_no_git() {
        let (root, services) = fixture();
        let mut session = super::super::super::Session::default();
        let default = resolve_workspace(&services, &session, "default").unwrap();
        std::fs::create_dir(root.join("folder")).unwrap();
        session.agent_folder = Some(root.join("folder").canonicalize().unwrap());
        session.agent_repository_request += 1;
        let choices = workspaces(&services, &session);
        assert_eq!(choices.len(), 2);
        assert!(!choices[1].git_available);
        assert_eq!(
            resolve_workspace(&services, &session, "default")
                .unwrap()
                .generation,
            default.generation
        );
        let former_id = choices[1].id.clone();
        session.agent_folder = None;
        session.agent_repository_request += 1;
        assert!(resolve_workspace(&services, &session, &former_id).is_err());
        drop(services);
        assert!(root.starts_with(std::env::temp_dir()));
        std::fs::remove_dir_all(root).unwrap();
    }
}
