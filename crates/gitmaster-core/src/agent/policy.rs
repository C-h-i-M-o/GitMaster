use super::PermissionMode;

/// 风险来自后端解析后的实际目标，不能由模型自行标记为安全。
#[derive(Debug, Clone, Copy)]
pub enum ToolRisk {
    File {
        outside: bool,
        sensitive: bool,
        write: bool,
    },
    Network {
        private: bool,
    },
    Git {
        changes_worktree_or_remote: bool,
    },
    Forbidden,
}
/// 执行策略与审批 UI 分离，任何档位都不能添加未注册能力。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyDecision {
    Allow,
    Ask,
    Deny,
}

/// 按已批准矩阵确定是否询问；完全访问不代表系统提权或任意 shell。
pub fn decide(mode: PermissionMode, risk: ToolRisk) -> PolicyDecision {
    use PolicyDecision::*;
    if matches!(risk, ToolRisk::Forbidden) {
        return Deny;
    }
    if mode == PermissionMode::FullAccess {
        return Allow;
    }
    match risk {
        ToolRisk::File {
            outside, sensitive, ..
        } => {
            if sensitive || (outside && mode == PermissionMode::RequestApproval) {
                Ask
            } else {
                Allow
            }
        }
        ToolRisk::Network { private } => {
            if private || mode == PermissionMode::RequestApproval {
                Ask
            } else {
                Allow
            }
        }
        ToolRisk::Git {
            changes_worktree_or_remote,
        } => {
            if changes_worktree_or_remote {
                Ask
            } else {
                Allow
            }
        }
        ToolRisk::Forbidden => Deny,
    }
}
