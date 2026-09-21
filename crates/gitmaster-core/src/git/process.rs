use super::{GitExecutable, OperationError};
use std::{
    ffi::OsString,
    io::Read,
    path::Path,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};
const TIMEOUT: Duration = Duration::from_secs(10);
const STDERR_LIMIT: usize = 64 * 1024;

/// 内部进程输出；stderr 只供错误分类，不透传给前端。
#[derive(Debug)]
pub(crate) struct ProcessOutput {
    pub stdout: Vec<u8>,
    pub truncated: bool,
    pub success: bool,
    pub exit_code: Option<i32>,
    pub stderr: Vec<u8>,
}

/// 构建固定安全环境，清除继承的 Git 定位和配置注入。
fn command(git: &GitExecutable, cwd: &Path) -> Command {
    let mut command = Command::new(&git.path);
    command.current_dir(cwd);
    for (key, _) in std::env::vars_os().filter(|(key, _)| key.to_string_lossy().starts_with("GIT_"))
    {
        command.env_remove(key);
    }
    command
        .env("LC_ALL", "C")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("GIT_ALLOW_PROTOCOL", "")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_PAGER", "cat");
    command.args([
        "--no-pager",
        "--no-optional-locks",
        "-c",
        "core.fsmonitor=false",
        "-c",
        "core.hooksPath=",
        "-c",
        "diff.external=",
        "-c",
        "protocol.allow=never",
        "-c",
        "core.quotePath=true",
    ]);
    command
}

/// 运行只读 Git；禁用仓库配置的过滤进程并限制时间与输出。
pub(crate) fn run_git(
    git: &GitExecutable,
    cwd: &Path,
    args: &[OsString],
    limit: usize,
    allow_truncate: bool,
) -> Result<ProcessOutput, OperationError> {
    let mut cmd = command(git, cwd);
    // status/diff 的工作区转换也可能运行 clean/filter，逐项覆盖而不执行它们。
    if args.iter().any(|arg| arg == "status" || arg == "diff") {
        let mut config = command(git, cwd);
        config.args([
            "config",
            "--null",
            "--name-only",
            "--get-regexp",
            "^filter\\..*\\.(clean|smudge|process|required)$",
        ]);
        let keys = run_command(config, 65536, false, TIMEOUT)?;
        if !keys.success && keys.exit_code != Some(1) {
            return Err(OperationError::new("GIT_EXECUTION_FAILED"));
        }
        for key in keys.stdout.split(|b| *b == 0).filter(|key| !key.is_empty()) {
            let key = std::str::from_utf8(key).map_err(|_| OperationError::new("PARSE_FAILED"))?;
            cmd.arg("-c").arg(format!(
                "{key}={}",
                if key.ends_with(".required") {
                    "false"
                } else {
                    ""
                }
            ));
        }
    }
    cmd.args(args);
    run_command(cmd, limit, allow_truncate, TIMEOUT)
}

/// 有界并发读取两个管道，超限/超时立即终止并等待回收。
fn run_command(
    mut command: Command,
    limit: usize,
    allow_truncate: bool,
    timeout: Duration,
) -> Result<ProcessOutput, OperationError> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|_| OperationError::new("GIT_EXECUTION_FAILED"))?;
    let stdout = child.stdout.take().expect("已配置 stdout 管道");
    let stderr = child.stderr.take().expect("已配置 stderr 管道");
    let stop = Arc::new(AtomicBool::new(false));
    let out_stop = stop.clone();
    let err_stop = stop.clone();
    let out_thread = thread::spawn(move || read_limited(stdout, limit, out_stop));
    let err_thread = thread::spawn(move || read_limited(stderr, STDERR_LIMIT, err_stop));
    let started = Instant::now();
    let mut error = None;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {}
            Err(_) => {
                error = Some(OperationError::new("GIT_EXECUTION_FAILED"));
                break;
            }
        }
        if stop.load(Ordering::Acquire) {
            break;
        }
        if started.elapsed() >= timeout {
            error = Some(OperationError::new("TIMEOUT"));
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }
    // 对已退出子进程 kill 失败不影响 wait；所有退出路径均收敛于此。
    let _ = child.kill();
    let status = child.wait();
    let out = out_thread
        .join()
        .map_err(|_| OperationError::new("GIT_EXECUTION_FAILED"))?;
    let err = err_thread
        .join()
        .map_err(|_| OperationError::new("GIT_EXECUTION_FAILED"))?;
    if let Some(error) = error {
        return Err(error);
    }
    let (stdout, truncated) = out.map_err(|_| OperationError::new("GIT_EXECUTION_FAILED"))?;
    let (stderr, stderr_truncated) =
        err.map_err(|_| OperationError::new("GIT_EXECUTION_FAILED"))?;
    if stderr_truncated || (truncated && !allow_truncate) {
        return Err(OperationError::new("OUTPUT_LIMIT"));
    }
    let status = status.map_err(|_| OperationError::new("GIT_EXECUTION_FAILED"))?;
    Ok(ProcessOutput {
        exit_code: status.code(),
        stdout,
        truncated,
        stderr,
        success: status.success(),
    })
}

