use super::*;
use crate::git::repository::{open_repository, query, tests::Fixture};
use std::{
    fs,
    time::{Duration, Instant},
};

/// 仅在唯一测试仓库生成当前 HEAD，返回完整提交 OID。
fn commit(f: &Fixture, name: &str, bytes: &[u8]) -> String {
    f.write(name, bytes);
    f.command(&["add", "."]);
    f.command(&["commit", "-m", name]);
    String::from_utf8(query(&f.git, &f.root, &["rev-parse", "HEAD"], 1024).unwrap())
        .unwrap()
        .trim()
        .into()
}

/// 建立当前远端读取会话并准备指定整合模式。
fn prepare(
    f: &Fixture,
    coordinator: &RepositoryCoordinator,
    mode: IntegrateMode,
) -> (RepositoryHandle, WritePreview) {
    let (repo, state) = open_repository(&f.git, &f.root).unwrap();
    let remote = RemoteSession::new(&f.git, &repo).unwrap();
    let id = remote.state().remote_branches[0].remote_branch_id.clone();
    let preview =
        prepare_integrate(coordinator, &f.git, &repo, &state, &remote, &id, mode).unwrap();
    (repo, preview)
}

/// 等待同一个操作终态，绝不以重发 execute 模拟网络重试。
fn finish(
    coordinator: &RepositoryCoordinator,
    repo: &RepositoryHandle,
    preview: &WritePreview,
) -> OperationResult {
    let handle = coordinator
        .execute(Some(&repo.id), &preview.plan_id)
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(record) = coordinator
            .read_operation(Some(&handle.operation_id))
            .unwrap()
        {
            if let Some(result) = record.result {
                return result;
            }
        }
        assert!(Instant::now() < deadline, "整合未在测试期限内完成");
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// 快进准备只读，执行仅推进当前分支并检出确认目标。
#[test]
fn fast_forward_preparation_is_readonly_and_execution_uses_confirmed_oid() {
    let f = Fixture::new();
    let base = commit(&f, "file", b"base\n");
    f.command(&["switch", "-c", "incoming"]);
    let target = commit(&f, "file", b"incoming\n");
    f.command(&["update-ref", "refs/remotes/origin/main", &target]);
    f.command(&["switch", "main"]);
    let before = [
        "HEAD",
        "index",
        "refs/heads/main",
        "refs/remotes/origin/main",
    ]
    .map(|name| fs::read(f.root.join(".git").join(name)).unwrap());
    let objects = query(&f.git, &f.root, &["count-objects", "-v"], 4096).unwrap();
    let coordinator = RepositoryCoordinator::new();
    let (repo, preview) = prepare(&f, &coordinator, IntegrateMode::FastForward);
    assert_eq!(preview.paths, ["file"]);
    assert_eq!(
        before,
        [
            "HEAD",
            "index",
            "refs/heads/main",
            "refs/remotes/origin/main"
        ]
        .map(|name| fs::read(f.root.join(".git").join(name)).unwrap())
    );
    assert_eq!(
        query(&f.git, &f.root, &["count-objects", "-v"], 4096).unwrap(),
        objects
    );
    assert!(
        matches!(finish(&coordinator, &repo, &preview), OperationResult::Succeeded { commit_oid: Some(oid), refresh: WriteRefresh::Ready { state }, .. } if oid == target && state.changes.is_empty() && state.operations.is_empty())
    );
    assert_eq!(fs::read(f.root.join("file")).unwrap(), b"incoming\n");
    assert_eq!(
        query(&f.git, &f.root, &["rev-parse", "HEAD^"], 1024).unwrap(),
        format!("{base}\n").as_bytes()
    );
    assert!(!repo.git_dir.join("MERGE_HEAD").exists());
}

/// 干净分叉只准备合并结果，真实 HEAD 不动，双亲在最终保存前仍可确认。
#[test]
fn clean_divergence_stops_before_creating_merge_commit() {
    let f = Fixture::new();
    commit(&f, "base", b"base\n");
    f.command(&["branch", "incoming"]);
    let local = commit(&f, "local", b"local\n");
    f.command(&["switch", "incoming"]);
    let target = commit(&f, "remote", b"remote\n");
    f.command(&["update-ref", "refs/remotes/origin/main", &target]);
    f.command(&["switch", "main"]);
    let coordinator = RepositoryCoordinator::new();
    let (repo, preview) = prepare(&f, &coordinator, IntegrateMode::Merge);
    assert_eq!(preview.parent_oids, [local.clone(), target.clone()]);
    assert!(
        matches!(finish(&coordinator, &repo, &preview), OperationResult::Succeeded { commit_oid: None, refresh: WriteRefresh::Ready { state }, .. } if state.operations == ["merge"])
    );
    assert_eq!(
        query(&f.git, &f.root, &["rev-parse", "HEAD"], 1024).unwrap(),
        format!("{local}\n").as_bytes()
    );
    assert_eq!(
        fs::read(repo.git_dir.join("MERGE_HEAD")).unwrap(),
        format!("{target}\n").as_bytes()
    );
    assert_eq!(fs::read(f.root.join("local")).unwrap(), b"local\n");
    assert_eq!(fs::read(f.root.join("remote")).unwrap(), b"remote\n");
}

/// 普通文本冲突是待处理状态，不能误报无影响失败或暗中生成提交。
#[test]
fn text_conflict_returns_real_three_stage_resolution_state() {
    let f = Fixture::new();
    commit(&f, "file", b"base\n");
    f.command(&["branch", "incoming"]);
    let local = commit(&f, "file", b"local\n");
    f.command(&["switch", "incoming"]);
    let target = commit(&f, "file", b"incoming\n");
    f.command(&["update-ref", "refs/remotes/origin/main", &target]);
    f.command(&["switch", "main"]);
    let coordinator = RepositoryCoordinator::new();
    let (repo, preview) = prepare(&f, &coordinator, IntegrateMode::Merge);
    let result = finish(&coordinator, &repo, &preview);
    let OperationResult::NeedsResolution { conflict, .. } = result else {
        panic!("预期真实冲突状态，实际 {result:?}");
    };
    assert_eq!(conflict.unresolved_count, 1);
    assert_eq!(conflict.paths, ["file"]);
    let session = ConflictSession::new(&f.git, &repo).unwrap();
    assert_eq!(session.state().merge_session_id, conflict.merge_session_id);
    let document = session
        .read_document(&session.state().files[0].conflict_id)
        .unwrap();
    assert_eq!(document.base.as_deref(), Some("base\n"));
    assert_eq!(document.local.as_deref(), Some("local\n"));
    assert_eq!(document.incoming.as_deref(), Some("incoming\n"));
    assert_eq!(
        query(&f.git, &f.root, &["rev-parse", "HEAD"], 1024).unwrap(),
        format!("{local}\n").as_bytes()
    );
}

/// equal/ahead/unrelated 与错误模式不能伪装成可执行的整合计划。
#[test]
fn integrate_rejects_unnecessary_unrelated_and_wrong_mode() {
    let f = Fixture::new();
    let base = commit(&f, "file", b"base\n");
    let local = commit(&f, "file", b"local\n");
    f.command(&["switch", "-c", "incoming", &base]);
    let target = commit(&f, "file", b"incoming\n");
    f.command(&["switch", "--orphan", "unrelated"]);
    let unrelated = commit(&f, "file", b"unrelated\n");
    f.command(&["switch", "main"]);
    for (oid, mode, code) in [
        (&local, IntegrateMode::FastForward, "NOTHING_TO_COMMIT"),
        (&base, IntegrateMode::Merge, "NOTHING_TO_COMMIT"),
        (&target, IntegrateMode::FastForward, "NON_FAST_FORWARD"),
        (&unrelated, IntegrateMode::Merge, "NO_COMMON_ANCESTOR"),
    ] {
        f.command(&["update-ref", "refs/remotes/origin/main", oid]);
        let (repo, state) = open_repository(&f.git, &f.root).unwrap();
        let remote = RemoteSession::new(&f.git, &repo).unwrap();
        let id = remote.state().remote_branches[0].remote_branch_id.clone();
        assert_eq!(
            prepare_integrate(
                &RepositoryCoordinator::new(),
                &f.git,
                &repo,
                &state,
                &remote,
                &id,
                mode
            )
            .unwrap_err()
            .code,
            code
        );
    }
}

/// 确认后真实内容、索引、配置或跟踪引用变化均拒绝，不因状态字母相同而继续。
#[test]
fn integrate_rechecks_preconditions_without_overwriting_external_state() {
    for change in ["file", "index", "config", "remote"] {
        let f = Fixture::new();
        let base = commit(&f, "file", b"base\n");
        f.command(&["switch", "-c", "incoming"]);
        let target = commit(&f, "file", b"incoming\n");
        f.command(&["update-ref", "refs/remotes/origin/main", &target]);
        f.command(&["switch", "main"]);
        let coordinator = RepositoryCoordinator::new();
        let (repo, preview) = prepare(&f, &coordinator, IntegrateMode::FastForward);
        match change {
            "file" => f.write("file", b"external\n"),
            "index" => {
                f.write("file", b"external\n");
                f.command(&["add", "file"]);
            }
            "config" => f.command(&["config", "core.autocrlf", "true"]),
            _ => f.command(&["update-ref", "refs/remotes/origin/main", &base]),
        }
        let before = fs::read(f.root.join("file")).unwrap();
        let index = fs::read(repo.git_dir.join("index")).unwrap();
        assert!(matches!(
            finish(&coordinator, &repo, &preview),
            OperationResult::Failed { .. }
        ));
        assert_eq!(fs::read(f.root.join("file")).unwrap(), before);
        assert_eq!(fs::read(repo.git_dir.join("index")).unwrap(), index);
        assert!(!repo.git_dir.join("MERGE_HEAD").exists());
    }
}

/// 快进不得覆盖被忽略的用户文件，即使原生 merge 默认允许这样做。
#[test]
fn integrate_preserves_ignored_collision_and_rejects_custom_driver() {
    let f = Fixture::new();
    commit(&f, ".gitignore", b"ignored\n");
    f.command(&["switch", "-c", "incoming"]);
    f.write("ignored", b"remote\n");
    f.command(&["add", "-f", "ignored"]);
    f.command(&["commit", "-m", "incoming"]);
    f.command(&["update-ref", "refs/remotes/origin/main", "HEAD"]);
    f.command(&["switch", "main"]);
    f.write("ignored", b"user secret\n");
    let coordinator = RepositoryCoordinator::new();
    let (repo, preview) = prepare(&f, &coordinator, IntegrateMode::FastForward);
    assert!(matches!(
        finish(&coordinator, &repo, &preview),
        OperationResult::Failed { .. }
    ));
    assert_eq!(fs::read(f.root.join("ignored")).unwrap(), b"user secret\n");
    f.command(&["config", "merge.default", "custom"]);
    let (repo, state) = open_repository(&f.git, &f.root).unwrap();
    let remote = RemoteSession::new(&f.git, &repo).unwrap();
    let id = remote.state().remote_branches[0].remote_branch_id.clone();
    assert_eq!(
        prepare_integrate(
            &coordinator,
            &f.git,
            &repo,
            &state,
            &remote,
            &id,
            IntegrateMode::FastForward
        )
        .unwrap_err()
        .code,
        "UNSUPPORTED_WRITE_CONFIGURATION"
    );
}

/// 合并两侧修改属性时必须匹配原生 Git 的最终检出字节，不能只采用传入树规则。
#[test]
fn merge_attribute_changes_match_native_checkout() {
    for incoming_attributes in [false, true] {
        let f = Fixture::new();
        f.write(".gitattributes", b"file.txt text eol=lf\n");
        commit(&f, "file.txt", b"base\n");
        f.command(&["branch", "incoming"]);
        if !incoming_attributes {
            f.write(".gitattributes", b"file.txt text eol=crlf\n");
        }
        commit(&f, "local", b"local\n");
        f.command(&["switch", "incoming"]);
        if incoming_attributes {
            f.write(".gitattributes", b"file.txt text eol=crlf\n");
        }
        let target = commit(&f, "file.txt", b"incoming\n");
        f.command(&["update-ref", "refs/remotes/origin/main", &target]);
        f.command(&["switch", "main"]);
        f.command(&["merge", "--no-ff", "--no-commit", "incoming"]);
        let expected = fs::read(f.root.join("file.txt")).unwrap();
        assert_eq!(expected, b"incoming\r\n");
        f.command(&["merge", "--abort"]);
        let coordinator = RepositoryCoordinator::new();
        let (repo, preview) = prepare(&f, &coordinator, IntegrateMode::Merge);
        let result = finish(&coordinator, &repo, &preview);
        assert!(
            matches!(
                result,
                OperationResult::Succeeded {
                    commit_oid: None,
                    ..
                }
            ),
            "{result:?}"
        );
        assert_eq!(fs::read(f.root.join("file.txt")).unwrap(), expected);
    }
}

/// 最后校验之后外部属性和自定义驱动变化也不能进入私有合并环境。
#[test]
fn captured_merge_environment_keeps_global_rules_and_ignores_late_driver() {
    let f = Fixture::new();
    commit(&f, "file", b"base\n");
    f.command(&["branch", "incoming"]);
    commit(&f, "file", b"local\n");
    f.command(&["switch", "incoming"]);
    let target = commit(&f, "file", b"incoming\n");
    f.command(&["switch", "main"]);
    let attributes = tempfile::NamedTempFile::new().unwrap();
    fs::write(attributes.path(), b"file merge=union\n").unwrap();
    f.command(&[
        "config",
        "core.attributesFile",
        attributes.path().to_str().unwrap(),
    ]);
    let (repo, _) = open_repository(&f.git, &f.root).unwrap();
    let deadline = Instant::now() + BUDGET;
    let fingerprint = guard::fingerprint(&f.git, &repo, &["file".into()], false, deadline).unwrap();
    let captured = CapturedTree::capture(&f.git, &repo, &fingerprint, &target, deadline)
        .unwrap()
        .for_merge(&f.git, &repo, deadline)
        .unwrap();
    fs::write(attributes.path(), b"file merge=binary\n").unwrap();
    fs::create_dir_all(repo.common_dir.join("info")).unwrap();
    fs::write(repo.common_dir.join("info/attributes"), b"file merge=pwn\n").unwrap();
    f.command(&["config", "merge.pwn.driver", "touch executed"]);
    let args = [
        "merge",
        "--no-ff",
        "--no-commit",
        "--no-edit",
        "-s",
        "ort",
        &target,
    ]
    .map(OsString::from);
    let output = captured.run(&f.git, &repo, &args, deadline).unwrap();
    assert!(output.success, "捕获的 union 合并应成功");
    let content = fs::read_to_string(f.root.join("file")).unwrap();
    assert!(content.contains("local\n"));
    assert!(content.contains("incoming\n"));
    assert!(!content.contains("<<<<<<<"));
    assert!(!f.root.join("executed").exists());
    assert_eq!(fs::read(attributes.path()).unwrap(), b"file merge=binary\n");
}
