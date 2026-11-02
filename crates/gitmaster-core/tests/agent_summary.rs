use gitmaster_core::{
    agent::{
        summary::collect_summary,
        tools::git::{GitContext, GitTools},
    },
    git::{
        coordinator::RepositoryCoordinator, environment::resolve_git, repository::open_repository,
    },
};
use std::process::Command;

/// 创建提交证据夹具，命令仅在临时仓库运行。
fn git(path: &std::path::Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(path)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}
fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-b", "main"]);
    git(dir.path(), &["config", "user.name", "总结测试"]);
    git(
        dir.path(),
        &["config", "user.email", "summary@example.invalid"],
    );
    dir
}
fn commit(dir: &std::path::Path, path: &str, content: &str, message: &str) -> String {
    std::fs::write(dir.join(path), content).unwrap();
    git(dir, &["add", "--", path]);
    git(dir, &["commit", "-m", message]);
    git(dir, &["rev-parse", "HEAD"])
}
fn tools(path: &std::path::Path) -> GitTools {
    let git = resolve_git(None).unwrap();
    let (repository, _) = open_repository(&git, path).unwrap();
    GitTools::new(GitContext {
        git,
        repository,
        coordinator: RepositoryCoordinator::new(),
    })
}

#[test]
fn summary_reads_root_and_selected_commit_objects_not_dirty_worktree_or_skipped_history() {
    let dir = fixture();
    let first = commit(dir.path(), "a.txt", "初始功能", "初始提交");
    let _skipped = commit(dir.path(), "unrelated.txt", "不应混入的功能", "未指定提交");
    let last = commit(dir.path(), "a.txt", "后续修正", "修正提交");
    std::fs::write(dir.path().join("a.txt"), "工作区的未提交内容").unwrap();
    let evidence = collect_summary(&tools(dir.path()), &[first.clone(), last.clone()]).unwrap();
    assert_eq!(evidence.commits.len(), 2);
    assert_eq!(evidence.commits[0].oid, first);
    assert!(evidence.commits[0].parents.is_empty());
    assert_eq!(evidence.commits[1].oid, last);
    let text = serde_json::to_string(&evidence).unwrap();
    assert!(text.contains("初始功能"));
    assert!(text.contains("后续修正"));
    assert!(!text.contains("工作区的未提交内容"));
    assert!(!text.contains("不应混入的功能"));
}

#[test]
fn summary_invalid_commit_fails_instead_of_selecting_a_similar_sha() {
    let dir = fixture();
    let oid = commit(dir.path(), "a.txt", "内容", "提交");
    assert!(collect_summary(
        &tools(dir.path()),
        &[oid, "0000000000000000000000000000000000000000".into()]
    )
    .is_err());
    assert!(collect_summary(&tools(dir.path()), &["--all".into()]).is_err());
}

#[test]
fn merge_summary_records_first_parent_comparison_explicitly() {
    let dir = fixture();
    commit(dir.path(), "a.txt", "初始", "初始");
    git(dir.path(), &["checkout", "-b", "feature"]);
    commit(dir.path(), "feature.txt", "新增功能", "功能");
    git(dir.path(), &["checkout", "main"]);
    commit(dir.path(), "main.txt", "主线", "主线");
    git(dir.path(), &["merge", "--no-ff", "feature", "-m", "合并"]);
    let oid = git(dir.path(), &["rev-parse", "HEAD"]);
    let evidence = collect_summary(&tools(dir.path()), &[oid]).unwrap();
    assert_eq!(evidence.commits[0].parents.len(), 2);
    assert!(evidence.commits[0].comparison.contains("第一父提交"));
    assert!(evidence.commits[0].patch.contains("新增功能"));
    assert!(!evidence.commits[0].patch.contains("+主线"));
}
