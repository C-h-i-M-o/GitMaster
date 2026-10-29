//! 缓存命名空间由真实仓库实体和影响历史解释的元数据组成，不使用进程会话 ID。
use super::*;
use cap_std::fs::Dir;
use sha2::{Digest, Sha256};
use std::io::Read;

/// 每次命中前重新核对目录实体，仓库重建及 linked worktree 不共享展示块。
pub(super) fn scope(repo: &RepositoryHandle) -> Option<String> {
    let mut hash = Sha256::new();
    for path in [&repo.root, &repo.git_dir, &repo.common_dir] {
        let canonical = std::fs::canonicalize(path).ok()?;
        let identity = crate::git::files::save_context::directory_identity(path).ok()?;
        for value in [canonical.to_string_lossy().as_bytes(), identity.as_bytes()] {
            hash.update((value.len() as u64).to_le_bytes());
            hash.update(value);
        }
    }
    for path in [&repo.git_dir, &repo.common_dir] {
        let dir = Dir::open_ambient_dir(path, cap_std::ambient_authority()).ok()?;
        for name in [
            "config",
            "config.worktree",
            "shallow",
            "packed-refs",
            "info/grafts",
        ] {
            add_file(&dir, name, &mut hash)?;
        }
        match dir.open_dir("refs/replace") {
            Ok(replacements) => {
                let mut names = Vec::new();
                for entry in replacements.entries().ok()? {
                    let entry = entry.ok()?;
                    if !entry.file_type().ok()?.is_file() || names.len() >= 1_000 {
                        return None;
                    }
                    names.push(entry.file_name().into_string().ok()?);
                }
                names.sort();
                for name in names {
                    add_file(&replacements, &name, &mut hash)?;
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                hash.update(b"no-replacements")
            }
            Err(_) => return None,
        }
    }
    Some(format!("{:x}", hash.finalize()))
}

/// 元数据只进入摘要，原始配置及路径不写入持久缓存或日志。
fn add_file(dir: &Dir, name: &str, hash: &mut Sha256) -> Option<()> {
    hash.update((name.len() as u64).to_le_bytes());
    hash.update(name.as_bytes());
    match dir.symlink_metadata(name) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            hash.update(b"absent");
            return Some(());
        }
        Err(_) => return None,
        Ok(metadata) if !metadata.file_type().is_file() || metadata.len() > 1024 * 1024 => {
            return None
        }
        _ => (),
    }
    let mut bytes = Vec::new();
    dir.open(name)
        .ok()?
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > 1024 * 1024 {
        return None;
    }
    hash.update((bytes.len() as u64).to_le_bytes());
    hash.update(&bytes);
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 命中仍需当前会话签发 OID；重开磁盘缓存能解析有效详情而不运行 Git。
    #[test]
    fn history_cache_hit_preserves_session_gate_and_parser() {
        let root = tempfile::tempdir().unwrap();
        let worktree = root.path().join("repo");
        let git_dir = worktree.join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        let repo = RepositoryHandle {
            id: "test".into(),
            root: worktree,
            git_dir: git_dir.clone(),
            common_dir: git_dir,
        };
        let disk = root.path().join("cache");
        let oid = "a".repeat(40);
        let bytes = format!("{oid}\0\0作者\0a@example.invalid\02026-10-29\0提交者\0b@example.invalid\02026-10-29\0测试说明\n");
        let store = crate::cache::ContentCache::new(&disk).unwrap();
        assert!(store.put(
            &scope(&repo).unwrap(),
            &format!("history-utf8-v1:test:detail:{oid}"),
            bytes.as_bytes()
        ));
        drop(store);
        let cache = Arc::new(crate::cache::ContentCache::new(&disk).unwrap());
        let mut history = HistorySession {
            content_cache: None,
            diff_directory: Mutex::new(None),
            git: GitExecutable {
                path: root.path().join("git-must-not-run.exe"),
                version: "test".into(),
                source: "test".into(),
            },
            repository: repo,
            snapshot_id: "current".into(),
            tips: Vec::new(),
            commits: vec![oid.clone()],
            returned: HashSet::new(),
            issued_cursors: HashSet::new(),
            details: HashMap::new(),
            file_selection: None,
        };
        history.set_content_cache(cache);
        assert_eq!(
            history.commit_detail(&oid).unwrap_err().code,
            "INVALID_INPUT"
        );
        history.returned.insert(oid.clone());
        let detail = history.commit_detail(&oid).unwrap();
        assert_eq!(detail.subject, "测试说明");
        assert_eq!(detail.oid, oid);
        assert!(parse_detail(bytes.as_bytes(), &"b".repeat(40)).is_err());
        assert!(parse_detail(b"invalid", &oid).is_err());
    }

    /// 配置、替代对象、linked worktree 和同路径重建均隔离已保存结果。
    #[test]
    fn history_cache_scope_tracks_repository_identity_and_interpretation() {
        let root = tempfile::tempdir().unwrap();
        let worktree = root.path().join("repo");
        let git_dir = worktree.join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        let repo = RepositoryHandle {
            id: "test".into(),
            root: worktree.clone(),
            git_dir: git_dir.clone(),
            common_dir: git_dir.clone(),
        };
        let original = scope(&repo).unwrap();
        std::fs::write(git_dir.join("config"), b"[core]\nignorecase = true\n").unwrap();
        let configured = scope(&repo).unwrap();
        assert_ne!(original, configured);
        std::fs::create_dir_all(git_dir.join("refs/replace")).unwrap();
        std::fs::write(
            git_dir.join("refs/replace").join("a".repeat(40)),
            "b".repeat(40),
        )
        .unwrap();
        let replaced = scope(&repo).unwrap();
        assert_ne!(configured, replaced);
        let linked = root.path().join("linked");
        std::fs::create_dir(&linked).unwrap();
        let linked_repo = RepositoryHandle {
            root: linked,
            ..repo.clone()
        };
        assert_ne!(replaced, scope(&linked_repo).unwrap());
        // 保留旧目录实体，避免测试受文件系统立即重用目录标识影响。
        std::fs::rename(&worktree, root.path().join("old-repo")).unwrap();
        std::fs::create_dir_all(&git_dir).unwrap();
        assert_ne!(original, scope(&repo).unwrap());
    }
}
