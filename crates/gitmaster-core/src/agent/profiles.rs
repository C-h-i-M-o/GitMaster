use super::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

/// 平台凭据接口；实现不得回退到普通配置文件。
pub trait CredentialVault: Send + Sync {
    /// 读取指定不透明凭据；不存在时返回 None。
    fn read(&self, id: &str) -> AgentResult<Option<String>>;
    /// 保存凭据，失败必须返回错误，不能宣称已保存。
    fn write(&self, id: &str, secret: &str) -> AgentResult<()>;
    /// 删除凭据；已不存在视为成功。
    fn delete(&self, id: &str) -> AgentResult<()>;
}

/// 密码只作为单次保存输入，不实现 Debug 或 Serialize。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveProfile {
    pub id: Option<String>,
    pub name: String,
    pub protocol: ProviderProtocol,
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
}

/// 设置返回是否有凭据和清理警告，不返回密钥。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSettingsView {
    pub settings: AgentSettings,
    pub has_api_keys: BTreeMap<String, bool>,
    pub warnings: Vec<String>,
}

/// 串行处理密钥与普通配置的组合操作，保留旧配置直到新凭据落稳。
pub struct ProfileService<V: CredentialVault> {
    store: Arc<AgentStore>,
    vault: V,
    gate: Mutex<()>,
}
impl<V: CredentialVault> ProfileService<V> {
    /// 注入真实平台密钥库，测试仅替代这一边界。
    pub fn new(store: Arc<AgentStore>, vault: V) -> Self {
        Self {
            store,
            vault,
            gate: Mutex::new(()),
        }
    }

    /// 返回安全设置快照；密钥库不可用时仍能查看普通配置。
    pub fn view(&self) -> AgentResult<AgentSettingsView> {
        let settings = self.store.settings()?;
        let mut view = AgentSettingsView {
            settings,
            has_api_keys: BTreeMap::new(),
            warnings: vec![],
        };
        if view.settings.permission_review_required {
            view.warnings
                .push("AI 配置已升级；原完全访问权限已收紧，请检查新权限设置。".into());
        }
        for profile in &view.settings.profiles {
            match self.vault.read(&profile.credential_id) {
                Ok(key) => {
                    view.has_api_keys
                        .insert(profile.id.clone(), key.is_some_and(|k| !k.is_empty()));
                }
                Err(_) => {
                    view.has_api_keys.insert(profile.id.clone(), false);
                    if !view
                        .warnings
                        .iter()
                        .any(|message| message.contains("系统密钥库暂不可用"))
                    {
                        view.warnings
                            .push("系统密钥库暂不可用，请允许 GitMaster 访问密钥库后重试。".into());
                    }
                }
            }
        }
        Ok(view)
    }

    /// 新凭据采用新 ID；配置保存失败时旧凭据及旧引用仍可使用。
    pub fn save(&self, request: SaveProfile) -> AgentResult<AgentSettingsView> {
        let _guard = self.gate.lock().map_err(AgentError::storage)?;
        let settings = self.store.settings()?;
        let old =
            match request.id.as_deref() {
                Some(id) => Some(settings.profiles.iter().find(|p| p.id == id).ok_or_else(
                    || {
                        AgentError::new(
                            "AGENT_PROFILE_MISSING",
                            "要修改的 API 配置已不存在，请重新读取设置。",
                        )
                    },
                )?),
                None => None,
            };
        let key = request
            .api_key
            .as_deref()
            .map(str::trim)
            .filter(|k| !k.is_empty());
        if key.is_some_and(|k| k.len() > 8192 || k.chars().any(char::is_control)) {
            return Err(AgentError::new(
                "AGENT_INVALID_KEY",
                "API Key 格式不正确，请检查空格或换行。",
            ));
        }
        if old.is_none() && key.is_none() {
            return Err(AgentError::new(
                "AGENT_KEY_REQUIRED",
                "首次配置需要填写 API Key。",
            ));
        }
        let credential_id = if key.is_some() {
            new_id()
        } else {
            old.expect("已验证旧配置").credential_id.clone()
        };
        let profile = ProviderProfile {
            id: old.map_or_else(new_id, |p| p.id.clone()),
            name: request.name.trim().into(),
            protocol: request.protocol,
            base_url: request.base_url.trim().into(),
            model: request.model.trim().into(),
            credential_id: credential_id.clone(),
        };
        profile.validate()?;
        for text in [&profile.name, &profile.model, &profile.base_url] {
            super::privacy::ensure_safe_text(text, key)?;
        }
        if let Some(key) = key {
            self.vault.write(&credential_id, key)?;
        }
        if let Err(error) = self.store.save_profile(profile) {
            if key.is_some() && self.vault.delete(&credential_id).is_err() {
                return Err(AgentError::new("AGENT_KEY_CLEANUP", "配置保存失败，旧配置未变更；新凭据未能清理，请在系统密钥库中检查 GitMaster 条目。"));
            }
            return Err(error);
        }
        let current = self.store.settings()?;
        let cleanup_failed = key.is_some()
            && old.is_some_and(|p| {
                !current
                    .providers
                    .iter()
                    .any(|provider| provider.credential_id == p.credential_id)
                    && self.vault.delete(&p.credential_id).is_err()
            });
        let mut view = self.view()?;
        if cleanup_failed {
            view.warnings.push(
                "新配置已保存，但旧凭据清理失败，请在系统密钥库中删除不再使用的 GitMaster 条目。"
                    .into(),
            );
        }
        Ok(view)
    }

    /// 删除密钥失败时保留配置；配置删除失败则恢复原有密钥。
    pub fn delete(&self, id: &str) -> AgentResult<AgentSettingsView> {
        let _guard = self.gate.lock().map_err(AgentError::storage)?;
        let settings = self.store.settings()?;
        let profile = settings
            .profiles
            .iter()
            .find(|p| p.id == id)
            .ok_or_else(|| AgentError::new("AGENT_PROFILE_MISSING", "API 配置不存在。"))?;
        let shared = settings
            .models
            .iter()
            .filter(|model| model.id != id)
            .any(|model| {
                settings.providers.iter().any(|provider| {
                    provider.id == model.provider_id
                        && provider.credential_id == profile.credential_id
                })
            });
        if shared {
            self.store.delete_profile(id)?;
            return self.view();
        }
        let old_key = self.vault.read(&profile.credential_id)?;
        self.vault.delete(&profile.credential_id)?;
        if let Err(error) = self.store.delete_profile(id) {
            if let Some(key) = old_key {
                if self.vault.write(&profile.credential_id, &key).is_err() {
                    return Err(AgentError::new(
                        "AGENT_KEY_RESTORE",
                        "删除配置失败，原密钥未能恢复，请重新填写该配置的 API Key。",
                    ));
                }
            }
            return Err(error);
        }
        self.view()
    }

    /// 仅供后端请求模型时获取已保存凭据，不注册为 IPC 接口。
    pub fn key_for(&self, profile: &ProviderProfile) -> AgentResult<String> {
        self.vault
            .read(&profile.credential_id)?
            .filter(|key| !key.is_empty())
            .ok_or_else(|| {
                AgentError::new("AGENT_KEY_MISSING", "找不到 API Key，请在设置中重新填写。")
            })
    }
}
