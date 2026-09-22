//! 冲突保存准备与执行的真实 Git 行为验收。
use super::*;
use crate::git::coordinator::RepositoryCoordinator;
use crate::git::repository::{open_repository, query, tests::Fixture};
use std::{
    process::Command,
    time::{Duration, Instant},
};

/// 隔离测试 Git 环境，并保留原生冲突命令的非零退出码。
fn git(f: &Fixture, args: &[&str]) -> std::process::Output {
    let mut command = Command::new(&f.git.path);
    for (key, _) in std::env::vars_os().filter(|(key, _)| key.to_string_lossy().starts_with("GIT_"))
    {
        command.env_remove(key);
    }
    command
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .args(args)
        .current_dir(&f.root)
        .output()
        .unwrap()
}

/// 用原生三方合并建立两个真实的普通文本冲突，验证单文件保存边界。
fn conflict() -> (Fixture, RepositoryHandle, ConflictSession, String) {
    let f = Fixture::new();
    f.write("file", b"base\n");
    f.write("other", b"base\n");
    f.command(&["add", "."]);
    f.command(&["commit", "-m", "base"]);
    f.command(&["branch", "incoming"]);
    f.command(&["switch", "incoming"]);
    f.write("file", b"incoming\n");
    f.write("other", b"incoming\n");
    f.command(&["add", "."]);
    f.command(&["commit", "-m", "incoming"]);
    f.command(&["switch", "main"]);
    f.write("file", b"local\n");
    f.write("other", b"local\n");
    f.command(&["add", "."]);
    f.command(&["commit", "-m", "local"]);
    assert_eq!(
        git(&f, &["merge", "--no-commit", "--no-ff", "incoming"])
            .status
            .code(),
        Some(1)
    );
    let (repo, _) = open_repository(&f.git, &f.root).unwrap();
    let session = ConflictSession::new(&f.git, &repo).unwrap();
    let id = session.state().files[0].conflict_id.clone();
    (f, repo, session, id)
}

/// 只启动一次保存任务，等待相同句柄进入终态。
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
        assert!(Instant::now() < deadline, "保存操作未在测试期限内完成");
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// 准备不修改仓库，保存仅消除所选文件的 stage 并保留另一个冲突。
#[test]
fn prepare_save_is_readonly_and_execution_resolves_only_selected_path() {
    let (f, repo, session, id) = conflict();
    let coordinator = RepositoryCoordinator::new();
    let head = std::fs::read(repo.git_dir.join("HEAD")).unwrap();
    let index = std::fs::read(repo.git_dir.join("index")).unwrap();
    let other_stage = query(&f.git, &f.root, &["ls-files", "-u", "--", "other"], 4096).unwrap();
    let merge_head = std::fs::read(repo.git_dir.join("MERGE_HEAD")).unwrap();
    let objects = query(&f.git, &f.root, &["count-objects", "-v"], 4096).unwrap();
    let document = session.read_document(&id).unwrap();
    let preview = prepare_save(
        &coordinator,
        &session,
        &id,
        &document.fingerprint,
        "resolved\n",
    )
    .unwrap();
    assert_eq!(std::fs::read(repo.git_dir.join("HEAD")).unwrap(), head);
    assert_eq!(std::fs::read(repo.git_dir.join("index")).unwrap(), index);
    assert_eq!(
        std::fs::read(repo.git_dir.join("MERGE_HEAD")).unwrap(),
        merge_head
    );
    assert_eq!(
        query(&f.git, &f.root, &["count-objects", "-v"], 4096).unwrap(),
        objects
    );
    assert!(matches!(
        finish(&coordinator, &repo, &preview),
        OperationResult::Succeeded { .. }
    ));
    assert!(repo.git_dir.join("MERGE_HEAD").is_file());
    assert_eq!(std::fs::read(f.root.join("file")).unwrap(), b"resolved\n");
    assert_eq!(
        query(&f.git, &f.root, &["ls-files", "-u", "--", "other"], 4096).unwrap(),
        other_stage
    );
}

/// 打开编辑器后发生的外部改动，使旧文档指纹失效。
#[test]
fn stale_fingerprint_is_rejected_without_clobbering_external_change() {
    let (f, repo, session, id) = conflict();
    let coordinator = RepositoryCoordinator::new();
    let document = session.read_document(&id).unwrap();
    f.write("file", b"external\n");
    let error = prepare_save(
        &coordinator,
        &session,
        &id,
        &document.fingerprint,
        "resolved\n",
    )
    .unwrap_err();
    assert_eq!(error.code, "STALE_CONFLICT");
    assert_eq!(std::fs::read(f.root.join("file")).unwrap(), b"external\n");
    assert!(repo.git_dir.join("MERGE_HEAD").is_file());
}

