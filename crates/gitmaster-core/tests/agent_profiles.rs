use gitmaster_core::agent::*;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

/// 测试替身只替代平台密钥库，配置保存仍经过真实文件系统。
#[derive(Clone, Default)]
struct Vault {
    entries: Arc<Mutex<HashMap<String, String>>>,
    fail: Arc<Mutex<bool>>,
}
impl CredentialVault for Vault {
    fn read(&self, id: &str) -> AgentResult<Option<String>> {
        Ok(self.entries.lock().unwrap().get(id).cloned())
    }
    fn write(&self, id: &str, secret: &str) -> AgentResult<()> {
        if *self.fail.lock().unwrap() {
            return Err(AgentError::new("KEY_FAILED", "测试拒绝"));
        }
        self.entries
            .lock()
            .unwrap()
            .insert(id.into(), secret.into());
        Ok(())
    }
    fn delete(&self, id: &str) -> AgentResult<()> {
        if *self.fail.lock().unwrap() {
            return Err(AgentError::new("KEY_FAILED", "测试拒绝"));
        }
        self.entries.lock().unwrap().remove(id);
        Ok(())
    }
}
/// 模拟用户在受控密码框中提交配置。
fn request(id: Option<String>, key: Option<&str>) -> SaveProfile {
    SaveProfile {
        id,
        name: "模型".into(),
        protocol: ProviderProtocol::Responses,
        base_url: "https://example.com/v1".into(),
        model: "test".into(),
        api_key: key.map(str::to_owned),
    }
}

#[test]
fn failed_key_rotation_preserves_original_profile_and_key() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(AgentStore::open(dir.path().into()).unwrap());
    let vault = Vault::default();
    let service = ProfileService::new(store.clone(), vault.clone());
    service.save(request(None, Some("private-old"))).unwrap();
    let profile = store.settings().unwrap().profiles[0].clone();
    *vault.fail.lock().unwrap() = true;
    assert!(service
        .save(request(Some(profile.id.clone()), Some("private-new")))
        .is_err());
    assert_eq!(
        store.settings().unwrap().profiles[0].credential_id,
        profile.credential_id
    );
    assert_eq!(
        vault.read(&profile.credential_id).unwrap().as_deref(),
        Some("private-old")
    );
    assert!(!std::fs::read_to_string(dir.path().join("settings.json"))
        .unwrap()
        .contains("private-"));
}

#[test]
fn successful_rotation_deletes_old_key_and_empty_edit_keeps_current_key() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(AgentStore::open(dir.path().into()).unwrap());
    let vault = Vault::default();
    let service = ProfileService::new(store.clone(), vault.clone());
    service.save(request(None, Some("private-old"))).unwrap();
    let old = store.settings().unwrap().profiles[0].clone();
    service
        .save(request(Some(old.id.clone()), Some("private-new")))
        .unwrap();
    let new = store.settings().unwrap().profiles[0].clone();
    assert_ne!(old.credential_id, new.credential_id);
    assert!(vault.read(&old.credential_id).unwrap().is_none());
    service.save(request(Some(old.id.clone()), None)).unwrap();
    assert_eq!(
        store.settings().unwrap().profiles[0].credential_id,
        new.credential_id
    );
    assert_eq!(service.key_for(&new).unwrap(), "private-new");
    service.delete(&new.id).unwrap();
    assert!(store.settings().unwrap().profiles.is_empty());
    assert!(vault.entries.lock().unwrap().is_empty());
}

#[test]
fn new_profile_without_key_and_forged_profile_id_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let service = ProfileService::new(
        Arc::new(AgentStore::open(dir.path().into()).unwrap()),
        Vault::default(),
    );
    assert!(service.save(request(None, None)).is_err());
    assert!(service
        .save(request(Some("aaaa".into()), Some("secret")))
        .is_err());
}

/// 多模型共用凭据时删除一个模型不能删除另一模型仍使用的密钥。
#[test]
fn deleting_one_model_keeps_shared_credential() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(AgentStore::open(dir.path().into()).unwrap());
    let vault = Vault::default();
    let service = ProfileService::new(store.clone(), vault.clone());
    service
        .save(request(None, Some("shared-test-key")))
        .unwrap();
    let first = store.settings().unwrap().profiles[0].clone();
    let mut second = first.clone();
    second.id = "bb".into();
    second.model = "other-model".into();
    store.save_profile(second.clone()).unwrap();
    service.delete(&first.id).unwrap();
    assert_eq!(service.key_for(&second).unwrap(), "shared-test-key");
    service.delete(&second.id).unwrap();
    assert!(vault.entries.lock().unwrap().is_empty());
}

