use serde::Serialize;

/// 可跨客户端消费的稳定错误，不包含 stderr 或敏感上下文。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OperationError {
    pub code: String,
    pub retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<ErrorDiagnostic>,
}

/// 可安全展示给客户端的进程诊断信息，不包含命令参数或输出内容。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ErrorDiagnostic {
    pub stage: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub os_code: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
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
            diagnostic: None,
        }
    }

    /// 附加白名单阶段及机器码，避免把敏感执行上下文传给客户端。
    pub fn with_diagnostic(
        mut self,
        stage: &str,
        os_code: Option<u32>,
        exit_code: Option<i32>,
    ) -> Self {
        self.diagnostic = Some(ErrorDiagnostic {
            stage: match stage {
                "revParse" | "status" | "log" | "revList" | "forEachRef" | "windowsProcess" => {
                    stage
                }
                _ => "gitQuery",
            }
            .to_owned(),
            os_code,
            exit_code,
        });
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 诊断结构只保留白名单字段，不承载原始错误文本。
    #[test]
    fn diagnostic_excludes_raw_text() {
        let error = OperationError::new("GIT_EXECUTION_FAILED").with_diagnostic(
            "gitQuery",
            None,
            Some(129),
        );
        let diagnostic = error.diagnostic.unwrap();
        assert_eq!(diagnostic.stage, "gitQuery");
        assert_eq!(diagnostic.exit_code, Some(129));
        assert!(error.code.contains("GIT_EXECUTION_FAILED"));
        let unknown =
            OperationError::new("GIT_EXECUTION_FAILED").with_diagnostic("secret-path", None, None);
        assert_eq!(unknown.diagnostic.unwrap().stage, "gitQuery");
    }
}
