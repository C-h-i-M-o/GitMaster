use super::{process::run_git, GitEnvironment, GitExecutable, OperationError};
use std::{
    collections::HashSet,
    ffi::OsString,
    io::Read,
    path::{Path, PathBuf},
};
const MIN_VERSION: (u32, u32, u32) = (2, 39, 0);

/// 检测手动路径或按顺序选择系统候选，不静默替换失败的显式设置。
pub fn resolve_git(manual: Option<&Path>) -> Result<GitExecutable, OperationError> {
    if let Some(path) = manual {
        return verify(path, "manual");
    }
    select_candidates(path_candidates())
}

/// 只描述已经通过验证的环境，不在持有桌面会话锁时再启动子进程。
pub fn describe_git(git: &GitExecutable) -> Result<GitEnvironment, OperationError> {
    let path = git
        .path
        .to_str()
        .ok_or_else(|| OperationError::new("UNSUPPORTED_PATH_ENCODING"))?;
    Ok(GitEnvironment::Ready {
        executable_path: path.to_owned(),
        version: git.version.clone(),
        source: git.source.clone(),
    })
}

/// 验证候选的原生格式与真实 Git 版本，保留平台后缀用于展示。
fn verify(path: &Path, source: &str) -> Result<GitExecutable, OperationError> {
    let path = validate_candidate(path)?;
    let mut git = GitExecutable {
        path,
        version: String::new(),
        source: source.into(),
    };
    let out = run_git(
        &git,
        &std::env::temp_dir(),
        &[OsString::from("--version")],
        4096,
        false,
    )?;
    if !out.success {
        return Err(OperationError::new("GIT_EXECUTION_FAILED"));
    }
    let (version, display) =
        parse_version(&out.stdout).ok_or_else(|| OperationError::new("GIT_UNSUPPORTED"))?;
    if version < MIN_VERSION {
        return Err(OperationError::new("GIT_UNSUPPORTED"));
    }
    git.version = display;
    Ok(git)
}

/// 数值比较版本号，允许 Apple/Windows 的附加标识。
fn parse_version(bytes: &[u8]) -> Option<((u32, u32, u32), String)> {
    let text = std::str::from_utf8(bytes).ok()?.trim();
    let version = text.strip_prefix("git version ")?;
    if version.contains(['\n', '\r']) {
        return None;
    }
    let token = version.split_whitespace().next()?;
    let mut parts = token.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next().unwrap_or("0").parse().ok()?;
    Some(((major, minor, patch), version.to_owned()))
}

/// PATH 在前，平台常见位置在后；不执行用户 shell 初始化文件。
fn path_candidates() -> Vec<(PathBuf, &'static str)> {
    let mut candidates = Vec::new();
    if let Some(path) = std::env::var_os("PATH") {
        for directory in std::env::split_paths(&path) {
            candidates.push((
                directory.join(if cfg!(windows) { "git.exe" } else { "git" }),
                "path",
            ));
        }
    }
    #[cfg(target_os = "macos")]
    for path in [
        "/opt/homebrew/bin/git",
        "/usr/local/bin/git",
        "/usr/bin/git",
    ] {
        candidates.push((path.into(), "common"));
    }
    #[cfg(windows)]
    for key in ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"] {
        if let Some(base) = std::env::var_os(key) {
            candidates.push((PathBuf::from(base).join("Git/cmd/git.exe"), "common"));
        }
    }
    candidates
}

/// 逐个探测并去重，全部失败时保留版本不支持等有意义的原因。
fn select_candidates(candidates: Vec<(PathBuf, &str)>) -> Result<GitExecutable, OperationError> {
    let mut seen = HashSet::new();
    let mut failure = OperationError::new("GIT_NOT_FOUND");
    for (path, source) in candidates {
        let Ok(path) = validate_candidate(&path) else {
            continue;
        };
        if !seen.insert(path.clone()) {
            continue;
        }
        match verify(&path, source) {
            Ok(git) => return Ok(git),
            Err(error) => failure = error,
        }
    }
    Err(failure)
}

/// 识别系统可执行文件头，拒绝脚本包装器。
fn native_magic(bytes: [u8; 4]) -> bool {
    bytes == [0x7f, b'E', b'L', b'F']
        || bytes[..2] == [b'M', b'Z']
        || matches!(
            u32::from_be_bytes(bytes),
            0xfeedface
                | 0xfeedfacf
                | 0xcefaedfe
                | 0xcffaedfe
                | 0xcafebabe
                | 0xbebafeca
                | 0xcafebabf
                | 0xbfbafeca
        )
}

/// 避免执行缺少开发工具时会触发安装提示的 macOS Git 占位程序。
fn mac_git_available(path: &Path) -> bool {
    if !cfg!(target_os = "macos") || path != Path::new("/usr/bin/git") {
        return true;
    }
    [
        "/Library/Developer/CommandLineTools/usr/bin/git",
        "/Applications/Xcode.app/Contents/Developer/usr/bin/git",
    ]
    .iter()
    .any(|p| Path::new(p).is_file())
}

