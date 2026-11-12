//! AI 配置单独持久化；密钥由系统凭据存储持有，不进入 JSON。
use gitmaster_core::git::OperationError;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    io::Write,
    path::Path,
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

static SETTINGS_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// API 协议只允许首版已验证的三种格式。
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum Protocol {
    OpenaiCompletions,
    OpenaiResponses,
    AnthropicMessages,
}
/// 权限预设在会话创建时固定，默认值不覆盖旧会话。
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum Permission {
    ReadOnly,
    WorkspaceWrite,
    DangerFullAccess,
}
/// 每个自定义模型必须明确容量，避免不可靠的默认推断。
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Model {
    pub model_id: String,
    pub display_name: String,
    pub context_window: u64,
    pub max_output_tokens: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_efforts: Option<HashMap<String, Option<String>>>,
}
/// 配置只保存凭据引用；创建或修改密钥时由后端产生新引用。
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Provider {
    pub provider_id: String,
    pub display_name: String,
    pub protocol: Protocol,
    pub base_url: String,
    pub credential_ref: Option<String>,
    pub allow_insecure_http: bool,
    pub models: Vec<Model>,
}
/// 持久化模型选择不接受只含模型名的模糊路由。
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelRef {
    pub provider_id: String,
    pub model_id: String,
    pub reasoning_effort: Option<String>,
}
/// AI 配置与现有 Git 设置的文件和版本完全独立。
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AiSettings {
    pub version: u8,
    pub revision: u64,
    pub providers: Vec<Provider>,
    pub default_model: Option<ModelRef>,
    pub default_permission: Permission,
}
impl Default for AiSettings {
    /// 首次启动不预填密钥、服务商或虚构模型。
    fn default() -> Self {
        Self {
            version: 1,
            revision: 0,
            providers: vec![],
            default_model: None,
            default_permission: Permission::WorkspaceWrite,
        }
    }
}
/// 配置回读仅返回密钥存在性，存储故障与未配置分开显示。
#[derive(Serialize)]
pub struct Snapshot {
    pub settings: AiSettings,
    pub credentials: HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}
/// 留空保留与明确删除使用不同的动作。
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum CredentialAction {
    Keep,
    Replace { value: String },
    Remove,
}