/// 未清除标记和超过上限的草稿不得进入可执行计划。
#[test]
fn markers_and_oversized_content_are_rejected() {
    let (f, _repo, session, id) = conflict();
    let coordinator = RepositoryCoordinator::new();
    let document = session.read_document(&id).unwrap();
    for content in [
        "<<<<<<< HEAD\nkeep\n=======\nother\n>>>>>>> incoming\n",
        &"x".repeat(1024 * 1024 + 1),
    ] {
        let error =
            prepare_save(&coordinator, &session, &id, &document.fingerprint, content).unwrap_err();
        assert!(matches!(
            error.code.as_str(),
            "UNSUPPORTED_CONFLICT" | "OUTPUT_LIMIT"
        ));
    }
    assert!(std::fs::read(f.root.join("file"))
        .unwrap()
        .starts_with(b"<<<<<<< HEAD"));
}

/// 建立保留 BOM 与 CRLF 的冲突，确认保存草稿写回字节与原生 add 一致。
#[test]
fn bom_crlf_save_preserves_encoding_and_native_stage_zero_blob() {
    let f = Fixture::new();
    f.write(".gitattributes", b"file -text\n");
    f.write("file", b"\xef\xbb\xbfheader\r\nbase\r\n");
    f.command(&["add", "."]);
    f.command(&["commit", "-m", "base"]);
    f.command(&["branch", "incoming"]);
    f.command(&["switch", "incoming"]);
    f.write("file", b"\xef\xbb\xbfheader\r\nincoming\r\n");
    f.command(&["add", "."]);
    f.command(&["commit", "-m", "incoming"]);
    f.command(&["switch", "main"]);
    f.write("file", b"\xef\xbb\xbfheader\r\nlocal\r\n");
    f.command(&["add", "."]);
    f.command(&["commit", "-m", "local"]);
    assert_eq!(
        git(&f, &["merge", "--no-commit", "--no-ff", "incoming"])
            .status
            .code(),
        Some(1)
    );
    let (repo, _) = open_repository(&f.git, &f.root).unwrap();
    let session = ConflictSession::new(&f.git, &repo).unwrap();
    let id = session.state().files[0].conflict_id.clone();
    let doc = session.read_document(&id).unwrap();
    assert_eq!(doc.encoding, "utf8-bom");
    assert_eq!(doc.line_ending, "crlf");
    let coordinator = RepositoryCoordinator::new();
    let preview = prepare_save(
        &coordinator,
        &session,
        &id,
        &doc.fingerprint,
        "header\nresolved\n",
    )
    .unwrap();
    assert!(matches!(
        finish(&coordinator, &repo, &preview),
        OperationResult::Succeeded { .. }
    ));
    let saved = std::fs::read(f.root.join("file")).unwrap();
    assert_eq!(saved, b"\xef\xbb\xbfheader\r\nresolved\r\n");
    let oid = String::from_utf8(query(&f.git, &f.root, &["rev-parse", ":0:file"], 1024).unwrap())
        .unwrap();
    let expected = query(&f.git, &f.root, &["hash-object", "file"], 1024).unwrap();
    assert_eq!(
        oid.trim().as_bytes(),
        String::from_utf8(expected).unwrap().trim().as_bytes()
    );
}

/// 准备后文件、索引或配置变化都必须拒绝执行并保留外部状态。
#[test]
fn save_rechecks_file_index_and_config_changes() {
    for change in ["file", "index", "config"] {
        let (f, repo, session, id) = conflict();
        let coordinator = RepositoryCoordinator::new();
        let doc = session.read_document(&id).unwrap();
        let preview =
            prepare_save(&coordinator, &session, &id, &doc.fingerprint, "resolved\n").unwrap();
        match change {
            "file" => f.write("file", b"external\n"),
            "index" => {
                f.write("file", b"external\n");
                f.command(&["add", "file"]);
            }
            "config" => f.command(&["config", "core.autocrlf", "true"]),
            _ => unreachable!(),
        }
        let result = finish(&coordinator, &repo, &preview);
        assert!(
            matches!(result, OperationResult::Failed { .. }),
            "{change}: {result:?}"
        );
        if change != "config" {
            assert_eq!(std::fs::read(f.root.join("file")).unwrap(), b"external\n");
        }
    }
}
