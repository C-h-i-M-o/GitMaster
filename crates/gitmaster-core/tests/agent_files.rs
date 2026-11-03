use gitmaster_core::agent::{
    policy::PolicyDecision, provider::Cancellation, tools::files::FileTools, PermissionMode,
    ToolCall,
};
use serde_json::{json, Value};

/// 真正进入仓库写队列后取消，释放读取屏障也不能创建目标文件。
#[tokio::test(flavor = "multi_thread")]
async fn queued_file_write_does_not_execute_after_cancel() {
    use gitmaster_core::agent::{
        provider::Cancellation,
        tools::{git::GitContext, registry::Toolbox},
    };
    use gitmaster_core::git::{
        coordinator::{CoordinationKey, RepositoryCoordinator},
        environment::resolve_git,
        repository::open_repository,
    };
    use std::{
        sync::{mpsc, Arc},
        time::Duration,
    };
    let dir = tempfile::tempdir().unwrap();
    assert!(std::process::Command::new("git")
        .args(["init", "-b", "main"])
        .current_dir(dir.path())
        .output()
        .unwrap()
        .status
        .success());
    let git = resolve_git(None).unwrap();
    let (repository, _) = open_repository(&git, dir.path()).unwrap();
    let key = CoordinationKey::repository(&repository).unwrap();
    let coordinator = RepositoryCoordinator::new();
    let tools = Arc::new(
        Toolbox::new(
            Some(GitContext {
                git,
                repository,
                coordinator: coordinator.clone(),
            }),
            vec![],
        )
        .unwrap(),
    );
    let prepared = tools
        .prepare(
            call(
                "write_file",
                json!({"path":"cancelled.txt", "content":"不得写入", "expectedVersion":null}),
            ),
            PermissionMode::FullAccess,
        )
        .await
        .unwrap();
    let (entered, waiting) = mpsc::channel();
    let (release, held) = mpsc::channel();
    let reader = coordinator.clone();
    let read = std::thread::spawn(move || {
        reader.read(&key, || {
            entered.send(()).unwrap();
            held.recv().unwrap();
            Ok(())
        })
    });
    waiting.recv_timeout(Duration::from_secs(5)).unwrap();
    let cancel = Cancellation::new();
    let signal = cancel.clone();
    let write = tokio::spawn(async move { tools.execute(prepared, true, &signal).await });
    tokio::time::timeout(Duration::from_secs(5), async {
        while coordinator.begin_prepare().is_ok() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert!(!dir.path().join("cancelled.txt").exists());
    cancel.cancel();
    release.send(()).unwrap();
    let _ = read.join().unwrap();
    assert!(write.await.unwrap().is_err(), "停止后排队写入必须取消");
    assert!(!dir.path().join("cancelled.txt").exists());
}

/// 用真实工具参数经过生产校验，不直接调用文件系统写实现。
fn call(name: &str, arguments: Value) -> ToolCall {
    ToolCall {
        id: "call_1".into(),
        name: name.into(),
        arguments,
    }
}

#[test]
fn file_edit_requires_the_read_version_and_rejects_external_edits() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("note.md"), "原内容").unwrap();
    let tools = FileTools::new(Some(dir.path().into()), vec![], None).unwrap();
    let read = tools
        .prepare(
            &call("read_file", json!({"path":"note.md"})),
            PermissionMode::RequestApproval,
        )
        .unwrap();
    let result = tools.execute(read, false, &Cancellation::new()).unwrap();
    assert_eq!(result["content"], "原内容");
    let request = call(
        "write_file",
        json!({"path":"note.md","content":"新内容","expectedVersion":result["version"]}),
    );
    let write = tools
        .prepare(&request, PermissionMode::RequestApproval)
        .unwrap();
    assert_eq!(write.decision, PolicyDecision::Allow);
    std::fs::write(dir.path().join("note.md"), "用户已编辑").unwrap();
    assert!(tools.execute(write, false, &Cancellation::new()).is_err());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("note.md")).unwrap(),
        "用户已编辑"
    );
}

#[test]
fn outside_write_is_gated_and_full_access_can_create_it() {
    let project = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let tools = FileTools::new(Some(project.path().into()), vec![], None).unwrap();
    let path = outside.path().join("new.txt");
    let request = call(
        "write_file",
        json!({"path":path,"content":"外部文件","expectedVersion":null}),
    );
    let pending = tools
        .prepare(&request, PermissionMode::RequestApproval)
        .unwrap();
    assert_eq!(pending.decision, PolicyDecision::Ask);
    assert!(tools.execute(pending, false, &Cancellation::new()).is_err());
    assert!(!path.exists());
    let allowed = tools.prepare(&request, PermissionMode::FullAccess).unwrap();
    tools.execute(allowed, false, &Cancellation::new()).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "外部文件");
    let overwrite = tools.prepare(&request, PermissionMode::FullAccess).unwrap();
    assert!(tools
        .execute(overwrite, false, &Cancellation::new())
        .is_err());
}

#[test]
fn protected_agent_data_and_unknown_parameters_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let tools = FileTools::new(
        Some(dir.path().into()),
        vec![dir.path().join("private")],
        None,
    )
    .unwrap();
    assert!(tools
        .prepare(
            &call(
                "write_file",
                json!({"path":"private/state.json","content":"bad"})
            ),
            PermissionMode::FullAccess
        )
        .is_err());
    assert!(tools
        .prepare(
            &call("read_file", json!({"path":"a","shell":"touch evil"})),
            PermissionMode::FullAccess
        )
        .is_err());
}

#[cfg(unix)]
#[test]
fn symlink_target_is_classified_and_change_after_approval_is_rejected() {
    use std::os::unix::fs::symlink;
    let project = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("inside"), "内").unwrap();
    std::fs::write(outside.path().join("outside"), "外").unwrap();
    symlink(project.path().join("inside"), project.path().join("link")).unwrap();
    let tools = FileTools::new(Some(project.path().into()), vec![], None).unwrap();
    let read = tools
        .prepare(
            &call("read_file", json!({"path":"link"})),
            PermissionMode::RequestApproval,
        )
        .unwrap();
    std::fs::remove_file(project.path().join("link")).unwrap();
    symlink(outside.path().join("outside"), project.path().join("link")).unwrap();
    assert!(tools.execute(read, true, &Cancellation::new()).is_err());
    let read = tools
        .prepare(
            &call("read_file", json!({"path":"link"})),
            PermissionMode::RequestApproval,
        )
        .unwrap();
    assert_eq!(read.decision, PolicyDecision::Ask);
    assert_eq!(
        tools.execute(read, true, &Cancellation::new()).unwrap()["content"],
        "外"
    );
}
