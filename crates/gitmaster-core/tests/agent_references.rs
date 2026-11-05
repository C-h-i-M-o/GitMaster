use gitmaster_core::agent::{
    provider::Cancellation,
    references::{verify_references, ReferenceKind, UserReferenceSpec},
    tools::registry::Toolbox,
    AgentStore, EventKind, PermissionMode, ProviderProfile, ProviderProtocol, ToolCall,
};
use serde_json::json;
use std::sync::Arc;

fn tools(root: &std::path::Path) -> Arc<Toolbox> {
    Arc::new(Toolbox::scoped(None, root.into(), root.into(), vec![]).unwrap())
}

fn file_spec(path: &str) -> UserReferenceSpec {
    UserReferenceSpec {
        kind: ReferenceKind::File,
        path: Some(path.into()),
        start_line: None,
        end_line: None,
        expected_version: None,
        text: None,
        staged: None,
    }
}

#[tokio::test]
async fn text_reference_is_explicit_bounded_and_marked_as_user_selected() {
    let dir = tempfile::tempdir().unwrap();
    let toolbox = tools(dir.path());
    let specs = [UserReferenceSpec {
        kind: ReferenceKind::Text,
        path: None,
        start_line: None,
        end_line: None,
        expected_version: None,
        text: Some("用户明确选中的文本".into()),
        staged: None,
    }];

    let verified = verify_references(&toolbox, &specs, &Cancellation::new())
        .await
        .unwrap();
    assert_eq!(verified.len(), 1);
    assert_eq!(verified[0].text, "用户明确选中的文本");
    assert!(verified[0].label.contains("用户选中"));
    assert!(verified[0].version.len() == 64);
}

#[tokio::test]
async fn file_reference_uses_scoped_tool_and_selects_inclusive_one_based_lines() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("note.txt"), "第一行\nsecond\n第三行\n").unwrap();
    let toolbox = tools(dir.path());
    let mut spec = file_spec("note.txt");
    spec.start_line = Some(2);
    spec.end_line = Some(3);

    let verified = verify_references(&toolbox, &[spec], &Cancellation::new())
        .await
        .unwrap();
    assert_eq!(verified[0].text, "second\n第三行\n");
    assert_eq!(verified[0].start_line, Some(2));
    assert_eq!(verified[0].end_line, Some(3));
    let expected_path = std::fs::canonicalize(dir.path().join("note.txt")).unwrap();
    assert_eq!(verified[0].path.as_deref(), expected_path.to_str());
}

#[tokio::test]
async fn explicit_text_reference_checks_version_and_rejects_empty_text() {
    let dir = tempfile::tempdir().unwrap();
    let toolbox = tools(dir.path());
    let mut spec = UserReferenceSpec {
        kind: ReferenceKind::Text,
        path: None,
        start_line: None,
        end_line: None,
        expected_version: None,
        text: Some("显式选区".into()),
        staged: None,
    };
    let first = verify_references(&toolbox, &[spec.clone()], &Cancellation::new())
        .await
        .unwrap();
    spec.expected_version = Some(first[0].version.clone());
    assert!(
        verify_references(&toolbox, &[spec.clone()], &Cancellation::new())
            .await
            .is_ok()
    );
    spec.expected_version = Some("0".repeat(64));
    assert!(verify_references(&toolbox, &[spec], &Cancellation::new())
        .await
        .is_err());

    let empty = UserReferenceSpec {
        kind: ReferenceKind::Text,
        path: None,
        start_line: None,
        end_line: None,
        expected_version: None,
        text: Some(String::new()),
        staged: None,
    };
    assert!(verify_references(&toolbox, &[empty], &Cancellation::new())
        .await
        .is_err());
}

