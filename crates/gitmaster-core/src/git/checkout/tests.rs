use super::*;
use crate::git::{
    branches::BranchSession,
    repository::{open_repository, query, tests::Fixture},
    write_guard,
};
use std::{fs, time::Duration};

/// 读取切换不应改写的公共配置与分支引用字节。
fn metadata_bytes(repo: &RepositoryHandle) -> Vec<(String, Vec<u8>)> {
    ["config"]
        .into_iter()
        .map(|name| {
            let path = repo.common_dir.join(name);
            (name.to_owned(), fs::read(path).unwrap_or_default())
        })
        .chain(
            ["refs/heads/main", "refs/heads/feature"]
                .into_iter()
                .map(|name| {
                    let path = repo.common_dir.join(name);
                    (name.to_owned(), fs::read(path).unwrap_or_default())
                }),
        )
        .chain(std::iter::once((
            "config.worktree".to_owned(),
            fs::read(repo.git_dir.join("config.worktree")).unwrap_or_default(),
        )))
        .collect()
}

/// 运行检出并返回目标分支的确认快照。
fn capture_target(
    fixture: &Fixture,
    repo: &RepositoryHandle,
    expected: &write_guard::WriteFingerprint,
    name: &str,
) -> CapturedCheckout {
    let target = BranchSession::new(&fixture.git, repo)
        .unwrap()
        .list()
        .branches
        .into_iter()
        .find(|branch| branch.name == name)
        .unwrap();
    CapturedCheckout::capture(
        &fixture.git,
        repo,
        expected,
        &target,
        Instant::now() + Duration::from_secs(120),
    )
    .unwrap()
    .0
}

/// 确认后目标被外部移动，执行也不能检出后来加入的树。
#[test]
fn captured_checkout_rejects_live_target_move_before_writing() {
    let f = Fixture::new();
    f.write("a", b"base\n");
    f.command(&["add", "."]);
    f.command(&["commit", "-m", "base"]);
    f.command(&["switch", "-c", "feature"]);
    f.write("a", b"confirmed\n");
    f.command(&["add", "."]);
    f.command(&["commit", "-m", "confirmed"]);
    f.command(&["switch", "main"]);
    let (repo, _) = open_repository(&f.git, &f.root).unwrap();
    let deadline = Instant::now() + Duration::from_secs(120);
    let expected = write_guard::branch_fingerprint(&f.git, &repo, deadline).unwrap();
    let target = BranchSession::new(&f.git, &repo)
        .unwrap()
        .list()
        .branches
        .into_iter()
        .find(|branch| branch.name == "feature")
        .unwrap();
    let (captured, _) =
        CapturedCheckout::capture(&f.git, &repo, &expected, &target, deadline).unwrap();
    // 直接调用已经通过执行前检查的检出层，精确模拟最后检查之后的竞态。
    f.command(&["update-ref", "refs/heads/feature", "main"]);
    let error = captured.execute(&f.git, &repo, deadline).unwrap_err();
    assert_eq!(error.code, "STALE_WRITE_PLAN");
    assert_eq!(fs::read(f.root.join("a")).unwrap(), b"base\n");
    assert_eq!(
        query(&f.git, &f.root, &["branch", "--show-current"], 1024).unwrap(),
        b"main\n"
    );
}

