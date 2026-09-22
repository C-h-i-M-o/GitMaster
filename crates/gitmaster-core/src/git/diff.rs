use super::*;
use super::{
    process::run_git,
    repository::{command_error, read_repository_state},
};
use std::{
    ffi::OsString,
    io::Read,
    path::{Component, Path},
};
const MAX_BYTES: usize = 1024 * 1024;
const MAX_LINES: usize = 5000;

/// 根据当前快照读取单文件差异，不接收前端路径。
pub fn read_file_diff(
    git: &GitExecutable,
    repository: &RepositoryHandle,
    snapshot: &RepositoryState,
    change_id: &str,
    side: DiffSide,
) -> Result<FileDiff, OperationError> {
    if snapshot.repository_id != repository.id {
        return Err(OperationError::new("STALE_REQUEST"));
    }
    let change = snapshot
        .changes
        .iter()
        .find(|c| c.change_id == change_id)
        .ok_or_else(|| OperationError::new("FILE_UNAVAILABLE"))?;
    let fresh = read_repository_state(git, repository)?;
    if fresh.head != snapshot.head
        || fresh.operations != snapshot.operations
        || fresh.changes != snapshot.changes
    {
        return Err(OperationError::new("STALE_REQUEST"));
    }
    if change.kind == "conflicted" {
        return Ok(FileDiff::Unsupported {
            reason: "conflict".into(),
        });
    }
    if change.kind == "submodule" {
        return Ok(FileDiff::Unsupported {
            reason: "submodule".into(),
        });
    }
    let valid = match side {
        DiffSide::Untracked => change.kind == "untracked",
        DiffSide::Staged => change.kind == "tracked" && change.index_status != ".",
        DiffSide::Unstaged => change.kind == "tracked" && change.worktree_status != ".",
    };
    if !valid {
        return Err(OperationError::new("FILE_UNAVAILABLE"));
    }
    if side == DiffSide::Untracked {
        return preview(repository, &change.path);
    }
    let stats = diff_args(change, side, true);
    let out = run_git(git, &repository.root, &stats, 65536, false)?;
    if !out.success {
        return Err(command_error(&out.stderr));
    }
    if out
        .stdout
        .split(|b| *b == 0)
        .any(|row| row.starts_with(b"-\t-\t"))
    {
        return Ok(FileDiff::Binary);
    }
    let out = run_git(
        git,
        &repository.root,
        &diff_args(change, side, false),
        MAX_BYTES,
        true,
    )?;
    if !out.success && !out.truncated {
        return Err(command_error(&out.stderr));
    }
    text_diff(out.stdout, out.truncated)
}

/// 固定参数和 literal pathspec，不让文件名成为选项或路径模式。
fn diff_args(change: &FileChange, side: DiffSide, stats: bool) -> Vec<OsString> {
    let mut args: Vec<OsString> = [
        "--literal-pathspecs",
        "-c",
        "diff.renames=true",
        "diff",
        "--no-ext-diff",
        "--no-textconv",
        "--no-color",
        "--ignore-submodules=all",
        "--src-prefix=a/",
        "--dst-prefix=b/",
    ]
    .iter()
    .map(OsString::from)
    .collect();
    if side == DiffSide::Staged {
        args.push("--cached".into());
    }
    if stats {
        args.extend(["--numstat".into(), "-z".into()]);
    } else {
        args.push("--unified=3".into());
    }
    args.push("--".into());
    args.push(change.path.clone().into());
    if let Some(original) = &change.original_path {
        args.push(original.into());
    }
    args
}

