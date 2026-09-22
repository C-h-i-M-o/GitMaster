//! 合并完成准备与执行的真实 Git 行为验收。
use super::*;
use crate::git::coordinator::RepositoryCoordinator;
use crate::git::repository::{open_repository, query, tests::Fixture};
use std::{
    process::Command,
    time::{Duration, Instant},
};

/// 使用隔离配置执行 Git，避免测试继承调用进程的 GIT_* 状态。
fn git(f: &Fixture, args: &[&str]) -> std::process::Output {
    let mut command = Command::new(&f.git.path);
    for (key, _) in std::env::vars_os().filter(|(key, _)| key.to_string_lossy().starts_with("GIT_"))
    {
        command.env_remove(key);
    }
    command
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .args(args)
        .current_dir(&f.root)
        .output()
        .unwrap()
}

/// 等待一次性计划进入终态，避免重复执行同一计划。
fn finish(
    c: &RepositoryCoordinator,
    repo: &RepositoryHandle,
    preview: &WritePreview,
) -> OperationResult {
    let handle = c.execute(Some(&repo.id), &preview.plan_id).unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(record) = c.read_operation(Some(&handle.operation_id)).unwrap() {
            if let Some(result) = record.result {
                return result;
            }
        }
        assert!(Instant::now() < deadline, "合并完成操作未在测试期限内完成");
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// 建立一个无冲突的 no-ff no-commit 合并，并返回只读会话。
fn clean_merge() -> (Fixture, RepositoryHandle, ConflictSession, String, String) {
    let f = Fixture::new();
    f.write("base", b"base\n");
    f.command(&["add", "."]);
    f.command(&["commit", "-m", "base"]);
    f.command(&["branch", "incoming"]);
    f.write("local", b"local\n");
    f.command(&["add", "."]);
    f.command(&["commit", "-m", "local"]);
    let local = String::from_utf8(query(&f.git, &f.root, &["rev-parse", "HEAD"], 1024).unwrap())
        .unwrap()
        .trim()
        .into();
    f.command(&["switch", "incoming"]);
    f.write("incoming", b"incoming\n");
    f.command(&["add", "."]);
    f.command(&["commit", "-m", "incoming"]);
    let incoming = String::from_utf8(query(&f.git, &f.root, &["rev-parse", "HEAD"], 1024).unwrap())
        .unwrap()
        .trim()
        .into();
    f.command(&["switch", "main"]);
    assert!(git(&f, &["merge", "--no-commit", "--no-ff", "incoming"])
        .status
        .success());
    let (repo, _) = open_repository(&f.git, &f.root).unwrap();
    let session = ConflictSession::new(&f.git, &repo).unwrap();
    (f, repo, session, local, incoming)
}

/// 干净合并准备保持仓库不变，执行后生成准确双亲并清理合并标记。
#[test]
fn clean_merge_preview_is_readonly_and_execution_creates_two_parent_commit() {
    let (f, repo, session, local, incoming) = clean_merge();
    let c = RepositoryCoordinator::new();
    let before = (
        std::fs::read(repo.git_dir.join("HEAD")).unwrap(),
        std::fs::read(repo.git_dir.join("index")).unwrap(),
        query(&f.git, &f.root, &["count-objects", "-v"], 4096).unwrap(),
    );
    let preview = prepare_finish(&c, &session, "原样消息\n含换行").unwrap();
    assert_eq!(preview.parent_oids, [local.clone(), incoming.clone()]);
    assert_eq!(preview.message.as_deref(), Some("原样消息\n含换行"));
    assert_eq!(before.0, std::fs::read(repo.git_dir.join("HEAD")).unwrap());
    assert_eq!(before.1, std::fs::read(repo.git_dir.join("index")).unwrap());
    assert_eq!(
        before.2,
        query(&f.git, &f.root, &["count-objects", "-v"], 4096).unwrap()
    );
    assert!(matches!(
        finish(&c, &repo, &preview),
        OperationResult::Succeeded { .. }
    ));
    let parents = String::from_utf8(
        query(
            &f.git,
            &f.root,
            &["rev-list", "--parents", "-1", "HEAD"],
            4096,
        )
        .unwrap(),
    )
    .unwrap();
    let fields: Vec<_> = parents.split_whitespace().map(str::to_owned).collect();
    assert_eq!(&fields[1..], &[local, incoming]);
    assert_eq!(
        String::from_utf8(query(&f.git, &f.root, &["log", "-1", "--format=%B"], 4096).unwrap())
            .unwrap(),
        "原样消息\n含换行\n"
    );
    for name in ["MERGE_HEAD", "MERGE_MSG", "MERGE_MODE", "AUTO_MERGE"] {
        assert!(!repo.git_dir.join(name).exists(), "残留 {name}");
    }
}

/// 未解决的真实三阶段索引必须在准备阶段拒绝。
#[test]
fn unresolved_index_is_rejected() {
    let f = Fixture::new();
    f.write("file", b"base\n");
    f.command(&["add", "."]);
    f.command(&["commit", "-m", "base"]);
    f.command(&["branch", "incoming"]);
    f.command(&["switch", "incoming"]);
    f.write("file", b"incoming\n");
    f.command(&["add", "."]);
    f.command(&["commit", "-m", "incoming"]);
    f.command(&["switch", "main"]);
    f.write("file", b"local\n");
    f.command(&["add", "."]);
    f.command(&["commit", "-m", "local"]);
    assert_eq!(
        git(&f, &["merge", "--no-commit", "--no-ff", "incoming"])
            .status
            .code(),
        Some(1)
    );
    let (repo, _) = open_repository(&f.git, &f.root).unwrap();
    let session = ConflictSession::new(&f.git, &repo).unwrap();
    let error = prepare_finish(&RepositoryCoordinator::new(), &session, "finish").unwrap_err();
    assert_eq!(error.code, "UNRESOLVED_CONFLICTS");
}

/// 完整索引进入提交，未暂存工作区修改保持原样。
#[test]
fn staged_snapshot_includes_extra_file_but_excludes_unstaged_change() {
    let (f, repo, _, _, _) = clean_merge();
    f.write("staged", b"staged\n");
    f.command(&["add", "staged"]);
    f.write("local", b"unstaged\n");
    let session = ConflictSession::new(&f.git, &repo).unwrap();
    let c = RepositoryCoordinator::new();
    let preview = prepare_finish(&c, &session, "finish").unwrap();
    assert!(matches!(
        finish(&c, &repo, &preview),
        OperationResult::Succeeded { .. }
    ));
    assert_eq!(
        query(&f.git, &f.root, &["show", "HEAD:staged"], 4096).unwrap(),
        b"staged\n"
    );
    assert_eq!(
        query(&f.git, &f.root, &["show", "HEAD:local"], 4096).unwrap(),
        b"local\n"
    );
    assert_eq!(std::fs::read(f.root.join("local")).unwrap(), b"unstaged\n");
}

/// 准备后的索引、配置或合并标记变化必须使执行失败并保留外部变化。
#[test]
fn finish_rechecks_external_changes() {
    for change in ["index", "config", "merge_head"] {
        let (f, repo, session, _, _) = clean_merge();
        let c = RepositoryCoordinator::new();
        let preview = prepare_finish(&c, &session, "finish").unwrap();
        match change {
            "index" => {
                f.write("external", b"external\n");
                f.command(&["add", "external"]);
            }
            "config" => f.command(&["config", "core.autocrlf", "true"]),
            "merge_head" => f.write(".git/MERGE_HEAD", b"deadbeef\n"),
            _ => unreachable!(),
        }
        assert!(
            matches!(finish(&c, &repo, &preview), OperationResult::Failed { .. }),
            "{change}"
        );
        if change == "config" {
            assert_eq!(
                String::from_utf8(
                    query(&f.git, &f.root, &["config", "--get", "core.autocrlf"], 1024).unwrap()
                )
                .unwrap()
                .trim(),
                "true"
            );
        }
    }
}

/// 即使合并树与当前 HEAD 相同，双亲关系仍应产生真实合并提交。
#[test]
fn identical_tree_still_creates_merge_commit() {
    let (f, repo, _, _, _) = clean_merge();
    f.command(&[
        "restore",
        "--source=HEAD",
        "--staged",
        "--worktree",
        "--",
        ".",
    ]);
    let session = ConflictSession::new(&f.git, &repo).unwrap();
    let c = RepositoryCoordinator::new();
    let preview = prepare_finish(&c, &session, "same tree").unwrap();
    assert!(matches!(
        finish(&c, &repo, &preview),
        OperationResult::Succeeded { .. }
    ));
    let parents = String::from_utf8(
        query(
            &f.git,
            &f.root,
            &["rev-list", "--parents", "-1", "HEAD"],
            4096,
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(parents.split_whitespace().count(), 3);
}
