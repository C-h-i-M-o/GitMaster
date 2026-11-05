use super::*;

/// 配置页草稿的密钥只用于当次操作，不实现 Serialize/Debug。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderDraft {
    pub id: Option<String>,
    pub preset_id: Option<String>,
    pub name: String,
    pub base_url: String,
    pub protocol: ProviderProtocol,
    pub api_key: Option<String>,
    #[serde(default)]
    pub allow_stored_key_for_changed_host: bool,
}
/// 模型草稿与连接分离；修改必须带读取时的 revision。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelDraft {
    pub id: Option<String>,
    pub model_id: String,
    pub label: String,
    pub protocol: ProviderProtocol,
    pub enabled: bool,
    #[serde(default)]
    pub defaults: ModelParameters,
    pub expected_revision: Option<u64>,
}
/// 一个保存动作原子发布连接和本次修改的模型。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveProvider {
    #[serde(flatten)]
    pub provider: ProviderDraft,
    pub expected_revision: Option<u64>,
    pub models: Vec<ModelDraft>,
}
/// 独立修改模型时不要求重填供应商密钥。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveModel {
    pub provider_id: String,
    #[serde(flatten)]
    pub model: ModelDraft,
}

impl<V: CredentialVault> ProfileService<V> {
    /// 仅缓存后端收到的模型元数据，容量有界，普通设置读取不会联网。
    pub fn remember_capabilities(
        &self,
        profile: &ProviderProfile,
        models: &[provider::discovery::DiscoveredModel],
    ) -> AgentResult<()> {
        let endpoint = profile.endpoint()?;
        let mut cache = self.capability_cache.lock().map_err(AgentError::storage)?;
        if cache.len() + models.len() > 4096 {
            cache.clear();
        }
        for model in models {
            cache.insert(
                (endpoint.clone(), model.model_id.clone()),
                model.capabilities.clone(),
            );
        }
        Ok(())
    }
    /// 能力从当前端点缓存或原模型持久记录读取，再检查端点绑定。
    pub fn model_capabilities(&self, profile: &ProviderProfile) -> AgentResult<ModelCapabilities> {
        let cached = self
            .capability_cache
            .lock()
            .map_err(AgentError::storage)?
            .get(&(profile.endpoint()?, profile.model.clone()))
            .cloned();
        let saved = if cached.is_none() {
            self.store
                .settings()?
                .models
                .into_iter()
                .find(|model| model.id == profile.id)
                .map(|model| model.capabilities)
        } else {
            None
        };
        Ok(provider::catalog::effective_capabilities(
            profile,
            &cached.or(saved).unwrap_or_default(),
        ))
    }
    /// 验证草稿身份与凭据发送目的地；返回值仅可留在后端内存。
    pub fn draft_credentials(
        &self,
        draft: &ProviderDraft,
    ) -> AgentResult<(ProviderProfile, String)> {
        let _guard = self.gate.lock().map_err(AgentError::storage)?;
        self.resolve_draft(draft)
    }
    /// 同主机可保留原 Key；改主机必须提供新 Key 或用户明确选择复用。
    fn resolve_draft(&self, draft: &ProviderDraft) -> AgentResult<(ProviderProfile, String)> {
        let settings = self.store.settings()?;
        let old = match &draft.id {
            Some(id) => Some(
                settings
                    .providers
                    .iter()
                    .find(|p| &p.id == id)
                    .ok_or_else(connection_missing)?,
            ),
            None => None,
        };
        let profile = ProviderProfile {
            id: old.map_or_else(new_id, |p| p.id.clone()),
            name: draft.name.trim().into(),
            base_url: draft.base_url.trim().into(),
            model: "connection-validation".into(),
            protocol: draft.protocol,
            credential_id: old.map_or_else(new_id, |p| p.credential_id.clone()),
        };
        profile.validate()?;
        if draft.preset_id.as_ref().is_some_and(|id| {
            !provider::catalog::presets()
                .iter()
                .any(|preset| &preset.id == id)
        }) {
            return Err(AgentError::new(
                "AGENT_PRESET",
                "请选择有效的供应商预设或自定义服务。",
            ));
        }
        let key = match draft
            .api_key
            .as_deref()
            .filter(|key| !key.trim().is_empty())
        {
            Some(key) if key.len() <= 8192 && !key.chars().any(char::is_control) => key.to_owned(),
            Some(_) => return Err(AgentError::new("AGENT_INVALID_KEY", "API Key 格式不正确。")),
            None => {
                let old =
                    old.ok_or_else(|| AgentError::new("AGENT_KEY_REQUIRED", "请先填写 API Key。"))?;
                let origin = |text: &str| {
                    url::Url::parse(text)
                        .map(|url| url.origin())
                        .map_err(|_| AgentError::corrupt())
                };
                if origin(&old.base_url)? != origin(&profile.base_url)?
                    && !draft.allow_stored_key_for_changed_host
                {
                    return Err(AgentError::new(
                        "AGENT_KEY_HOST_CHANGED",
                        "服务主机已改变；请填写新 Key，或明确允许向新主机发送已保存的 Key。",
                    ));
                }
                self.key_for(&profile)?
            }
        };
        for value in [&profile.name, &profile.base_url] {
            privacy::ensure_safe_text(value, Some(&key))?;
        }
        Ok((profile, key))
    }

