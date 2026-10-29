//! M2/M3 DTO 的桌面消费级 serde 契约测试。
use gitmaster_core::git::types::{
    CloneRecovery, CloneStage, ConflictWriteRequest, LocalWriteRequest, OperationKind,
    OperationResult, WriteTarget,
};

/// 结果摘要区分两种冲突，但撤销写能力仍必须通过读取会话获取。
#[test]
fn conflict_results_identify_merge_and_revert() {
    use gitmaster_core::git::types::{ConflictSummary, WriteRefresh};
    for (conflict, kind, field) in [
        (
            ConflictSummary::Merge {
                merge_session_id: "session".into(),
                unresolved_count: 1,
                paths: vec!["file".into()],
            },
            "merge",
            "mergeSessionId",
        ),
        (
            ConflictSummary::Revert {
                target_commit_oid: "target".into(),
                unresolved_count: 1,
                paths: vec!["file".into()],
            },
            "revert",
            "targetCommitOid",
        ),
    ] {
        let value = serde_json::to_value(OperationResult::NeedsResolution {
            operation_id: "operation".into(),
            kind: if kind == "merge" {
                OperationKind::Integrate
            } else {
                OperationKind::Revert
            },
            conflict,
            refresh: WriteRefresh::NotApplicable,
        })
        .unwrap();
        assert_eq!(value["outcome"], "needsResolution");
        assert_eq!(value["conflict"]["kind"], kind);
        assert!(value["conflict"][field].is_string());
        assert_eq!(value["conflict"]["unresolvedCount"], 1);
    }
}

/// 撤销请求使用独立类型；旧合并接口不能接收撤销会话的保存与完成语义。
#[test]
fn revert_requests_are_distinct_and_use_camel_case() {
    use gitmaster_core::git::RevertConflictWriteRequest;
    let start: LocalWriteRequest =
        serde_json::from_str(r#"{"kind":"revertCommit","commitOid":"abc"}"#).unwrap();
    assert!(matches!(start, LocalWriteRequest::RevertCommit { commit_oid } if commit_oid == "abc"));
    assert!(matches!(
        serde_json::from_str::<LocalWriteRequest>(r#"{"kind":"abortRevert"}"#).unwrap(),
        LocalWriteRequest::AbortRevert {}
    ));
    let save = r#"{"kind":"saveRevertConflict","conflictId":"file","fingerprint":"version","content":"正文"}"#;
    assert!(
        matches!(serde_json::from_str::<RevertConflictWriteRequest>(save).unwrap(), RevertConflictWriteRequest::SaveRevertConflict { conflict_id, fingerprint, .. } if conflict_id == "file" && fingerprint == "version")
    );
    assert!(serde_json::from_str::<ConflictWriteRequest>(save).is_err());
    assert!(serde_json::from_str::<RevertConflictWriteRequest>(
        r#"{"kind":"finishMerge","message":"错用"}"#
    )
    .is_err());
    assert!(
        matches!(serde_json::from_str::<RevertConflictWriteRequest>(r#"{"kind":"finishRevert","message":"撤销"}"#).unwrap(), RevertConflictWriteRequest::FinishRevert { message } if message == "撤销")
    );
}

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
        path_results: Vec::new(),
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
        path_results: Vec::new(),
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
