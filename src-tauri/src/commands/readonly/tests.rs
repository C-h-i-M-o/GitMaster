use super::*;
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// 为并行测试夹具补充进程内唯一序号，避免时间戳精度不足导致目录碰撞。
static FIXTURE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// 真实隔离仓库与其桌面会话；不操作应用设置或用户仓库。
struct Fixture {
    root: PathBuf,
    shared: DesktopState,
    repository_id: String,
    snapshot_id: String,
}
impl Fixture {
    /// 创建包含一个真实提交的临时仓库，按生产发布流程安装活动状态。
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "gitmaster-ipc-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            FIXTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let git = git::environment::resolve_git(None).unwrap();
        for args in [
            vec!["init", "-b", "main"],
            vec!["config", "user.name", "测试"],
            vec!["config", "user.email", "test@example.invalid"],
        ] {
            command(&git, &root, &args);
        }
        fs::write(root.join("文件.txt"), "base\n").unwrap();
        command(&git, &root, &["add", "."]);
        command(&git, &root, &["commit", "-m", "初始说明"]);
        let (repo, state) = git::repository::open_repository(&git, &root).unwrap();
        let repository_id = repo.id.clone();
        let snapshot_id = state.snapshot_id.clone();
        let shared = DesktopState::default();
        {
            let mut desktop = shared.lock().unwrap();
            desktop.install(Some(git), false).unwrap();
            let token = desktop.begin_repository_request().unwrap();
            let epoch = desktop.epoch;
            desktop
                .publish_repository(epoch, token, repo, state)
                .unwrap();
        }
        Self {
            root,
            shared,
            repository_id,
            snapshot_id,
        }
    }
    /// 使用与 command 相同的请求入口加载第一页。
    fn history(&self) -> HistoryPage {
        HistoryRequest::begin(&self.shared, &self.repository_id, None)
            .unwrap()
            .run(&self.shared)
            .unwrap()
    }
    /// 使用与 command 相同的请求入口加载项目文件列表。
    fn files(&self) -> ProjectFileList {
        ProjectFilesRequest::begin(&self.shared, &self.repository_id, &self.snapshot_id)
            .unwrap()
            .run(&self.shared, false)
            .unwrap()
    }
}
impl Drop for Fixture {
    /// 仅删除本夹具创建的唯一目录。
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// 准备夹具时隔离继承配置，提交不调用用户 hooks 或签名程序。
fn command(git: &GitExecutable, root: &std::path::Path, args: &[&str]) {
    let mut cmd = Command::new(&git.path);
    for (key, _) in std::env::vars_os().filter(|(key, _)| key.to_string_lossy().starts_with("GIT_"))
    {
        cmd.env_remove(key);
    }
    let output = cmd
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .args(["-c", "core.hooksPath=", "-c", "commit.gpgsign=false"])
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "临时仓库准备失败：{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// 贯通真实历史、详情、提交差异、项目文件与分支，并验证换列表后旧 ID 拒绝。
#[test]
fn real_repository_readers_preserve_bytes_and_invalidate_ids() {
    let f = Fixture::new();
    let paths = [
        ".git/index",
        ".git/HEAD",
        ".git/refs/heads/main",
        ".git/config",
        "文件.txt",
    ];
    let bytes: Vec<_> = paths
        .iter()
        .map(|path| fs::read(f.root.join(path)).unwrap())
        .collect();
    let page = f.history();
    assert_eq!(page.commits.len(), 1);
    let access = HistoryAccess::capture(&f.shared, &f.repository_id).unwrap();
    let oid = &page.commits[0].oid;
    let detail = access
        .run(&f.shared, &page.graph_snapshot_id, |history| {
            history.commit_detail(oid)
        })
        .unwrap();
    assert_eq!(detail.subject, "初始说明");
    let files = access
        .run(&f.shared, &page.graph_snapshot_id, |history| {
            history.commit_files(oid, None)
        })
        .unwrap();
    assert_eq!(files.files.len(), 1);
    let file_id = &files.files[0].file_id;
    let diff = access
        .run(&f.shared, &page.graph_snapshot_id, |history| {
            history.commit_file_diff(file_id)
        })
        .unwrap();
    assert!(matches!(diff, FileDiff::Text { content, .. } if content.contains("+base")));
    access
        .run(&f.shared, &page.graph_snapshot_id, |history| {
            history.commit_files(oid, None)
        })
        .unwrap();
    assert_eq!(
        access
            .run(&f.shared, &page.graph_snapshot_id, |history| history
                .commit_file_diff(file_id))
            .unwrap_err()
            .code,
        "FILE_UNAVAILABLE"
    );
    let project = f.files();
    let old_access =
        ProjectFileAccess::capture(&f.shared, &f.repository_id, &f.snapshot_id).unwrap();
    let current_access =
        ProjectFileAccess::capture(&f.shared, &f.repository_id, &f.snapshot_id).unwrap();
    assert!(
        matches!(current_access.run(&f.shared, &project.files[0].file_id).unwrap(), FileDiff::Text { content, .. } if content == "base\n")
    );
    f.files();
    assert_eq!(
        old_access
            .run(&f.shared, &project.files[0].file_id)
            .unwrap_err()
            .code,
        "FILE_UNAVAILABLE"
    );
    let list = BranchRequest::begin(&f.shared, &f.repository_id)
        .unwrap()
        .run(&f.shared)
        .unwrap();
    assert!(list.branches.iter().any(|b| b.name == "main" && b.current));
    for (path, original) in paths.iter().zip(bytes) {
        assert_eq!(fs::read(f.root.join(path)).unwrap(), original);
    }
}

/// 后发请求先完成时，三类旧初始化都不能覆盖当前缓存。
#[test]
fn late_initializers_cannot_overwrite_newer_slots() {
    let f = Fixture::new();
    let old_history = HistoryRequest::begin(&f.shared, &f.repository_id, None).unwrap();
    let latest = f.history();
    assert_eq!(old_history.run(&f.shared).unwrap_err().code, "STALE_GRAPH");
    assert!(HistoryAccess::capture(&f.shared, &f.repository_id)
        .unwrap()
        .run(&f.shared, &latest.graph_snapshot_id, |h| h
            .commit_detail(&latest.commits[0].oid))
        .is_ok());
    let old_files =
        ProjectFilesRequest::begin(&f.shared, &f.repository_id, &f.snapshot_id).unwrap();
    let latest_files = f.files();
    assert_eq!(
        old_files.run(&f.shared, false).unwrap_err().code,
        "STALE_REQUEST"
    );
    assert!(
        ProjectFileAccess::capture(&f.shared, &f.repository_id, &f.snapshot_id)
            .unwrap()
            .run(&f.shared, &latest_files.files[0].file_id)
            .is_ok()
    );
    let old_branches = BranchRequest::begin(&f.shared, &f.repository_id).unwrap();
    let latest_branches = BranchRequest::begin(&f.shared, &f.repository_id)
        .unwrap()
        .run(&f.shared)
        .unwrap();
    assert_eq!(
        old_branches.run(&f.shared).unwrap_err().code,
        "STALE_REQUEST"
    );
    assert_eq!(
        f.shared
            .lock()
            .unwrap()
            .branches
            .as_ref()
            .unwrap()
            .list()
            .branches[0]
            .branch_id,
        latest_branches.branches[0].branch_id
    );
}

/// 模块读取期间主会话锁可用；同仓库新图立即使在途旧图响应失效。
#[test]
fn graph_refresh_during_read_does_not_lock_desktop_or_publish_old_result() {
    let f = Fixture::new();
    let page = f.history();
    let access = HistoryAccess::capture(&f.shared, &f.repository_id).unwrap();
    let shared = f.shared.clone();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        access.run(&shared, &page.graph_snapshot_id, |history| {
            entered_tx.send(()).unwrap();
            release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            history.commit_detail(&page.commits[0].oid)
        })
    });
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(f.shared.0.try_lock().is_ok());
    let replacement = HistoryRequest::begin(&f.shared, &f.repository_id, None).unwrap();
    release_tx.send(()).unwrap();
    assert_eq!(worker.join().unwrap().unwrap_err().code, "STALE_GRAPH");
    replacement.run(&f.shared).unwrap();
}

