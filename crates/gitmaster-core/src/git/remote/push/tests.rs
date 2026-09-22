use super::*;
use crate::git::repository::{open_repository, query, tests::Fixture};
use std::{fs, process::Command, thread};

/// 所有推送都限制在独有临时 bare 仓库，生产 HTTPS 地址仅在替身中映射。
struct PushFixture {
    local: Fixture,
    peer: Fixture,
    bare: PathBuf,
    repo: RepositoryHandle,
    base: String,
}
impl PushFixture {
    /// 建立共同祖先、另一工作树和真实远端，之后只对这些临时路径写入。
    fn new() -> Self {
        Self::with_format("sha1")
    }
    /// 重新初始化本测试自建的空仓库以验证不同对象格式，不触碰用户仓库。
    fn with_format(format: &str) -> Self {
        let local = Fixture::new();
        let peer = Fixture::new();
        if format != "sha1" {
            for f in [&local, &peer] {
                fs::remove_dir_all(f.root.join(".git")).unwrap();
                f.command(&["init", "-b", "main", &format!("--object-format={format}")]);
                f.command(&["config", "user.name", "测试"]);
                f.command(&["config", "user.email", "test@example.invalid"]);
                f.command(&["config", "commit.gpgsign", "false"]);
            }
        }
        let bare = peer.root.join("remote.git");
        local.write("file", b"base\n");
        local.command(&["add", "file"]);
        local.command(&["commit", "-m", "base"]);
        local.command(&[
            "init",
            "--bare",
            &format!("--object-format={format}"),
            bare.to_str().unwrap(),
        ]);
        local.command(&["push", bare.to_str().unwrap(), "HEAD:refs/heads/main"]);
        peer.command(&["fetch", bare.to_str().unwrap(), "refs/heads/main"]);
        peer.command(&["switch", "-C", "main", "FETCH_HEAD"]);
        let mut url = ::url::Url::parse("https://fixture.invalid/").unwrap();
        url.set_path(bare.to_str().unwrap());
        local.command(&["remote", "add", "origin", url.as_str()]);
        let (repo, _) = open_repository(&local.git, &local.root).unwrap();
        let base = oid(&local, "HEAD");
        Self {
            local,
            peer,
            bare,
            repo,
            base,
        }
    }
    /// 用真实文件修改构造本地领先提交。
    fn advance(&self) -> String {
        self.local.write("file", b"outgoing\n");
        self.local.command(&["add", "file"]);
        self.local.command(&["commit", "-m", "上传中文提交"]);
        oid(&self.local, "HEAD")
    }
    /// 每次捕获真实本地状态和远端 ID，不模拟计划校验。
    fn prepare(
        &self,
        coordinator: &RepositoryCoordinator,
        target: &str,
        runner: NetworkRunner,
    ) -> Result<WritePreview, OperationError> {
        let state =
            crate::git::repository::read_repository_state(&self.local.git, &self.repo).unwrap();
        let session = RemoteSession::new(&self.local.git, &self.repo).unwrap();
        prepare_with_runner(
            coordinator,
            &self.local.git,
            &self.repo,
            &state,
            &session,
            &session.state().remotes[0].remote_id,
            target,
            runner,
        )
    }
    /// 读取真实 bare 目标，可观察实际远端影响。
    fn remote_oid(&self, reference: &str) -> String {
        String::from_utf8(
            query(
                &self.local.git,
                &self.bare,
                &["rev-parse", "--verify", reference],
                4096,
            )
            .unwrap(),
        )
        .unwrap()
        .trim()
        .into()
    }
}

/// 从真实 Git 查询固定引用，测试期待值不调用生产推送解析器。
fn oid(f: &Fixture, reference: &str) -> String {
    String::from_utf8(query(&f.git, &f.root, &["rev-parse", reference], 4096).unwrap())
        .unwrap()
        .trim()
        .into()
}

