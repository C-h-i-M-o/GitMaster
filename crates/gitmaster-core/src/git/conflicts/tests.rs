//! 冲突只读会话的真实 Git 验收夹具。
use super::*;
use crate::git::repository::{open_repository, tests::Fixture};
use std::process::Command;

/// 预期冲突的原生命令保留退出码，并使用与夹具一致的隔离配置。
pub(super) fn git(f: &Fixture, args: &[&str]) -> std::process::Output {
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

/// 在具名 main 与 incoming 上修改同一行，原生 merge 产生真正未合并 stage。
pub(super) fn branch_conflict() -> (Fixture, String) {
    let f = Fixture::new();
    f.write("file", b"base\n");
    f.command(&["add", "."]);
    f.command(&["commit", "-m", "base"]);
    f.command(&["branch", "incoming"]);
    f.command(&["checkout", "incoming"]);
    f.write("file", b"incoming\n");
    f.command(&["add", "."]);
    f.command(&["commit", "-m", "incoming"]);
    f.command(&["checkout", "main"]);
    f.write("file", b"local\n");
    f.command(&["add", "."]);
    f.command(&["commit", "-m", "local"]);
    let out = git(&f, &["merge", "--no-commit", "--no-ff", "incoming"]);
    assert_eq!(out.status.code(), Some(1));
    (f, "file".into())
}

/// 原生三方冲突可读取三侧文本，且关键仓库文件未被只读读取修改。
#[test]
fn text_conflict_document_is_read_only() {
    let (f, path) = branch_conflict();
    let before_head = std::fs::read(f.root.join(".git/HEAD")).unwrap();
    let before_index = std::fs::read(f.root.join(".git/index")).unwrap();
    let (repo, _) = open_repository(&f.git, &f.root).unwrap();
    let session = ConflictSession::new(&f.git, &repo).unwrap();
    let state = session.state();
    let file = state.files.iter().find(|x| x.path == path).unwrap();
    assert!(matches!(
        file.editor_support,
        ConflictEditorSupport::Supported
    ));
    let doc = session.read_document(&file.conflict_id).unwrap();
    assert!(doc.base.as_deref() == Some("base\n"));
    assert!(doc.local.as_deref() == Some("local\n"));
    assert!(doc.incoming.as_deref() == Some("incoming\n"));
    assert_eq!(
        doc.result.as_bytes(),
        std::fs::read(f.root.join("file")).unwrap()
    );
    assert!(doc.result.contains("<<<<<<< HEAD"));
    assert_eq!(
        std::fs::read(f.root.join(".git/HEAD")).unwrap(),
        before_head
    );
    assert_eq!(
        std::fs::read(f.root.join(".git/index")).unwrap(),
        before_index
    );
}

/// 无冲突的 no-ff no-commit merge 成功，但冲突文件列表为空。
#[test]
fn clean_merge_has_empty_conflict_files() {
    let f = Fixture::new();
    f.write("file", b"base\n");
    f.command(&["add", "."]);
    f.command(&["commit", "-m", "base"]);
    f.command(&["branch", "incoming"]);
    f.write("local", b"local\n");
    f.command(&["add", "."]);
    f.command(&["commit", "-m", "local"]);
    f.command(&["checkout", "incoming"]);
    f.write("incoming", b"incoming\n");
    f.command(&["add", "."]);
    f.command(&["commit", "-m", "incoming"]);
    f.command(&["checkout", "main"]);
    let out = git(&f, &["merge", "--no-commit", "--no-ff", "incoming"]);
    assert_eq!(out.status.code(), Some(0));
    let (repo, _) = open_repository(&f.git, &f.root).unwrap();
    assert!(ConflictSession::new(&f.git, &repo)
        .unwrap()
        .state()
        .files
        .is_empty());
}

/// 解决冲突后旧会话不能继续读取已变化的索引。
#[test]
fn resolved_index_invalidates_old_session() {
    let (f, _) = branch_conflict();
    let (repo, _) = open_repository(&f.git, &f.root).unwrap();
    let session = ConflictSession::new(&f.git, &repo).unwrap();
    let id = session.state().files[0].conflict_id.clone();
    f.write("file", b"resolved\n");
    f.command(&["add", "file"]);
    assert_eq!(
        session.read_document(&id).unwrap_err().code,
        "STALE_CONFLICT"
    );
}

/// 按真实字节构造三方文件冲突，不把解析器单测冒充原生索引证据。
fn bytes_conflict(base: &[u8], local: &[u8], incoming: &[u8]) -> Fixture {
    let f = Fixture::new();
    f.write("file", base);
    f.command(&["add", "."]);
    f.command(&["commit", "-m", "base"]);
    f.command(&["branch", "incoming"]);
    f.write("file", local);
    f.command(&["commit", "-am", "local"]);
    f.command(&["switch", "incoming"]);
    f.write("file", incoming);
    f.command(&["commit", "-am", "incoming"]);
    f.command(&["switch", "main"]);
    let output = git(&f, &["merge", "--no-ff", "--no-commit", "incoming"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(f.root.join(".git/MERGE_HEAD").is_file());
    f
}

/// 二进制、无效编码和混合换行必须在列表中明确不支持，不能返回空可编辑文档。
#[test]
fn unsupported_bytes_are_classified_before_opening_editor() {
    for (base, local, incoming) in [
        (&b"base\0\n"[..], &b"local\0\n"[..], &b"incoming\0\n"[..]),
        (
            &b"base\xff\n"[..],
            &b"local\xff\n"[..],
            &b"incoming\xff\n"[..],
        ),
        (
            &b"header\r\nbase\n"[..],
            &b"header\r\nlocal\n"[..],
            &b"header\r\nincoming\n"[..],
        ),
    ] {
        let f = bytes_conflict(base, local, incoming);
        let (repo, _) = open_repository(&f.git, &f.root).unwrap();
        let session = ConflictSession::new(&f.git, &repo).unwrap();
        let state = session.state();
        assert_eq!(state.files.len(), 1);
        assert!(matches!(
            state.files[0].editor_support,
            ConflictEditorSupport::Unsupported { .. }
        ));
        assert_eq!(
            session
                .read_document(&state.files[0].conflict_id)
                .unwrap_err()
                .code,
            "UNSUPPORTED_CONFLICT"
        );
    }
}

/// 删除与 add/add 冲突缺少必要 stage，首版内置编辑器拒绝。
#[test]
fn delete_and_add_add_conflicts_require_external_resolution() {
    for add_add in [false, true] {
        let f = Fixture::new();
        f.write("base", b"base\n");
        if !add_add {
            f.write("file", b"base\n");
        }
        f.command(&["add", "."]);
        f.command(&["commit", "-m", "base"]);
        f.command(&["branch", "incoming"]);
        if add_add {
            f.write("file", b"local\n");
        } else {
            std::fs::remove_file(f.root.join("file")).unwrap();
        }
        f.command(&["add", "--all"]);
        f.command(&["commit", "-m", "local"]);
        f.command(&["switch", "incoming"]);
        f.write("file", b"incoming\n");
        f.command(&["add", "."]);
        f.command(&["commit", "-m", "incoming"]);
        f.command(&["switch", "main"]);
        assert_eq!(
            git(&f, &["merge", "--no-ff", "--no-commit", "incoming"])
                .status
                .code(),
            Some(1)
        );
        let (repo, _) = open_repository(&f.git, &f.root).unwrap();
        let session = ConflictSession::new(&f.git, &repo).unwrap();
        let state = session.state();
        assert_eq!(state.files.len(), 1);
        assert!(matches!(
            state.files[0].editor_support,
            ConflictEditorSupport::Unsupported { .. }
        ));
        assert_eq!(
            session
                .read_document(&state.files[0].conflict_id)
                .unwrap_err()
                .code,
            "UNSUPPORTED_CONFLICT"
        );
    }
}

/// BOM 与 CRLF 从原始工作区保留，重新建立会话可以恢复同一合并并更新草稿摘要。
#[test]
fn bom_crlf_and_session_recovery_preserve_actual_bytes() {
    let f = bytes_conflict(
        b"\xef\xbb\xbfheader\r\nbase\r\n",
        b"\xef\xbb\xbfheader\r\nlocal\r\n",
        b"\xef\xbb\xbfheader\r\nincoming\r\n",
    );
    let (repo, _) = open_repository(&f.git, &f.root).unwrap();
    let first = ConflictSession::new(&f.git, &repo).unwrap();
    let second = ConflictSession::new(&f.git, &repo).unwrap();
    assert_eq!(
        first.state().merge_session_id,
        second.state().merge_session_id
    );
    let id = first.state().files[0].conflict_id.clone();
    let doc = first.read_document(&id).unwrap();
    assert_eq!(doc.encoding, "utf8-bom");
    assert_eq!(doc.line_ending, "crlf");
    assert!(doc.result.starts_with("header\r\n"));
    f.write("file", b"\xef\xbb\xbfexternal draft\r\n");
    let changed = second.read_document(&id).unwrap();
    assert_eq!(changed.result, "external draft\r\n");
    assert_ne!(doc.fingerprint, changed.fingerprint);
    std::fs::write(
        repo.git_dir.join("CHERRY_PICK_HEAD"),
        format!("{}\n", first.state().head_oid),
    )
    .unwrap();
    assert_eq!(first.read_document(&id).unwrap_err().code, "STALE_CONFLICT");
}

/// 字节和行数都是编辑硬限制，不截断超限内容后允许保存。
#[test]
fn oversized_conflict_documents_are_not_editable() {
    let (f, _) = branch_conflict();
    let (repo, _) = open_repository(&f.git, &f.root).unwrap();
    for bytes in [vec![b'x'; MAX + 1], b"x\n".repeat(MAX_LINES + 1)] {
        f.write("file", &bytes);
        let session = ConflictSession::new(&f.git, &repo).unwrap();
        let file = &session.state().files[0];
        assert!(matches!(
            file.editor_support,
            ConflictEditorSupport::Unsupported { .. }
        ));
        assert_eq!(
            session.read_document(&file.conflict_id).unwrap_err().code,
            "UNSUPPORTED_CONFLICT"
        );
        assert_eq!(std::fs::read(f.root.join("file")).unwrap(), bytes);
    }
}

/// 结果链接/FIFO 与特殊元数据不跟随、不阻塞，外部目标保持不变。
#[cfg(unix)]
#[test]
fn unsafe_work_files_and_metadata_do_not_escape_or_block() {
    use std::os::unix::{ffi::OsStrExt, fs::symlink};
    let (f, _) = branch_conflict();
    let (repo, _) = open_repository(&f.git, &f.root).unwrap();
    let outside = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(outside.path(), b"private").unwrap();
    std::fs::remove_file(f.root.join("file")).unwrap();
    symlink(outside.path(), f.root.join("file")).unwrap();
    let session = ConflictSession::new(&f.git, &repo).unwrap();
    assert!(matches!(
        session.state().files[0].editor_support,
        ConflictEditorSupport::Unsupported { .. }
    ));
    assert_eq!(std::fs::read(outside.path()).unwrap(), b"private");
    std::fs::remove_file(f.root.join("file")).unwrap();
    let fifo = std::ffi::CString::new(f.root.join("file").as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
    let started = Instant::now();
    let session = ConflictSession::new(&f.git, &repo).unwrap();
    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(matches!(
        session.state().files[0].editor_support,
        ConflictEditorSupport::Unsupported { .. }
    ));
    std::fs::remove_file(repo.git_dir.join("index")).unwrap();
    let index = std::ffi::CString::new(repo.git_dir.join("index").as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(index.as_ptr(), 0o600) }, 0);
    let started = Instant::now();
    assert!(ConflictSession::new(&f.git, &repo).is_err());
    assert!(started.elapsed() < Duration::from_millis(250));
}

/// 内容相同但身份不同的 MERGE_HEAD 表示另一轮状态，旧会话不能复用。
#[test]
fn replaced_merge_generation_and_foreign_ids_are_rejected() {
    let (f, _) = branch_conflict();
    let (repo, _) = open_repository(&f.git, &f.root).unwrap();
    let session = ConflictSession::new(&f.git, &repo).unwrap();
    let id = session.state().files[0].conflict_id.clone();
    assert_eq!(
        session.read_document("foreign-id").unwrap_err().code,
        "STALE_CONFLICT"
    );
    let bytes = std::fs::read(repo.git_dir.join("MERGE_HEAD")).unwrap();
    let replacement = repo.git_dir.join("test-merge-head");
    std::fs::write(&replacement, bytes).unwrap();
    std::fs::rename(replacement, repo.git_dir.join("MERGE_HEAD")).unwrap();
    let current = ConflictSession::new(&f.git, &repo).unwrap();
    assert_ne!(
        current.state().merge_session_id,
        session.state().merge_session_id
    );
    assert_eq!(
        session.read_document(&id).unwrap_err().code,
        "STALE_CONFLICT"
    );
}
