use super::{FileChange, HeadState, OperationError};

/// 将 porcelain v2 NUL 字节流解析成 HEAD 和完整文件列表。
pub fn parse_status(bytes: &[u8]) -> Result<(HeadState, Vec<FileChange>), OperationError> {
    let invalid = || OperationError::new("PARSE_FAILED");
    if !bytes.ends_with(&[0]) {
        return Err(invalid());
    }
    let mut records = bytes[..bytes.len() - 1].split(|b| *b == 0);
    let mut oid = None;
    let mut branch = None;
    let mut changes = Vec::new();
    while let Some(raw) = records.next() {
        let record = std::str::from_utf8(raw)
            .map_err(|_| OperationError::new("UNSUPPORTED_PATH_ENCODING"))?;
        if let Some(value) = record.strip_prefix("# branch.oid ") {
            oid = Some(value.to_owned());
            continue;
        }
        if let Some(value) = record.strip_prefix("# branch.head ") {
            branch = Some(value.to_owned());
            continue;
        }
        if record.starts_with("# ") {
            continue;
        }
        let (path, original_path, xy, kind) = if let Some(path) = record.strip_prefix("? ") {
            (path.to_owned(), None, "??".to_owned(), "untracked")
        } else {
            let count = match raw.first() {
                Some(b'1') => 9,
                Some(b'2') => 10,
                Some(b'u') => 11,
                _ => return Err(invalid()),
            };
            let fields: Vec<&str> = record.splitn(count, ' ').collect();
            if fields.len() != count {
                return Err(invalid());
            }
            let xy = fields[1];
            if xy.len() != 2 || !xy.bytes().all(|b| b".MADRCUT".contains(&b)) {
                return Err(invalid());
            }
            let sub = fields[2];
            if sub.len() != 4 || !(sub.starts_with('N') || sub.starts_with('S')) {
                return Err(invalid());
            }
            let original = if raw[0] == b'2' {
                Some(
                    std::str::from_utf8(records.next().ok_or_else(invalid)?)
                        .map_err(|_| OperationError::new("UNSUPPORTED_PATH_ENCODING"))?
                        .to_owned(),
                )
            } else {
                None
            };
            (
                fields[count - 1].to_owned(),
                original,
                xy.to_owned(),
                if raw[0] == b'u' {
                    "conflicted"
                } else if sub.starts_with('S') {
                    "submodule"
                } else {
                    "tracked"
                },
            )
        };
        if path.is_empty() || original_path.as_ref().is_some_and(|p| p.is_empty()) {
            return Err(invalid());
        }
        changes.push(FileChange {
            change_id: changes.len().to_string(),
            path,
            original_path,
            index_status: xy[..1].to_owned(),
            worktree_status: xy[1..].to_owned(),
            kind: kind.to_owned(),
            binary: "unknown".into(),
        });
        if changes.len() > 10_000 {
            return Err(OperationError::new("OUTPUT_LIMIT"));
        }
    }
    let oid = oid.ok_or_else(invalid)?;
    let branch = branch.ok_or_else(invalid)?;
    if oid.is_empty() || branch.is_empty() {
        return Err(invalid());
    }
    let head = if oid == "(initial)" {
        HeadState::Unborn { name: branch }
    } else if branch == "(detached)" {
        HeadState::Detached { oid }
    } else {
        HeadState::Branch { name: branch, oid }
    };
    Ok((head, changes))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 空仓库明确返回 unborn，不假造提交。
    #[test]
    fn unborn_header() {
        let (head, files) = parse_status(b"# branch.oid (initial)\0# branch.head trunk\0").unwrap();
        assert_eq!(
            head,
            HeadState::Unborn {
                name: "trunk".into()
            }
        );
        assert!(files.is_empty());
    }

    /// 重命名的原路径为下一条 NUL 记录，新路径可能包含换行和空格。
    #[test]
    fn rename_and_two_sides() {
        let input = b"# branch.oid abc\0# branch.head main\02 RM N... 100644 100644 100644 abc abc R100 new \n.txt\0old name.txt\01 MM N... 100644 100644 100644 abc abc both.txt\0? :(glob)*\0";
        let (_, files) = parse_status(input).unwrap();
        assert_eq!(files.len(), 3);
        assert_eq!(files[0].path, "new \n.txt");
        assert_eq!(files[0].original_path.as_deref(), Some("old name.txt"));
        assert_eq!(files[1].index_status, "M");
        assert_eq!(files[1].worktree_status, "M");
        assert_eq!(files[2].kind, "untracked");
    }

    /// 冲突和子模块分别标记，不能当成普通文本文件。
    #[test]
    fn conflict_and_submodule() {
        let (_, files) = parse_status(b"# branch.oid abc\0# branch.head (detached)\0u UU N... 100644 100644 100644 100644 abc abc abc conflict\01 M. S... 160000 160000 160000 abc abc sub\0").unwrap();
        assert_eq!(files[0].kind, "conflicted");
        assert_eq!(files[1].kind, "submodule");
    }

    /// 截断、异常状态和不可无损表达的路径一律失败。
    #[test]
    fn rejects_invalid_records() {
        for input in [
            &b"# branch.oid abc\0# branch.head main\0? x"[..],
            &b"# branch.oid abc\0# branch.head main\0x bad\0"[..],
            &b"# branch.oid abc\0# branch.head main\01 ZZ N... 100644 100644 100644 abc abc x\0"[..],
        ] {
            assert_eq!(parse_status(input).unwrap_err().code, "PARSE_FAILED");
        }
        assert_eq!(
            parse_status(b"# branch.oid abc\0# branch.head main\0? \xff\0")
                .unwrap_err()
                .code,
            "UNSUPPORTED_PATH_ENCODING"
        );
    }
}
