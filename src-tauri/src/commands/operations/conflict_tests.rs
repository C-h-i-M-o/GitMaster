use super::super::resources::{ConflictRequest, RemoteRequest};
use super::tests::{run_git, Fixture};
use super::*;

/// 通过真实 Git 分支制造冲突，并验证保存结果与完成合并均经过桌面写入入口。
#[test]
fn conflict_integrate_save_and_finish_round_trip() {
    let fixture = Fixture::new();
    let git = fixture.shared.lock().unwrap().git.clone().unwrap();
    run_git(
        &git,
        &fixture.root,
        &[
            "remote",
            "add",
            "origin",
            "https://example.invalid/repo.git",
        ],
    );

    run_git(&git, &fixture.root, &["switch", "-c", "incoming"]);
    std::fs::write(fixture.root.join("file.txt"), "incoming\n").unwrap();
    run_git(&git, &fixture.root, &["add", "."]);
    run_git(&git, &fixture.root, &["commit", "-m", "incoming"]);
    let incoming_oid = String::from_utf8(
        std::process::Command::new(&git.path)
            .arg("-C")
            .arg(&fixture.root)
            .args(["rev-parse", "HEAD"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .to_owned();
    run_git(
        &git,
        &fixture.root,
        &["update-ref", "refs/remotes/origin/incoming", &incoming_oid],
    );
    run_git(&git, &fixture.root, &["switch", "main"]);
    std::fs::write(fixture.root.join("file.txt"), "local\n").unwrap();
    run_git(&git, &fixture.root, &["add", "."]);
    run_git(&git, &fixture.root, &["commit", "-m", "local"]);
    fixture.refresh();

    let remote = RemoteRequest::begin(&fixture.shared, &fixture.id)
        .unwrap()
        .run(&fixture.shared)
        .unwrap();
    let snapshot = fixture
        .shared
        .lock()
        .unwrap()
        .active
        .as_ref()
        .unwrap()
        .1
        .snapshot_id
        .clone();
    let remote_branch_id = remote
        .remote_branches
        .iter()
        .find(|branch| branch.name.ends_with("/incoming"))
        .unwrap()
        .remote_branch_id
        .clone();
    let preparation = WritePreparation::remote(
        &fixture.shared,
        &fixture.id,
        &snapshot,
        RemoteWriteRequest::Integrate {
            remote_branch_id,
            mode: IntegrateMode::Merge,
        },
    )
    .unwrap();
    let preview = preparation.run(&fixture.shared).unwrap();
    let handle = execute(&fixture.shared, Some(&fixture.id), &preview.plan_id).unwrap();
    let result = fixture.finish(&handle).result.unwrap();
    assert!(matches!(result, OperationResult::NeedsResolution { .. }));

    fixture.refresh();
    let conflict_state = ConflictRequest::begin(&fixture.shared, &fixture.id)
        .unwrap()
        .run(&fixture.shared)
        .unwrap();
    let conflict = conflict_state.files.first().unwrap().clone();
    let document = fixture
        .shared
        .lock()
        .unwrap()
        .conflicts
        .as_ref()
        .unwrap()
        .read_document(&conflict.conflict_id)
        .unwrap();
    let save = WritePreparation::conflict(
        &fixture.shared,
        &fixture.id,
        &conflict_state.merge_session_id,
        ConflictWriteRequest::SaveConflict {
            conflict_id: conflict.conflict_id,
            fingerprint: document.fingerprint,
            content: "resolved\n".into(),
        },
    )
    .unwrap()
    .run(&fixture.shared)
    .unwrap();
    let save_handle = execute(&fixture.shared, Some(&fixture.id), &save.plan_id).unwrap();
    assert!(matches!(
        fixture.finish(&save_handle).result,
        Some(OperationResult::Succeeded { .. })
    ));

    fixture.refresh();
    let conflict_state = ConflictRequest::begin(&fixture.shared, &fixture.id)
        .unwrap()
        .run(&fixture.shared)
        .unwrap();
    let finish = WritePreparation::conflict(
        &fixture.shared,
        &fixture.id,
        &conflict_state.merge_session_id,
        ConflictWriteRequest::FinishMerge {
            message: "resolve conflict".into(),
        },
    )
    .unwrap()
    .run(&fixture.shared)
    .unwrap();
    let finish_handle = execute(&fixture.shared, Some(&fixture.id), &finish.plan_id).unwrap();
    assert!(matches!(
        fixture.finish(&finish_handle).result,
        Some(OperationResult::Succeeded { .. })
    ));
    assert!(!fixture.root.join(".git/MERGE_HEAD").exists());
    let parents = String::from_utf8(
        std::process::Command::new(&git.path)
            .arg("-C")
            .arg(&fixture.root)
            .args(["rev-list", "--parents", "-1", "HEAD"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    assert_eq!(parents.split_whitespace().count(), 3);
}
