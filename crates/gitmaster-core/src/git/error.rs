use serde::Serialize;

/// 可跨客户端消费的稳定错误，不包含 stderr 或敏感上下文。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OperationError {
    pub code: String,
    pub retryable: bool,
}

impl OperationError {
    /// 用稳定代码构造错误；展示层负责中文文案。
    pub fn new(code: &str) -> Self {
        Self {
            code: code.to_owned(),
            retryable: matches!(
                code,
                "TIMEOUT"
                    | "STALE_WRITE_PLAN"
                    | "STALE_GRAPH"
                    | "STALE_CONFLICT"
                    | "OPERATION_IN_PROGRESS"
                    | "QUEUE_FULL"
                    | "REMOTE_CHANGED"
                    | "AUTH_REQUIRED"
                    | "NETWORK_FAILED"
                    | "WRITE_OUTCOME_UNKNOWN"
                    | "STALE_REQUEST"
                    | "FILE_UNAVAILABLE"
                    | "GIT_EXECUTION_FAILED"
                    | "SETTINGS_IO"
            ),
        }
    }
}