/// 持续排空管道但不保留超过上限的字节，通知父线程停止进程。
fn read_limited(
    mut reader: impl Read,
    limit: usize,
    stop: Arc<AtomicBool>,
) -> std::io::Result<(Vec<u8>, bool)> {
    let mut bytes = Vec::with_capacity(limit.min(8192));
    let mut buffer = [0; 8192];
    let mut truncated = false;
    loop {
        let size = match reader.read(&mut buffer) {
            Ok(n) => n,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => {
                stop.store(true, Ordering::Release);
                return Err(error);
            }
        };
        if size == 0 {
            break;
        }
        let kept = size.min(limit.saturating_sub(bytes.len()));
        bytes.extend_from_slice(&buffer[..kept]);
        if kept < size {
            truncated = true;
            stop.store(true, Ordering::Release);
        }
    }
    Ok((bytes, truncated))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// 测试子进程入口；普通测试运行不会执行，也不包含生产后门。
    #[test]
    #[ignore]
    fn process_fixture() {
        match std::env::var("GITMASTER_PROCESS_FIXTURE").as_deref() {
            Ok("timeout") => thread::sleep(Duration::from_secs(5)),
            Ok("flood") => {
                let worker = thread::spawn(|| {
                    let mut err = std::io::stderr();
                    for _ in 0..128 {
                        let _ = err.write_all(&[b'e'; 8192]);
                    }
                });
                let mut out = std::io::stdout();
                for _ in 0..128 {
                    let _ = out.write_all(&[b'o'; 8192]);
                }
                let _ = worker.join();
            }
            Ok("stdout") => {
                let mut out = std::io::stdout();
                loop {
                    if out.write_all(&[b'x'; 8192]).is_err() {
                        break;
                    }
                }
            }
            _ => {}
        }
    }

    /// 构造仅用于测试的当前测试程序子进程。
    fn fixture(mode: &str) -> Command {
        let mut c = Command::new(std::env::current_exe().unwrap());
        c.args([
            "--ignored",
            "--exact",
            "git::process::tests::process_fixture",
            "--nocapture",
        ])
        .env("GITMASTER_PROCESS_FIXTURE", mode);
        c
    }

    /// 真实等待超时会回收子进程，而不是等待其五秒自然结束。
    #[test]
    fn timeout_reaps_child() {
        let start = Instant::now();
        assert_eq!(
            run_command(fixture("timeout"), 4096, false, Duration::from_millis(100))
                .unwrap_err()
                .code,
            "TIMEOUT"
        );
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    /// 双管道洪泛不死锁，任一管道超限迅速结束。
    #[test]
    fn both_pipes_are_bounded() {
        let start = Instant::now();
        assert_eq!(
            run_command(fixture("flood"), 4096, false, Duration::from_secs(2))
                .unwrap_err()
                .code,
            "OUTPUT_LIMIT"
        );
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    /// 截断结果保留上限内内容且即时终止无限输出。
    #[test]
    fn truncated_output_terminates_process() {
        let out = run_command(fixture("stdout"), 4096, true, Duration::from_secs(2)).unwrap();
        assert_eq!(out.stdout.len(), 4096);
        assert!(out.truncated);
    }

    /// 配置继承通过构造命令检查，测试不修改全局环境。
    #[test]
    fn readonly_environment_and_no_network() {
        let git = GitExecutable {
            path: "git".into(),
            version: String::new(),
            source: "path".into(),
        };
        let c = command(&git, Path::new("."));
        let env: Vec<_> = c.get_envs().collect();
        assert!(env.contains(&(
            std::ffi::OsStr::new("GIT_ALLOW_PROTOCOL"),
            Some(std::ffi::OsStr::new(""))
        )));
        assert!(env.contains(&(
            std::ffi::OsStr::new("GIT_OPTIONAL_LOCKS"),
            Some(std::ffi::OsStr::new("0"))
        )));
    }
}
