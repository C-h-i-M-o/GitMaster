//! 只使用系统密钥库；明文密钥不会进入设置文件、日志或读取接口。
use gitmaster_core::agent::{AgentError, AgentResult, CredentialVault};

/// macOS Keychain / Windows Credential Manager 的薄适配。
pub struct SystemCredentialVault;

/// 限定应用自己的服务命名空间和不透明凭据标识。
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn entry(id: &str) -> AgentResult<keyring::Entry> {
    if id.is_empty() || id.len() > 96 || !id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-') {
        return Err(AgentError::new("AGENT_INVALID_ID", "凭据标识无效。"));
    }
    keyring::Entry::new("com.gitmaster.agent.api", id).map_err(|_| credential_error())
}
/// 不输出平台错误中的条目、路径或密码信息。
fn credential_error() -> AgentError {
    AgentError::new(
        "AGENT_CREDENTIALS",
        "系统密钥库不可用，请检查 GitMaster 的密钥库访问权限。",
    )
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
impl CredentialVault for SystemCredentialVault {
    /// 读取本应用凭据，缺失与无权限使用不同结果。
    fn read(&self, id: &str) -> AgentResult<Option<String>> {
        match entry(id)?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(credential_error()),
        }
    }
    /// 交给平台加密保存，不创建普通文件回退。
    fn write(&self, id: &str, secret: &str) -> AgentResult<()> {
        entry(id)?
            .set_password(secret)
            .map_err(|_| credential_error())
    }
    /// 删除本应用条目，重复删除仍保持幂等。
    fn delete(&self, id: &str) -> AgentResult<()> {
        match entry(id)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(credential_error()),
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
impl CredentialVault for SystemCredentialVault {
    /// 未支持的平台明确拒绝，不使用 keyring 的默认内存 mock 冒充持久化。
    fn read(&self, _id: &str) -> AgentResult<Option<String>> {
        Err(credential_error())
    }
    /// 未支持的平台不保存密钥。
    fn write(&self, _id: &str, _secret: &str) -> AgentResult<()> {
        Err(credential_error())
    }
    /// 未支持的平台不报告已删除。
    fn delete(&self, _id: &str) -> AgentResult<()> {
        Err(credential_error())
    }
}