/// 在目录能力边界内读取普通文件，禁止符号链接及特殊文件。
pub(crate) fn preview(
    repository: &RepositoryHandle,
    path: &str,
) -> Result<FileDiff, OperationError> {
    if Path::new(path)
        .components()
        .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(OperationError::new("FILE_UNAVAILABLE"));
    }
    let dir = cap_std::fs::Dir::open_ambient_dir(&repository.root, cap_std::ambient_authority())
        .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
    let metadata = dir
        .symlink_metadata(path)
        .map_err(|_| OperationError::new("FILE_UNAVAILABLE"))?;
    if metadata.file_type().is_symlink() {
        return Ok(FileDiff::Unsupported {
            reason: "symlink".into(),
        });
    }
    if !metadata.is_file() {
        return Err(OperationError::new("FILE_UNAVAILABLE"));
    }
    let mut options = cap_std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use cap_std::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW);
    }
    let file = dir
        .open_with(path, &options)
        .map_err(|_| OperationError::new("FILE_UNAVAILABLE"))?;
    if !file
        .metadata()
        .map_err(|_| OperationError::new("FILE_UNAVAILABLE"))?
        .is_file()
    {
        return Err(OperationError::new("FILE_UNAVAILABLE"));
    }
    let mut bytes = Vec::new();
    file.take((MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| OperationError::new("FILE_UNAVAILABLE"))?;
    let truncated = bytes.len() > MAX_BYTES;
    bytes.truncate(MAX_BYTES);
    text_diff(bytes, truncated)
}

