use super::*;

/// 只读取已保存的单次修改工件，不将历史路径解释为文件系统读取授权。
#[tauri::command]
pub async fn agent_read_file_change(
    state: State<'_, AgentState>,
    id: String,
    artifact_id: String,
) -> AgentResult<FileChangeContent> {
    let services = state.services()?;
    work(move || services.store.read_file_change(&id, &artifact_id)).await
}

/// 按工作区读取本地记忆，跨工作区偏好需要显式开启。
#[tauri::command]
pub async fn agent_list_memories(
    state: State<'_, AgentState>,
    workspace_id: String,
    include_user: bool,
) -> AgentResult<Vec<MemoryEntry>> {
    let services = state.services()?;
    work(move || services.store.list_memories(&workspace_id, include_user)).await
}

/// 只有用户的管理动作保存记忆，模型工具不会直接调用此 IPC。
#[tauri::command]
pub async fn agent_save_memory(
    state: State<'_, AgentState>,
    desktop: State<'_, DesktopState>,
    request: MemoryInput,
) -> AgentResult<MemoryEntry> {
    let services = state.services()?;
    if let Some(workspace_id) = &request.workspace_id {
        let session = desktop.0.lock().map_err(|_| storage())?;
        workspaces::resolve_workspace(&services, &session, workspace_id)?;
    }
    work(move || services.store.save_memory(request)).await
}

/// 使用修订号删除，阻止旧弹窗误删刚编辑的记忆。
#[tauri::command]
pub async fn agent_delete_memory(
    state: State<'_, AgentState>,
    id: String,
    expected_revision: u64,
) -> AgentResult<()> {
    let services = state.services()?;
    work(move || services.store.delete_memory(&id, expected_revision)).await
}

/// 预览检索来源与排除原因；实际模型请求会重新按真实上下文预算检索。
#[tauri::command]
pub async fn agent_select_memories(
    state: State<'_, AgentState>,
    workspace_id: String,
    include_user: bool,
    query: String,
    excluded_ids: Vec<String>,
    budget_tokens: usize,
) -> AgentResult<MemorySelection> {
    let services = state.services()?;
    work(move || {
        services.store.select_memories(
            &workspace_id,
            include_user,
            &query,
            &excluded_ids,
            budget_tokens,
        )
    })
    .await
}

/// 聊天搜索按工作区和归档状态隔离，正文命中携带可定位事件序号。
#[tauri::command]
pub async fn agent_search_chats(
    state: State<'_, AgentState>,
    workspace_id: String,
    query: String,
    archived: bool,
) -> AgentResult<ChatSearchResults> {
    let services = state.services()?;
    work(move || services.store.search_chats(&workspace_id, &query, archived)).await
}

/// 运行期间选项已冻结；空闲时才允许为下一轮设置跨工作区偏好和排除项。
#[tauri::command]
pub async fn agent_set_chat_memory(
    state: State<'_, AgentState>,
    id: String,
    request: ChatMemoryConfig,
) -> AgentResult<ChatSession> {
    let services = state.services()?;
    work(move || {
        let _gate = services.gate.lock().map_err(|_| storage())?;
        idle(&services, &id)?;
        services.store.set_chat_memory(&id, request)
    })
    .await
}
