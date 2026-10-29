use super::*;
use crate::git::{
    conflicts::ConflictSession,
    repository::{open_repository, tests::Fixture},
};
use std::process::Command;

/// 已确认的中止还原索引与冲突文件，但不移动分支或删除未跟踪内容。
#[test]
fn revert_abort_restores_conflict_without_moving_head_or_cleaning_untracked() {
    use crate::git::{coordinator::RepositoryCoordinator, repository::read_repository_state};
    use std::time::{Duration, Instant};
    let (fixture, repo, target) = conflicting_revert();
    let state = read_repository_state(&fixture.git, &repo).unwrap();
    anchor::RevertAnchor::new(&state.head, &target)
        .unwrap()
        .create(&repo, Instant::now() + Duration::from_secs(10))
        .unwrap();
    fixture.write("untracked.txt", b"keep untracked\n");
    fixture.write("unrelated", b"later draft\n");
    let state = read_repository_state(&fixture.git, &repo).unwrap();
    let index = std::fs::read(repo.git_dir.join("index")).unwrap();
    let file = std::fs::read(repo.root.join("file")).unwrap();
    let coordinator = RepositoryCoordinator::new();
    let preview = prepare_abort(&coordinator, &fixture.git, &repo, &state).unwrap();
    assert_eq!(preview.kind, OperationKind::AbortRevert);
    assert!(!preview.paths.contains(&"untracked.txt".into()));
    assert_eq!(std::fs::read(repo.git_dir.join("index")).unwrap(), index);
    assert_eq!(std::fs::read(repo.root.join("file")).unwrap(), file);
    let handle = coordinator
        .execute(Some(&repo.id), &preview.plan_id)
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(125);
    let result = loop {
        if let Some(result) = coordinator
            .read_operation(Some(&handle.operation_id))
            .unwrap()
            .and_then(|record| record.result)
        {
            break result;
        }
        assert!(Instant::now() < deadline, "中止撤销未在期限内结束");
        std::thread::sleep(Duration::from_millis(10));
    };
    assert!(
        matches!(
            result,
            OperationResult::Succeeded {
                commit_oid: None,
                ..
            }
        ),
        "{result:?}"
    );
    let current = read_repository_state(&fixture.git, &repo).unwrap();
    assert_eq!(current.head, state.head);
    assert!(current.operations.is_empty());
    assert_eq!(std::fs::read(repo.root.join("file")).unwrap(), b"later\n");
    assert_eq!(
        std::fs::read(repo.root.join("unrelated")).unwrap(),
        b"later draft\n"
    );
    assert_eq!(
        std::fs::read(repo.root.join("untracked.txt")).unwrap(),
        b"keep untracked\n"
    );
    assert!(!repo.git_dir.join(anchor::FILE).exists());
}