/// 仅替换传输协议：命令及 refspec 仍完全使用生产构造，交给真实 Git 执行。
fn local_transport(
    git: &GitExecutable,
    cwd: &Path,
    args: &[OsString],
    objects: Option<&Path>,
    _settings: &[(String, String)],
    _deadline: Instant,
    _progress: &mut dyn FnMut(u64, u64),
) -> Result<ProcessOutput, OperationError> {
    let mut args = args.to_vec();
    let position = args
        .iter()
        .position(|arg| {
            arg.to_str()
                .is_some_and(|s| s.starts_with("https://fixture.invalid/"))
        })
        .unwrap();
    let url = ::url::Url::parse(args[position].to_str().unwrap()).unwrap();
    let path = ::url::Url::parse(&format!("file://{}", url.path()))
        .unwrap()
        .to_file_path()
        .unwrap();
    args[position] = path.into_os_string();
    let mut cmd = Command::new(&git.path);
    for (key, _) in std::env::vars_os().filter(|(key, _)| key.to_string_lossy().starts_with("GIT_"))
    {
        cmd.env_remove(key);
    }
    cmd.current_dir(cwd)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_ALLOW_PROTOCOL", "file")
        .args([
            "-c",
            "core.hooksPath=",
            "-c",
            "maintenance.auto=false",
            "-c",
            "gc.auto=0",
        ]);
    if let Some(objects) = objects {
        cmd.env("GIT_OBJECT_DIRECTORY", objects);
    }
    let output = cmd.args(args).output().unwrap();
    Ok(ProcessOutput {
        stdout: output.stdout,
        stderr: output.stderr,
        success: output.status.success(),
        exit_code: output.status.code(),
        truncated: false,
    })
}