/// 刷新开始、刷新发布和切换仓库都使旧缓存失效，包括刷新期间启动的请求。
#[test]
fn repository_refresh_and_switch_reject_old_contexts() {
    let f = Fixture::new();
    let page = f.history();
    let project = f.files();
    let old = HistoryAccess::capture(&f.shared, &f.repository_id).unwrap();
    let old_file = ProjectFileAccess::capture(&f.shared, &f.repository_id, &f.snapshot_id).unwrap();
    let (epoch, token, repo, state) = {
        let mut desktop = f.shared.lock().unwrap();
        let token = desktop.begin_repository_request().unwrap();
        let (repo, state) = desktop.active.clone().unwrap();
        (desktop.epoch, token, repo, state)
    };
    assert_eq!(
        old.run(&f.shared, &page.graph_snapshot_id, |h| h
            .commit_detail(&page.commits[0].oid))
            .unwrap_err()
            .code,
        "STALE_REQUEST"
    );
    assert_eq!(
        old_file
            .run(&f.shared, &project.files[0].file_id)
            .unwrap_err()
            .code,
        "STALE_REQUEST"
    );
    let during_refresh = HistoryRequest::begin(&f.shared, &f.repository_id, None).unwrap();
    f.shared
        .lock()
        .unwrap()
        .publish_repository(epoch, token, repo, state)
        .unwrap();
    assert_eq!(
        during_refresh.run(&f.shared).unwrap_err().code,
        "STALE_REQUEST"
    );
    let pending = HistoryRequest::begin(&f.shared, &f.repository_id, None).unwrap();
    let other = Fixture::new();
    let other_active = other.shared.lock().unwrap().active.clone().unwrap();
    {
        let mut desktop = f.shared.lock().unwrap();
        let token = desktop.begin_repository_request().unwrap();
        let epoch = desktop.epoch;
        desktop
            .publish_repository(epoch, token, other_active.0, other_active.1)
            .unwrap();
    }
    assert_eq!(pending.run(&f.shared).unwrap_err().code, "STALE_REQUEST");
    assert!(HistoryRequest::begin(&f.shared, &f.repository_id, None).is_err());
    assert!(HistoryRequest::begin(&f.shared, &other.repository_id, None)
        .unwrap()
        .run(&f.shared)
        .is_ok());
}
