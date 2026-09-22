//! M2/M3 DTO 的桌面消费级 serde 契约测试。
use gitmaster_core::git::types::{
    CloneRecovery, CloneStage, ConflictWriteRequest, LocalWriteRequest, OperationKind,
    OperationResult, WriteTarget,
};

/// 前端的 camelCase 字段必须映射到真实请求参数。
#[test]
fn write_requests_use_camel_case_fields() {
    let request: LocalWriteRequest =
        serde_json::from_str(r#"{"kind":"stage","changeIds":["c1"]}"#).unwrap();
    assert!(matches!(request, LocalWriteRequest::Stage { change_ids } if change_ids == ["c1"]));
    let conflict: ConflictWriteRequest = serde_json::from_str(
        r#"{"kind":"saveConflict","conflictId":"f1","fingerprint":"p1","content":"ok"}"#,
    )
    .unwrap();
    assert!(
        matches!(conflict, ConflictWriteRequest::SaveConflict { conflict_id, fingerprint, .. } if conflict_id == "f1" && fingerprint == "p1")
    );
    assert!(serde_json::from_str::<ConflictWriteRequest>(
        r#"{"kind":"saveConflict","conflictId":"f1","content":"ok"}"#
    )
    .is_err());
}

/// 前端可识别目标、空值、操作身份和结果分支。
#[test]
fn operation_result_and_target_are_consumable() {
    let target = serde_json::to_value(WriteTarget::Remote {
        remote_id: "r1".into(),
        display_url: "https://example.invalid/repo".into(),
        ref_name: "refs/heads/main".into(),
        oid: None,
        source_oid: Some("abc".into()),
        pending_commits: Vec::new(),
    })
    .unwrap();
    assert_eq!(target["kind"], "remote");
    assert_eq!(target["remoteId"], "r1");
    assert!(target["oid"].is_null());
    assert_eq!(target["sourceOid"], "abc");
    let fetch_target = serde_json::to_value(WriteTarget::Remote {
        remote_id: "r1".into(),
        display_url: "https://example.invalid/repo".into(),
        ref_name: "refs/heads/main".into(),
        oid: Some("abc".into()),
        source_oid: None,
        pending_commits: Vec::new(),
    })
    .unwrap();
    assert!(fetch_target["sourceOid"].is_null());
    let result = OperationResult::Unknown {
        operation_id: "op1".into(),
        kind: OperationKind::Fetch,
        error: gitmaster_core::git::error::OperationError::new("WRITE_OUTCOME_UNKNOWN"),
        clone_recovery: None,
        refresh: gitmaster_core::git::types::WriteRefresh::NotApplicable,
    };
    let value = serde_json::to_value(result).unwrap();
    assert_eq!(value["outcome"], "unknown");
    assert_eq!(value["operationId"], "op1");
    assert!(value["error"].is_object());
    assert!(value["cloneRecovery"].is_null());
    assert_eq!(value["refresh"]["status"], "notApplicable");
}

/// 克隆失败时向前端提供可恢复路径与阶段。
#[test]
fn clone_failure_serializes_recovery_context() {
    let result = OperationResult::Failed {
        operation_id: "op-clone".into(),
        kind: OperationKind::Clone,
        error: gitmaster_core::git::error::OperationError::new("CHECKOUT_UNSUPPORTED"),
        clone_recovery: Some(CloneRecovery {
            path: "/tmp/example".into(),
            stage: CloneStage::Checkout,
        }),
        refresh: gitmaster_core::git::types::WriteRefresh::NotApplicable,
    };
    let value = serde_json::to_value(result).unwrap();
    assert_eq!(value["outcome"], "failed");
    assert_eq!(value["cloneRecovery"]["path"], "/tmp/example");
    assert_eq!(value["cloneRecovery"]["stage"], "checkout");
}