    /// 先验证所有模型，再写新凭据；配置原子发布失败时清理新凭据并保留旧配置。
    pub fn save_provider(&self, request: SaveProvider) -> AgentResult<AgentSettingsView> {
        let _guard = self.gate.lock().map_err(AgentError::storage)?;
        let (profile, key) = self.resolve_draft(&request.provider)?;
        if request.models.len() > 128 {
            return Err(AgentError::new(
                "AGENT_MODEL_LIMIT",
                "最多保存 128 个模型。",
            ));
        }
        let previous = self.store.settings()?;
        let old = previous.providers.iter().find(|p| p.id == profile.id);
        check_revision(old.map(|p| p.revision), request.expected_revision)?;
        let new_key = request
            .provider
            .api_key
            .as_ref()
            .is_some_and(|key| !key.trim().is_empty())
            || old.is_none();
        let credential_id = if new_key {
            new_id()
        } else {
            profile.credential_id.clone()
        };
        let connection = ProviderConnection {
            id: profile.id.clone(),
            name: profile.name,
            base_url: profile.base_url,
            credential_id: credential_id.clone(),
            preset_id: request.provider.preset_id,
            revision: next_revision(old.map(|p| p.revision))?,
        };
        let mut models = vec![];
        let mut ids = std::collections::HashSet::new();
        for draft in &request.models {
            let model = prepare_model(self, &connection, draft, &previous.models, Some(&key))?;
            if !ids.insert(model.id.clone()) {
                return Err(AgentError::new(
                    "AGENT_MODEL_DUPLICATE",
                    "同一模型配置不能重复保存。",
                ));
            }
            models.push(model);
        }
        if new_key {
            self.vault.write(&credential_id, &key)?;
        }
        let result = self.store.change_configuration(|document| {
            check_revision(
                document
                    .providers
                    .iter()
                    .find(|p| p.id == connection.id)
                    .map(|p| p.revision),
                request.expected_revision,
            )?;
            for draft in &request.models {
                prepare_model(self, &connection, draft, &document.models, Some(&key))?;
            }
            document.providers.retain(|p| p.id != connection.id);
            document.providers.push(connection);
            for model in models {
                document.models.retain(|m| m.id != model.id);
                document.models.push(model);
            }
            Ok(())
        });
        if let Err(error) = result {
            if new_key && self.vault.delete(&credential_id).is_err() {
                return Err(AgentError::new(
                    "AGENT_KEY_CLEANUP",
                    "配置保存失败，旧配置未改变；新凭据清理失败，请检查系统密钥库。",
                ));
            }
            return Err(error);
        }
        let current = self.store.settings()?;
        let cleanup_failed = new_key
            && old.is_some_and(|old| {
                !current
                    .providers
                    .iter()
                    .any(|p| p.credential_id == old.credential_id)
                    && self.vault.delete(&old.credential_id).is_err()
            });
        let mut view = self.view()?;
        if cleanup_failed {
            view.warnings
                .push("新配置已保存，旧凭据清理失败，请检查系统密钥库。".into());
        }
        Ok(view)
    }

    /// 更新单个模型，不复制供应商连接或密钥。
    pub fn save_model(&self, request: SaveModel) -> AgentResult<AgentSettingsView> {
        let _guard = self.gate.lock().map_err(AgentError::storage)?;
        self.store.change_configuration(|document| {
            let connection = document
                .providers
                .iter()
                .find(|p| p.id == request.provider_id)
                .ok_or_else(connection_missing)?;
            let key = self.vault.read(&connection.credential_id)?;
            let model = prepare_model(
                self,
                connection,
                &request.model,
                &document.models,
                key.as_deref(),
            )?;
            document.models.retain(|m| m.id != model.id);
            document.models.push(model);
            Ok(())
        })?;
        self.view()
    }

