use super::*;
use crate::git::repository::{open_repository, query, tests::Fixture};
use std::{fs, process::Command, thread};

/// 两个临时工作树和独立 bare 远端，HTTPS 名称仅供测试替身映射到本地路径。
struct FetchFixture {
    source: Fixture,
    local: Fixture,
    bare: PathBuf,
    repo: RepositoryHandle,
    state: RepositoryState,
}
impl FetchFixture {
    /// 建立共同祖先与本地跟踪引用，准备阶段不连接任何真实主机。
    fn new() -> Self {
        let source = Fixture::new();
        let local = Fixture::new();
        let bare = source.root.join("remote.git");
        source.command(&["init", "--bare", bare.to_str().unwrap()]);
        source.write("file", b"base\n");
        source.command(&["add", "file"]);
        source.command(&["commit", "-m", "base"]);
        source.command(&["push", bare.to_str().unwrap(), "HEAD:refs/heads/main"]);
        let mut url = ::url::Url::parse("https://fixture.invalid/").unwrap();
        url.set_path(bare.to_str().unwrap());
        local.command(&["remote", "add", "origin", url.as_str()]);
        local.command(&[
            "fetch",
            bare.to_str().unwrap(),
            "refs/heads/main:refs/remotes/origin/main",
        ]);
        local.command(&["switch", "-C", "main", "refs/remotes/origin/main"]);
        let (repo, state) = open_repository(&local.git, &local.root).unwrap();
        Self {
            source,
            local,
            bare,
            repo,
            state,
        }
    }
    /// 在测试 bare 中增加新提交及无关分支/标签，核验 fetch 不扩大范围。
    fn advance(&self) -> String {
        self.source.write("file", b"incoming\n");
        self.source.command(&["add", "file"]);
        self.source.command(&["commit", "-m", "incoming"]);
        self.source.command(&["tag", "unrequested"]);
        self.source.command(&[
            "push",
            self.bare.to_str().unwrap(),
            "HEAD:refs/heads/main",
            "HEAD:refs/heads/other",
            "refs/tags/unrequested",
        ]);
        String::from_utf8(
            query(
                &self.source.git,
                &self.source.root,
                &["rev-parse", "HEAD"],
                4096,
            )
            .unwrap(),
        )
        .unwrap()
        .trim()
        .into()
    }
    /// 准备时使用生产完整校验，只替换已确认后的网络传输实现。
    fn prepare(
        &self,
        coordinator: &RepositoryCoordinator,
        session: &RemoteSession,
        runner: NetworkRunner,
    ) -> Result<WritePreview, OperationError> {
        let metadata = session.state();
        prepare_with_runner(
            coordinator,
            &self.local.git,
            &self.repo,
            &self.state,
            session,
            &metadata.remotes[0].remote_id,
            &metadata.remote_branches[0].remote_branch_id,
            runner,
        )
    }
}

/// 测试下载替身执行真实本地 Git fetch；该能力不会编译进入生产代码。
fn local_download(
    git: &GitExecutable,
    cwd: &Path,
    args: &[OsString],
    objects: Option<&Path>,
    _settings: &[(String, String)],
    _deadline: Instant,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<ProcessOutput, OperationError> {
    let mut args = args.to_vec();
    let position = args.len() - 2;
    let url = ::url::Url::parse(args[position].to_str().unwrap()).unwrap();
    assert_eq!(url.host_str(), Some("fixture.invalid"));
    let url = ::url::Url::parse(&format!("file://{}", url.path())).unwrap();
    args[position] = url.to_file_path().unwrap().into_os_string();
    let output = local_command(git, cwd, objects)
        .args(args)
        .output()
        .unwrap();
    progress(1, 1);
    Ok(ProcessOutput {
        stdout: output.stdout,
        stderr: output.stderr,
        success: output.status.success(),
        exit_code: output.status.code(),
        truncated: false,
    })
}

/// 测试替身不继承配置、认证或定位变量；只对 fixture 路径开放 file 协议。
fn local_command(git: &GitExecutable, cwd: &Path, objects: Option<&Path>) -> Command {
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
    cmd
}

/// 等待实际后台终态，未完成时有界等待，测试不会无限轮询。
fn finish(
    coordinator: &RepositoryCoordinator,
    repository_id: &str,
    preview: &WritePreview,
) -> OperationResult {
    let handle = coordinator
        .execute(Some(repository_id), &preview.plan_id)
        .unwrap();
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
        assert!(Instant::now() < deadline, "获取任务未在测试预算内结束");
        thread::sleep(Duration::from_millis(5));
    }
}

