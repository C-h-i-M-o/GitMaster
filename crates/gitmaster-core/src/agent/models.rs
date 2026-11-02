use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fmt,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

/// 安全的用户错误，不包含 HTTP 响应原文、鉴权头或底层凭据错误。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentError {
    pub code: String,
    pub message: String,
}
impl AgentError {
    /// 用稳定错误码与中文提示构造可跨 IPC 传递的错误。
    pub fn new(code: &str, message: &str) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
    /// 将底层文件错误统一映射，避免输出路径中的隐私信息。
    pub(crate) fn storage(_: impl fmt::Display) -> Self {
        Self::new(
            "AGENT_STORAGE",
            "无法读写 AI 本地数据，请检查目录权限和磁盘空间。",
        )
    }
    /// 标记损坏或未来版本，阻止覆盖无法理解的数据。
    pub(crate) fn corrupt() -> Self {
        Self::new(
            "AGENT_STORAGE_CORRUPT",
            "AI 数据损坏或版本不受支持，原始文件已保留。",
        )
    }
}
impl fmt::Display for AgentError {
    /// 错误日志只使用已经脱敏的中文提示。
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for AgentError {}
pub type AgentResult<T> = Result<T, AgentError>;

/// 三种独立的模型工具调用协议。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ProviderProtocol {
    ChatCompletions,
    Responses,
    AnthropicMessages,
}

/// 明确的后端权限档位，默认使用请求批准。
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PermissionMode {
    #[default]
    RequestApproval,
    AutoApprove,
    FullAccess,
}

/// 普通配置只保存凭据引用，不携带 API Key。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderProfile {
    pub id: String,
    pub name: String,
    pub protocol: ProviderProtocol,
    pub base_url: String,
    pub model: String,
    pub credential_id: String,
}
impl ProviderProfile {
    /// 校验配置输入，阻止含凭据 URL 和非回环明文传输。
    pub fn validate(&self) -> AgentResult<()> {
        valid_id(&self.id)?;
        valid_id(&self.credential_id)?;
        super::privacy::ensure_safe_text(&self.name, None)?;
        super::privacy::ensure_safe_text(&self.model, None)?;
        if self.name.trim().is_empty()
            || self.name.len() > 160
            || self.model.trim().is_empty()
            || self.model.len() > 256
            || self.model.chars().any(char::is_control)
        {
            return Err(AgentError::new(
                "AGENT_INVALID_PROFILE",
                "请填写有效的配置名称和模型 ID。",
            ));
        }
        let url = url::Url::parse(&self.base_url)
            .map_err(|_| AgentError::new("AGENT_INVALID_URL", "API 地址格式不正确。"))?;
        let loopback = match url.host() {
            Some(url::Host::Domain(host)) => host.eq_ignore_ascii_case("localhost"),
            Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
            Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
            None => false,
        };
        if url.host().is_none()
            || (!matches!(url.scheme(), "https") && !(url.scheme() == "http" && loopback))
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(AgentError::new(
                "AGENT_INVALID_URL",
                "API 地址须使用 HTTPS（本机服务可用 HTTP），且不能包含凭据、查询参数或片段。",
            ));
        }
        Ok(())
    }
    /// 同时接受 API 根地址与完整协议端点，不重复附加路径。
    pub fn endpoint(&self) -> AgentResult<String> {
        self.validate()?;
        let suffix = match self.protocol {
            ProviderProtocol::ChatCompletions => "/chat/completions",
            ProviderProtocol::Responses => "/responses",
            ProviderProtocol::AnthropicMessages => "/messages",
        };
        let base = self.base_url.trim_end_matches('/');
        Ok(if base.ends_with(suffix) {
            base.into()
        } else {
            format!("{base}{suffix}")
        })
    }
}

/// 独立的 Agent 配置不会改变既有工作台设置版本。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentSettings {
    pub version: u8,
    pub profiles: Vec<ProviderProfile>,
    pub active_profile_id: Option<String>,
    pub default_permission_mode: PermissionMode,
}
impl Default for AgentSettings {
    /// 未配置时不发起模型请求，默认要求对越界和风险操作询问。
    fn default() -> Self {
        Self {
            version: 1,
            profiles: vec![],
            active_profile_id: None,
            default_permission_mode: PermissionMode::default(),
        }
    }
}

/// 聊天元数据与仓库路径绑定；重启后仍需验证实际仓库身份。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChatSession {
    pub version: u8,
    pub id: String,
    pub title: String,
    pub repository_path: Option<String>,
    pub profile_id: String,
    pub permission_mode: PermissionMode,
    pub archived: bool,
    pub created_at: u64,
    pub updated_at: u64,
}

/// 已完整解析的工具调用，参数仍需各工具的严格类型检查。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

/// 审批由运行控制器签发，绑定完整调用及当前运行。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalRequest {
    pub id: String,
    pub call: ToolCall,
    pub reason: String,
    pub target: String,
    #[serde(default)]
    pub preview: Option<Value>,
}

/// 运行终态用于区分取消、中断、失败和成功，不据此推断副作用已回滚。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RunStatus {
    Completed,
    Cancelled,
    Failed,
    Interrupted,
}

/// 持久会话事实；流式分片单独通知界面，完整模型响应才落盘。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum EventKind {
    RunStarted {
        profile: ProviderProfile,
        permission_mode: PermissionMode,
    },
    UserMessage {
        text: String,
    },
    AssistantMessage {
        text: String,
        protocol: ProviderProtocol,
        wire: Value,
        calls: Vec<ToolCall>,
    },
    ToolCall {
        call: ToolCall,
    },
    ApprovalRequested {
        approval: ApprovalRequest,
    },
    ApprovalDecided {
        approval_id: String,
        approved: bool,
    },
    ToolResult {
        call_id: String,
        name: String,
        result: Value,
        is_error: bool,
    },
    SummaryEvidence {
        call_id: String,
        evidence: Value,
        related_chats: Vec<String>,
    },
    RunEnded {
        status: RunStatus,
        message: Option<String>,
    },
}

/// 持久事件按聊天单调编号，前端可丢弃重复与过期通知。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentEvent {
    pub version: u8,
    pub sequence: u64,
    pub session_id: String,
    pub run_id: String,
    pub timestamp: u64,
    #[serde(flatten)]
    pub payload: EventKind,
}
/// 完整聊天读取结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatRecord {
    pub session: ChatSession,
    pub events: Vec<AgentEvent>,
}
/// 损坏聊天独立报告，不隐藏仍能使用的其他聊天。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatList {
    pub chats: Vec<ChatSession>,
    pub unavailable: Vec<String>,
}
/// 仅统计 Agent 自有目录的真实文件字节数。
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentUsage {
    pub chat_bytes: u64,
    pub archived_bytes: u64,
    pub other_bytes: u64,
    pub location: String,
}

/// 返回本地时间戳，采用 UTC 毫秒存储，由客户端按本地时区展示。
pub fn timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}
/// 生成不含路径字符的进程唯一标识；时间、进程和序号共同避免冲突。
pub fn new_id() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!(
        "{nanos:x}-{:x}-{:x}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}
/// 不透明 ID 只接受有界十六进制片段，禁止路径与目录穿越。
pub(crate) fn valid_id(id: &str) -> AgentResult<()> {
    if id.is_empty() || id.len() > 96 || !id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-') {
        return Err(AgentError::new("AGENT_INVALID_ID", "AI 数据标识无效。"));
    }
    Ok(())
}