/// 校验无损绝对路径、执行权限、平台占位与原生文件格式。
fn validate_candidate(path: &Path) -> Result<PathBuf, OperationError> {
    let invalid = || OperationError::new("GIT_PATH_INVALID");
    let path = std::fs::canonicalize(path).map_err(|_| invalid())?;
    if path.to_str().is_none() {
        return Err(OperationError::new("UNSUPPORTED_PATH_ENCODING"));
    }
    let meta = std::fs::metadata(&path).map_err(|_| invalid())?;
    if !meta.is_file() || !mac_git_available(&path) {
        return Err(invalid());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o111 == 0 {
            return Err(invalid());
        }
    }
    #[cfg(windows)]
    if path
        .extension()
        .and_then(|s| s.to_str())
        .is_some_and(|s| matches!(s.to_ascii_lowercase().as_str(), "cmd" | "bat" | "ps1"))
    {
        return Err(invalid());
    }
    let mut header = [0; 4];
    std::fs::File::open(&path)
        .and_then(|mut f| f.read_exact(&mut header))
        .map_err(|_| invalid())?;
    if !native_magic(header) {
        return Err(invalid());
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    /// 平台后缀保留，但不是 Git 的输出不被误认。
    #[test]
    fn versions() {
        for (input, expected) in [
            ("git version 2.43.0 (Apple Git-143)", (2, 43, 0)),
            ("git version 2.55.0.windows.3", (2, 55, 0)),
            ("git version 2.39", (2, 39, 0)),
        ] {
            assert_eq!(parse_version(input.as_bytes()).unwrap().0, expected);
        }
        assert!(parse_version(b"version 2.43").is_none());
        assert!(parse_version(b"git version bad").is_none());
        assert!(parse_version(b"git version 2.43.0\ninjected").is_none());
    }
    /// 数值兼容边界不按字符串比较，并覆盖最低版本及前一个次版本。
    #[test]
    fn compatibility_floor() {
        assert!(parse_version(b"git version 2.38.9").unwrap().0 < MIN_VERSION);
        assert!(parse_version(b"git version 2.39.0").unwrap().0 >= MIN_VERSION);
        assert!(parse_version(b"git version 2.100.0").unwrap().0 > MIN_VERSION);
    }

    /// 小端 Mach-O 与通用二进制头均可识别，shell 文本拒绝。
    #[test]
    fn native_headers() {
        assert!(native_magic([0xcf, 0xfa, 0xed, 0xfe]));
        assert!(native_magic([0xca, 0xfe, 0xba, 0xbe]));
        assert!(!native_magic(*b"#!/b"));
    }
    /// 显式无效路径不会回退，无候选和目录分别返回契约错误。
    #[test]
    fn missing() {
        assert_eq!(
            resolve_git(Some(Path::new("/missing/git")))
                .unwrap_err()
                .code,
            "GIT_PATH_INVALID"
        );
        assert_eq!(
            resolve_git(Some(&std::env::temp_dir())).unwrap_err().code,
            "GIT_PATH_INVALID"
        );
        assert_eq!(select_candidates(vec![]).unwrap_err().code, "GIT_NOT_FOUND");
    }
    /// 后面的有效候选可跳过前面的失效文件，source 与真实绝对路径保留。
    #[test]
    fn candidate_order() {
        let real = resolve_git(None).unwrap();
        let selected = select_candidates(vec![
            (PathBuf::from("/missing/git"), "path"),
            (real.path.clone(), "common"),
        ])
        .unwrap();
        assert_eq!(selected.path, real.path);
        assert_eq!(selected.source, "common");
        assert!(selected.path.is_absolute());
        assert!(matches!(
            describe_git(&selected).unwrap(),
            GitEnvironment::Ready { .. }
        ));
    }
    /// 可执行脚本不能作为 Git 原生二进制使用。
    #[cfg(unix)]
    #[test]
    fn script_is_rejected() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("gitmaster-script-{}", std::process::id()));
        std::fs::write(&root, b"#!/bin/sh\nexit 0\n").unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(
            validate_candidate(&root).unwrap_err().code,
            "GIT_PATH_INVALID"
        );
        std::fs::remove_file(root).unwrap();
    }
    /// 中文和空格目录中的原生 Git 可通过手动路径检测。
    #[cfg(unix)]
    #[test]
    fn native_git_in_unicode_path() {
        let git = resolve_git(None).unwrap();
        let dir = std::env::temp_dir().join(format!("gitmaster-中文 路径-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join(if cfg!(windows) { "git.exe" } else { "git" });
        std::fs::copy(&git.path, &target).unwrap();
        let selected = resolve_git(Some(&target)).unwrap();
        assert_eq!(selected.source, "manual");
        assert_eq!(selected.version, git.version);
        assert!(selected.path.to_str().unwrap().contains("中文 路径"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
