use super::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// 供应商连接只保存凭据引用，同一连接可供多个模型使用。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderConnection {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub credential_id: String,
    pub preset_id: Option<String>,
    pub revision: u64,
}

/// 模型身份独立于供应商，运行配置通过此 ID 选择。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelConfig {
    pub id: String,
    pub provider_id: String,
    pub model_id: String,
    pub label: String,
    pub protocol: ProviderProtocol,
    pub enabled: bool,
    pub revision: u64,
}

/// 磁盘 v2 只保存唯一来源；旧 profile 列表仅作为运行时兼容投影。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SettingsDocument {
    pub version: u8,
    pub providers: Vec<ProviderConnection>,
    pub models: Vec<ModelConfig>,
    pub active_model_config_id: Option<String>,
    pub default_permission_mode: PermissionMode,
    pub permission_review_required: bool,
}

/// 严格读取原有格式，未知字段和未来版本均不会被自动丢弃。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LegacySettings {
    version: u8,
    profiles: Vec<ProviderProfile>,
    active_profile_id: Option<String>,
    default_permission_mode: PermissionMode,
}

impl SettingsDocument {
    /// 新安装不自动提升现有权限；权限策略升级由独立任务接入。
    pub(crate) fn empty() -> Self {
        Self {
            version: 2,
            providers: vec![],
            models: vec![],
            active_model_config_id: None,
            default_permission_mode: PermissionMode::RequestApproval,
            permission_review_required: false,
        }
    }

    /// 从旧配置一对一保留账号、模型和凭据身份，不按 URL 合并。
    pub(crate) fn decode(value: serde_json::Value) -> AgentResult<(Self, bool)> {
        let (document, migrated) = match value.get("version").and_then(|v| v.as_u64()) {
            Some(1) => {
                let old: LegacySettings =
                    serde_json::from_value(value).map_err(|_| AgentError::corrupt())?;
                if old.version != 1 || old.profiles.len() > 32 {
                    return Err(AgentError::corrupt());
                }
                let mut document = Self::empty();
                let mut ids = HashSet::new();
                for profile in old.profiles {
                    if !ids.insert(profile.id.clone()) {
                        return Err(AgentError::corrupt());
                    }
                    document.put_profile(profile)?;
                }
                document.active_model_config_id = old.active_profile_id;
                document.permission_review_required =
                    old.default_permission_mode == PermissionMode::FullAccess;
                document.default_permission_mode = if document.permission_review_required {
                    PermissionMode::AutoApprove
                } else {
                    old.default_permission_mode
                };
                (document, true)
            }
            Some(2) => (
                serde_json::from_value(value).map_err(|_| AgentError::corrupt())?,
                false,
            ),
            _ => return Err(AgentError::corrupt()),
        };
        document.view()?;
        Ok((document, migrated))
    }

    /// 验证所有身份与引用后派生旧运行接口所需的扁平配置。
    pub(crate) fn view(&self) -> AgentResult<AgentSettings> {
        if self.version != 2 || self.providers.len() > 32 || self.models.len() > 128 {
            return Err(AgentError::corrupt());
        }
        let mut provider_ids = HashSet::new();
        for p in &self.providers {
            valid_id(&p.id)?;
            if p.revision == 0 || !provider_ids.insert(p.id.clone()) {
                return Err(AgentError::corrupt());
            }
            ProviderProfile {
                id: p.id.clone(),
                name: p.name.clone(),
                protocol: ProviderProtocol::Responses,
                base_url: p.base_url.clone(),
                model: "validation".into(),
                credential_id: p.credential_id.clone(),
            }
            .validate()?;
        }
        let mut model_ids = HashSet::new();
        let mut profiles = vec![];
        for m in &self.models {
            if m.revision == 0 || !model_ids.insert(m.id.clone()) {
                return Err(AgentError::corrupt());
            }
            let p = self
                .providers
                .iter()
                .find(|p| p.id == m.provider_id)
                .ok_or_else(AgentError::corrupt)?;
            let profile = ProviderProfile {
                id: m.id.clone(),
                name: m.label.clone(),
                protocol: m.protocol,
                base_url: p.base_url.clone(),
                model: m.model_id.clone(),
                credential_id: p.credential_id.clone(),
            };
            profile.validate()?;
            if m.enabled {
                profiles.push(profile);
            }
        }
        if self
            .active_model_config_id
            .as_ref()
            .is_some_and(|id| !profiles.iter().any(|p| &p.id == id))
        {
            return Err(AgentError::corrupt());
        }
        Ok(AgentSettings {
            version: 2,
            profiles,
            active_profile_id: self.active_model_config_id.clone(),
            default_permission_mode: self.default_permission_mode,
            providers: self.providers.clone(),
            models: self.models.clone(),
            active_model_config_id: self.active_model_config_id.clone(),
            permission_review_required: self.permission_review_required,
        })
    }

    /// 兼容原配置入口，保存到规范化连接/模型结构，不保存第二份 profile。
    pub(crate) fn put_profile(&mut self, profile: ProviderProfile) -> AgentResult<()> {
        profile.validate()?;
        let provider_id = self
            .models
            .iter()
            .find(|m| m.id == profile.id)
            .map_or_else(|| profile.id.clone(), |m| m.provider_id.clone());
        if let Some(p) = self.providers.iter_mut().find(|p| p.id == provider_id) {
            p.name = profile.name.clone();
            p.base_url = profile.base_url.clone();
            p.credential_id = profile.credential_id.clone();
            p.revision = p.revision.checked_add(1).ok_or_else(AgentError::corrupt)?;
        } else {
            if self.providers.len() >= 32 {
                return Err(AgentError::new(
                    "AGENT_PROFILE_LIMIT",
                    "最多保存 32 个供应商连接。",
                ));
            }
            self.providers.push(ProviderConnection {
                id: provider_id.clone(),
                name: profile.name.clone(),
                base_url: profile.base_url,
                credential_id: profile.credential_id,
                preset_id: None,
                revision: 1,
            });
        }
        if let Some(m) = self.models.iter_mut().find(|m| m.id == profile.id) {
            m.model_id = profile.model;
            m.label = profile.name;
            m.protocol = profile.protocol;
            m.enabled = true;
            m.revision = m.revision.checked_add(1).ok_or_else(AgentError::corrupt)?;
        } else {
            self.models.push(ModelConfig {
                id: profile.id.clone(),
                provider_id,
                model_id: profile.model,
                label: profile.name,
                protocol: profile.protocol,
                enabled: true,
                revision: 1,
            });
        }
        if self.active_model_config_id.is_none() {
            self.active_model_config_id = Some(profile.id);
        }
        Ok(())
    }
}