/// 无冲突撤销准备只读，确认后固定原 HEAD 并仅应用选中提交的反向修改。
#[test]
fn revert_start_previews_then_preserves_head_and_unrelated_content() {
    use crate::git::{coordinator::RepositoryCoordinator, repository::query};
    use std::time::{Duration, Instant};
    let fixture = Fixture::new();
    // 此用例验证撤销事务；换行策略固定在临时仓库，避免依赖本机全局配置。
    fixture.command(&["config", "core.autocrlf", "false"]);
    fixture.write("file", b"base\n");
    fixture.write("unrelated", b"preserved\n");
    fixture.command(&["add", "."]);
    fixture.command(&["commit", "-m", "base"]);
    fixture.write("file", b"target\n");
    fixture.command(&["add", "file"]);
    fixture.command(&["commit", "-m", "target"]);
    let (repo, state) = open_repository(&fixture.git, &fixture.root).unwrap();
    let HeadState::Branch { oid, .. } = &state.head else {
        panic!("预期分支");
    };
    let before_index = std::fs::read(repo.git_dir.join("index")).unwrap();
    let coordinator = RepositoryCoordinator::new();
    let preview = prepare_revert(&coordinator, &fixture.git, &repo, &state, oid).unwrap();
    assert_eq!(preview.kind, OperationKind::Revert);
    assert_eq!(preview.paths, ["file"]);
    assert!(!repo.git_dir.join("gitmaster-revert.json").exists());
    assert_eq!(
        std::fs::read(repo.git_dir.join("index")).unwrap(),
        before_index
    );
    let handle = coordinator
        .execute(Some(&repo.id), &preview.plan_id)
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(125);
    let result = loop {
        if let Some(result) = coordinator
            .read_operation(Some(&handle.operation_id))
            .unwrap()
            .and_then(|record| record.result)
        {
            break result;
        }
        assert!(Instant::now() < deadline, "撤销未按时结束");
        std::thread::sleep(Duration::from_millis(10));
    };
    assert!(
        matches!(
            result,
            OperationResult::Succeeded {
                commit_oid: None,
                ..
            }
        ),
        "{result:?}"
    );
    assert_eq!(std::fs::read(fixture.root.join("file")).unwrap(), b"base\n");
    assert_eq!(
        std::fs::read(fixture.root.join("unrelated")).unwrap(),
        b"preserved\n"
    );
    let head = query(&fixture.git, &repo.root, &["rev-parse", "HEAD"], 1024).unwrap();
    assert_eq!(String::from_utf8(head).unwrap().trim(), oid);
    let anchor = anchor::RevertAnchor::read(&repo).unwrap();
    assert_eq!(&anchor.original_head_oid, oid);
    assert_eq!(&anchor.target_commit_oid, oid);
    let session = RevertSession::new(&fixture.git, &repo).unwrap();
    assert_eq!(session.state().unresolved_count, 0);
    let preview = session.prepare_finish(&coordinator, "撤销 target").unwrap();
    assert_eq!(preview.kind, OperationKind::FinishRevert);
    assert_eq!(preview.parent_oids, [oid.clone()]);
    let staged = std::fs::read(repo.git_dir.join("index")).unwrap();
    fixture.write("unrelated", b"later draft\n");
    let handle = coordinator
        .execute(Some(&repo.id), &preview.plan_id)
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(125);
    let result = loop {
        if let Some(result) = coordinator
            .read_operation(Some(&handle.operation_id))
            .unwrap()
            .and_then(|record| record.result)
        {
            break result;
        }
        assert!(Instant::now() < deadline, "撤销提交未按时结束");
        std::thread::sleep(Duration::from_millis(10));
    };
    let OperationResult::Succeeded {
        commit_oid: Some(committed),
        ..
    } = result
    else {
        panic!("{result:?}");
    };
    let parents = query(
        &fixture.git,
        &repo.root,
        &["rev-list", "--parents", "-n", "1", &committed],
        1024,
    )
    .unwrap();
    assert_eq!(
        String::from_utf8(parents).unwrap().trim(),
        format!("{committed} {oid}")
    );
    assert_eq!(std::fs::read(repo.git_dir.join("index")).unwrap(), staged);
    assert_eq!(
        std::fs::read(fixture.root.join("unrelated")).unwrap(),
        b"later draft\n"
    );
    assert!(!repo.git_dir.join("REVERT_HEAD").exists());
    assert!(!repo.git_dir.join("gitmaster-revert.json").exists());
    assert!(session.prepare_finish(&coordinator, "重复撤销").is_err());
}