/// 删除已有模型时仍保留被未绑定连接引用的共享凭据。
#[test]
fn deleting_last_bound_model_keeps_key_for_unbound_provider() {
    let dir = tempfile::tempdir().unwrap();
    let document = serde_json::json!({
        "version": 2,
        "providers": [
            {"id":"aa","name":"已用连接","baseUrl":"https://example.com/v1","credentialId":"cc","presetId":null,"revision":1},
            {"id":"bb","name":"待用连接","baseUrl":"https://example.com/v1","credentialId":"cc","presetId":null,"revision":1}
        ],
        "models": [
            {"id":"dd","providerId":"aa","modelId":"test","label":"模型","protocol":"responses","enabled":true,"revision":1}
        ],
        "activeModelConfigId":"dd",
        "defaultPermissionMode":"requestApproval",
        "permissionReviewRequired":false
    });
    std::fs::write(dir.path().join("settings.json"), document.to_string()).unwrap();
    let store = Arc::new(AgentStore::open(dir.path().into()).unwrap());
    let vault = Vault::default();
    vault
        .entries
        .lock()
        .unwrap()
        .insert("cc".into(), "shared-key".into());
    let service = ProfileService::new(store.clone(), vault.clone());

    service.delete("dd").unwrap();

    assert_eq!(vault.read("cc").unwrap().as_deref(), Some("shared-key"));
    let settings = store.settings().unwrap();
    assert!(settings.profiles.is_empty());
    assert!(settings
        .providers
        .iter()
        .any(|provider| provider.id == "bb" && provider.credential_id == "cc"));
}

/// 同一连接仍有禁用模型时，删除最后一个启用模型不能清除凭据。
#[test]
fn deleting_enabled_model_keeps_key_for_disabled_sibling() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(AgentStore::open(dir.path().into()).unwrap());
    let vault = Vault::default();
    let service = ProfileService::new(store.clone(), vault.clone());
    service
        .save(request(None, Some("shared-test-key")))
        .unwrap();
    let first = store.settings().unwrap().profiles[0].clone();
    let path = dir.path().join("settings.json");
    let mut document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let mut disabled = document["models"][0].clone();
    disabled["id"] = serde_json::Value::String("bb".into());
    disabled["enabled"] = serde_json::Value::Bool(false);
    document["models"].as_array_mut().unwrap().push(disabled);
    std::fs::write(&path, document.to_string()).unwrap();

    service.delete(&first.id).unwrap();

    assert_eq!(
        vault.read(&first.credential_id).unwrap().as_deref(),
        Some("shared-test-key")
    );
    let settings = store.settings().unwrap();
    assert!(settings.models.iter().any(|model| !model.enabled));
    assert!(settings
        .providers
        .iter()
        .any(|provider| provider.credential_id == first.credential_id));
}