    /// 明确删除连接及其模型，聊天历史保留；共享 Key 仍由其他连接持有。
    pub fn delete_provider(
        &self,
        id: &str,
        expected_revision: u64,
    ) -> AgentResult<AgentSettingsView> {
        let _guard = self.gate.lock().map_err(AgentError::storage)?;
        let settings = self.store.settings()?;
        let provider = settings
            .providers
            .iter()
            .find(|p| p.id == id)
            .ok_or_else(connection_missing)?;
        check_revision(Some(provider.revision), Some(expected_revision))?;
        let shared = settings
            .providers
            .iter()
            .any(|p| p.id != id && p.credential_id == provider.credential_id);
        let old_key = if shared {
            None
        } else {
            self.vault.read(&provider.credential_id)?
        };
        if !shared {
            self.vault.delete(&provider.credential_id)?;
        }
        let result = self.store.change_configuration(|document| {
            check_revision(
                document
                    .providers
                    .iter()
                    .find(|p| p.id == id)
                    .map(|p| p.revision),
                Some(expected_revision),
            )?;
            document.providers.retain(|p| p.id != id);
            document.models.retain(|m| m.provider_id != id);
            Ok(())
        });
        if let Err(error) = result {
            if let Some(key) = old_key {
                if self.vault.write(&provider.credential_id, &key).is_err() {
                    return Err(AgentError::new(
                        "AGENT_KEY_RESTORE",
                        "删除失败且密钥恢复失败，请重新填写连接的 API Key。",
                    ));
                }
            }
            return Err(error);
        }
        self.view()
    }
}

/// 复用模型校验并检查 revision，禁止把其他连接的模型悄悄迁移过来。
fn prepare_model<V: CredentialVault>(
    service: &ProfileService<V>,
    connection: &ProviderConnection,
    draft: &ModelDraft,
    models: &[ModelConfig],
    key: Option<&str>,
) -> AgentResult<ModelConfig> {
    let old = match &draft.id {
        Some(id) => Some(
            models
                .iter()
                .find(|m| &m.id == id && m.provider_id == connection.id)
                .ok_or_else(connection_missing)?,
        ),
        None => None,
    };
    check_revision(old.map(|m| m.revision), draft.expected_revision)?;
    let profile = ProviderProfile {
        id: draft.id.clone().unwrap_or_else(new_id),
        name: draft.label.trim().into(),
        protocol: draft.protocol,
        base_url: connection.base_url.clone(),
        model: draft.model_id.trim().into(),
        credential_id: connection.credential_id.clone(),
    };
    profile.validate()?;
    for value in [&profile.name, &profile.model] {
        privacy::ensure_safe_text(value, key)?;
    }
    let cached = service
        .capability_cache
        .lock()
        .map_err(AgentError::storage)?
        .get(&(profile.endpoint()?, profile.model.clone()))
        .cloned();
    let capabilities = provider::catalog::effective_capabilities(
        &profile,
        &cached
            .or_else(|| old.map(|model| model.capabilities.clone()))
            .unwrap_or_default(),
    );
    draft.defaults.validate(&profile, &capabilities)?;
    Ok(ModelConfig {
        id: profile.id,
        provider_id: connection.id.clone(),
        model_id: profile.model,
        label: profile.name,
        protocol: profile.protocol,
        enabled: draft.enabled,
        revision: next_revision(old.map(|m| m.revision))?,
        capabilities,
        defaults: draft.defaults.clone(),
    })
}
/// 防止过期表单覆盖新的连接或模型数据。
fn check_revision(current: Option<u64>, expected: Option<u64>) -> AgentResult<()> {
    if current != expected {
        Err(AgentError::new(
            "AGENT_CONFIG_STALE",
            "配置已被更新，请重新打开后修改。",
        ))
    } else {
        Ok(())
    }
}
/// 递增配置版本，溢出时保留已有文件。
fn next_revision(current: Option<u64>) -> AgentResult<u64> {
    current
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(AgentError::corrupt)
}
/// 不回显无效标识或凭据信息。
fn connection_missing() -> AgentError {
    AgentError::new(
        "AGENT_PROVIDER_MISSING",
        "供应商连接或模型配置不存在，请刷新后重试。",
    )
}
