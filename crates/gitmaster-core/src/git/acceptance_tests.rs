use super::{
    diff::read_file_diff,
    repository::{open_repository, query, read_repository_state, tests::Fixture},
    *,
};
use std::fs;

/// 暂存重命名、删除、真实冲突分别与 Git 状态一致。
#[test]
fn rename_delete_and_conflict() {
    let f = Fixture::new();
    f.write("old name", b"original\n");
    f.write("delete", b"gone\n");
    f.command(&["add", "."]);
    f.command(&["commit", "-m", "base"]);
    f.command(&["mv", "old name", "new name"]);
    fs::remove_file(f.root.join("delete")).unwrap();
    let (repo, s) = open_repository(&f.git, &f.root).unwrap();
    let rename = s.changes.iter().find(|c| c.path == "new name").unwrap();
    assert_eq!(rename.original_path.as_deref(), Some("old name"));
    assert!(matches!(
        read_file_diff(&f.git, &repo, &s, &rename.change_id, DiffSide::Staged).unwrap(),
        FileDiff::Text { .. }
    ));
    let delete = s.changes.iter().find(|c| c.path == "delete").unwrap();
    assert_eq!(delete.worktree_status, "D");
    f.command(&["add", "."]);
    f.command(&["commit", "-m", "rename"]);
    f.command(&["checkout", "-b", "other"]);
    f.write("new name", b"other\n");
    f.command(&["add", "."]);
    f.command(&["commit", "-m", "other"]);
    f.command(&["checkout", "main"]);
    f.write("new name", b"main\n");
    f.command(&["add", "."]);
    f.command(&["commit", "-m", "main"]);
    let output = std::process::Command::new(&f.git.path)
        .current_dir(&f.root)
        .args(["-c", "commit.gpgsign=false", "merge", "other"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let (repo, s) = open_repository(&f.git, &f.root).unwrap();
    assert!(s.operations.contains(&"merge".into()));
    let change = s.changes.iter().find(|c| c.kind == "conflicted").unwrap();
    assert!(
        matches!(read_file_diff(&f.git,&repo,&s,&change.change_id,DiffSide::Unstaged).unwrap(),FileDiff::Unsupported{reason} if reason=="conflict")
    );
}

/// 用户配置的外部程序不能因只读状态或差异读取而被执行。
#[cfg(unix)]
#[test]
fn external_programs_are_disabled() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    f.write("a", b"base\n");
    f.command(&["add", "."]);
    f.command(&["commit", "-m", "base"]);
    f.write("a", b"changed\n");
    let script = f.root.join(".git/evil.sh");
    fs::write(
        &script,
        b"#!/bin/sh\nprintf called >> \"$(dirname \"$0\")/executed\"\ncat\n",
    )
    .unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
    let command = format!("\"{}\"", script.display());
    for key in [
        "core.fsmonitor",
        "diff.external",
        "diff.evil.textconv",
        "filter.evil.clean",
        "filter.evil.process",
    ] {
        f.command(&["config", key, &command]);
    }
    f.command(&["config", "filter.evil.required", "true"]);
    f.write(".gitattributes", b"a diff=evil filter=evil\n");
    let (repo, s) = open_repository(&f.git, &f.root).unwrap();
    let change = s.changes.iter().find(|c| c.path == "a").unwrap();
    assert!(matches!(
        read_file_diff(&f.git, &repo, &s, &change.change_id, DiffSide::Unstaged).unwrap(),
        FileDiff::Text { .. }
    ));
    assert!(!f.root.join(".git/executed").exists());
}

/// 缺失 promisor 对象不会被隐式下载或写回对象库。
#[test]
fn missing_object_does_not_fetch() {
    let f = Fixture::new();
    f.write("a", b"base\n");
    f.command(&["add", "."]);
    f.command(&["commit", "-m", "base"]);
    let blob =
        String::from_utf8(query(&f.git, &f.root, &["rev-parse", "HEAD:a"], 4096).unwrap()).unwrap();
    let blob = blob.trim();
    let object = f
        .root
        .join(".git/objects")
        .join(&blob[..2])
        .join(&blob[2..]);
    fs::remove_file(&object).unwrap();
    f.command(&["config", "remote.origin.promisor", "true"]);
    f.command(&["config", "extensions.partialClone", "origin"]);
    f.command(&[
        "config",
        "remote.origin.url",
        "https://example.invalid/never",
    ]);
    f.write("a", b"changed\n");
    let (repo, s) = open_repository(&f.git, &f.root).unwrap();
    let change = s.changes.iter().find(|c| c.path == "a").unwrap();
    assert!(read_file_diff(&f.git, &repo, &s, &change.change_id, DiffSide::Unstaged).is_err());
    assert!(!object.exists());
}

/// 状态列表超过文件上限必须失败，不能显示不完整的干净状态。
#[test]
fn status_file_limit() {
    let mut data = b"# branch.oid (initial)\0# branch.head main\0".to_vec();
    for i in 0..10_001 {
        data.extend_from_slice(format!("? {i}\0").as_bytes());
    }
    assert_eq!(
        super::status::parse_status(&data).unwrap_err().code,
        "OUTPUT_LIMIT"
    );
}

/// 临时子模块仅报告 gitlink 变化，不递归读取内部未提交内容。
#[test]
fn submodule_reports_gitlink_only() {
    let f = Fixture::new();
    let sub = Fixture::new();
    sub.write("a", b"base");
    sub.command(&["add", "."]);
    sub.command(&["commit", "-m", "base"]);
    f.command(&[
        "-c",
        "protocol.file.allow=always",
        "submodule",
        "add",
        sub.root.to_str().unwrap(),
        "sub",
    ]);
    f.command(&["commit", "-m", "submodule"]);
    fs::write(f.root.join("sub/a"), b"dirty").unwrap();
    let (repo, s) = open_repository(&f.git, &f.root).unwrap();
    assert!(s.changes.is_empty());
    let oid = query(&f.git, &sub.root, &["rev-parse", "HEAD"], 4096).unwrap();
    let oid = String::from_utf8(oid).unwrap();
    f.command(&[
        "update-index",
        "--add",
        "--cacheinfo",
        &format!("160000,{},other-sub", oid.trim()),
    ]);
    let s = read_repository_state(&f.git, &repo).unwrap();
    let c = s.changes.iter().find(|c| c.path == "other-sub").unwrap();
    assert_eq!(c.kind, "submodule");
}

/// 普通目录和包含非 UTF-8 文件名的仓库均有明确错误。
#[cfg(all(unix, not(target_os = "macos")))]
#[test]
fn non_repository_and_non_utf8_path() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let f = Fixture::new();
    assert_eq!(
        open_repository(&f.git, &std::env::temp_dir())
            .unwrap_err()
            .code,
        "NOT_REPOSITORY"
    );
    fs::write(f.root.join(OsString::from_vec(vec![0xff])), b"x").unwrap();
    assert_eq!(
        open_repository(&f.git, &f.root).unwrap_err().code,
        "UNSUPPORTED_PATH_ENCODING"
    );
}

/// 用 Git 官方测试开关模拟不同所有者，验证安全拒绝映射且不改配置。
#[test]
fn unsafe_repository_error_mapping() {
    let f = Fixture::new();
    let before = fs::read(f.root.join(".git/config")).unwrap();
    let out = std::process::Command::new(&f.git.path)
        .current_dir(&f.root)
        .env("GIT_TEST_ASSUME_DIFFERENT_OWNER", "1")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert_eq!(
        super::repository::command_error(&out.stderr).code,
        "UNSAFE_REPOSITORY"
    );
    assert_eq!(before, fs::read(f.root.join(".git/config")).unwrap());
}