/// 不包含敏感错误明文，避免系统凭据驱动将密钥带入日志。
fn error(code: &str) -> OperationError {
    OperationError::new(code)
}
/// 校验普通配置的输入边界；不能把调用方提供的凭据引用当授权。
pub fn validate_provider(provider: &Provider) -> Result<(), OperationError> {
    let id = provider.provider_id.as_bytes();
    if id.is_empty()
        || id.len() > 64
        || !id[0].is_ascii_lowercase()
        || !id
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
        || provider.display_name.trim().is_empty()
        || provider.display_name.encode_utf16().count() > 200
        || provider.base_url.len() > 2048
        || provider.models.is_empty()
        || provider.models.len() > 100
    {
        return Err(error("AI_INVALID_PROVIDER"));
    }
    let url = url::Url::parse(&provider.base_url).map_err(|_| error("AI_INVALID_URL"))?;
    let loopback = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !(url.scheme() == "https"
            || (url.scheme() == "http" && (loopback || provider.allow_insecure_http)))
    {
        return Err(error("AI_INVALID_URL"));
    }
    let mut names = std::collections::HashSet::new();
    for model in &provider.models {
        if model.model_id.trim().is_empty()
            || model.model_id.encode_utf16().count() > 200
            || model.model_id != model.model_id.trim()
            || !names.insert(&model.model_id)
            || model.display_name.trim().is_empty()
            || model.display_name.encode_utf16().count() > 200
            || model.context_window > 9_007_199_254_740_991
            || model.max_output_tokens == 0
            || model.max_output_tokens >= model.context_window
            || model.reasoning_efforts.as_ref().is_some_and(|efforts| {
                efforts.len() > 16
                    || efforts.iter().any(|(key, value)| {
                        key.is_empty()
                            || key.len() > 64
                            || value.as_ref().is_some_and(|v| v.len() > 64)
                    })
            })
        {
            return Err(error("AI_INVALID_MODEL"));
        }
    }
    Ok(())
}
/// 应用独占配置写锁，避免后台任务之间覆盖设置。
fn lock() -> Result<std::sync::MutexGuard<'static, ()>, OperationError> {
    SETTINGS_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| error("AI_SETTINGS_IO"))
}
/// 读取受控配置；缺失文件表示首次启动，损坏文件不能静默覆盖。
pub fn read(path: &Path) -> Result<AiSettings, OperationError> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(AiSettings::default()),
        Err(_) => return Err(error("AI_SETTINGS_IO")),
    };
    if bytes.len() > 1024 * 1024 {
        return Err(error("AI_SETTINGS_IO"));
    }
    let settings: AiSettings =
        serde_json::from_slice(&bytes).map_err(|_| error("AI_SETTINGS_IO"))?;
    if settings.version != 1 || settings.revision > 9_007_199_254_740_990 {
        return Err(error("AI_SETTINGS_IO"));
    }
    if settings.providers.len() > 64 {
        return Err(error("AI_SETTINGS_IO"));
    }
    let mut names = std::collections::HashSet::new();
    for provider in &settings.providers {
        validate_provider(provider)?;
        if !names.insert(&provider.provider_id) {
            return Err(error("AI_SETTINGS_IO"));
        }
    }
    Ok(settings)
}
/// 用同目录临时文件原子替换配置，失败时不发布半份 JSON。
fn write(path: &Path, settings: &AiSettings) -> Result<(), OperationError> {
    if settings.providers.len() > 64
        || serde_json::to_vec(&settings.providers)
            .map_err(|_| error("AI_SETTINGS_IO"))?
            .len()
            > 196608
    {
        return Err(error("AI_SETTINGS_TOO_LARGE"));
    }
    let parent = path.parent().ok_or_else(|| error("AI_SETTINGS_IO"))?;
    fs::create_dir_all(parent).map_err(|_| error("AI_SETTINGS_IO"))?;
    let temporary = parent.join(format!(".ai-{}-{}.tmp", std::process::id(), stamp()));
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        file.write_all(&serde_json::to_vec_pretty(settings)?)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
        return Err(error("AI_SETTINGS_IO"));
    }
    Ok(())
}
/// 非敏感唯一标识只用于文件名与凭据引用，不作为访问授权。
fn stamp() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}
/// 限定为本应用的凭据命名空间，不能读取其他应用的凭据。
fn credential(reference: &str) -> Result<keyring::Entry, OperationError> {
    if !reference.starts_with("GITMASTER_AI_")
        || reference.len() > 128
        || !reference
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
    {
        return Err(error("AI_CREDENTIAL_IO"));
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    return Err(error("AI_CREDENTIAL_UNAVAILABLE"));
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    keyring::Entry::new("dev.gitmaster.desktop.ai", reference)
        .map_err(|_| error("AI_CREDENTIAL_IO"))
}
/// 明文只供受信任进程桥接使用，此函数不是 Tauri command。
pub fn read_secret(reference: &str) -> Result<Option<String>, OperationError> {
    match credential(reference)?.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(_) => Err(error("AI_CREDENTIAL_IO")),
    }
}
/// 为前端生成安全描述符，不把 API 密钥附着到配置对象。
pub fn snapshot(settings: AiSettings) -> Snapshot {
    let credentials = settings
        .providers
        .iter()
        .map(|provider| {
            let state = match &provider.credential_ref {
                None => "missing",
                Some(reference) => match read_secret(reference) {
                    Ok(Some(_)) => "configured",
                    Ok(None) => "missing",
                    Err(_) => "unavailable",
                },
            };
            (provider.provider_id.clone(), state.to_owned())
        })
        .collect();
    Snapshot {
        settings,
        credentials,
        warning: None,
    }
}
/// 新引用先写系统存储，配置发布失败时补偿删除，旧密钥不受影响。
pub fn save_provider(
    path: &Path,
    expected: u64,
    mut provider: Provider,
    action: CredentialAction,
) -> Result<Snapshot, OperationError> {
    let _guard = lock()?;
    let mut settings = read(path)?;
    if settings.revision != expected {
        return Err(error("AI_STALE_SETTINGS"));
    }
    let old_ref = settings
        .providers
        .iter()
        .find(|p| p.provider_id == provider.provider_id)
        .and_then(|p| p.credential_ref.clone());
    provider.credential_ref = old_ref.clone();
    validate_provider(&provider)?;
    let mut created_ref = None;
    match action {
        CredentialAction::Keep => {}
        CredentialAction::Remove => provider.credential_ref = None,
        CredentialAction::Replace { value } => {
            let value = value.trim();
            if value.is_empty()
                || value.len() > 16384
                || !value.is_ascii()
                || value.bytes().any(|b| b.is_ascii_control())
            {
                return Err(error("AI_INVALID_CREDENTIAL"));
            }
            let reference = format!("GITMASTER_AI_{}_{}", std::process::id(), stamp());
            credential(&reference)?
                .set_password(value)
                .map_err(|_| error("AI_CREDENTIAL_IO"))?;
            created_ref = Some(reference.clone());
            provider.credential_ref = Some(reference);
        }
    }
    let id = provider.provider_id.clone();
    settings.providers.retain(|p| p.provider_id != id);
    settings.providers.push(provider);
    settings.revision += 1;
    if let Err(failure) = write(path, &settings) {
        if let Some(reference) = created_ref {
            let _ = credential(&reference).and_then(|entry| {
                entry
                    .delete_credential()
                    .map_err(|_| error("AI_CREDENTIAL_IO"))
            });
        }
        return Err(failure);
    }
    if let Some(reference) = old_ref {
        if !settings
            .providers
            .iter()
            .any(|p| p.credential_ref.as_ref() == Some(&reference))
        {
            let _ = credential(&reference).and_then(|entry| {
                entry
                    .delete_credential()
                    .map_err(|_| error("AI_CREDENTIAL_IO"))
            });
        }
    }
    Ok(snapshot(settings))
}
/// 删除提供商只清理其无其他引用的系统凭据，不触碰项目内容。
pub fn delete_provider(path: &Path, expected: u64, id: &str) -> Result<Snapshot, OperationError> {
    let _guard = lock()?;
    let mut settings = read(path)?;
    if settings.revision != expected {
        return Err(error("AI_STALE_SETTINGS"));
    }
    let reference = settings
        .providers
        .iter()
        .find(|p| p.provider_id == id)
        .and_then(|p| p.credential_ref.clone());
    settings.providers.retain(|p| p.provider_id != id);
    settings.revision += 1;
    write(path, &settings)?;
    if let Some(reference) = reference {
        if !settings
            .providers
            .iter()
            .any(|p| p.credential_ref.as_ref() == Some(&reference))
        {
            let _ = credential(&reference).and_then(|entry| {
                entry
                    .delete_credential()
                    .map_err(|_| error("AI_CREDENTIAL_IO"))
            });
        }
    }
    Ok(snapshot(settings))
}
/// 默认模型被删除时保留失效选择，由 UI 明确要求用户重新选择。
pub fn update_defaults(
    path: &Path,
    expected: u64,
    model: Option<ModelRef>,
    permission: Permission,
) -> Result<Snapshot, OperationError> {
    let _guard = lock()?;
    let mut settings = read(path)?;
    if settings.revision != expected {
        return Err(error("AI_STALE_SETTINGS"));
    }
    if let Some(model) = &model {
        if !settings.providers.iter().any(|p| {
            p.provider_id == model.provider_id
                && p.models.iter().any(|m| m.model_id == model.model_id)
        }) {
            return Err(error("AI_UNKNOWN_MODEL"));
        }
    }
    settings.default_model = model;
    settings.default_permission = permission;
    settings.revision += 1;
    write(path, &settings)?;
    Ok(snapshot(settings))
}