/// SHA-256 linked worktree 切换保持公共 refs/config，并更新工作树 HEAD 与 reflog。
#[test]
fn captured_checkout_supports_sha256_linked_worktree_and_preserves_metadata() {
    let f = Fixture::new();
    f.command(&["init", "--object-format=sha256", "-b", "main", "sha"]);
    f.command(&["-C", "sha", "config", "user.name", "测试"]);
    f.command(&["-C", "sha", "config", "user.email", "test@example.invalid"]);
    // 本用例核对精确 LF 字节，不能依赖 Windows 系统的 autocrlf 默认值。
    f.command(&["-C", "sha", "config", "core.autocrlf", "false"]);
    f.write("sha/file", b"main\n");
    f.command(&["-C", "sha", "add", "."]);
    f.command(&["-C", "sha", "commit", "-m", "main"]);
    f.command(&["-C", "sha", "switch", "-c", "feature"]);
    f.write("sha/file", b"feature\n");
    f.command(&["-C", "sha", "commit", "-am", "feature"]);
    f.command(&["-C", "sha", "switch", "--detach", "main"]);
    f.command(&["-C", "sha", "worktree", "add", "--detach", "linked", "main"]);
    let (repo, _) = open_repository(&f.git, &f.root.join("sha/linked")).unwrap();
    let expected =
        write_guard::branch_fingerprint(&f.git, &repo, Instant::now() + Duration::from_secs(120))
            .unwrap();
    let before = metadata_bytes(&repo);
    let old_head = fs::read(repo.git_dir.join("HEAD")).unwrap();
    let old_reflog = fs::read(repo.git_dir.join("logs/HEAD")).unwrap();
    let captured = capture_target(&f, &repo, &expected, "feature");
    let output = captured
        .execute(&f.git, &repo, Instant::now() + Duration::from_secs(120))
        .unwrap();
    assert!(output.success, "SHA-256 linked worktree 检出失败");
    assert_eq!(
        fs::read(f.root.join("sha/linked/file")).unwrap(),
        b"feature\n"
    );
    assert_eq!(metadata_bytes(&repo), before);
    assert_ne!(fs::read(repo.git_dir.join("HEAD")).unwrap(), old_head);
    assert_eq!(
        fs::read(repo.git_dir.join("HEAD")).unwrap(),
        b"ref: refs/heads/feature\n"
    );
    assert!(fs::read(repo.git_dir.join("logs/HEAD")).unwrap().len() > old_reflog.len());
}

/// 确认后仓库配置中的 filter 和 post-checkout hook 不得执行外部程序。
#[test]
fn captured_checkout_does_not_execute_configured_filter_or_hook() {
    let f = Fixture::new();
    f.write("file", b"target\n");
    f.command(&["add", "."]);
    f.command(&["commit", "-m", "target"]);
    f.command(&["switch", "-c", "feature"]);
    f.write("file", b"current\n");
    f.command(&["commit", "-am", "current"]);
    let (repo, _) = open_repository(&f.git, &f.root).unwrap();
    let marker = f.root.join("executed");
    let expected =
        write_guard::branch_fingerprint(&f.git, &repo, Instant::now() + Duration::from_secs(120))
            .unwrap();
    let captured = capture_target(&f, &repo, &expected, "main");
    // 在确认后注入配置与 hook，验证执行阶段不会信任新出现的外部程序。
    f.command(&["config", "extensions.worktreeConfig", "true"]);
    f.command(&[
        "config",
        "--worktree",
        "filter.pwn.smudge",
        "touch executed",
    ]);
    f.command(&["config", "--worktree", "filter.pwn.clean", "cat"]);
    fs::create_dir_all(repo.git_dir.join("info")).unwrap();
    fs::write(repo.git_dir.join("info/attributes"), b"file filter=pwn\n").unwrap();
    fs::create_dir_all(repo.git_dir.join("hooks")).unwrap();
    fs::write(
        repo.git_dir.join("hooks/post-checkout"),
        b"#!/bin/sh\ntouch executed\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            repo.git_dir.join("hooks/post-checkout"),
            fs::Permissions::from_mode(0o755),
        )
        .unwrap();
    }
    let before = metadata_bytes(&repo);
    let output = captured
        .execute(&f.git, &repo, Instant::now() + Duration::from_secs(120))
        .unwrap();
    assert!(output.success, "带外部 filter/hook 配置时检出失败");
    assert!(!marker.exists());
    assert_eq!(metadata_bytes(&repo), before);
}

