use super::*;
use crate::git::repository::{query, tests::Fixture};
use std::{fs, process::Command, thread};

/// 唯一临时源仓库、bare 远端和选择目录，任何写操作都不面向用户仓库。
struct CloneFixture {
    source: Fixture,
    parent: tempfile::TempDir,
    bare: PathBuf,
    url: String,
}
impl CloneFixture {
    /// 构造带中文文件和二进制内容的远端，HEAD 正确指向 main。
    fn new() -> Self {
        let source = Fixture::new();
        source.write("中文 文件", b"hello\n");
        source.write("binary", b"\0\xff\x01");
        source.command(&["add", "."]);
        source.command(&["commit", "-m", "initial"]);
        let bare = source.root.join("remote.git");
        source.command(&["init", "--bare", "-b", "main", bare.to_str().unwrap()]);
        source.command(&["push", bare.to_str().unwrap(), "HEAD:refs/heads/main"]);
        let mut url = ::url::Url::parse("https://fixture.invalid/").unwrap();
        url.set_path(bare.to_str().unwrap());
        Self {
            source,
            bare,
            url: url.to_string(),
            parent: tempfile::tempdir().unwrap(),
        }
    }
    /// 使用真实计划，只替换 HTTPS 到临时 bare 的传输映射。
    fn prepare(
        &self,
        coordinator: &RepositoryCoordinator,
        name: &str,
        runner: NetworkRunner,
    ) -> Result<ClonePreview, OperationError> {
        prepare_with_runner(
            coordinator,
            &self.source.git,
            self.parent.path(),
            &self.url,
            name,
            runner,
        )
    }
}

/// 仅测试配置允许 file 并重写 fixture 地址；传入 clone 的 URL 保持原值。
fn local_transport(
    git: &GitExecutable,
    cwd: &Path,
    args: &[OsString],
    _objects: Option<&Path>,
    _settings: &[(String, String)],
    _deadline: Instant,
    _progress: &mut dyn FnMut(u64, u64),
) -> Result<ProcessOutput, OperationError> {
    let original = args
        .iter()
        .find_map(|arg| {
            arg.to_str()
                .filter(|s| s.starts_with("https://fixture.invalid/"))
        })
        .unwrap();
    let url = ::url::Url::parse(original).unwrap();
    let path = ::url::Url::parse(&format!("file://{}", url.path())).unwrap();
    let mut cmd = Command::new(&git.path);
    for (key, _) in std::env::vars_os().filter(|(k, _)| k.to_string_lossy().starts_with("GIT_")) {
        cmd.env_remove(key);
    }
    let output = cmd
        .current_dir(cwd)
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
            "-c",
            &format!("url.{path}.insteadOf={original}"),
        ])
        .args(args)
        .output()
        .unwrap();
    Ok(ProcessOutput {
        stdout: output.stdout,
        stderr: output.stderr,
        success: output.status.success(),
        exit_code: output.status.code(),
        truncated: false,
    })
}