#[cfg(test)]
mod tests {
    use super::*;
    /// 测试从独立字面量构造配置，不能用生产校验器生成期望值。
    fn provider() -> Provider {
        serde_json::from_value(serde_json::json!({"providerId":"gateway","displayName":"网关","protocol":"openai-completions","baseUrl":"https://example.invalid/v1","credentialRef":null,"allowInsecureHttp":false,"models":[{"modelId":"fixture","displayName":"模型","contextWindow":8192,"maxOutputTokens":1024}]})).unwrap()
    }
    #[test]
    /// 后端必须独立拒绝凭据 URL，不能仅信任前端校验。
    fn rejects_credential_url_and_invalid_capacity() {
        let mut p = provider();
        p.base_url = "https://secret@example.invalid".into();
        assert!(validate_provider(&p).is_err());
        p = provider();
        p.models[0].max_output_tokens = 8192;
        assert!(validate_provider(&p).is_err());
        p = provider();
        p.models[0].model_id = " fixture ".into();
        assert!(validate_provider(&p).is_err());
        p = provider();
        p.models[0].model_id = "x".repeat(201);
        assert!(validate_provider(&p).is_err());
    }
    #[test]
    /// 客户端不能通过 JSON 混入 API 密钥字段。
    fn rejects_plaintext_key_in_configuration() {
        let mut value = serde_json::to_value(provider()).unwrap();
        value["apiKey"] = "secret".into();
        assert!(serde_json::from_value::<Provider>(value).is_err());
    }

    #[test]
    /// 配置版本冲突不能覆盖已保存内容，普通配置不得包含临时测试密钥。
    fn configuration_revision_and_native_credential_rotation() {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("settings.json");
        let first = save_provider(&path, 0, provider(), CredentialAction::Keep).unwrap();
        assert_eq!(first.settings.revision, 1);
        assert!(save_provider(&path, 0, provider(), CredentialAction::Keep).is_err());
        assert_eq!(read(&path).unwrap().revision, 1);
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        {
            let result = save_provider(
                &path,
                1,
                provider(),
                CredentialAction::Replace {
                    value: "gitmaster-temporary-test-key".into(),
                },
            );
            let saved = result.unwrap();
            let reference = saved.settings.providers[0].credential_ref.clone().unwrap();
            let secret = read_secret(&reference);
            let persisted = fs::read_to_string(&path).unwrap();
            let removed = save_provider(&path, 2, provider(), CredentialAction::Remove);
            // 即使后续断言失败，测试也已移除自己创建的系统凭据。
            let absent = read_secret(&reference);
            assert_eq!(
                secret.unwrap().as_deref(),
                Some("gitmaster-temporary-test-key")
            );
            assert!(!persisted.contains("gitmaster-temporary-test-key"));
            assert!(removed.unwrap().settings.providers[0]
                .credential_ref
                .is_none());
            assert_eq!(absent.unwrap(), None);
        }
    }
}
