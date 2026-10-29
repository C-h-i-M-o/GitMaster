//! 固定历史差异的缓存只存 Git 原始输出，不存文件选择能力。
use super::*;
use crate::git::{
    process::{run_git, run_history_diff, run_isolated_git},
    repository::command_error,
};
use std::{
    io::Read,
    time::{Duration, Instant},
};

impl HistorySession {
    /// 查询前保留会话门禁；缓存命中也重新解析，超限或截断输出不写入。
    pub(super) fn diff_query<T>(
        &self,
        oid: &str,
        parent: Option<&str>,
        format: &str,
        paths: &[&str],
        mut parse: impl FnMut(&[u8], bool) -> Result<T, OperationError>,
    ) -> Result<T, OperationError> {
        let args = files::diff_args(oid, parent, format, paths);
        let patch = format == "--patch";
        let limit = if patch { 1024 * 1024 } else { OUTPUT_LIMIT };
        let scope = fixed_scope(&self.git.version, &self.repository);
        let key = serde_json::to_string(&(
            "commit-diff-v1",
            &self.git.version,
            oid,
            parent,
            format,
            paths,
        ))
        .map_err(|_| OperationError::new("INVALID_INPUT"))?;
        if let (Some(cache), Some(scope)) = (&self.content_cache, &scope) {
            if let Some(bytes) = cache.get(scope, &key) {
                if bytes.len() <= limit {
                    if let Ok(value) = parse(&bytes, false) {
                        return Ok(value);
                    }
                }
            }
        }
        let out = if scope.is_some() {
            let mut directory = self
                .diff_directory
                .lock()
                .map_err(|_| OperationError::new("INTERNAL_ERROR"))?;
            if directory.is_none() {
                let private = tempfile::Builder::new()
                    .prefix("gitmaster-history-")
                    .tempdir()
                    .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
                let object_format = if oid.len() == 64 { "sha256" } else { "sha1" };
                let args = [
                    "init",
                    "--bare",
                    "--template=",
                    &format!("--object-format={object_format}"),
                ]
                .map(OsString::from);
                let initialized = run_isolated_git(
                    &self.git,
                    private.path(),
                    &args,
                    &[],
                    None,
                    &[],
                    Instant::now() + Duration::from_secs(10),
                )?;
                if !initialized.success {
                    return Err(command_error(&initialized.stderr));
                }
                *directory = Some(private);
            }
            let private = directory
                .as_ref()
                .ok_or_else(|| OperationError::new("INTERNAL_ERROR"))?;
            run_history_diff(
                &self.git,
                private.path(),
                &self.repository.common_dir.join("objects"),
                oid,
                &args,
                limit,
                patch,
            )?
        } else {
            run_git(&self.git, &self.repository.root, &args, limit, patch)?
        };
        if !out.success && !out.truncated {
            return Err(command_error(&out.stderr));
        }
        let value = parse(&out.stdout, out.truncated)?;
        if out.success && !out.truncated {
            if let (Some(cache), Some(scope)) = (&self.content_cache, &scope) {
                if fixed_scope(&self.git.version, &self.repository).as_ref() == Some(scope) {
                    cache.put(scope, &key, &out.stdout);
                }
            }
        }
        Ok(value)
    }
}

/// 旧 Git 和存在图解释覆盖的仓库保持原读取方式，不能套用独立对象视图。
fn fixed_scope(version: &str, repo: &RepositoryHandle) -> Option<String> {
    let mut parts = version.split('.');
    let major = parts.next()?.parse::<u32>().ok()?;
    let minor = parts.next()?.parse::<u32>().ok()?;
    if (major, minor) < (2, 43) {
        return None;
    }
    let scope = cache::scope(repo)?;
    for path in [&repo.git_dir, &repo.common_dir] {
        let dir = cap_std::fs::Dir::open_ambient_dir(path, cap_std::ambient_authority()).ok()?;
        for name in ["shallow", "info/grafts", "refs/replace", "reftable"] {
            match dir.symlink_metadata(name) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                _ => return None,
            }
        }
        match dir.open("packed-refs") {
            Ok(file) => {
                let mut bytes = Vec::new();
                file.take(1024 * 1024 + 1).read_to_end(&mut bytes).ok()?;
                if bytes.len() > 1024 * 1024
                    || bytes
                        .windows(b"refs/replace/".len())
                        .any(|part| part == b"refs/replace/")
                {
                    return None;
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(_) => return None,
        }
    }
    Some(scope)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 浅边界与替代对象不能进入忽略真实引用的私有查询，旧 Git 保持兼容读取。
    #[test]
    fn diff_cache_scope_rejects_graph_overrides_and_old_git() {
        let root = tempfile::tempdir().unwrap();
        let git_dir = root.path().join(".git");
        std::fs::create_dir_all(git_dir.join("info")).unwrap();
        let repo = RepositoryHandle {
            id: "repo".into(),
            root: root.path().into(),
            git_dir: git_dir.clone(),
            common_dir: git_dir.clone(),
        };
        assert!(fixed_scope("2.43.0", &repo).is_some());
        assert!(fixed_scope("2.55.0.windows.3", &repo).is_some());
        assert!(fixed_scope("2.39.0", &repo).is_none());
        for name in ["shallow", "info/grafts", "packed-refs"] {
            std::fs::write(git_dir.join(name), b"abc refs/replace/def\n").unwrap();
            assert!(fixed_scope("2.55.0", &repo).is_none(), "{name}");
            std::fs::remove_file(git_dir.join(name)).unwrap();
        }
        std::fs::create_dir(git_dir.join("refs")).unwrap();
        std::fs::create_dir(git_dir.join("refs/replace")).unwrap();
        assert!(fixed_scope("2.55.0", &repo).is_none());
    }
}
