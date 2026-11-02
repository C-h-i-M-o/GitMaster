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