/// 当前与目标分支属性不同的 CRLF/ident 规则仍按原生 Git 结果检出。
#[test]
fn captured_checkout_matches_native_crlf_and_ident_conversion() {
    let f = Fixture::new();
    f.write("file.txt", b"line\n$Id$\n");
    f.write(".gitattributes", b"file.txt text eol=crlf ident\n");
    f.command(&["add", "."]);
    f.command(&["commit", "-m", "main"]);
    f.command(&["switch", "-c", "feature"]);
    f.write(".gitattributes", b"file.txt text eol=lf ident\n");
    f.write("file.txt", b"changed\n$Id$\n");
    f.command(&["add", "."]);
    f.command(&["commit", "-m", "feature"]);
    let (repo, _) = open_repository(&f.git, &f.root).unwrap();
    // 先让原生 Git 生成目标工作树字节，再恢复当前分支作为捕获输入。
    f.command(&["switch", "main"]);
    let native = fs::read(f.root.join("file.txt")).unwrap();
    assert!(native.starts_with(b"line\r\n$Id: "));
    f.command(&["switch", "feature"]);
    let expected =
        write_guard::branch_fingerprint(&f.git, &repo, Instant::now() + Duration::from_secs(120))
            .unwrap();
    let captured = capture_target(&f, &repo, &expected, "main");
    let output = captured
        .execute(&f.git, &repo, Instant::now() + Duration::from_secs(120))
        .unwrap();
    assert!(output.success, "CRLF/ident 检出失败");
    assert_eq!(fs::read(f.root.join("file.txt")).unwrap(), native);
}

/// 源/目标引用锁确实阻止原生 Git 更新，检出后释放且不修改分支 OID。
#[test]
fn checkout_locks_block_native_ref_updates_and_release_after_switch() {
    let f = Fixture::new();
    f.write("file", b"base\n");
    f.command(&["add", "."]);
    f.command(&["commit", "-m", "base"]);
    f.command(&["switch", "-c", "nested/feature"]);
    f.write("file", b"target\n");
    f.command(&["commit", "-am", "target"]);
    f.command(&["switch", "main"]);
    f.command(&["pack-refs", "--all", "--prune"]);
    let (repo, _) = open_repository(&f.git, &f.root).unwrap();
    let deadline = Instant::now() + Duration::from_secs(120);
    let fingerprint = write_guard::branch_fingerprint(&f.git, &repo, deadline).unwrap();
    let captured = capture_target(&f, &repo, &fingerprint, "nested/feature");
    let before = fs::read(repo.common_dir.join("packed-refs")).unwrap();
    let output = captured
        .execute_with(&f.git, &repo, deadline, || {
            for reference in ["refs/heads/main", "refs/heads/nested/feature"] {
                let args = ["update-ref", reference, &captured.target_oid].map(OsString::from);
                let update = crate::git::process::run_local_git(
                    &f.git,
                    &repo.root,
                    &args,
                    &[],
                    None,
                    deadline,
                )?;
                assert!(!update.success, "原生 Git 不应更新已持锁引用");
                assert!(repo.common_dir.join(format!("{reference}.lock")).is_file());
            }
            Ok(())
        })
        .unwrap();
    assert!(output.success);
    assert_eq!(fs::read(f.root.join("file")).unwrap(), b"target\n");
    assert_eq!(
        fs::read(repo.common_dir.join("packed-refs")).unwrap(),
        before
    );
    assert!(!repo.common_dir.join("refs/heads/main.lock").exists());
    assert!(!repo
        .common_dir
        .join("refs/heads/nested/feature.lock")
        .exists());
}

/// 引用锁已存在或被外部替换时，清理不得删除外部文件。
#[test]
fn checkout_preserves_existing_and_replaced_reference_locks() {
    let f = Fixture::new();
    let (repo, _) = open_repository(&f.git, &f.root).unwrap();
    let path = repo.common_dir.join("refs/heads/main.lock");
    fs::write(&path, b"external").unwrap();
    assert!(ReferenceLock::acquire(&repo, "refs/heads/main").is_err());
    assert_eq!(fs::read(&path).unwrap(), b"external");
    fs::remove_file(&path).unwrap();
    let lock = ReferenceLock::acquire(&repo, "refs/heads/main").unwrap();
    fs::rename(&path, repo.common_dir.join("refs/heads/old-lock")).unwrap();
    fs::write(&path, b"replacement").unwrap();
    assert!(!lock.owns_lock());
    drop(lock);
    assert_eq!(fs::read(&path).unwrap(), b"replacement");
}

