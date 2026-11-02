use gitmaster_core::{
    agent::{
        policy::PolicyDecision,
        provider::Cancellation,
        tools::git::{GitContext, GitTools},
        PermissionMode, ToolCall,
    },
    git::{
        coordinator::RepositoryCoordinator, environment::resolve_git, repository::open_repository,
    },
};
use serde_json::{json, Value};
use std::process::Command;

/// 所有 Git 写测试只作用于临时仓库。
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
fn fixture() -> (tempfile::TempDir, GitTools) {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-b", "main"]);
    git(dir.path(), &["config", "user.name", "测试"]);
    git(
        dir.path(),
        &["config", "user.email", "test@example.invalid"],
    );
    std::fs::write(dir.path().join("README.md"), "初始\n").unwrap();
    git(dir.path(), &["add", "README.md"]);
    git(dir.path(), &["commit", "-m", "初始"]);
    let executable = resolve_git(None).unwrap();
    let (repository, _) = open_repository(&executable, dir.path()).unwrap();
    let tools = GitTools::new(GitContext {
        git: executable,
        repository,
        coordinator: RepositoryCoordinator::new(),
    });
    (dir, tools)
}
fn call(name: &str, arguments: Value) -> ToolCall {
    ToolCall {
        id: "call_1".into(),
        name: name.into(),
        arguments,
    }
}
fn execute(tools: &GitTools, name: &str, args: Value) -> Value {
    let prepared = tools
        .prepare(&call(name, args), PermissionMode::FullAccess)
        .unwrap();
    tools
        .execute(prepared, false, &Cancellation::new())
        .unwrap()
}

#[test]
fn git_tools_stage_unstage_commit_and_create_branch_using_core_coordinator() {
    let (dir, tools) = fixture();
    std::fs::write(dir.path().join("README.md"), "更新\n").unwrap();
    assert!(execute(&tools, "git_diff", json!({"staged":false}))["text"]
        .as_str()
        .unwrap()
        .contains("更新"));
    execute(&tools, "git_stage", json!({"paths":["README.md"]}));
    assert_eq!(
        git(dir.path(), &["diff", "--cached", "--name-only"]),
        "README.md"
    );
    execute(&tools, "git_unstage", json!({"paths":["README.md"]}));
    assert!(git(dir.path(), &["diff", "--cached", "--name-only"]).is_empty());
    execute(&tools, "git_stage", json!({"paths":["README.md"]}));
    execute(&tools, "git_commit", json!({"message":"工具提交"}));
    assert_eq!(git(dir.path(), &["log", "-1", "--format=%s"]), "工具提交");
    execute(&tools, "git_create_branch", json!({"name":"agent-test"}));
    assert!(git(dir.path(), &["branch", "--list", "agent-test"]).contains("agent-test"));
}

#[test]
fn branch_switch_requires_approval_and_rejects_stale_prepared_state() {
    let (dir, tools) = fixture();
    git(dir.path(), &["branch", "other"]);
    let pending = tools
        .prepare(
            &call("git_switch_branch", json!({"name":"other"})),
            PermissionMode::AutoApprove,
        )
        .unwrap();
    assert_eq!(pending.decision, PolicyDecision::Ask);
    assert!(tools.execute(pending, false, &Cancellation::new()).is_err());
    assert_eq!(git(dir.path(), &["branch", "--show-current"]), "main");
    let pending = tools
        .prepare(
            &call("git_switch_branch", json!({"name":"other"})),
            PermissionMode::AutoApprove,
        )
        .unwrap();
    std::fs::write(dir.path().join("README.md"), "用户的新修改").unwrap();
    assert!(tools.execute(pending, true, &Cancellation::new()).is_err());
    assert_eq!(git(dir.path(), &["branch", "--show-current"]), "main");
}

#[test]
fn git_show_rejects_options_and_uses_only_requested_commit_content() {
    let (dir, tools) = fixture();
    let oid = git(dir.path(), &["rev-parse", "HEAD"]);
    std::fs::write(dir.path().join("README.md"), "未提交内容").unwrap();
    let result = execute(
        &tools,
        "git_show",
        json!({"commitId":oid,"path":"README.md"}),
    );
    assert_eq!(result["text"], "初始\n");
    assert!(tools
        .prepare(
            &call("git_show", json!({"commitId":"--all"})),
            PermissionMode::FullAccess
        )
        .is_err());
    assert!(tools
        .prepare(
            &call("git_commit", json!({"message":"x","shell":"touch bad"})),
            PermissionMode::FullAccess
        )
        .is_err());
    assert!(tools
        .prepare(&call("git_reset", json!({})), PermissionMode::FullAccess)
        .is_err());
}

