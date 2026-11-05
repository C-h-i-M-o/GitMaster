use gitmaster_core::agent::{
    policy::{decide, PolicyDecision, ToolRisk},
    PermissionMode,
};

#[test]
fn permission_matrix_distinguishes_external_files_network_and_git_risks() {
    use PermissionMode::*;
    use PolicyDecision::*;
    let normal = ToolRisk::File {
        outside: false,
        sensitive: false,
        write: true,
    };
    let external = ToolRisk::File {
        outside: true,
        sensitive: false,
        write: true,
    };
    let sensitive = ToolRisk::File {
        outside: false,
        sensitive: true,
        write: false,
    };
    assert_eq!(decide(RequestApproval, normal), Ask);
    assert_eq!(decide(RequestApproval, external), Ask);
    assert_eq!(decide(AutoApprove, external), Allow);
    assert_eq!(decide(AutoApprove, sensitive), Deny);
    assert_eq!(decide(FullAccess, sensitive), Deny);
    assert_eq!(
        decide(RequestApproval, ToolRisk::Network { private: false }),
        Ask
    );
    assert_eq!(
        decide(AutoApprove, ToolRisk::Network { private: false }),
        Ask
    );
    assert_eq!(
        decide(AutoApprove, ToolRisk::Network { private: true }),
        Ask
    );
    assert_eq!(
        decide(
            AutoApprove,
            ToolRisk::Git {
                changes_worktree_or_remote: true
            }
        ),
        Ask
    );
    assert_eq!(
        decide(
            FullAccess,
            ToolRisk::Git {
                changes_worktree_or_remote: true
            }
        ),
        Ask
    );
}

#[test]
fn all_modes_reject_tools_that_bypass_the_registered_capabilities() {
    for mode in [
        PermissionMode::RequestApproval,
        PermissionMode::AutoApprove,
        PermissionMode::FullAccess,
    ] {
        assert_eq!(decide(mode, ToolRisk::Forbidden), PolicyDecision::Deny);
    }
}