/// 准备只读，执行只更新一个跟踪引用；本地文件、索引、HEAD、配置及 FETCH_HEAD 保持。
#[cfg(unix)]
#[test]
fn fetch_updates_only_selected_tracking_ref_and_preserves_local_state() {
    let f = FetchFixture::new();
    let new_oid = f.advance();
    f.local.write("file", b"local work\n");
    let state = crate::git::repository::read_repository_state(&f.local.git, &f.repo).unwrap();
    let f = FetchFixture { state, ..f };
    let session = RemoteSession::new(&f.local.git, &f.repo).unwrap();
    let coordinator = RepositoryCoordinator::new();
    let paths = [
        ".git/config",
        ".git/HEAD",
        ".git/index",
        ".git/FETCH_HEAD",
        ".git/refs/heads/main",
        "file",
    ];
    let original: Vec<_> = paths
        .iter()
        .map(|p| fs::read(f.local.root.join(p)).unwrap())
        .collect();
    let tracking_before = fs::read(f.local.root.join(".git/refs/remotes/origin/main")).unwrap();
    let preview = f.prepare(&coordinator, &session, local_download).unwrap();
    assert_eq!(
        fs::read(f.local.root.join(".git/refs/remotes/origin/main")).unwrap(),
        tracking_before
    );
    assert!(
        matches!(&preview.target, Some(WriteTarget::Remote { source_oid: None, ref_name, .. }) if ref_name == "refs/heads/main")
    );
    assert!(session.state().last_fetched_at.is_none());
    assert!(matches!(
        finish(&coordinator, &f.repo.id, &preview),
        OperationResult::Succeeded {
            kind: OperationKind::Fetch,
            ..
        }
    ));
    assert_eq!(
        fs::read_to_string(f.local.root.join(".git/refs/remotes/origin/main"))
            .unwrap()
            .trim(),
        new_oid
    );
    assert!(session.state().last_fetched_at.is_some());
    assert_eq!(
        session.refreshed().unwrap().state().last_fetched_at,
        session.state().last_fetched_at
    );
    for (path, bytes) in paths.iter().zip(original) {
        assert_eq!(
            fs::read(f.local.root.join(path)).unwrap(),
            bytes,
            "意外修改 {path}"
        );
    }
    assert!(!f.local.root.join(".git/refs/remotes/origin/other").exists());
    assert!(!f.local.root.join(".git/refs/tags/unrequested").exists());
    assert!(coordinator
        .execute(Some(&f.repo.id), &preview.plan_id)
        .is_ok());
}

/// 模拟远端传输失败，不发布跟踪引用或伪造成功获取时间。
fn failing_download(
    _git: &GitExecutable,
    _cwd: &Path,
    _args: &[OsString],
    _objects: Option<&Path>,
    _settings: &[(String, String)],
    _deadline: Instant,
    _progress: &mut dyn FnMut(u64, u64),
) -> Result<ProcessOutput, OperationError> {
    Err(OperationError::new("TIMEOUT"))
}