/// 检测二进制并无损解码，字节截断只允许舍弃末尾不完整字符。
pub(super) fn text_diff(
    mut bytes: Vec<u8>,
    mut truncated: bool,
) -> Result<FileDiff, OperationError> {
    if bytes.contains(&0) {
        return Ok(FileDiff::Binary);
    }
    let mut line_count = 0;
    let cutoff = bytes.iter().position(|b| {
        if *b == b'\n' {
            line_count += 1;
        }
        line_count == MAX_LINES
    });
    if let Some(cutoff) = cutoff {
        if cutoff + 1 < bytes.len() {
            bytes.truncate(cutoff + 1);
            truncated = true;
        }
    }
    let content = match String::from_utf8(bytes) {
        Ok(text) => text,
        Err(error) if truncated && error.utf8_error().error_len().is_none() => {
            let valid = error.utf8_error().valid_up_to();
            let mut bytes = error.into_bytes();
            bytes.truncate(valid);
            String::from_utf8(bytes).map_err(|_| OperationError::new("PARSE_FAILED"))?
        }
        Err(_) => {
            return Ok(FileDiff::Unsupported {
                reason: "encoding".into(),
            })
        }
    };
    Ok(FileDiff::Text { content, truncated })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::repository::{open_repository, tests::Fixture};

    /// 比较侧必须隔离暂存和工作区内容，并支持 unborn。
    #[test]
    fn staged_unstaged_and_unborn() {
        let f = Fixture::new();
        f.write("a", b"staged\n");
        f.command(&["add", "."]);
        f.write("a", b"working\n");
        let (repo, state) = open_repository(&f.git, &f.root).unwrap();
        for (side, yes, no) in [
            (DiffSide::Staged, "+staged", "+working"),
            (DiffSide::Unstaged, "+working", "+staged"),
        ] {
            let FileDiff::Text { content, .. } =
                read_file_diff(&f.git, &repo, &state, &state.changes[0].change_id, side).unwrap()
            else {
                panic!("预期文本");
            };
            assert!(content.contains(yes));
            assert!(!content.contains(no));
        }
    }

    /// 路径魔法与前导短横线只选择目标文件。
    #[test]
    fn literal_names_and_binary() {
        let f = Fixture::new();
        // Windows 禁止冒号和星号，改用合法通配样本及诱饵验证字面匹配。
        let target = if cfg!(windows) {
            "-[target].txt"
        } else {
            ":(glob)*"
        };
        f.write(target, b"only target\n");
        f.write(
            if cfg!(windows) { "-t.txt" } else { "-other" },
            b"private\n",
        );
        f.write("bin", b"a\0b");
        f.command(&["add", "."]);
        let (repo, state) = open_repository(&f.git, &f.root).unwrap();
        let change = state.changes.iter().find(|c| c.path == target).unwrap();
        let FileDiff::Text { content, .. } =
            read_file_diff(&f.git, &repo, &state, &change.change_id, DiffSide::Staged).unwrap()
        else {
            panic!("预期文本")
        };
        assert!(content.contains("only target"));
        assert!(!content.contains("private"));
        let change = state.changes.iter().find(|c| c.path == "bin").unwrap();
        assert!(matches!(
            read_file_diff(&f.git, &repo, &state, &change.change_id, DiffSide::Staged).unwrap(),
            FileDiff::Binary
        ));
    }

    /// 过期快照和错误比较侧不能变为任意文件读取。
    #[test]
    fn rejects_stale_and_invalid_side() {
        let f = Fixture::new();
        f.write("a", b"one\n");
        let (repo, state) = open_repository(&f.git, &f.root).unwrap();
        assert!(
            read_file_diff(&f.git, &repo, &state, "../../secret", DiffSide::Untracked).is_err()
        );
        assert!(read_file_diff(
            &f.git,
            &repo,
            &state,
            &state.changes[0].change_id,
            DiffSide::Staged
        )
        .is_err());
        f.command(&["add", "."]);
        assert_eq!(
            read_file_diff(
                &f.git,
                &repo,
                &state,
                &state.changes[0].change_id,
                DiffSide::Untracked
            )
            .unwrap_err()
            .code,
            "STALE_REQUEST"
        );
    }

    /// 未跟踪内容只作文本预览，编码错误与超限都能辨别。
    #[test]
    fn preview_encoding_and_limit() {
        let f = Fixture::new();
        f.write("text", b"<script>alert(1)</script>");
        f.write("encoding", b"\xff\xfe");
        f.write("long", &b"line\n".repeat(6000));
        let (repo, state) = open_repository(&f.git, &f.root).unwrap();
        for c in &state.changes {
            let diff =
                read_file_diff(&f.git, &repo, &state, &c.change_id, DiffSide::Untracked).unwrap();
            match c.path.as_str() {
                "text" => assert!(matches!(
                    diff,
                    FileDiff::Text {
                        truncated: false,
                        ..
                    }
                )),
                "encoding" => assert!(matches!(diff, FileDiff::Unsupported { .. })),
                "long" => assert!(matches!(
                    diff,
                    FileDiff::Text {
                        truncated: true,
                        ..
                    }
                )),
                _ => unreachable!(),
            }
        }
    }

    /// 未跟踪符号链接不能读取仓库外内容。
    #[cfg(unix)]
    #[test]
    fn untracked_symlink_is_not_followed() {
        let f = Fixture::new();
        std::os::unix::fs::symlink("/etc/passwd", f.root.join("link")).unwrap();
        let (repo, state) = open_repository(&f.git, &f.root).unwrap();
        assert!(
            matches!(read_file_diff(&f.git,&repo,&state,&state.changes[0].change_id,DiffSide::Untracked).unwrap(),FileDiff::Unsupported{reason} if reason=="symlink")
        );
    }
    /// 目录能力不会跟随指向仓库外的链接，即使绕过预检也拒绝打开。
    #[cfg(unix)]
    #[test]
    fn directory_capability_rejects_escape() {
        let f = Fixture::new();
        let outside = Fixture::new();
        outside.write("secret", b"private");
        std::os::unix::fs::symlink(outside.root.join("secret"), f.root.join("link")).unwrap();
        let dir =
            cap_std::fs::Dir::open_ambient_dir(&f.root, cap_std::ambient_authority()).unwrap();
        assert!(dir.open("link").is_err());
        assert!(dir.open("../secret").is_err());
    }

    /// 截断必须恰好保留最多五千行，不能多显示下一行。
    #[test]
    fn exact_line_limit() {
        let FileDiff::Text { content, truncated } =
            text_diff(b"line\n".repeat(5001), false).unwrap()
        else {
            panic!("预期文本");
        };
        assert!(truncated);
        assert_eq!(content.lines().count(), 5000);
        let FileDiff::Text { truncated, .. } = text_diff(b"line\n".repeat(5000), false).unwrap()
        else {
            panic!("预期文本");
        };
        assert!(!truncated);
    }
}