/// 建立只在目标文件上有三方冲突的单提交撤销，不接触用户仓库或全局配置。
fn conflicting_revert() -> (Fixture, crate::git::RepositoryHandle, String) {
    let fixture = Fixture::new();
    fixture.command(&["config", "core.autocrlf", "false"]);
    fixture.write("file", b"base\n");
    fixture.write("unrelated", b"preserved\n");
    fixture.command(&["add", "."]);
    fixture.command(&["commit", "-m", "base"]);
    fixture.write("file", b"target\n");
    fixture.command(&["add", "file"]);
    fixture.command(&["commit", "-m", "target"]);
    fixture.write("file", b"later\n");
    fixture.command(&["add", "file"]);
    fixture.command(&["commit", "-m", "later"]);
    let output = Command::new(&fixture.git.path)
        .current_dir(&fixture.root)
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .args([
            "-c",
            "core.hooksPath=",
            "-c",
            "rerere.enabled=false",
            "revert",
            "--no-commit",
            "HEAD~1",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let (repo, state) = open_repository(&fixture.git, &fixture.root).unwrap();
    assert_eq!(state.operations, ["revert"]);
    let target =
        String::from_utf8(std::fs::read(repo.git_dir.join("REVERT_HEAD")).unwrap()).unwrap();
    (fixture, repo, target.trim().to_owned())
}

/// 重建撤销会话读取正确三方：base 是被撤销内容，local 是当前内容，incoming 是原父版本。
#[test]
fn revert_session_reopens_real_three_way_state_without_merge_capability() {
    let (fixture, repo, target) = conflicting_revert();
    assert!(ConflictSession::new(&fixture.git, &repo).is_err());
    let session = RevertSession::new(&fixture.git, &repo).unwrap();
    let state = session.state();
    assert_eq!(state.target_commit_oid, target);
    assert_eq!(state.unresolved_count, 1);
    let document = session.read_document(&state.files[0].conflict_id).unwrap();
    assert_eq!(document.revert_session_id, state.revert_session_id);
    assert_eq!(document.base.as_deref(), Some("target\n"));
    assert_eq!(document.local.as_deref(), Some("later\n"));
    assert_eq!(document.incoming.as_deref(), Some("base\n"));
    let reopened = RevertSession::new(&fixture.git, &repo).unwrap();
    assert_eq!(reopened.state().revert_session_id, state.revert_session_id);
    std::fs::write(
        repo.git_dir.join("REVERT_HEAD"),
        format!("{}\n", state.observed_head_oid),
    )
    .unwrap();
    assert!(session.read_document(&state.files[0].conflict_id).is_err());
}

/// 撤销冲突保存后可完成单父提交，过程保留无关内容与原历史。
#[test]
fn revert_conflict_save_preserves_head_marker_and_unrelated_file() {
    use crate::git::{coordinator::RepositoryCoordinator, repository::query};
    use std::time::{Duration, Instant};

    let (fixture, repo, target) = conflicting_revert();
    let original = crate::git::repository::read_repository_state(&fixture.git, &repo).unwrap();
    anchor::RevertAnchor::new(&original.head, &target)
        .unwrap()
        .create(&repo, Instant::now() + Duration::from_secs(10))
        .unwrap();
    let session = RevertSession::new(&fixture.git, &repo).unwrap();
    let state = session.state();
    let id = &state.files[0].conflict_id;
    let document = session.read_document(id).unwrap();
    let index_before = std::fs::read(repo.git_dir.join("index")).unwrap();
    let marker = std::fs::read(repo.git_dir.join("REVERT_HEAD")).unwrap();
    let work_before = std::fs::read(fixture.root.join("file")).unwrap();
    let coordinator = RepositoryCoordinator::new();
    let preview = session
        .prepare_save(&coordinator, id, &document.fingerprint, "resolved\n")
        .unwrap();
    assert!(matches!(preview.kind, OperationKind::SaveRevertConflict));
    assert_eq!(
        std::fs::read(repo.git_dir.join("index")).unwrap(),
        index_before
    );
    assert_eq!(
        std::fs::read(fixture.root.join("file")).unwrap(),
        work_before
    );
    let handle = coordinator
        .execute(Some(&repo.id), &preview.plan_id)
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(125);
    loop {
        if let Some(result) = coordinator
            .read_operation(Some(&handle.operation_id))
            .unwrap()
            .and_then(|record| record.result)
        {
            assert!(
                matches!(result, OperationResult::Succeeded { .. }),
                "{result:?}"
            );
            break;
        }
        assert!(Instant::now() < deadline, "撤销冲突保存未在期限内结束");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(
        std::fs::read(fixture.root.join("file")).unwrap(),
        b"resolved\n"
    );
    assert_eq!(
        std::fs::read(fixture.root.join("unrelated")).unwrap(),
        b"preserved\n"
    );
    assert_eq!(
        std::fs::read(repo.git_dir.join("REVERT_HEAD")).unwrap(),
        marker
    );
    assert!(!repo.git_dir.join("MERGE_HEAD").exists());
    assert_eq!(
        query(&fixture.git, &repo.root, &["show", ":file"], 4096).unwrap(),
        b"resolved\n"
    );
    let current_session = RevertSession::new(&fixture.git, &repo).unwrap();
    let current = current_session.state();
    assert_eq!(current.observed_head_oid, state.observed_head_oid);
    assert_eq!(current.unresolved_count, 0);
    let finish = current_session
        .prepare_finish(&coordinator, "解决冲突后完成撤销")
        .unwrap();
    assert_eq!(finish.parent_oids, [state.observed_head_oid.clone()]);
    let handle = coordinator
        .execute(Some(&repo.id), &finish.plan_id)
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(125);
    let result = loop {
        if let Some(result) = coordinator
            .read_operation(Some(&handle.operation_id))
            .unwrap()
            .and_then(|record| record.result)
        {
            break result;
        }
        assert!(Instant::now() < deadline, "冲突处理后的撤销提交未按时结束");
        std::thread::sleep(Duration::from_millis(20));
    };
    let OperationResult::Succeeded {
        commit_oid: Some(oid),
        ..
    } = result
    else {
        panic!("{result:?}");
    };
    assert_eq!(
        query(
            &fixture.git,
            &repo.root,
            &["rev-list", "--parents", "-n", "1", &oid],
            1024
        )
        .unwrap(),
        format!("{oid} {}\n", state.observed_head_oid).as_bytes()
    );
    assert_eq!(
        query(
            &fixture.git,
            &repo.root,
            &["show", &format!("{oid}:file")],
            4096
        )
        .unwrap(),
        b"resolved\n"
    );
    assert_eq!(
        std::fs::read(fixture.root.join("unrelated")).unwrap(),
        b"preserved\n"
    );
    assert!(!repo.git_dir.join("REVERT_HEAD").exists());
    assert!(!repo.git_dir.join(anchor::FILE).exists());
}