/// 模拟下载期间外部程序移动目标，保留外部值而不是覆盖回来。
fn concurrent_download(
    git: &GitExecutable,
    cwd: &Path,
    args: &[OsString],
    objects: Option<&Path>,
    settings: &[(String, String)],
    deadline: Instant,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<ProcessOutput, OperationError> {
    let result = local_download(git, cwd, args, objects, settings, deadline, progress)?;
    assert!(result.success);
    let output = local_command(git, cwd, objects)
        .args(["rev-parse", PRIVATE_REF])
        .output()
        .unwrap();
    let oid = String::from_utf8(output.stdout).unwrap();
    let root = objects.unwrap().parent().unwrap().parent().unwrap();
    let moved = local_command(git, root, None)
        .args(["update-ref", "refs/remotes/origin/main", oid.trim()])
        .output()
        .unwrap();
    assert!(moved.status.success());
    Ok(result)
}

/// 超时和外部更新分别返回失败，均不标记本次获取成功。
#[cfg(unix)]
#[test]
fn failed_download_and_external_ref_change_do_not_publish_success() {
    let f = FetchFixture::new();
    let new_oid = f.advance();
    let old = fs::read(f.local.root.join(".git/refs/remotes/origin/main")).unwrap();
    let session = RemoteSession::new(&f.local.git, &f.repo).unwrap();
    let coordinator = RepositoryCoordinator::new();
    let preview = f.prepare(&coordinator, &session, failing_download).unwrap();
    assert!(
        matches!(finish(&coordinator, &f.repo.id, &preview), OperationResult::Failed { error, .. } if error.code == "TIMEOUT")
    );
    assert_eq!(
        old,
        fs::read(f.local.root.join(".git/refs/remotes/origin/main")).unwrap()
    );
    let preview = f
        .prepare(&coordinator, &session, concurrent_download)
        .unwrap();
    assert!(
        matches!(finish(&coordinator, &f.repo.id, &preview), OperationResult::Failed { error, .. } if error.code == "REMOTE_CHANGED")
    );
    assert_eq!(
        fs::read_to_string(f.local.root.join(".git/refs/remotes/origin/main"))
            .unwrap()
            .trim(),
        new_oid
    );
    assert!(session.state().last_fetched_at.is_none());
}

/// 仓库级认证命令在准备时拒绝，确认后配置变化使计划失效。
#[cfg(unix)]
#[test]
fn authentication_scope_and_stale_configuration_are_enforced() {
    let f = FetchFixture::new();
    let coordinator = RepositoryCoordinator::new();
    let session = RemoteSession::new(&f.local.git, &f.repo).unwrap();
    let preview = f.prepare(&coordinator, &session, local_download).unwrap();
    f.local
        .command(&["config", "credential.helper", "!touch auth-should-not-run"]);
    assert!(
        matches!(finish(&coordinator, &f.repo.id, &preview), OperationResult::Failed { error, .. } if error.code == "STALE_WRITE_PLAN")
    );
    let session = session.refreshed().unwrap();
    assert_eq!(
        f.prepare(&coordinator, &session, local_download)
            .unwrap_err()
            .code,
        "UNTRUSTED_AUTH_CONFIGURATION"
    );
    assert!(!f.local.root.join("auth-should-not-run").exists());
}

/// refspec 映射支持自定义跟踪目录，但不允许跨远端选择或歧义范围。
#[test]
fn source_ref_mapping_is_explicit_and_unambiguous() {
    assert_eq!(
        map_source_ref(
            &["+refs/heads/*:refs/remotes/origin/*"],
            "refs/remotes/origin/topic/a"
        )
        .unwrap(),
        "refs/heads/topic/a"
    );
    assert_eq!(
        map_source_ref(
            &["refs/heads/trunk:refs/remotes/special/current"],
            "refs/remotes/special/current"
        )
        .unwrap(),
        "refs/heads/trunk"
    );
    assert!(map_source_ref(
        &["+refs/heads/*:refs/remotes/origin/*"],
        "refs/remotes/other/main"
    )
    .is_err());
    assert!(map_source_ref(
        &[
            "refs/heads/main:refs/remotes/origin/x",
            "refs/heads/other:refs/remotes/origin/x"
        ],
        "refs/remotes/origin/x"
    )
    .is_err());
    assert!(map_source_ref(&["^refs/heads/private"], "refs/remotes/origin/x").is_err());
}

/// 发布阶段本身拒绝错误旧 OID，引用变成符号引用也不能更新其指向的本地分支。
#[cfg(unix)]
#[test]
fn tracking_publication_uses_compare_exchange_without_dereferencing() {
    let f = FetchFixture::new();
    let new_oid = f.advance();
    f.local.command(&[
        "fetch",
        f.bare.to_str().unwrap(),
        "refs/heads/main:refs/remotes/seed/main",
    ]);
    let before = fs::read(f.local.root.join(".git/refs/heads/main")).unwrap();
    let old_oid = std::str::from_utf8(&before).unwrap().trim();
    f.local
        .command(&["update-ref", "refs/remotes/origin/main", &new_oid]);
    let deadline = Instant::now() + Duration::from_secs(10);
    let rejected = publish_tracking(
        &f.local.git,
        &f.repo,
        "refs/remotes/origin/main",
        old_oid,
        old_oid,
        deadline,
    )
    .unwrap();
    assert!(!rejected.success);
    assert_eq!(
        fs::read_to_string(f.local.root.join(".git/refs/remotes/origin/main"))
            .unwrap()
            .trim(),
        new_oid
    );
    f.local.command(&[
        "symbolic-ref",
        "refs/remotes/origin/main",
        "refs/heads/main",
    ]);
    let result = publish_tracking(
        &f.local.git,
        &f.repo,
        "refs/remotes/origin/main",
        &new_oid,
        old_oid,
        deadline,
    )
    .unwrap();
    assert!(result.success);
    assert_eq!(
        fs::read(f.local.root.join(".git/refs/heads/main")).unwrap(),
        before
    );
    assert_eq!(
        fs::read_to_string(f.local.root.join(".git/refs/remotes/origin/main"))
            .unwrap()
            .trim(),
        new_oid
    );
}