/// 操作独立于组件存活，有界等待且重复请求必须返回同一个任务。
fn finish(coordinator: &RepositoryCoordinator, preview: &ClonePreview) -> OperationResult {
    let handle = coordinator.execute(None, &preview.plan_id).unwrap();
    assert_eq!(
        coordinator
            .execute(None, &preview.plan_id)
            .unwrap()
            .operation_id,
        handle.operation_id
    );
    let deadline = Instant::now() + Duration::from_secs(15);
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

/// 准备不创建目标，执行后是可打开且干净的真实仓库，中文与二进制字节保持。
#[cfg(unix)]
#[test]
fn clone_publishes_checked_out_repository_and_keeps_original_url() {
    let f = CloneFixture::new();
    let coordinator = RepositoryCoordinator::new();
    let preview = f.prepare(&coordinator, "新 项目", local_transport).unwrap();
    let target = f.parent.path().join("新 项目");
    assert!(!target.exists());
    assert_eq!(
        preview.target_path,
        fs::canonicalize(f.parent.path())
            .unwrap()
            .join("新 项目")
            .to_str()
            .unwrap()
    );
    let result = finish(&coordinator, &preview);
    assert!(
        matches!(&result, OperationResult::Succeeded { clone_path: Some(path), refresh: WriteRefresh::Ready { state }, .. } if path == &preview.target_path && state.changes.is_empty()),
        "{result:?}"
    );
    assert_eq!(fs::read(target.join("中文 文件")).unwrap(), b"hello\n");
    assert_eq!(fs::read(target.join("binary")).unwrap(), b"\0\xff\x01");
    assert_eq!(
        String::from_utf8(
            query(
                &f.source.git,
                &target,
                &["config", "remote.origin.url"],
                4096
            )
            .unwrap()
        )
        .unwrap()
        .trim(),
        f.url
    );
    assert_eq!(
        query(&f.source.git, &target, &["status", "--porcelain"], 4096).unwrap(),
        b""
    );
    assert!(!target.join(".git/objects/info/alternates").exists());
}

/// 目标名称和已有目标均在联网前拒绝，外部在确认后创建目标也保留原字节。
#[cfg(unix)]
#[test]
fn clone_never_overwrites_existing_or_late_targets() {
    let f = CloneFixture::new();
    let coordinator = RepositoryCoordinator::new();
    fs::create_dir(f.parent.path().join("a")).unwrap();
    for name in [
        "",
        "..",
        "../escape",
        "/absolute",
        "a/b",
        "a\\b",
        "-option",
        ".git",
    ] {
        assert!(f.prepare(&coordinator, name, local_transport).is_err());
    }
    fs::write(f.parent.path().join("exists"), b"keep").unwrap();
    assert_eq!(
        f.prepare(&coordinator, "exists", local_transport)
            .unwrap_err()
            .code,
        "TARGET_EXISTS"
    );
    let preview = f.prepare(&coordinator, "late", local_transport).unwrap();
    fs::create_dir(f.parent.path().join("late")).unwrap();
    fs::write(f.parent.path().join("late/owned"), b"keep").unwrap();
    assert!(
        matches!(finish(&coordinator,&preview),OperationResult::Failed {error,clone_recovery:None,..} if error.code=="TARGET_EXISTS")
    );
    assert_eq!(
        fs::read(f.parent.path().join("late/owned")).unwrap(),
        b"keep"
    );
    assert!(!f.parent.path().join("late/.git").exists());
}

/// 所选父目录在确认后被替换，克隆不能写入替换目录或被移动的旧目录。
#[cfg(unix)]
#[test]
fn clone_detects_replaced_parent_before_download() {
    let f = CloneFixture::new();
    let coordinator = RepositoryCoordinator::new();
    let parent = f.parent.path().join("selected");
    fs::create_dir(&parent).unwrap();
    let preview = prepare_with_runner(
        &coordinator,
        &f.source.git,
        &parent,
        &f.url,
        "project",
        local_transport,
    )
    .unwrap();
    fs::rename(&parent, f.parent.path().join("moved")).unwrap();
    fs::create_dir(&parent).unwrap();
    assert!(
        matches!(finish(&coordinator,&preview),OperationResult::Failed {error,clone_recovery:None,..} if error.code=="STALE_WRITE_PLAN")
    );
    assert!(!parent.join("project").exists());
    assert!(!f.parent.path().join("moved/project").exists());
}

/// 不支持的目标已经下载完成，必须保留仓库数据并标明检出阶段，不能虚报成功。
#[cfg(unix)]
#[test]
fn clone_retains_download_when_checkout_is_unsupported() {
    for unsafe_kind in ["filter", "symlink", "gitlink"] {
        let f = CloneFixture::new();
        let coordinator = RepositoryCoordinator::new();
        match unsafe_kind {
            "filter" => {
                f.source.write(".gitattributes", b"* filter=lfs\n");
                f.source.command(&["add", ".gitattributes"]);
            }
            "symlink" => {
                std::os::unix::fs::symlink("/outside-should-not-read", f.source.root.join("link"))
                    .unwrap();
                f.source.command(&["add", "link"]);
            }
            _ => {
                let oid = String::from_utf8(
                    query(&f.source.git, &f.source.root, &["rev-parse", "HEAD"], 4096).unwrap(),
                )
                .unwrap();
                f.source.command(&[
                    "update-index",
                    "--add",
                    "--cacheinfo",
                    &format!("160000,{},module", oid.trim()),
                ]);
            }
        }
        f.source.command(&["commit", "-m", "unsupported"]);
        f.source
            .command(&["push", f.bare.to_str().unwrap(), "HEAD:refs/heads/main"]);
        let preview = f.prepare(&coordinator, "partial", local_transport).unwrap();
        assert!(
            matches!(finish(&coordinator,&preview),OperationResult::Failed {clone_recovery:Some(CloneRecovery {path,stage:CloneStage::Checkout}),..} if path==preview.target_path)
        );
        assert!(f.parent.path().join("partial/.git/objects").exists());
        assert!(!f.parent.path().join("partial/link").exists());
        assert!(!f.parent.path().join("partial/中文 文件").exists());
    }
}

/// 网络失败只保留可定位的目录，不创建虚假的可打开成功状态。
fn failed_download(
    _git: &GitExecutable,
    cwd: &Path,
    _args: &[OsString],
    _objects: Option<&Path>,
    _settings: &[(String, String)],
    _deadline: Instant,
    _progress: &mut dyn FnMut(u64, u64),
) -> Result<ProcessOutput, OperationError> {
    fs::create_dir(cwd.join("repository")).unwrap();
    fs::write(cwd.join("repository/incomplete"), b"received").unwrap();
    Err(OperationError::new("NETWORK_FAILED"))
}

/// 下载中断的真实残留被保留；调用方取得路径和 download 阶段。
#[cfg(unix)]
#[test]
fn clone_failed_download_exposes_retained_path_and_stage() {
    let f = CloneFixture::new();
    let coordinator = RepositoryCoordinator::new();
    let preview = f.prepare(&coordinator, "partial", failed_download).unwrap();
    assert!(
        matches!(finish(&coordinator,&preview),OperationResult::Failed {error,clone_recovery:Some(CloneRecovery {path,stage:CloneStage::Download}),refresh:WriteRefresh::NotApplicable,..} if error.code=="NETWORK_FAILED" && path==preview.target_path)
    );
    assert_eq!(
        fs::read(f.parent.path().join("partial/incomplete")).unwrap(),
        b"received"
    );
}

/// 空远端和显式 CRLF/可执行文件使用真实 Git 检出规则。
#[cfg(unix)]
#[test]
fn clone_handles_empty_remote_and_preserves_checkout_bytes_and_modes() {
    use std::os::unix::fs::PermissionsExt;
    let f = CloneFixture::new();
    let coordinator = RepositoryCoordinator::new();
    f.source.write(".gitattributes", b"*.txt text eol=crlf\n");
    f.source.write("lines.txt", b"a\nb\n");
    f.source.write("run.sh", b"#!/bin/sh\nexit 0\n");
    fs::set_permissions(
        f.source.root.join("run.sh"),
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    f.source
        .command(&["add", ".gitattributes", "lines.txt", "run.sh"]);
    f.source.command(&["commit", "-m", "attributes"]);
    f.source
        .command(&["push", f.bare.to_str().unwrap(), "HEAD:refs/heads/main"]);
    let preview = f.prepare(&coordinator, "content", local_transport).unwrap();
    assert!(matches!(
        finish(&coordinator, &preview),
        OperationResult::Succeeded { .. }
    ));
    assert_eq!(
        fs::read(f.parent.path().join("content/lines.txt")).unwrap(),
        b"a\r\nb\r\n"
    );
    assert_ne!(
        fs::metadata(f.parent.path().join("content/run.sh"))
            .unwrap()
            .permissions()
            .mode()
            & 0o111,
        0
    );
    f.source.command(&[
        "--git-dir",
        f.bare.to_str().unwrap(),
        "update-ref",
        "-d",
        "refs/heads/main",
    ]);
    let preview = f.prepare(&coordinator, "empty", local_transport).unwrap();
    assert!(
        matches!(finish(&coordinator,&preview),OperationResult::Succeeded {commit_oid:None,refresh:WriteRefresh::Ready {state},..} if matches!(state.head,HeadState::Unborn {..}) && state.changes.is_empty())
    );
}

/// 最后下载阶段外部创建目标；传输依旧真实，发布必须拒绝并保留私有完整仓库。
fn target_appears_during_download(
    git: &GitExecutable,
    cwd: &Path,
    args: &[OsString],
    objects: Option<&Path>,
    settings: &[(String, String)],
    deadline: Instant,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<ProcessOutput, OperationError> {
    let result = local_transport(git, cwd, args, objects, settings, deadline, progress)?;
    let original = args
        .iter()
        .find_map(|a| {
            a.to_str()
                .filter(|s| s.starts_with("https://fixture.invalid/"))
        })
        .unwrap();
    let url = ::url::Url::parse(original).unwrap();
    let bare = ::url::Url::parse(&format!("file://{}", url.path()))
        .unwrap()
        .to_file_path()
        .unwrap();
    let target = fs::read_to_string(bare.parent().unwrap().join("late-target-path")).unwrap();
    fs::create_dir(&target).unwrap();
    fs::write(Path::new(&target).join("existing"), b"keep").unwrap();
    Ok(result)
}

/// 发布竞争时原目录完全保持，失败记录指向本次留下的私有目录。
#[cfg(unix)]
#[test]
fn clone_publication_collision_retains_private_download_without_overwrite() {
    let f = CloneFixture::new();
    let coordinator = RepositoryCoordinator::new();
    let target = f.parent.path().join("late");
    fs::write(
        f.source.root.join("late-target-path"),
        target.to_str().unwrap(),
    )
    .unwrap();
    let preview = f
        .prepare(&coordinator, "late", target_appears_during_download)
        .unwrap();
    let result = finish(&coordinator, &preview);
    let recovery = match result {
        OperationResult::Failed {
            error,
            clone_recovery: Some(recovery),
            ..
        } => {
            assert_eq!(error.code, "TARGET_EXISTS");
            recovery
        }
        _ => panic!("发布冲突未保留恢复信息"),
    };
    assert!(matches!(recovery.stage, CloneStage::Publish));
    let root = PathBuf::from(recovery.path);
    assert!(root
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("gitmaster-clone-"));
    assert!(root.join("repository/.git/objects").exists());
    assert_eq!(fs::read(target.join("existing")).unwrap(), b"keep");
    assert!(!target.join(".git").exists());
    fs::remove_dir_all(root).unwrap();
}

/// 用户属性路径若意外为 FIFO，准备必须有界拒绝，不能等待另一个程序打开管道。
#[cfg(unix)]
#[test]
fn clone_attribute_capture_does_not_wait_for_fifo_writer() {
    use std::{os::unix::ffi::OsStrExt, sync::mpsc};
    let root = tempfile::tempdir().unwrap();
    let fifo = root.path().join("attributes-pipe");
    let cpath = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(cpath.as_ptr(), 0o600) }, 0);
    let auth = AuthPolicy {
        settings: Vec::new(),
        entries: vec![super::super::auth::Entry {
            key: "core.attributesfile".into(),
            value: fifo.to_str().unwrap().into(),
        }],
        digest: [0; 32],
    };
    let (tx, rx) = mpsc::channel();
    thread::scope(|scope| {
        scope.spawn(|| {
            tx.send(capture_attributes(&auth, root.path())).unwrap();
        });
        let result = rx.recv_timeout(Duration::from_millis(250));
        if result.is_err() {
            // 仅释放错误实现的测试线程，确保测试失败后不会留下阻塞后台线程。
            let writer = fs::OpenOptions::new().write(true).open(&fifo).unwrap();
            drop(writer);
        }
        assert!(result.is_ok(), "属性 FIFO 使准备发生无界等待");
        assert_eq!(
            result.unwrap().unwrap_err().code,
            "UNSUPPORTED_WRITE_CONFIGURATION"
        );
    });
}

/// 下载错误与目标竞争同时发生，记录仍应说明最先失败的下载阶段和真实残留路径。
fn failed_download_with_target_collision(
    git: &GitExecutable,
    cwd: &Path,
    args: &[OsString],
    objects: Option<&Path>,
    settings: &[(String, String)],
    deadline: Instant,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<ProcessOutput, OperationError> {
    target_appears_during_download(git, cwd, args, objects, settings, deadline, progress)?;
    Err(OperationError::new("NETWORK_FAILED"))
}

/// 发布失败不能把原先的下载错误改写成另一阶段，私有残留须继续可定位。
#[cfg(unix)]
#[test]
fn clone_retains_original_failure_stage_when_recovery_publication_fails() {
    let f = CloneFixture::new();
    let coordinator = RepositoryCoordinator::new();
    let target = f.parent.path().join("late");
    fs::write(
        f.source.root.join("late-target-path"),
        target.to_str().unwrap(),
    )
    .unwrap();
    let preview = f
        .prepare(&coordinator, "late", failed_download_with_target_collision)
        .unwrap();
    let result = finish(&coordinator, &preview);
    let (error, recovery) = match result {
        OperationResult::Failed {
            error,
            clone_recovery: Some(recovery),
            ..
        } => (error, recovery),
        _ => panic!("缺少下载失败信息"),
    };
    let root = PathBuf::from(&recovery.path);
    assert!(root
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("gitmaster-clone-"));
    assert!(root.join("repository/.git/objects").exists());
    fs::remove_dir_all(root).unwrap();
    assert_eq!(error.code, "NETWORK_FAILED");
    assert!(matches!(recovery.stage, CloneStage::Download));
    assert_eq!(fs::read(target.join("existing")).unwrap(), b"keep");
}

/// 合法的深层路径仍能发布，复制缓冲不能随目录深度成倍占用线程栈。
#[cfg(unix)]
#[test]
fn clone_publishes_deep_tree_without_per_directory_stack_buffer() {
    let f = CloneFixture::new();
    let coordinator = RepositoryCoordinator::new();
    let deep = format!("{}leaf", "d/".repeat(40));
    f.source.write(&deep, b"deep content\n");
    f.source.command(&["add", &deep]);
    f.source.command(&["commit", "-m", "deep"]);
    f.source
        .command(&["push", f.bare.to_str().unwrap(), "HEAD:refs/heads/main"]);
    let preview = f.prepare(&coordinator, "deep", local_transport).unwrap();
    assert!(matches!(
        finish(&coordinator, &preview),
        OperationResult::Succeeded { .. }
    ));
    assert_eq!(
        fs::read(f.parent.path().join("deep").join(deep)).unwrap(),
        b"deep content\n"
    );
}
