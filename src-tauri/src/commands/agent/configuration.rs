use super::*;
use gitmaster_core::agent::provider::discovery::{ConnectionTestReport, DiscoveredModelPage};

/// 配置请求标识只用于窗口内取消，不接受路径或无界字符串。
fn valid_id(id: &str) -> AgentResult<()> {
    if id.is_empty()
        || id.len() > 96
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() || byte == b'-')
    {
        return Err(AgentError::new("AGENT_INVALID_ID", "配置请求标识无效。"));
    }
    Ok(())
}

/// 连接和模型一次发布，密钥只在阻塞池访问系统凭据库。
#[tauri::command]
pub async fn agent_save_provider(
    state: State<'_, AgentState>,
    request: SaveProvider,
) -> AgentResult<AgentSettingsView> {
    let services = state.services()?;
    work(move || services.profiles.save_provider(request)).await
}
/// 修改模型默认参数或启用状态，不复制连接。
#[tauri::command]
pub async fn agent_save_model(
    state: State<'_, AgentState>,
    request: SaveModel,
) -> AgentResult<AgentSettingsView> {
    let services = state.services()?;
    work(move || services.profiles.save_model(request)).await
}
/// 删除用户确认的连接和关联模型，旧聊天保留。
#[tauri::command]
pub async fn agent_delete_provider(
    state: State<'_, AgentState>,
    id: String,
    expected_revision: u64,
) -> AgentResult<AgentSettingsView> {
    let services = state.services()?;
    work(move || services.profiles.delete_provider(&id, expected_revision)).await
}

/// 请求租约在完成、失败或任务被丢弃时清除登记，防止配置操作泄漏。
struct ConfigurationRequest {
    services: Arc<AgentServices>,
    key: String,
    cancel: Cancellation,
}
impl Drop for ConfigurationRequest {
    /// 清理只针对本次窗口绑定的请求标识。
    fn drop(&mut self) {
        if let Ok(mut requests) = self.services.configuration_requests.lock() {
            requests.remove(&self.key);
        }
    }
}
/// 将取消权限绑定原窗口，限制同时在途的配置请求数量。
fn register(
    services: Arc<AgentServices>,
    window: &tauri::WebviewWindow,
    id: &str,
) -> AgentResult<ConfigurationRequest> {
    valid_id(id)?;
    let key = format!("{}:{id}", window.label());
    let cancel = Cancellation::new();
    {
        let mut requests = services
            .configuration_requests
            .lock()
            .map_err(|_| storage())?;
        if requests.len() >= 8 || requests.contains_key(&key) {
            return Err(AgentError::new(
                "AGENT_CONFIG_BUSY",
                "已有配置请求正在执行，请先取消或稍后重试。",
            ));
        }
        requests.insert(key.clone(), cancel.clone());
    }
    Ok(ConfigurationRequest {
        services,
        key,
        cancel,
    })
}

/// 未保存草稿也能发现模型；读取失败不写配置、不清空已启用模型。
#[tauri::command]
pub async fn agent_list_models(
    window: tauri::WebviewWindow,
    state: State<'_, AgentState>,
    provider: ProviderDraft,
    request_id: String,
    cursor: Option<String>,
) -> AgentResult<DiscoveredModelPage> {
    let services = state.services()?;
    let lease = register(services.clone(), &window, &request_id)?;
    let (profile, key) = work({
        let services = services.clone();
        move || services.profiles.draft_credentials(&provider)
    })
    .await?;
    let page = ModelClient::new()?
        .list_models(&profile, &key, request_id, cursor, &lease.cancel)
        .await?;
    lease.cancel.check()?;
    services
        .profiles
        .remember_capabilities(&profile, &page.models)?;
    Ok(page)
}

/// 当前草稿快照用于文本和模拟工具协议测试，不创建持久聊天或本地副作用。
#[tauri::command]
pub async fn agent_test_model(
    window: tauri::WebviewWindow,
    state: State<'_, AgentState>,
    provider: ProviderDraft,
    model: ModelDraft,
    request_id: String,
) -> AgentResult<ConnectionTestReport> {
    let services = state.services()?;
    let lease = register(services.clone(), &window, &request_id)?;
    let (profile, key, parameters, capabilities) = work(move || {
        let (mut profile, key) = services.profiles.draft_credentials(&provider)?;
        profile.id = model.id.unwrap_or_else(new_id);
        profile.model = model.model_id;
        profile.name = model.label;
        profile.protocol = model.protocol;
        profile.validate()?;
        for value in [&profile.name, &profile.model] {
            privacy::ensure_safe_text(value, Some(&key))?;
        }
        let capabilities = services.profiles.model_capabilities(&profile)?;
        model.defaults.validate(&profile, &capabilities)?;
        Ok((profile, key, model.defaults, capabilities))
    })
    .await?;
    ModelClient::new()?
        .test_model(
            &profile,
            &key,
            &parameters,
            &capabilities,
            request_id,
            &lease.cancel,
        )
        .await
}

/// 取消本窗口的模型发现或连接测试；已完成请求按幂等成功处理。
#[tauri::command]
pub async fn agent_cancel_configuration_request(
    window: tauri::WebviewWindow,
    state: State<'_, AgentState>,
    request_id: String,
) -> AgentResult<()> {
    valid_id(&request_id)?;
    let services = state.services()?;
    let requests = services
        .configuration_requests
        .lock()
        .map_err(|_| storage())?;
    if let Some(cancel) = requests.get(&format!("{}:{request_id}", window.label())) {
        cancel.cancel();
    }
    Ok(())
}
