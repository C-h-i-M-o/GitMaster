use super::*;
use crate::git::coordinator::RepositoryCoordinator;
use crate::git::repository::{open_repository, tests::Fixture};

/// 消费一次确认计划并等待同一操作的真实终态，不重试写入。
fn execute(
    coordinator: &RepositoryCoordinator,
    repo: &RepositoryHandle,
    preview: &WritePreview,
) -> OperationResult {
    let handle = coordinator
        .execute(Some(&repo.id), &preview.plan_id)
        .unwrap();
    let deadline = Instant::now() + BUDGET + Duration::from_secs(5);
    loop {
        if let Some(result) = coordinator
            .read_operation(Some(&handle.operation_id))
            .unwrap()
            .unwrap()
            .result
        {
            return result;
        }
        assert!(Instant::now() < deadline, "恢复操作未在预算内返回终态");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// 从暂存区恢复只覆盖明确选择的工作文件，保留索引与其他未提交内容。
#[test]
fn restore_selected_file_preserves_index_and_unselected_work() {
    let f = Fixture::new();
    f.command(&["config", "core.autocrlf", "false"]);
    f.write("a", b"base\n");
    f.write("b", b"base\n");
    f.command(&["add", "."]);
    f.command(&["commit", "-m", "base"]);
    f.write("a", b"staged\n");
    f.command(&["add", "a"]);
    f.write("a", b"working\n");
    f.write("b", b"unselected\n");
    let (repo, state) = open_repository(&f.git, &f.root).unwrap();
    let session = RestoreSession::new(&f.git, &repo, &state, RestoreSource::Index).unwrap();
    let file = session
        .page(None)
        .unwrap()
        .files
        .into_iter()
        .find(|file| file.path == "a")
        .unwrap();
    let coordinator = RepositoryCoordinator::new();
    let index = std::fs::read(repo.git_dir.join("index")).unwrap();
    let preview = session
        .prepare(&coordinator, &state, &[file.file_id])
        .unwrap();
    assert_eq!(std::fs::read(f.root.join("a")).unwrap(), b"working\n");
    assert_eq!(preview.paths, ["a"]);
    let result = execute(&coordinator, &repo, &preview);
    assert!(
        matches!(result, OperationResult::Succeeded { .. }),
        "{result:?}"
    );
    assert_eq!(std::fs::read(f.root.join("a")).unwrap(), b"staged\n");
    assert_eq!(std::fs::read(f.root.join("b")).unwrap(), b"unselected\n");
    assert_eq!(std::fs::read(repo.git_dir.join("index")).unwrap(), index);
}

/// 预览后外部编辑使旧计划失败，不覆盖新内容，也不重复执行一次性计划。
#[test]
fn restore_refuses_external_edit_after_preview() {
    let f = Fixture::new();
    f.write("a", b"base\n");
    f.command(&["add", "."]);
    f.command(&["commit", "-m", "base"]);
    f.write("a", b"draft\n");
    let (repo, state) = open_repository(&f.git, &f.root).unwrap();
    let source = RestoreSession::new(&f.git, &repo, &state, RestoreSource::Index).unwrap();
    let coordinator = RepositoryCoordinator::new();
    let index = std::fs::read(repo.git_dir.join("index")).unwrap();
    let preview = source
        .prepare(
            &coordinator,
            &state,
            &[source.page(None).unwrap().files[0].file_id.clone()],
        )
        .unwrap();
    f.write("a", b"external\n");
    let result = execute(&coordinator, &repo, &preview);
    assert!(
        matches!(result, OperationResult::Failed { ref error, .. } if error.code == "STALE_WRITE_PLAN"),
        "{result:?}"
    );
    assert_eq!(std::fs::read(f.root.join("a")).unwrap(), b"external\n");
    assert_eq!(std::fs::read(repo.git_dir.join("index")).unwrap(), index);
}

/// 来源中存在但工作区已删除的二进制文件可无损恢复，索引不随恢复改变。
#[test]
fn restore_recreates_deleted_binary_file() {
    let f = Fixture::new();
    f.write("nested/a.bin", b"a\0b\xff");
    f.command(&["add", "."]);
    f.command(&["commit", "-m", "base"]);
    std::fs::remove_file(f.root.join("nested/a.bin")).unwrap();
    std::fs::remove_dir(f.root.join("nested")).unwrap();
    let (repo, state) = open_repository(&f.git, &f.root).unwrap();
    let source = RestoreSession::new(&f.git, &repo, &state, RestoreSource::Index).unwrap();
    let coordinator = RepositoryCoordinator::new();
    let index = std::fs::read(repo.git_dir.join("index")).unwrap();
    let preview = source
        .prepare(
            &coordinator,
            &state,
            &[source.page(None).unwrap().files[0].file_id.clone()],
        )
        .unwrap();
    let Some(WriteTarget::Restore { files, .. }) = &preview.target else {
        panic!("缺少恢复目标")
    };
    assert!(files[0].before.is_none());
    assert!(files[0].after.text.is_none());
    let result = execute(&coordinator, &repo, &preview);
    assert!(
        matches!(result, OperationResult::Succeeded { .. }),
        "{result:?}"
    );
    assert_eq!(
        std::fs::read(f.root.join("nested/a.bin")).unwrap(),
        b"a\0b\xff"
    );
    assert_eq!(std::fs::read(repo.git_dir.join("index")).unwrap(), index);
}

/// 历史文件身份不能授权覆盖同路径的未跟踪文件。
#[test]
fn restore_refuses_untracked_collision() {
    let f = Fixture::new();
    f.write("a", b"base\n");
    f.command(&["add", "."]);
    f.command(&["commit", "-m", "base"]);
    f.command(&["rm", "--cached", "a"]);
    f.write("a", b"untracked\n");
    let (repo, state) = open_repository(&f.git, &f.root).unwrap();
    let source = RestoreSession::new(&f.git, &repo, &state, RestoreSource::Head).unwrap();
    let coordinator = RepositoryCoordinator::new();
    let error = source
        .prepare(
            &coordinator,
            &state,
            &[source.page(None).unwrap().files[0].file_id.clone()],
        )
        .unwrap_err();
    assert_eq!(error.code, "UNTRACKED_COLLISION");
    assert_eq!(std::fs::read(f.root.join("a")).unwrap(), b"untracked\n");
}

/// 来源会话必须保存已选提交，后续 HEAD 变化不能切换恢复内容。
#[test]
fn restore_source_freezes_commit_and_preserves_real_index() {
    let f = Fixture::new();
    f.write("文件.txt", b"base\n");
    f.command(&["add", "."]);
    f.command(&["commit", "-m", "base"]);
    let (repo, state) = open_repository(&f.git, &f.root).unwrap();
    let before = std::fs::read(repo.git_dir.join("index")).unwrap();
    let session = RestoreSession::new(&f.git, &repo, &state, RestoreSource::Head).unwrap();
    assert_eq!(std::fs::read(repo.git_dir.join("index")).unwrap(), before);
    let first = session.page(None).unwrap();
    f.write("文件.txt", b"next\n");
    f.command(&["add", "."]);
    f.command(&["commit", "-m", "next"]);
    assert_eq!(session.page(None).unwrap().source_oid, first.source_oid);
    assert_eq!(first.files[0].path, "文件.txt");
    assert!(session.selected(&["文件.txt".into()]).is_err());
    assert!(session.selected(&[first.files[0].file_id.clone()]).is_ok());
}

/// 构造纯内存来源映射，测试分页不需要额外创建真实 Git 子进程。
fn mapped_session(count: usize) -> RestoreSession {
    let git = GitExecutable {
        path: "git".into(),
        version: "test".into(),
        source: "test".into(),
    };
    let repo = RepositoryHandle {
        id: "repo".into(),
        root: "root".into(),
        git_dir: "root/.git".into(),
        common_dir: "root/.git".into(),
    };
    let state = RepositoryState {
        index_identity: [0; 32],
        repository_id: "repo".into(),
        snapshot_id: "snapshot".into(),
        root_path: "root".into(),
        head: HeadState::Unborn {
            name: "main".into(),
        },
        operations: Vec::new(),
        changes: Vec::new(),
    };
    let entries = (0..count)
        .map(|index| guard::IndexEntry {
            path: format!("file-{index:05}"),
            mode: "100644".into(),
            oid: "a".repeat(40),
        })
        .collect();
    RestoreSession::from_entries(&git, &repo, &state, RestoreSource::Index, None, entries).unwrap()
}

/// 后续页不会重复首屏，其他来源或任意偏移的游标均不能使用。
#[test]
fn source_pages_are_bounded_and_bound_to_their_session() {
    let source = mapped_session(101);
    let first = source.page(None).unwrap();
    assert_eq!(first.files.len(), 100);
    let second = source.page(first.next_cursor.as_deref()).unwrap();
    assert_eq!(second.files.len(), 1);
    assert_eq!(second.files[0].path, "file-00100");
    assert!(second.next_cursor.is_none());
    assert!(mapped_session(101)
        .page(first.next_cursor.as_deref())
        .is_err());
    for offset in ["0", "1", "200", "18446744073709551615"] {
        assert!(source
            .page(Some(&format!("{}:{offset}", source.id)))
            .is_err());
    }
    assert!(source
        .selected(&[
            first.files[0].file_id.clone(),
            first.files[0].file_id.clone()
        ])
        .is_err());
    assert_eq!(source.selected(&[]).unwrap_err().code, "EMPTY_SELECTION");
}

/// 空来源是完整空页，不能出现不能使用的下一页入口。
#[test]
fn empty_source_has_no_cursor() {
    let page = mapped_session(0).page(None).unwrap();
    assert!(page.files.is_empty());
    assert!(page.next_cursor.is_none());
}