/// 粘贴到名称/模型的明显凭据和当前新 Key 不能绕过密码字段进入普通配置。
#[test]
fn provider_saves_multiple_models_atomically_and_host_change_requires_consent() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(AgentStore::open(dir.path().into()).unwrap());
    let vault = Vault::default();
    let service = ProfileService::new(store.clone(), vault.clone());
    let input = serde_json::json!({"id":null,"presetId":null,"name":"同一账号","baseUrl":"https://example.com/v1","protocol":"responses","apiKey":"shared-model-test-key","allowStoredKeyForChangedHost":false,"expectedRevision":null,"models":[
        {"id":null,"modelId":"flash","label":"快速","protocol":"responses","enabled":true,"defaults":{},"expectedRevision":null},
        {"id":null,"modelId":"pro","label":"专业","protocol":"responses","enabled":true,"defaults":{},"expectedRevision":null}
    ]});
    let view = service
        .save_provider(serde_json::from_value(input).unwrap())
        .unwrap();
    assert_eq!(view.settings.providers.len(), 1);
    assert_eq!(view.settings.models.len(), 2);
    assert_eq!(vault.entries.lock().unwrap().len(), 1);
    assert_eq!(
        view.settings.profiles[0].credential_id,
        view.settings.profiles[1].credential_id
    );
    let provider = &view.settings.providers[0];
    let mut draft: ProviderDraft = serde_json::from_value(serde_json::json!({"id":provider.id,"presetId":null,"name":"新地址","baseUrl":"https://other.example/v1","protocol":"responses","apiKey":null,"allowStoredKeyForChangedHost":false})).unwrap();
    assert_eq!(
        service.draft_credentials(&draft).unwrap_err().code,
        "AGENT_KEY_HOST_CHANGED"
    );
    draft.allow_stored_key_for_changed_host = true;
    assert_eq!(
        service.draft_credentials(&draft).unwrap().1,
        "shared-model-test-key"
    );
    let before = std::fs::read(dir.path().join("settings.json")).unwrap();
    let invalid = SaveProvider {
        provider: draft,
        expected_revision: Some(99),
        models: vec![],
    };
    assert_eq!(
        service.save_provider(invalid).err().unwrap().code,
        "AGENT_CONFIG_STALE"
    );
    assert_eq!(
        before,
        std::fs::read(dir.path().join("settings.json")).unwrap()
    );
    service
        .delete_provider(&provider.id, provider.revision)
        .unwrap();
    assert!(vault.entries.lock().unwrap().is_empty());
}

/// 多模型草稿中任一非法参数均不能留下部分配置或新 Key。
#[test]
fn invalid_model_in_batch_preserves_old_configuration_and_key_count() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(AgentStore::open(dir.path().into()).unwrap());
    let vault = Vault::default();
    let service = ProfileService::new(store.clone(), vault.clone());
    let input = serde_json::json!({"id":null,"presetId":null,"name":"账号","baseUrl":"https://example.com/v1","protocol":"responses","apiKey":"new-test-key","allowStoredKeyForChangedHost":false,"expectedRevision":null,"models":[
        {"id":null,"modelId":"flash","label":"快速","protocol":"responses","enabled":true,"defaults":{},"expectedRevision":null},
        {"id":null,"modelId":"pro","label":"专业","protocol":"responses","enabled":true,"defaults":{"reasoningEffort":"invented"},"expectedRevision":null}
    ]});
    assert_eq!(
        service
            .save_provider(serde_json::from_value(input).unwrap())
            .err()
            .unwrap()
            .code,
        "AGENT_MODEL_PARAMETERS"
    );
    assert!(store.settings().unwrap().models.is_empty());
    assert!(vault.entries.lock().unwrap().is_empty());
}

/// 粘贴到名称/模型的明显凭据和当前新 Key 不能绕过密码字段进入普通配置。
#[test]
fn credentials_in_plain_profile_fields_are_rejected_before_storage() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(AgentStore::open(dir.path().into()).unwrap());
    let vault = Vault::default();
    let service = ProfileService::new(store.clone(), vault.clone());
    let mut input = request(None, Some("private-test-key"));
    input.name = "包含 private-test-key 的名称".into();
    assert!(service.save(input).is_err());
    let mut input = request(None, Some("private-test-key"));
    input.model = "OPENAI_API_KEY=another-secret".into();
    assert!(service.save(input).is_err());
    assert!(store.settings().unwrap().profiles.is_empty());
    assert!(vault.entries.lock().unwrap().is_empty());
    assert!(!dir.path().join("settings.json").exists());
}

/// 旧入口编辑时也不得把既有密钥带往新主机或复制到普通字段。
#[test]
fn legacy_edit_cannot_reuse_stored_key_on_another_host_or_echo_it() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(AgentStore::open(dir.path().into()).unwrap());
    let vault = Vault::default();
    let service = ProfileService::new(store.clone(), vault);
    service
        .save(request(None, Some("existing-private-test-key")))
        .unwrap();
    let id = store.settings().unwrap().profiles[0].id.clone();
    let mut changed = request(Some(id.clone()), None);
    changed.base_url = "https://other.example/v1".into();
    assert!(service.save(changed).is_err());
    let mut echoed = request(Some(id), None);
    echoed.name = "existing-private-test-key".into();
    assert!(service.save(echoed).is_err());
}