/// 精确属性规则覆盖带 glob、引号、中文、换行和反斜线的真实路径。
#[cfg(unix)]
#[test]
fn checkout_literal_attributes_match_native_for_special_paths() {
    let f = Fixture::new();
    let names = [
        "a[1]*?.txt",
        "子目录/带 空格\"\\名\n.txt",
        "!.txt",
        "#.txt",
        "ordinary.txt",
    ];
    f.write(
        ".gitattributes",
        b"*.txt text eol=crlf ident\nordinary.txt -text -ident\n",
    );
    for name in names {
        f.write(name, b"target\n$Id$\n");
    }
    f.command(&["add", "."]);
    f.command(&["commit", "-m", "target"]);
    f.command(&["switch", "-c", "feature"]);
    for name in names {
        f.write(name, b"source\n$Id$\n");
    }
    f.command(&["commit", "-am", "source"]);
    f.command(&["switch", "main"]);
    let native = names.map(|name| fs::read(f.root.join(name)).unwrap());
    assert!(native[0].starts_with(b"target\r\n$Id: "));
    assert_eq!(native[4], b"target\n$Id$\n");
    f.command(&["switch", "feature"]);
    let (repo, _) = open_repository(&f.git, &f.root).unwrap();
    let deadline = Instant::now() + Duration::from_secs(120);
    let fingerprint = write_guard::branch_fingerprint(&f.git, &repo, deadline).unwrap();
    let captured = capture_target(&f, &repo, &fingerprint, "main");
    let output = captured.execute(&f.git, &repo, deadline).unwrap();
    assert!(output.success);
    assert_eq!(
        names.map(|name| fs::read(f.root.join(name)).unwrap()),
        native
    );
}

/// 文件和目录互换沿用原生 switch 的检出与清理规则。
#[test]
fn checkout_supports_file_directory_transitions() {
    let f = Fixture::new();
    f.write("path", b"file\n");
    f.command(&["add", "."]);
    f.command(&["commit", "-m", "file"]);
    f.command(&["switch", "-c", "feature"]);
    fs::remove_file(f.root.join("path")).unwrap();
    f.write("path/child", b"child\n");
    f.command(&["add", "."]);
    f.command(&["commit", "-m", "directory"]);
    for target in ["main", "feature"] {
        let (repo, _) = open_repository(&f.git, &f.root).unwrap();
        let deadline = Instant::now() + Duration::from_secs(120);
        let fingerprint = write_guard::branch_fingerprint(&f.git, &repo, deadline).unwrap();
        let captured = capture_target(&f, &repo, &fingerprint, target);
        assert!(captured.execute(&f.git, &repo, deadline).unwrap().success);
    }
    assert_eq!(fs::read(f.root.join("path/child")).unwrap(), b"child\n");
}

/// 原生 switch 使用隔离配置前仍从真实仓库核对后来出现的 worktree 占用。
#[test]
fn checkout_rejects_late_worktree_occupation() {
    let f = Fixture::new();
    f.write("file", b"base\n");
    f.command(&["add", "."]);
    f.command(&["commit", "-m", "base"]);
    f.command(&["branch", "feature"]);
    let (repo, _) = open_repository(&f.git, &f.root).unwrap();
    let deadline = Instant::now() + Duration::from_secs(120);
    let fingerprint = write_guard::branch_fingerprint(&f.git, &repo, deadline).unwrap();
    let captured = capture_target(&f, &repo, &fingerprint, "feature");
    let other = tempfile::tempdir().unwrap();
    f.command(&["worktree", "add", other.path().to_str().unwrap(), "feature"]);
    assert_eq!(
        captured.execute(&f.git, &repo, deadline).unwrap_err().code,
        "BRANCH_IN_USE"
    );
    assert_eq!(
        query(&f.git, &f.root, &["branch", "--show-current"], 1024).unwrap(),
        b"main\n"
    );
}