/// 有界等待后台任务，重复 execute 不能造成第二次上传。
fn finish(
    coordinator: &RepositoryCoordinator,
    repo: &RepositoryHandle,
    preview: &WritePreview,
) -> OperationResult {
    let handle = coordinator
        .execute(Some(&repo.id), &preview.plan_id)
        .unwrap();
    let again = coordinator
        .execute(Some(&repo.id), &preview.plan_id)
        .unwrap();
    assert_eq!(handle.operation_id, again.operation_id);
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(result) = coordinator
            .read_operation(Some(&handle.operation_id))
            .unwrap()
            .unwrap()
            .result
        {
            return result;
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
}

/// 捕获单一源和全部待上传摘要，真实远端仅变更所选分支，本地状态字节保持不变。
#[cfg(unix)]
#[test]
fn push_fast_forward_and_new_branch_preserve_unselected_state() {
    let f = PushFixture::new();
    let source = f.advance();
    let mut push_url = ::url::Url::parse("https://fixture.invalid/").unwrap();
    push_url.set_path(f.bare.to_str().unwrap());
    f.local
        .command(&["config", "remote.origin.pushurl", push_url.as_str()]);
    f.local.command(&[
        "config",
        "remote.origin.url",
        "https://fetch-only.invalid/repo",
    ]);
    f.local.command(&["tag", "not-requested"]);
    f.local.command(&["config", "push.followTags", "true"]);
    f.local.command(&["config", "push.default", "matching"]);
    f.local
        .command(&["config", "remote.origin.push", "refs/heads/*:refs/heads/*"]);
    f.local.write("file", b"uncommitted work\n");
    let paths = [".git/HEAD", ".git/index", ".git/config", "file"];
    let before: Vec<_> = paths
        .iter()
        .map(|p| fs::read(f.local.root.join(p)).unwrap())
        .collect();
    let coordinator = RepositoryCoordinator::new();
    let preview = f.prepare(&coordinator, "main", local_transport).unwrap();
    match &preview.target {
        Some(WriteTarget::Remote {
            oid,
            source_oid,
            pending_commits,
            ref_name,
            display_url,
            ..
        }) => {
            assert!(display_url.starts_with("https://fixture.invalid/"));
            assert_eq!(oid.as_ref(), Some(&f.base));
            assert_eq!(source_oid.as_ref(), Some(&source));
            assert_eq!(ref_name, "refs/heads/main");
            assert_eq!(pending_commits.len(), 1);
            assert_eq!(pending_commits[0].oid, source);
            assert_eq!(pending_commits[0].subject, "上传中文提交");
            assert_eq!(pending_commits[0].parent_oids, [f.base.clone()]);
        }
        _ => panic!("缺少真实推送预览"),
    }
    assert_eq!(f.remote_oid("refs/heads/main"), f.base);
    assert!(
        matches!(finish(&coordinator, &f.repo, &preview), OperationResult::Succeeded { commit_oid: Some(oid), .. } if oid == source)
    );
    assert_eq!(f.remote_oid("refs/heads/main"), source);
    let preview = f
        .prepare(&coordinator, "topic/new", local_transport)
        .unwrap();
    assert!(
        matches!(&preview.target, Some(WriteTarget::Remote { oid: None, pending_commits, .. }) if pending_commits.len() == 2)
    );
    assert!(matches!(
        finish(&coordinator, &f.repo, &preview),
        OperationResult::Succeeded { .. }
    ));
    assert_eq!(f.remote_oid("refs/heads/topic/new"), source);
    let refs = query(
        &f.local.git,
        &f.bare,
        &["for-each-ref", "--format=%(refname)"],
        4096,
    )
    .unwrap();
    assert_eq!(
        std::str::from_utf8(&refs).unwrap(),
        "refs/heads/main\nrefs/heads/topic/new\n"
    );
    for (path, bytes) in paths.iter().zip(before) {
        assert_eq!(
            fs::read(f.local.root.join(path)).unwrap(),
            bytes,
            "不应改变 {path}"
        );
    }
    assert!(!f.local.root.join(".git/refs/remotes/origin/main").exists());
}

/// 若忽略多目标、mirror、签名和客户端 hook，测试将观察到准备被错误接受。
#[cfg(unix)]
#[test]
fn push_rejects_unsupported_configuration_and_live_hook() {
    use std::os::unix::fs::PermissionsExt;
    let f = PushFixture::new();
    f.advance();
    let coordinator = RepositoryCoordinator::new();
    for (key, value) in [
        ("remote.origin.pushurl", "https://other.invalid/repo"),
        ("remote.origin.mirror", "true"),
        ("remote.origin.receivepack", "custom-command"),
        ("remote.origin.uploadpack", "custom-command"),
        ("remote.origin.proxy", "custom-command"),
        ("push.gpgsign", "if-asked"),
        ("push.pushoption", "unconfirmed-option"),
    ] {
        if key.ends_with("pushurl") {
            f.local
                .command(&["config", "--add", key, "https://first.invalid/repo"]);
        }
        f.local.command(&["config", "--add", key, value]);
        assert_eq!(
            f.prepare(&coordinator, "main", local_transport)
                .unwrap_err()
                .code,
            "UNSUPPORTED_WRITE_CONFIGURATION",
            "应拒绝 {key}"
        );
        f.local.command(&["config", "--unset-all", key]);
    }
    let hook = f.repo.common_dir.join("hooks/pre-push");
    fs::write(&hook, b"#!/bin/sh\ntouch should-not-run\n").unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(
        f.prepare(&coordinator, "main", local_transport)
            .unwrap_err()
            .code,
        "UNSUPPORTED_WRITE_CONFIGURATION"
    );
    fs::remove_file(&hook).unwrap();
    let preview = f.prepare(&coordinator, "main", local_transport).unwrap();
    fs::write(&hook, b"#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(
        matches!(finish(&coordinator, &f.repo, &preview), OperationResult::Failed { error, .. } if error.code == "UNSUPPORTED_WRITE_CONFIGURATION")
    );
    assert_eq!(f.remote_oid("refs/heads/main"), f.base);
    assert!(!f.repo.git_dir.join("should-not-run").exists());
}

/// 浅仓库不能假定可见历史完整，不能将截断后的列表作为新建分支预览。
#[cfg(unix)]
#[test]
fn push_rejects_shallow_history_before_network() {
    let f = PushFixture::new();
    let source = f.advance();
    fs::write(f.repo.common_dir.join("shallow"), format!("{source}\n")).unwrap();
    let coordinator = RepositoryCoordinator::new();
    assert_eq!(
        f.prepare(&coordinator, "new", local_transport)
            .unwrap_err()
            .code,
        "UNSUPPORTED_WRITE_CONFIGURATION"
    );
}

/// 非快进和确认后外部变更不能触发上传，不能把远端查询错误当作不存在。
#[cfg(unix)]
#[test]
fn push_rejects_non_fast_forward_and_changed_inputs() {
    let f = PushFixture::new();
    let source = f.advance();
    let coordinator = RepositoryCoordinator::new();
    let preview = f.prepare(&coordinator, "main", local_transport).unwrap();
    f.peer.write("peer", b"other commit\n");
    f.peer.command(&["add", "peer"]);
    f.peer.command(&["commit", "-m", "peer"]);
    let changed = oid(&f.peer, "HEAD");
    f.peer
        .command(&["push", f.bare.to_str().unwrap(), "HEAD:refs/heads/main"]);
    assert!(
        matches!(finish(&coordinator, &f.repo, &preview), OperationResult::Failed { error, .. } if error.code == "REMOTE_CHANGED")
    );
    assert_eq!(f.remote_oid("refs/heads/main"), changed);
    f.local
        .command(&["fetch", f.bare.to_str().unwrap(), "refs/heads/main"]);
    assert_eq!(
        f.prepare(&coordinator, "main", local_transport)
            .unwrap_err()
            .code,
        "NON_FAST_FORWARD"
    );
    let preview = f.prepare(&coordinator, "new", local_transport).unwrap();
    f.local.command(&[
        "config",
        "remote.origin.url",
        "https://changed.invalid/repo",
    ]);
    assert!(
        matches!(finish(&coordinator, &f.repo, &preview), OperationResult::Failed { error, .. } if error.code == "STALE_WRITE_PLAN")
    );
    let mut address = ::url::Url::parse("https://fixture.invalid/").unwrap();
    address.set_path(f.bare.to_str().unwrap());
    f.local
        .command(&["config", "remote.origin.url", address.as_str()]);
    let preview = f.prepare(&coordinator, "new", local_transport).unwrap();
    f.local
        .command(&["update-ref", "refs/heads/main", &f.base, &source]);
    assert!(
        matches!(finish(&coordinator, &f.repo, &preview), OperationResult::Failed { error, .. } if error.code == "STALE_WRITE_PLAN")
    );
    assert!(query(
        &f.local.git,
        &f.bare,
        &["rev-parse", "--verify", "refs/heads/new"],
        4096
    )
    .is_err());
}

/// 模拟服务器已经接受上传但客户端未得到退出结果，保留真实网络副作用。
fn accepted_but_response_lost(
    git: &GitExecutable,
    cwd: &Path,
    args: &[OsString],
    objects: Option<&Path>,
    settings: &[(String, String)],
    deadline: Instant,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<ProcessOutput, OperationError> {
    let output = local_transport(git, cwd, args, objects, settings, deadline, progress)?;
    if args[0] == "push" {
        assert!(output.success);
        Err(OperationError::new("TIMEOUT"))
    } else {
        Ok(output)
    }
}

/// 上传成功之后模拟查询连接断开；真实远端已变化，客户端只能报告未知。
fn accepted_but_verification_lost(
    git: &GitExecutable,
    cwd: &Path,
    args: &[OsString],
    objects: Option<&Path>,
    settings: &[(String, String)],
    deadline: Instant,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<ProcessOutput, OperationError> {
    let marker = cwd.join("test-uploaded");
    if marker.exists() {
        return Err(OperationError::new("NETWORK_FAILED"));
    }
    let output = local_transport(git, cwd, args, objects, settings, deadline, progress)?;
    if args[0] == "push" {
        assert!(output.success);
        fs::write(marker, b"uploaded").unwrap();
    }
    Ok(output)
}

/// 在最后远端查询之后制造竞争提交，真实普通 push 必须拒绝而不能覆盖它。
fn racing_remote(
    git: &GitExecutable,
    cwd: &Path,
    args: &[OsString],
    objects: Option<&Path>,
    settings: &[(String, String)],
    deadline: Instant,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<ProcessOutput, OperationError> {
    if args[0] == "push" {
        let url = args
            .iter()
            .find_map(|arg| {
                arg.to_str()
                    .filter(|s| s.starts_with("https://fixture.invalid/"))
            })
            .unwrap();
        let url = ::url::Url::parse(url).unwrap();
        let bare = ::url::Url::parse(&format!("file://{}", url.path()))
            .unwrap()
            .to_file_path()
            .unwrap();
        let output = Command::new(&git.path)
            .current_dir(&bare)
            .args([
                "-c",
                "core.hooksPath=",
                "update-ref",
                "refs/heads/main",
                "refs/heads/concurrent",
            ])
            .output()
            .unwrap();
        assert!(output.status.success());
    }
    local_transport(git, cwd, args, objects, settings, deadline, progress)
}

/// 丢失响应后只查询实际结果；既不能把未知写当失败，也不能自动重复推送。
#[cfg(unix)]
#[test]
fn push_reconciles_lost_response_and_preserves_unknown_result() {
    for (runner, unknown) in [
        (accepted_but_response_lost as NetworkRunner, false),
        (accepted_but_verification_lost as NetworkRunner, true),
    ] {
        let f = PushFixture::new();
        let source = f.advance();
        let coordinator = RepositoryCoordinator::new();
        let preview = f.prepare(&coordinator, "main", runner).unwrap();
        let result = finish(&coordinator, &f.repo, &preview);
        if unknown {
            assert!(
                matches!(result, OperationResult::Unknown { error, .. } if error.code == "WRITE_OUTCOME_UNKNOWN")
            );
        } else {
            assert!(
                matches!(result, OperationResult::Succeeded { commit_oid: Some(oid), .. } if oid == source)
            );
        }
        assert_eq!(f.remote_oid("refs/heads/main"), source);
    }
}

/// 预检与上传之间的分叉靠真实 Git 非强制约束保护，不假设预检锁住了远端。
#[cfg(unix)]
#[test]
fn push_race_is_rejected_by_native_non_force_update() {
    let f = PushFixture::new();
    f.advance();
    f.peer.write("peer", b"parallel\n");
    f.peer.command(&["add", "peer"]);
    f.peer.command(&["commit", "-m", "parallel"]);
    let concurrent = oid(&f.peer, "HEAD");
    f.peer.command(&[
        "push",
        f.bare.to_str().unwrap(),
        "HEAD:refs/heads/concurrent",
    ]);
    let coordinator = RepositoryCoordinator::new();
    let preview = f.prepare(&coordinator, "main", racing_remote).unwrap();
    let result = finish(&coordinator, &f.repo, &preview);
    assert!(
        matches!(result, OperationResult::Failed { error, .. } if error.code == "REMOTE_REJECTED"),
        "普通推送应明确拒绝竞争写入"
    );
    assert_eq!(f.remote_oid("refs/heads/main"), concurrent);
}

/// 解析器拒绝近似路径、重复行、symref 和错误 OID，失败绝不变成新建目标。
#[test]
fn push_remote_reference_parser_requires_exact_single_record() {
    let oid = "a".repeat(40);
    assert_eq!(parse_remote_oid(b"", "refs/heads/main", 40).unwrap(), None);
    assert_eq!(
        parse_remote_oid(
            format!("{oid}\trefs/heads/main\n").as_bytes(),
            "refs/heads/main",
            40
        )
        .unwrap(),
        Some(oid.clone())
    );
    for value in [
        format!("{oid}\trefs/heads/main/other\n"),
        format!("{oid}\trefs/heads/main\n{oid}\trefs/heads/main\n"),
        "ref: refs/heads/main\tHEAD\n".into(),
        "xyz\trefs/heads/main\n".into(),
        "\n".into(),
    ] {
        assert_eq!(
            parse_remote_oid(value.as_bytes(), "refs/heads/main", 40)
                .unwrap_err()
                .code,
            "PARSE_FAILED"
        );
    }
    assert!(parse_remote_oid(
        format!("{oid}\trefs/heads/main\n").as_bytes(),
        "refs/heads/main",
        64
    )
    .is_err());
}

/// 展示上限必须区分 1000 与 1001，不能静默遗漏被上传的祖先历史。
#[cfg(unix)]
#[test]
fn push_preview_refuses_history_beyond_display_limit() {
    use std::{io::Write, process::Stdio};
    let f = PushFixture::new();
    let mut input = String::new();
    for index in 1..1000 {
        input.push_str(&format!("commit refs/heads/main\nmark :{index}\ncommitter 测试 <test@example.invalid> {} +0000\ndata 4\nmore\n", 1000000000 + index));
        if index == 1 {
            input.push_str(&format!("from {}\n", f.base));
        } else {
            input.push_str(&format!("from :{}\n", index - 1));
        }
        input.push('\n');
    }
    let mut child = Command::new(&f.local.git.path)
        .current_dir(&f.local.root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .args(["-c", "core.hooksPath=", "fast-import", "--quiet"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let coordinator = RepositoryCoordinator::new();
    let preview = f.prepare(&coordinator, "new", local_transport).unwrap();
    assert!(
        matches!(preview.target, Some(WriteTarget::Remote { pending_commits, .. }) if pending_commits.len() == 1000)
    );
    f.local
        .command(&["commit", "--allow-empty", "-m", "one too many"]);
    assert_eq!(
        f.prepare(&coordinator, "new", local_transport)
            .unwrap_err()
            .code,
        "OUTPUT_LIMIT"
    );
    assert!(query(
        &f.local.git,
        &f.bare,
        &["rev-parse", "--verify", "refs/heads/new"],
        4096
    )
    .is_err());
}

/// 相对 hooksPath 按实际客户端工作树解析，原生 Git 会执行的 hook 必须被预检识别。
#[cfg(unix)]
#[test]
fn push_resolves_relative_client_hook_against_worktree_root() {
    use std::os::unix::fs::PermissionsExt;
    let f = PushFixture::new();
    f.advance();
    f.local
        .command(&["config", "core.hooksPath", "custom-hooks"]);
    let hook = f.local.root.join("custom-hooks/pre-push");
    f.local.write(
        "custom-hooks/pre-push",
        b"#!/bin/sh\nprintf invoked > native-hook-marker\nexit 1\n",
    );
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o700)).unwrap();
    let native = Command::new(&f.local.git.path)
        .current_dir(&f.local.root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .args(["push", f.bare.to_str().unwrap(), "HEAD:refs/heads/main"])
        .output()
        .unwrap();
    assert!(!native.status.success());
    assert_eq!(
        fs::read(f.local.root.join("native-hook-marker")).unwrap(),
        b"invoked"
    );
    fs::remove_file(f.local.root.join("native-hook-marker")).unwrap();
    let coordinator = RepositoryCoordinator::new();
    assert_eq!(
        f.prepare(&coordinator, "main", local_transport)
            .unwrap_err()
            .code,
        "UNSUPPORTED_WRITE_CONFIGURATION"
    );
    assert!(!f.local.root.join("native-hook-marker").exists());
    f.local.command(&[
        "config",
        "core.hooksPath",
        f.local.root.join("custom-hooks").to_str().unwrap(),
    ]);
    assert_eq!(
        f.prepare(&coordinator, "main", local_transport)
            .unwrap_err()
            .code,
        "UNSUPPORTED_WRITE_CONFIGURATION"
    );
    f.local.command(&["config", "core.hooksPath", ""]);
    assert!(f.prepare(&coordinator, "main", local_transport).is_ok());
}

/// SHA-256 对象及 linked worktree 从共同对象目录上传，不硬编码 .git 或 40 位 OID。
#[cfg(unix)]
#[test]
fn push_supports_sha256_and_linked_worktree_objects() {
    let f = PushFixture::with_format("sha256");
    let source = f.advance();
    assert_eq!(source.len(), 64);
    let linked = f.peer.root.join("linked");
    f.local.command(&[
        "worktree",
        "add",
        "-b",
        "linked",
        linked.to_str().unwrap(),
        &source,
    ]);
    let (repo, state) = open_repository(&f.local.git, &linked).unwrap();
    assert_ne!(repo.git_dir, repo.common_dir);
    let session = RemoteSession::new(&f.local.git, &repo).unwrap();
    let coordinator = RepositoryCoordinator::new();
    let preview = prepare_with_runner(
        &coordinator,
        &f.local.git,
        &repo,
        &state,
        &session,
        &session.state().remotes[0].remote_id,
        "main",
        local_transport,
    )
    .unwrap();
    assert!(
        matches!(&preview.target, Some(WriteTarget::Remote { pending_commits, .. }) if pending_commits.len() == 1 && pending_commits[0].oid == source)
    );
    assert!(
        matches!(finish(&coordinator, &repo, &preview), OperationResult::Succeeded { commit_oid: Some(oid), .. } if oid == source)
    );
    assert_eq!(f.remote_oid("refs/heads/main"), source);
    assert_eq!(fs::read(linked.join("file")).unwrap(), b"outgoing\n");
}
