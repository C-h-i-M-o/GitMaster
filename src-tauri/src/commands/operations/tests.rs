use super::*;
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::Arc,
    time::{Duration, Instant},
};

/// 临时仓库夹具只测试桌面调度，不操作用户工作区。
pub(super) struct Fixture {
    pub(super) root: PathBuf,
    pub(super) shared: DesktopState,
    pub(super) id: String,
}
impl Fixture {
    /// 创建含初始提交的工作树，并按生产流程发布快照。
    pub(super) fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "gitmaster-writes-{}-{}",
            std::process::id(),
            super::super::resources::next_parent_id()
        ));
        fs::create_dir(&root).unwrap();
        let git = git::environment::resolve_git(None).unwrap();
        for args in [
            vec!["init", "-b", "main"],
            vec!["config", "user.name", "测试"],
            vec!["config", "user.email", "test@example.invalid"],
        ] {
            run_git(&git, &root, &args);
        }
        fs::write(root.join("file.txt"), "base\n").unwrap();
        run_git(&git, &root, &["add", "."]);
        run_git(&git, &root, &["commit", "-m", "base"]);
        let (repo, snapshot) = git::repository::open_repository(&git, &root).unwrap();
        let id = repo.id.clone();
        let shared = DesktopState::default();
        {
            let mut s = shared.lock().unwrap();
            s.install(Some(git), false).unwrap();
            let epoch = s.epoch;
            let token = s.begin_repository_request().unwrap();
            s.publish_repository(epoch, token, repo, snapshot).unwrap();
        }
        Self { root, shared, id }
    }
    /// 外部修改后按真实读取刷新活动快照。
    pub(super) fn refresh(&self) {
        let (ctx, token) = {
            let mut s = self.shared.lock().unwrap();
            let ctx = Context::capture(&s, &self.id).unwrap();
            let token = s.begin_repository_request().unwrap();
            (ctx, token)
        };
        let state = git::repository::read_repository_state(&ctx.git, &ctx.repository).unwrap();
        self.shared
            .lock()
            .unwrap()
            .publish_repository(ctx.epoch, token, ctx.repository, state)
            .unwrap();
    }
    /// 捕获生产本地准备请求。
    fn prepare(&self, request: LocalWriteRequest) -> WritePreparation {
        let snapshot = self
            .shared
            .lock()
            .unwrap()
            .active
            .as_ref()
            .unwrap()
            .1
            .snapshot_id
            .clone();
        WritePreparation::local(&self.shared, &self.id, &snapshot, request).unwrap()
    }
    /// 等待真实后台任务终态，超时视为失败。
    pub(super) fn finish(&self, handle: &OperationHandle) -> OperationRecord {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let record = self
                .shared
                .lock()
                .unwrap()
                .coordinator
                .read_operation(Some(&handle.operation_id))
                .unwrap()
                .unwrap();
            if record.result.is_some() {
                return record;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
impl Drop for Fixture {
    /// 清理自己创建的临时测试目录。
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
/// 使用明确参数运行夹具 Git，失败展示 stderr。
pub(super) fn run_git(git: &GitExecutable, root: &std::path::Path, args: &[&str]) {
    let out = Command::new(&git.path)
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// 连续准备只允许最新返回计划执行；重复确认仅返回同一个任务。
#[test]
fn stage_commit_and_duplicate_execute() {
    let f = Fixture::new();
    fs::write(f.root.join("file.txt"), "changed\n").unwrap();
    f.refresh();
    let ids = f
        .shared
        .lock()
        .unwrap()
        .active
        .as_ref()
        .unwrap()
        .1
        .changes
        .iter()
        .map(|c| c.change_id.clone())
        .collect();
    let preview = f
        .prepare(LocalWriteRequest::Stage { change_ids: ids })
        .run(&f.shared)
        .unwrap();
    let handle = execute(&f.shared, Some(&f.id), &preview.plan_id).unwrap();
    assert_eq!(
        execute(&f.shared, Some(&f.id), &preview.plan_id)
            .unwrap()
            .operation_id,
        handle.operation_id
    );
    assert!(matches!(
        f.finish(&handle).result.unwrap(),
        OperationResult::Succeeded { .. }
    ));
    f.refresh();
    assert_eq!(
        execute(&f.shared, Some(&f.id), &preview.plan_id)
            .unwrap()
            .operation_id,
        handle.operation_id
    );
    let preview = f
        .prepare(LocalWriteRequest::Commit {
            message: "保存说明".into(),
        })
        .run(&f.shared)
        .unwrap();
    let handle = execute(&f.shared, Some(&f.id), &preview.plan_id).unwrap();
    assert!(matches!(
        f.finish(&handle).result.unwrap(),
        OperationResult::Succeeded { .. }
    ));
    f.refresh();
    assert!(f
        .shared
        .lock()
        .unwrap()
        .active
        .as_ref()
        .unwrap()
        .1
        .changes
        .is_empty());
}

/// 晚启动的准备、旧预览和刷新前的预览均不得重新获得执行权。
#[test]
fn stale_preparation_and_preview_are_rejected() {
    let f = Fixture::new();
    let late = f.prepare(LocalWriteRequest::CreateBranch {
        name: "late".into(),
    });
    let current = f
        .prepare(LocalWriteRequest::CreateBranch {
            name: "current".into(),
        })
        .run(&f.shared)
        .unwrap();
    assert_eq!(late.run(&f.shared).unwrap_err().code, "STALE_WRITE_PLAN");
    let newer = f
        .prepare(LocalWriteRequest::CreateBranch {
            name: "newer".into(),
        })
        .run(&f.shared)
        .unwrap();
    assert_eq!(
        execute(&f.shared, Some(&f.id), &current.plan_id)
            .unwrap_err()
            .code,
        "STALE_WRITE_PLAN"
    );
    f.refresh();
    assert_eq!(
        execute(&f.shared, Some(&f.id), &newer.plan_id)
            .unwrap_err()
            .code,
        "STALE_WRITE_PLAN"
    );
}

/// 分支创建完成后重新签发分支映射，并通过真实 ID 执行切换。
#[test]
fn create_and_switch_branch_uses_branch_session_mapping() {
    let f = Fixture::new();
    let create = f
        .prepare(LocalWriteRequest::CreateBranch {
            name: "topic".into(),
        })
        .run(&f.shared)
        .unwrap();
    let handle = execute(&f.shared, Some(&f.id), &create.plan_id).unwrap();
    assert!(matches!(
        f.finish(&handle).result,
        Some(OperationResult::Succeeded { .. })
    ));

    let (git, repo) = {
        let session = f.shared.lock().unwrap();
        (
            session.git.clone().unwrap(),
            session.active.as_ref().unwrap().0.clone(),
        )
    };
    let branches = Arc::new(git::branches::BranchSession::new(&git, &repo).unwrap());
    let branch_id = branches
        .list()
        .branches
        .iter()
        .find(|branch| branch.name == "topic")
        .unwrap()
        .branch_id
        .clone();
    f.shared.lock().unwrap().branches = Some(branches);
    let switch = f
        .prepare(LocalWriteRequest::SwitchBranch { branch_id })
        .run(&f.shared)
        .unwrap();
    let handle = execute(&f.shared, Some(&f.id), &switch.plan_id).unwrap();
    assert!(matches!(
        f.finish(&handle).result,
        Some(OperationResult::Succeeded { .. })
    ));
}

/// clone 只能使用选择器签发的父目录身份，伪造 ID 必须被拒绝。
#[test]
fn clone_rejects_unissued_parent_id() {
    let f = Fixture::new();
    let request = CloneRequest {
        url: "https://example.invalid/repo.git".into(),
        parent_directory_id: "forged-parent".into(),
        directory_name: "clone-target".into(),
    };
    let error = match ClonePreparation::begin(&f.shared, request) {
        Ok(_) => panic!("伪造父目录 ID 不应获得准备权限"),
        Err(error) => error,
    };
    assert_eq!(error.code, "STALE_REQUEST");
}

/// 活动写任务即便仍排队也阻止环境切换，完成后旧结果查询不安装仓库。
#[test]
fn queued_write_blocks_environment_and_old_result_cannot_switch_repository() {
    use std::sync::mpsc;
    let f = Fixture::new();
    let preview = f
        .prepare(LocalWriteRequest::CreateBranch {
            name: "queued".into(),
        })
        .run(&f.shared)
        .unwrap();
    let ctx = Context::capture(&f.shared.lock().unwrap(), &f.id).unwrap();
    let key = git::coordinator::CoordinationKey::repository(&ctx.repository).unwrap();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let blocker = std::thread::spawn(move || {
        ctx.coordinator.read(&key, || {
            entered_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            Ok(())
        })
    });
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let handle = execute(&f.shared, Some(&f.id), &preview.plan_id).unwrap();
    let error = f.shared.lock().unwrap().install(None, true).unwrap_err();
    release_tx.send(()).unwrap();
    blocker.join().unwrap().unwrap();
    assert_eq!(error.code, "OPERATION_IN_PROGRESS");
    assert!(matches!(
        f.finish(&handle).result.unwrap(),
        OperationResult::Succeeded { .. }
    ));
    let other = Fixture::new();
    let (repo, state) = other.shared.lock().unwrap().active.clone().unwrap();
    let expected_snapshot = state.snapshot_id.clone();
    {
        let mut s = f.shared.lock().unwrap();
        let epoch = s.epoch;
        let token = s.begin_repository_request().unwrap();
        s.publish_repository(epoch, token, repo, state).unwrap();
    }
    let record = f
        .shared
        .lock()
        .unwrap()
        .coordinator
        .read_operation(Some(&handle.operation_id))
        .unwrap()
        .unwrap();
    assert_eq!(
        record.progress.handle.repository_id.as_deref(),
        Some(f.id.as_str())
    );
    let s = f.shared.lock().unwrap();
    assert_eq!(s.active.as_ref().unwrap().0.id, other.id);
    assert_eq!(s.active.as_ref().unwrap().1.snapshot_id, expected_snapshot);
}

/// 远端初始化迟到不能重新签发旧映射；当前映射仍可正常读取。
#[test]
fn remote_initialization_publishes_only_latest_request() {
    use super::super::resources::RemoteRequest;
    let f = Fixture::new();
    let old = RemoteRequest::begin(&f.shared, &f.id).unwrap();
    let current = RemoteRequest::begin(&f.shared, &f.id)
        .unwrap()
        .run(&f.shared)
        .unwrap();
    assert_eq!(old.run(&f.shared).unwrap_err().code, "STALE_REQUEST");
    assert_eq!(
        f.shared
            .lock()
            .unwrap()
            .remotes
            .as_ref()
            .unwrap()
            .state()
            .repository_id,
        current.repository_id
    );
}