#[test]
fn truncated_git_text_preserves_the_last_complete_utf8_character() {
    let (dir, tools) = fixture();
    std::fs::write(
        dir.path().join("large.txt"),
        format!("{}中", "a".repeat(128 * 1024 - 1)),
    )
    .unwrap();
    git(dir.path(), &["add", "large.txt"]);
    git(dir.path(), &["commit", "-m", "长文本"]);
    let oid = git(dir.path(), &["rev-parse", "HEAD"]);
    let result = execute(
        &tools,
        "git_show",
        json!({"commitId":oid,"path":"large.txt"}),
    );
    assert_eq!(result["truncated"], true);
    assert_eq!(result["text"].as_str().unwrap().len(), 128 * 1024 - 1);
}

/// 网络传输仅在测试可执行替身中映射到临时 bare 仓库，生产 URL 校验保持不变。
#[cfg(unix)]
#[test]
fn remote_tools_fetch_push_and_pull_against_temporary_bare() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, initial) = fixture();
    let remote = tempfile::tempdir().unwrap();
    let bare = remote.path().join("remote.git");
    git(dir.path(), &["init", "--bare", bare.to_str().unwrap()]);
    git(
        dir.path(),
        &["push", bare.to_str().unwrap(), "HEAD:refs/heads/main"],
    );
    let remote_url = format!("https://fixture.invalid{}", bare.display());
    git(dir.path(), &["remote", "add", "origin", &remote_url]);
    git(
        dir.path(),
        &[
            "fetch",
            bare.to_str().unwrap(),
            "refs/heads/main:refs/remotes/origin/main",
        ],
    );
    git(
        dir.path(),
        &["branch", "--set-upstream-to", "origin/main", "main"],
    );
    let wrapper = remote.path().join("fixture-git");
    let real = resolve_git(None).unwrap();
    let script = format!(
        r#"#!/usr/bin/env python3
import os,sys,urllib.parse
args=sys.argv[1:]
network=any(a.startswith('https://fixture.invalid/') for a in args)
if network:
    args=[urllib.parse.unquote(urllib.parse.urlparse(a).path) if a.startswith('https://fixture.invalid/') else a for a in args]
    args=['protocol.file.allow=always' if a=='protocol.allow=never' else a for a in args]
    os.environ['GIT_ALLOW_PROTOCOL']='file'
os.execv({git},[{git}]+args)
"#,
        git = serde_json::to_string(real.path.to_str().unwrap()).unwrap()
    );
    std::fs::write(&wrapper, script).unwrap();
    std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut executable = real;
    executable.path = wrapper;
    let (repository, _) = open_repository(&executable, dir.path()).unwrap();
    let tools = GitTools::new(GitContext {
        git: executable,
        repository,
        coordinator: RepositoryCoordinator::new(),
    });
    drop(initial);
    execute(&tools, "git_fetch", json!({"remote":"origin"}));
    std::fs::write(dir.path().join("README.md"), "本地更新\n").unwrap();
    execute(&tools, "git_stage", json!({"paths":["README.md"]}));
    execute(&tools, "git_commit", json!({"message":"本地更新"}));
    execute(
        &tools,
        "git_push",
        json!({"remote":"origin","branch":"main"}),
    );
    assert_eq!(
        git(dir.path(), &["rev-parse", "HEAD"]),
        git(
            remote.path(),
            &[
                "--git-dir",
                bare.to_str().unwrap(),
                "rev-parse",
                "refs/heads/main"
            ]
        )
    );
    let source = remote.path().join("source");
    git(
        remote.path(),
        &[
            "clone",
            "--branch",
            "main",
            bare.to_str().unwrap(),
            source.to_str().unwrap(),
        ],
    );
    git(&source, &["config", "user.name", "测试"]);
    git(&source, &["config", "user.email", "test@example.invalid"]);
    std::fs::write(source.join("README.md"), "远端更新\n").unwrap();
    git(&source, &["add", "README.md"]);
    git(&source, &["commit", "-m", "远端更新"]);
    git(&source, &["push", "origin", "main"]);
    execute(&tools, "git_pull", json!({}));
    assert_eq!(
        std::fs::read_to_string(dir.path().join("README.md")).unwrap(),
        "远端更新\n"
    );
    assert_eq!(
        git(dir.path(), &["rev-parse", "HEAD"]),
        git(&source, &["rev-parse", "HEAD"])
    );
}