/// 私有初始化非零退出必须穿透真实进程边界，不能丢失阶段和退出码或暴露输出。
#[cfg(unix)]
#[test]
fn private_init_failure_preserves_safe_diagnostic() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    let executable = tempfile::tempdir().unwrap();
    let path = executable.path().join("拒绝初始化 git");
    fs::write(
        &path,
        b"#!/bin/sh\nprintf '%s\\n' 'SECRET_PATH_TOKEN' >&2\nexit 1\n",
    )
    .unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    let git = GitExecutable {
        path,
        ..f.git.clone()
    };
    let captured = CapturedTree {
        directory: tempfile::tempdir().unwrap(),
        settings: Vec::new(),
        global_attributes: None,
    };
    let private_path = captured.directory.path().to_owned();
    let error = captured
        .private_query(
            &git,
            &["init", "--template=", "--initial-branch=gitmaster-private"],
            Instant::now() + Duration::from_secs(10),
        )
        .unwrap_err();
    drop(captured);
    assert!(!private_path.exists());
    assert_eq!(error.code, "GIT_EXECUTION_FAILED");
    assert!(!format!("{error:?}").contains("SECRET_PATH_TOKEN"));
    let diagnostic = error.diagnostic.expect("必须保留初始化诊断");
    assert_eq!(diagnostic.stage, "checkoutInit");
    assert_eq!(diagnostic.exit_code, Some(1));
    assert_eq!(diagnostic.os_code, None);
}

/// 准备切换不应向源对象存储创建目录；源对象只读仍可捕获隔离检出环境。
#[cfg(unix)]
#[test]
fn capture_with_readonly_source_objects_does_not_initialize_source_store() {
    use std::os::unix::fs::PermissionsExt;
    for format in ["sha1", "sha256"] {
        let f = Fixture::new();
        if format == "sha256" {
            fs::remove_dir_all(f.root.join(".git")).unwrap();
            f.command(&["init", "--object-format=sha256", "-b", "main"]);
            f.command(&["config", "user.name", "测试"]);
            f.command(&["config", "user.email", "test@example.invalid"]);
        }
        f.write("中文 目录/a", b"base\n");
        f.command(&["add", "."]);
        f.command(&["commit", "-m", "base"]);
        f.command(&["switch", "-c", "feature"]);
        f.write("中文 目录/a", b"feature\n");
        f.command(&["add", "."]);
        f.command(&["commit", "-m", "feature"]);
        f.command(&["switch", "main"]);
        let (repo, _) = open_repository(&f.git, &f.root).unwrap();
        let deadline = Instant::now() + Duration::from_secs(120);
        let fingerprint = write_guard::branch_fingerprint(&f.git, &repo, deadline).unwrap();
        let target = query(&f.git, &f.root, &["rev-parse", "feature"], 256).unwrap();
        let target = std::str::from_utf8(&target).unwrap().trim();
        let objects = repo.common_dir.join("objects");
        for name in ["info", "pack"] {
            fs::remove_dir(objects.join(name)).unwrap();
        }
        let before = metadata_bytes(&repo);
        let index = fs::read(repo.git_dir.join("index")).unwrap();
        fs::set_permissions(&objects, fs::Permissions::from_mode(0o555)).unwrap();
        let result = CapturedTree::capture(&f.git, &repo, &fingerprint, target, deadline);
        fs::set_permissions(&objects, fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(metadata_bytes(&repo), before);
        assert_eq!(fs::read(repo.git_dir.join("index")).unwrap(), index);
        assert_eq!(fs::read(f.root.join("中文 目录/a")).unwrap(), b"base\n");
        assert!(!objects.join("info").exists());
        assert!(!objects.join("pack").exists());
        assert!(
            result.is_ok(),
            "源对象只读时准备必须成功：{:?}",
            result.err()
        );
    }
}