#[tokio::test]
async fn file_reference_rejects_stale_version_bad_ranges_and_sensitive_files() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("note.txt"), "before\nafter\n").unwrap();
    std::fs::write(dir.path().join(".env"), "TOKEN=not-for-reference\n").unwrap();
    let toolbox = tools(dir.path());

    let mut stale = file_spec("note.txt");
    stale.expected_version = Some("0".repeat(64));
    assert!(verify_references(&toolbox, &[stale], &Cancellation::new())
        .await
        .is_err());

    let mut invalid_range = file_spec("note.txt");
    invalid_range.start_line = Some(2);
    invalid_range.end_line = Some(1);
    assert!(
        verify_references(&toolbox, &[invalid_range], &Cancellation::new())
            .await
            .is_err()
    );
    assert!(
        verify_references(&toolbox, &[file_spec(".env")], &Cancellation::new())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn git_diff_reference_is_read_only_versioned_and_requires_git_context() {
    use gitmaster_core::{
        agent::{
            summary::collect_summary,
            tools::git::{GitContext, GitTools},
        },
        git::{
            coordinator::RepositoryCoordinator, environment::resolve_git,
            repository::open_repository,
        },
    };
    use std::process::Command;

    let dir = tempfile::tempdir().unwrap();
    let run_git = |args: &[&str]| {
        let output = Command::new("git")
            .args(args)
            .current_dir(dir.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
        output
    };
    run_git(&["init", "-b", "main"]);
    run_git(&["config", "user.name", "GitMaster Test"]);
    run_git(&["config", "user.email", "test@example.invalid"]);
    std::fs::write(dir.path().join("note.txt"), "baseline\n").unwrap();
    run_git(&["add", "--", "note.txt"]);
    run_git(&[
        "-c",
        "user.name=GitMaster Test",
        "-c",
        "user.email=test@example.invalid",
        "commit",
        "-m",
        "baseline",
    ]);
    std::fs::write(dir.path().join("note.txt"), "first edit\n").unwrap();

    let git = resolve_git(None).unwrap();
    let (repository, _) = open_repository(&git, dir.path()).unwrap();
    let context = GitContext {
        git,
        repository,
        coordinator: RepositoryCoordinator::new(),
    };
    let git_tools = GitTools::new(context.clone());
    let toolbox = Arc::new(
        Toolbox::scoped(Some(context), dir.path().into(), dir.path().into(), vec![]).unwrap(),
    );
    let spec = UserReferenceSpec {
        kind: ReferenceKind::GitDiff,
        path: Some("note.txt".into()),
        start_line: None,
        end_line: None,
        expected_version: None,
        text: None,
        staged: Some(false),
    };
    let first = verify_references(&toolbox, &[spec.clone()], &Cancellation::new())
        .await
        .unwrap();
    assert!(first[0].text.contains("first edit"));
    assert_eq!(first[0].path.as_deref(), Some("note.txt"));
    assert!(!first[0].version.is_empty());

    std::fs::write(dir.path().join("note.txt"), "second edit\n").unwrap();
    let mut stale = spec;
    stale.expected_version = Some(first[0].version.clone());
    assert!(verify_references(&toolbox, &[stale], &Cancellation::new())
        .await
        .is_err());

    let non_git = tools(dir.path());
    assert!(verify_references(
        &non_git,
        &[UserReferenceSpec {
            kind: ReferenceKind::GitDiff,
            path: Some("note.txt".into()),
            start_line: None,
            end_line: None,
            expected_version: None,
            text: None,
            staged: Some(false),
        }],
        &Cancellation::new()
    )
    .await
    .is_err());

    // 通配符字符按 literal pathspec 处理；相似路径不得被扩展进差异。
    std::fs::write(dir.path().join("literal[1].txt"), "literal baseline\n").unwrap();
    std::fs::write(dir.path().join("literal1.txt"), "other baseline\n").unwrap();
    run_git(&["add", "--", "note.txt", "literal[1].txt", "literal1.txt"]);
    run_git(&[
        "-c",
        "user.name=GitMaster Test",
        "-c",
        "user.email=test@example.invalid",
        "commit",
        "-m",
        "ordinary files",
    ]);
    std::fs::write(
        dir.path().join("literal[1].txt"),
        "literal selected change\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("literal1.txt"),
        "other matching path change\n",
    )
    .unwrap();
    let literal = verify_references(
        &toolbox,
        &[UserReferenceSpec {
            kind: ReferenceKind::GitDiff,
            path: Some("literal[1].txt".into()),
            start_line: None,
            end_line: None,
            expected_version: None,
            text: None,
            staged: Some(false),
        }],
        &Cancellation::new(),
    )
    .await
    .unwrap();
    assert!(literal[0].text.contains("literal selected change"));
    assert!(!literal[0].text.contains("other matching path change"));

    // Unicode 普通路径关闭 Git C 引用后可与审核清单逐字绑定。
    std::fs::write(dir.path().join("中文.txt"), "unicode baseline\n").unwrap();
    run_git(&["add", "--", "中文.txt"]);
    run_git(&[
        "-c",
        "user.name=GitMaster Test",
        "-c",
        "user.email=test@example.invalid",
        "commit",
        "-m",
        "unicode path",
    ]);
    std::fs::write(dir.path().join("中文.txt"), "unicode changed\n").unwrap();
    let unicode = verify_references(
        &toolbox,
        &[UserReferenceSpec {
            kind: ReferenceKind::GitDiff,
            path: Some("中文.txt".into()),
            start_line: None,
            end_line: None,
            expected_version: None,
            text: None,
            staged: Some(false),
        }],
        &Cancellation::new(),
    )
    .await
    .unwrap();
    assert!(unicode[0].text.contains("diff --git a/中文.txt b/中文.txt"));
    assert!(unicode[0].text.contains("unicode changed"));

    // 整仓路径清单含敏感项时，工作区diff、整提交show和开发总结都拒绝读取。
    std::fs::write(dir.path().join(".env"), "ALPHA=not-patterned-value\n").unwrap();
    run_git(&["add", "--", ".env"]);
    run_git(&[
        "-c",
        "user.name=GitMaster Test",
        "-c",
        "user.email=test@example.invalid",
        "commit",
        "-m",
        "sensitive path fixture",
    ]);
    let sensitive_commit = String::from_utf8(run_git(&["rev-parse", "HEAD"]).stdout)
        .unwrap()
        .trim()
        .to_owned();
    std::fs::write(dir.path().join(".env"), "ALPHA=changed-value\n").unwrap();

    assert!(verify_references(
        &toolbox,
        &[UserReferenceSpec {
            kind: ReferenceKind::GitDiff,
            path: Some(".env".into()),
            start_line: None,
            end_line: None,
            expected_version: None,
            text: None,
            staged: Some(false),
        }],
        &Cancellation::new(),
    )
    .await
    .is_err());
    assert!(verify_references(
        &toolbox,
        &[UserReferenceSpec {
            kind: ReferenceKind::GitDiff,
            path: None,
            start_line: None,
            end_line: None,
            expected_version: None,
            text: None,
            staged: Some(false),
        }],
        &Cancellation::new(),
    )
    .await
    .is_err());
    let sensitive_path_call = ToolCall {
        id: "a2".into(),
        name: "git_show".into(),
        arguments: json!({"commitId":sensitive_commit,"path":".env"}),
    };
    assert!(git_tools
        .prepare(&sensitive_path_call, PermissionMode::RequestApproval)
        .is_err());
    let full_commit_call = ToolCall {
        id: "a3".into(),
        name: "git_show".into(),
        arguments: json!({"commitId":sensitive_commit}),
    };
    let prepared = git_tools
        .prepare(&full_commit_call, PermissionMode::RequestApproval)
        .unwrap();
    assert!(git_tools
        .execute(prepared, false, &Cancellation::new())
        .is_err());
    assert!(collect_summary(&git_tools, &[sensitive_commit]).is_err());
}

#[tokio::test]
async fn references_enforce_item_total_text_and_cancellation_limits() {
    let dir = tempfile::tempdir().unwrap();
    let toolbox = tools(dir.path());
    let too_many = (0..9)
        .map(|_| UserReferenceSpec {
            kind: ReferenceKind::Text,
            path: None,
            start_line: None,
            end_line: None,
            expected_version: None,
            text: Some("x".into()),
            staged: None,
        })
        .collect::<Vec<_>>();
    assert!(verify_references(&toolbox, &too_many, &Cancellation::new())
        .await
        .is_err());

    let too_large = UserReferenceSpec {
        kind: ReferenceKind::Text,
        path: None,
        start_line: None,
        end_line: None,
        expected_version: None,
        text: Some("x".repeat(64 * 1024 + 1)),
        staged: None,
    };
    assert!(
        verify_references(&toolbox, &[too_large], &Cancellation::new())
            .await
            .is_err()
    );

    let cancelled = Cancellation::new();
    cancelled.cancel();
    assert!(verify_references(
        &toolbox,
        &[UserReferenceSpec {
            kind: ReferenceKind::Text,
            path: None,
            start_line: None,
            end_line: None,
            expected_version: None,
            text: Some("text".into()),
            staged: None,
        }],
        &cancelled,
    )
    .await
    .is_err());
}

#[test]
fn tool_result_pagination_is_chat_scoped_utf8_safe_and_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let store = AgentStore::open(dir.path().join("agent")).unwrap();
    store
        .save_profile(ProviderProfile {
            id: "aa".into(),
            name: "测试".into(),
            protocol: ProviderProtocol::Responses,
            base_url: "http://127.0.0.1:9999/v1".into(),
            model: "test-model".into(),
            credential_id: "bb".into(),
        })
        .unwrap();
    let chat = store.create_chat("aa", None).unwrap();
    let other_chat = store.create_chat("aa", None).unwrap();
    let result = format!("A{}B{}C", "中".repeat(5000), "é".repeat(5000));
    let event = store
        .append(
            &chat.id,
            "ab",
            EventKind::ToolResult {
                call_id: "cd".into(),
                name: "read_file".into(),
                result: json!({"text":result}),
                is_error: false,
            },
        )
        .unwrap();

    let first = store
        .read_tool_result(&chat.id, event.sequence, 0, 8192)
        .unwrap();
    assert_eq!(first.offset, 0);
    assert!(first.text.len() <= 8192);
    assert_eq!(first.next_offset, Some(first.text.len()));
    assert!(first.truncated);
    let second = store
        .read_tool_result(&chat.id, event.sequence, first.next_offset.unwrap(), 8192)
        .unwrap();
    assert!(second.text.starts_with('中') || second.text.starts_with('é'));
    assert!(store
        .read_tool_result(&chat.id, event.sequence, 2, 128)
        .is_err());
    assert!(store
        .read_tool_result(&chat.id, event.sequence, 0, 8193)
        .is_err());
    assert!(store
        .read_tool_result(&other_chat.id, event.sequence, 0, 100)
        .is_err());
}

#[test]
fn tool_result_reader_rejects_non_tool_result_events() {
    let dir = tempfile::tempdir().unwrap();
    let store = AgentStore::open(dir.path().join("agent")).unwrap();
    store
        .save_profile(ProviderProfile {
            id: "aa".into(),
            name: "测试".into(),
            protocol: ProviderProtocol::Responses,
            base_url: "http://127.0.0.1:9999/v1".into(),
            model: "test-model".into(),
            credential_id: "bb".into(),
        })
        .unwrap();
    let chat = store.create_chat("aa", None).unwrap();
    let event = store
        .append(
            &chat.id,
            "ab",
            EventKind::UserMessage {
                text: "not a tool result".into(),
            },
        )
        .unwrap();
    assert!(store
        .read_tool_result(&chat.id, event.sequence, 0, 100)
        .is_err());
}

/// 确保公开的测试合同中的权限模式仍与显式用户发起引用相匹配。
#[test]
fn request_approval_is_the_only_reference_mode_used() {
    assert_eq!(PermissionMode::default(), PermissionMode::RequestApproval);
}
