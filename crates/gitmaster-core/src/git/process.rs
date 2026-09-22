use super::{GitExecutable, OperationError};
#[cfg(unix)]
use std::os::{fd::AsRawFd, unix::process::CommandExt};
#[cfg(not(any(unix, windows)))]
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::{
    ffi::OsString,
    io::{Read, Write},
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
const TIMEOUT: Duration = Duration::from_secs(10);
const STDERR_LIMIT: usize = 64 * 1024;
const STDIN_LIMIT: usize = 8 * 1024 * 1024;

/// Git 子进程的资源执行策略。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ExecutionPolicy {
    Read,
    LocalWrite,
    Network,
}

impl ExecutionPolicy {
    /// 返回策略的统一执行 deadline。
    fn timeout(self) -> Duration {
        match self {
            Self::Read => Duration::from_secs(10),
            Self::LocalWrite => Duration::from_secs(60),
            Self::Network => Duration::from_secs(15 * 60),
        }
    }
}

/// 内部进程输出；stderr 只供错误分类，不透传给前端。
#[derive(Debug)]
pub(crate) struct ProcessOutput {
    pub stdout: Vec<u8>,
    pub truncated: bool,
    pub success: bool,
    pub exit_code: Option<i32>,
    pub stderr: Vec<u8>,
}

#[cfg(windows)]
#[path = "process_windows.rs"]
mod windows;
#[cfg(windows)]
use windows::run as run_command_with_progress;

/// 返回平台的空设备，隔离配置时不使用用户可写文件。
#[cfg(any(unix, windows))]
fn null_device() -> &'static str {
    if cfg!(windows) {
        "NUL"
    } else {
        "/dev/null"
    }
}

/// 构建固定安全环境，清除继承的 Git 定位和配置注入。
fn command(git: &GitExecutable, cwd: &Path) -> Command {
    let mut command = Command::new(&git.path);
    command.current_dir(cwd);
    for (key, _) in std::env::vars_os().filter(|(key, _)| {
        key.to_string_lossy()
            .to_ascii_uppercase()
            .starts_with("GIT_")
    }) {
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
    run_git_with_policy(
        git,
        cwd,
        args,
        &[],
        ExecutionPolicy::Read,
        limit,
        allow_truncate,
    )
}

/// 按策略运行 Git，并在 stdin 中传入有界字节序列。
pub(crate) fn run_git_with_policy(
    git: &GitExecutable,
    cwd: &Path,
    args: &[OsString],
    stdin: &[u8],
    policy: ExecutionPolicy,
    limit: usize,
    allow_truncate: bool,
) -> Result<ProcessOutput, OperationError> {
    run_git_until(
        git,
        cwd,
        args,
        stdin,
        policy,
        limit,
        allow_truncate,
        Instant::now() + policy.timeout(),
    )
}

/// 带总期限的只读入口，配置探测和业务命令共享剩余预算。
pub(crate) fn run_git_read_until(
    git: &GitExecutable,
    cwd: &Path,
    args: &[OsString],
    limit: usize,
    allow_truncate: bool,
    deadline: Instant,
) -> Result<ProcessOutput, OperationError> {
    run_git_until(
        git,
        cwd,
        args,
        &[],
        ExecutionPolicy::Read,
        limit,
        allow_truncate,
        deadline,
    )
}

/// 将所有子查询限制在调用者提供的同一个截止时间内。
fn run_git_until(
    git: &GitExecutable,
    cwd: &Path,
    args: &[OsString],
    stdin: &[u8],
    policy: ExecutionPolicy,
    limit: usize,
    allow_truncate: bool,
    deadline: Instant,
) -> Result<ProcessOutput, OperationError> {
    if stdin.len() > STDIN_LIMIT {
        return Err(OperationError::new("INVALID_INPUT"));
    }
    let mut cmd = command(git, cwd);
    // status/diff 的工作区转换也可能运行 clean/filter，逐项覆盖而不执行它们。
    if policy == ExecutionPolicy::Read && args.iter().any(|arg| arg == "status" || arg == "diff") {
        let mut config = command(git, cwd);
        config.args([
            "config",
            "--null",
            "--name-only",
            "--get-regexp",
            "^filter\\..*\\.(clean|smudge|process|required)$",
        ]);
        let keys = run_command(config, 65536, false, remaining(deadline, TIMEOUT)?)?;
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
    if policy == ExecutionPolicy::Network {
        cmd.env("GIT_ALLOW_PROTOCOL", "https:ssh");
    }
    cmd.args(args);
    run_command_with_deadline(
        cmd,
        stdin,
        limit,
        allow_truncate,
        remaining(deadline, policy.timeout())?,
        policy == ExecutionPolicy::Network,
    )
}

/// 冲突读取只解析已捕获的私有索引副本，不重新打开变化中的真实索引。
pub(crate) fn read_captured_index(
    git: &GitExecutable,
    cwd: &Path,
    index: &Path,
    deadline: Instant,
) -> Result<ProcessOutput, OperationError> {
    let mut cmd = command(git, cwd);
    cmd.env("GIT_INDEX_FILE", index)
        .args(["ls-files", "--stage", "-z"]);
    run_command_with_deadline(
        cmd,
        &[],
        8 * 1024 * 1024,
        false,
        remaining(deadline, TIMEOUT)?,
        false,
    )
}

/// 本地业务执行器：临时索引由核心产生，统一截止时间覆盖准备和执行中的各个命令。
pub(crate) fn run_local_git(
    git: &GitExecutable,
    cwd: &Path,
    args: &[OsString],
    input: &[u8],
    index: Option<&Path>,
    deadline: Instant,
) -> Result<ProcessOutput, OperationError> {
    #[cfg(not(any(unix, windows)))]
    return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
    #[cfg(any(unix, windows))]
    {
        let timeout = deadline
            .checked_duration_since(Instant::now())
            .ok_or_else(|| OperationError::new("TIMEOUT"))?
            .min(ExecutionPolicy::LocalWrite.timeout());
        let mut cmd = command(git, cwd);
        cmd.env("GIT_CONFIG_NOSYSTEM", "0");
        if let Some(index) = index {
            cmd.env("GIT_INDEX_FILE", index);
        }
        cmd.args([
            "-c",
            "maintenance.auto=false",
            "-c",
            "gc.auto=0",
            "-c",
            "core.untrackedCache=false",
        ]);
        cmd.args(args);
        run_command_with_deadline(cmd, input, 8 * 1024 * 1024, false, timeout, false)
    }
}

/// 私有转换仓库不继承用户配置或属性，仅使用捕获的内建设置与对象存储。
pub(crate) fn run_isolated_git(
    git: &GitExecutable,
    cwd: &Path,
    args: &[OsString],
    input: &[u8],
    objects: Option<&Path>,
    settings: &[(String, String)],
    deadline: Instant,
) -> Result<ProcessOutput, OperationError> {
    #[cfg(not(any(unix, windows)))]
    return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
    #[cfg(any(unix, windows))]
    {
        let mut cmd = command(git, cwd);
        let null = if cfg!(windows) { "NUL" } else { "/dev/null" };
        cmd.env("GIT_CONFIG_GLOBAL", null)
            .env("GIT_CONFIG_SYSTEM", null)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_ATTR_NOSYSTEM", "1");
        if let Some(objects) = objects {
            cmd.env("GIT_OBJECT_DIRECTORY", objects);
        }
        for (key, value) in settings {
            cmd.arg("-c").arg(format!("{key}={value}"));
        }
        cmd.args([
            "-c",
            "core.attributesFile=",
            "-c",
            "core.splitIndex=false",
            "-c",
            "maintenance.auto=false",
            "-c",
            "gc.auto=0",
        ]);
        cmd.args(args);
        run_command_with_deadline(
            cmd,
            input,
            8 * 1024 * 1024,
            false,
            remaining(deadline, ExecutionPolicy::LocalWrite.timeout())?,
            false,
        )
    }
}

/// 分支检出隔离公共配置和属性，真实引用由上层持锁，HEAD/索引仍由 Git 更新。
pub(crate) fn run_worktree_git(
    git: &GitExecutable,
    repo: &super::RepositoryHandle,
    common: &Path,
    settings: &[(String, String)],
    args: &[OsString],
    deadline: Instant,
) -> Result<ProcessOutput, OperationError> {
    #[cfg(not(any(unix, windows)))]
    return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
    #[cfg(any(unix, windows))]
    {
        let mut cmd = command(git, &repo.root);
        cmd.env("GIT_DIR", &repo.git_dir)
            .env("GIT_WORK_TREE", &repo.root)
            .env("GIT_COMMON_DIR", common)
            .env("GIT_OBJECT_DIRECTORY", repo.common_dir.join("objects"))
            .env("GIT_CONFIG_GLOBAL", null_device())
            .env("GIT_CONFIG_SYSTEM", null_device())
            .env("GIT_ATTR_NOSYSTEM", "1")
            .env("GIT_NO_REPLACE_OBJECTS", "1");
        for (key, value) in settings {
            cmd.arg("-c").arg(format!("{key}={value}"));
        }
        cmd.args([
            "-c",
            "core.bare=false",
            "-c",
            "extensions.worktreeConfig=false",
            "-c",
            "core.attributesFile=",
            "-c",
            "core.excludesFile=",
            "-c",
            "core.splitIndex=false",
            "-c",
            "core.untrackedCache=false",
            "-c",
            "maintenance.auto=false",
            "-c",
            "gc.auto=0",
        ]);
        cmd.args(args);
        run_command_with_deadline(
            cmd,
            &[],
            8 * 1024 * 1024,
            false,
            remaining(deadline, ExecutionPolicy::LocalWrite.timeout())?,
            false,
        )
    }
}

/// 执行受限网络 Git 操作；凭据相关设置只能由上层筛选后传入。
pub(crate) fn run_network_git(
    git: &GitExecutable,
    cwd: &Path,
    args: &[OsString],
    objects: Option<&Path>,
    settings: &[(String, String)],
    deadline: Instant,
    on_progress: &mut dyn FnMut(u64, u64),
) -> Result<ProcessOutput, OperationError> {
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (git, cwd, args, objects, settings, deadline, on_progress);
        return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
    }
    #[cfg(any(unix, windows))]
    {
        let mut cmd = network_command(git, cwd, objects, settings);
        cmd.args(args);
        run_command_with_progress(
            cmd,
            &[],
            1024 * 1024,
            false,
            remaining(deadline, ExecutionPolicy::Network.timeout())?,
            true,
            on_progress,
        )
    }
}

/// 将认证值放入受控子进程环境，进程参数和临时文件中不出现凭据。
#[cfg(any(unix, windows))]
fn network_command(
    git: &GitExecutable,
    cwd: &Path,
    objects: Option<&Path>,
    settings: &[(String, String)],
) -> Command {
    let mut cmd = command(git, cwd);
    cmd.env("GIT_CONFIG_GLOBAL", null_device())
        .env("GIT_CONFIG_SYSTEM", null_device())
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_ATTR_NOSYSTEM", "1")
        .env("GIT_ALLOW_PROTOCOL", "https:ssh")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ASKPASS", "")
        .env("SSH_ASKPASS", "")
        .env("SSH_ASKPASS_REQUIRE", "never")
        .env(
            "GIT_SSH_COMMAND",
            "ssh -oBatchMode=yes -oStrictHostKeyChecking=yes",
        )
        .env("GIT_SSH_VARIANT", "ssh");
    if let Some(objects) = objects {
        cmd.env("GIT_OBJECT_DIRECTORY", objects);
    }
    // 捕获配置可能包含认证 header，不把值放进可见的进程参数或临时文件。
    cmd.env("GIT_CONFIG_COUNT", settings.len().to_string());
    for (index, (key, value)) in settings.iter().enumerate() {
        cmd.env(format!("GIT_CONFIG_KEY_{index}"), key)
            .env(format!("GIT_CONFIG_VALUE_{index}"), value);
    }
    cmd.args([
        "-c",
        "protocol.allow=never",
        "-c",
        "protocol.https.allow=always",
        "-c",
        "protocol.ssh.allow=always",
        "-c",
        "http.sslVerify=true",
        "-c",
        "http.saveCookies=false",
        "-c",
        "credential.interactive=false",
        "-c",
        "core.askPass=",
        "-c",
        "fetch.fsckObjects=true",
        "-c",
        "transfer.fsckObjects=true",
        "-c",
        "maintenance.auto=false",
        "-c",
        "gc.auto=0",
    ]);
    cmd
}

/// 读取带 scope 的配置原文，供上层区分 trusted 配置来源。
pub(crate) fn inspect_scoped_config(
    git: &GitExecutable,
    cwd: &Path,
    deadline: Instant,
) -> Result<Vec<u8>, OperationError> {
    let mut cmd = Command::new(&git.path);
    cmd.current_dir(cwd);
    for (key, _) in std::env::vars_os().filter(|(key, _)| {
        key.to_string_lossy()
            .to_ascii_uppercase()
            .starts_with("GIT_")
    }) {
        cmd.env_remove(key);
    }
    cmd.env("LC_ALL", "C")
        .env("GIT_CONFIG_NOSYSTEM", "0")
        .args(["config", "--includes", "--null", "--show-scope", "--list"]);
    let out = run_command(cmd, 1024 * 1024, false, remaining(deadline, TIMEOUT)?)?;
    if out.success {
        Ok(out.stdout)
    } else {
        Err(OperationError::new("GIT_EXECUTION_FAILED"))
    }
}

/// 解析 Git 本地收发对象的进度，不向调用方透传 stderr 文本。
#[cfg(any(unix, windows))]
fn parse_progress(stderr: &[u8], callback: &mut dyn FnMut(u64, u64)) {
    for line in stderr.split(|byte| *byte == b'\n' || *byte == b'\r') {
        let text = String::from_utf8_lossy(line);
        let Some(rest) = text
            .strip_prefix("Receiving objects: ")
            .or_else(|| text.strip_prefix("Writing objects: "))
        else {
            continue;
        };
        let Some(percent_end) = rest.find('%') else {
            continue;
        };
        if !rest[..percent_end]
            .trim()
            .parse::<u8>()
            .is_ok_and(|value| value <= 100)
        {
            continue;
        }
        let Some(open) = rest[percent_end..].find('(') else {
            continue;
        };
        let Some(close) = rest[percent_end + open..].find(')') else {
            continue;
        };
        let pair = &rest[percent_end + open + 1..percent_end + open + close];
        let Some((done, total)) = pair.split_once('/') else {
            continue;
        };
        let Ok(done) = done.trim().parse::<u64>() else {
            continue;
        };
        let Ok(total) = total.trim().parse::<u64>() else {
            continue;
        };
        if total > 0 && done <= total {
            callback(done, total);
        }
    }
}

/// 只读取有效配置，不用安全覆盖参数遮住用户要求的 hook/签名设置。
pub(crate) fn inspect_local_config(
    git: &GitExecutable,
    cwd: &Path,
    deadline: Instant,
) -> Result<Vec<u8>, OperationError> {
    let mut cmd = Command::new(&git.path);
    cmd.current_dir(cwd);
    for (key, _) in std::env::vars_os().filter(|(key, _)| {
        key.to_string_lossy()
            .to_ascii_uppercase()
            .starts_with("GIT_")
    }) {
        cmd.env_remove(key);
    }
    cmd.env("LC_ALL", "C")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .args(["config", "--includes", "--null", "--list"]);
    let timeout = deadline
        .checked_duration_since(Instant::now())
        .ok_or_else(|| OperationError::new("TIMEOUT"))?
        .min(TIMEOUT);
    let out = run_command(cmd, 1024 * 1024, false, timeout)?;
    if out.success {
        Ok(out.stdout)
    } else {
        Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"))
    }
}

/// 超过总期限直接拒绝，单个进程同时受策略上限限制。
fn remaining(deadline: Instant, maximum: Duration) -> Result<Duration, OperationError> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|duration| !duration.is_zero())
        .map(|duration| duration.min(maximum))
        .ok_or_else(|| OperationError::new("TIMEOUT"))
}

/// 支持平台共用策略入口，各平台执行器均负责有界管道与进程树回收。
#[cfg(any(unix, windows))]
fn run_command_with_deadline(
    command: Command,
    stdin: &[u8],
    limit: usize,
    allow_truncate: bool,
    timeout: Duration,
    network: bool,
) -> Result<ProcessOutput, OperationError> {
    run_command_with_progress(
        command,
        stdin,
        limit,
        allow_truncate,
        timeout,
        network,
        &mut |_, _| {},
    )
}

/// 网络进度在管道消费时通知业务层，普通读取仍使用相同资源回收路径。
#[cfg(unix)]
fn run_command_with_progress(
    mut command: Command,
    stdin: &[u8],
    limit: usize,
    allow_truncate: bool,
    timeout: Duration,
    network: bool,
    on_progress: &mut dyn FnMut(u64, u64),
) -> Result<ProcessOutput, OperationError> {
    if stdin.len() > STDIN_LIMIT {
        return Err(OperationError::new("INVALID_INPUT"));
    }
    let deadline = Instant::now() + timeout;
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command.process_group(0);
    let mut child = command
        .spawn()
        .map_err(|_| OperationError::new("GIT_EXECUTION_FAILED"))?;
    let mut input = child.stdin.take();
    let mut stdout = child.stdout.take().expect("已配置 stdout 管道");
    let mut stderr = child.stderr.take().expect("已配置 stderr 管道");
    let mut output = PipeOutput::default();
    let mut errors = PipeOutput::default();
    let mut progress = NetworkProgress::default();
    let output_limit = if network {
        limit.min(1024 * 1024)
    } else {
        limit
    };
    let mut written = 0;
    let mut status = None;
    let run = (|| -> Result<(), OperationError> {
        nonblocking(input.as_ref().expect("已配置 stdin 管道"))?;
        nonblocking(&stdout)?;
        nonblocking(&stderr)?;
        loop {
            if Instant::now() >= deadline {
                return Err(OperationError::new("TIMEOUT"));
            }
            if written == stdin.len() {
                input = None;
            }
            if let Some(pipe) = &mut input {
                match pipe.write(&stdin[written..stdin.len().min(written + 65536)]) {
                    Ok(0) => return Err(OperationError::new("GIT_EXECUTION_FAILED")),
                    Ok(count) => written += count,
                    Err(error)
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                        ) => {}
                    Err(_) => return Err(OperationError::new("GIT_EXECUTION_FAILED")),
                }
            }
            drain_pipe(&mut stdout, &mut output, output_limit)?;
            if network {
                drain_network_stderr(&mut stderr, &mut errors, &mut progress, on_progress)?;
            } else {
                drain_pipe(&mut stderr, &mut errors, STDERR_LIMIT)?;
            }
            if (!network && errors.truncated) || output.truncated {
                return Ok(());
            }
            if status.is_none() {
                status = child
                    .try_wait()
                    .map_err(|_| OperationError::new("GIT_EXECUTION_FAILED"))?;
            }
            if status.is_some() && output.closed && errors.closed && input.is_none() {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(2));
        }
    })();
    // 所有路径都关闭输入、终止本执行器创建的进程组，并等待直接子进程。
    drop(input);
    unsafe {
        libc::kill(-(child.id() as i32), libc::SIGKILL);
    }
    let _ = child.kill();
    let waited = child
        .wait()
        .map_err(|_| OperationError::new("GIT_EXECUTION_FAILED"));
    run?;
    let status = status.map(Ok).unwrap_or(waited)?;
    if (!network && errors.truncated) || (output.truncated && !allow_truncate) {
        return Err(OperationError::new("OUTPUT_LIMIT"));
    }
    Ok(ProcessOutput {
        stdout: output.bytes,
        stderr: errors.bytes,
        truncated: output.truncated,
        success: status.success(),
        exit_code: status.code(),
    })
}

/// 当前管道的有界数据与结束状态。
#[cfg(any(unix, windows))]
#[derive(Default)]
struct PipeOutput {
    bytes: Vec<u8>,
    truncated: bool,
    closed: bool,
}

/// 网络行解析最多保留 8 KiB；超长行整行丢弃后从下个边界恢复。
#[cfg(any(unix, windows))]
#[derive(Default)]
struct NetworkProgress {
    line: Vec<u8>,
    discarding: bool,
    last: Option<(u64, u64)>,
}

#[cfg(any(unix, windows))]
impl NetworkProgress {
    /// 同时处理 CR 动态行和 LF 消息，回调只携带可信格式中的数字。
    fn consume(&mut self, bytes: &[u8], callback: &mut dyn FnMut(u64, u64)) {
        for byte in bytes {
            if matches!(*byte, b'\r' | b'\n') {
                if !self.discarding {
                    parse_progress(&self.line, &mut |done, total| {
                        if self
                            .last
                            .is_none_or(|(previous, expected)| expected != total || done > previous)
                        {
                            self.last = Some((done, total));
                            callback(done, total);
                        }
                    });
                }
                self.line.clear();
                self.discarding = false;
            } else if !self.discarding {
                if self.line.len() == 8192 {
                    self.line.clear();
                    self.discarding = true;
                } else {
                    self.line.push(*byte);
                }
            }
        }
    }
}

/// 网络 stderr 持续消费并保留有界尾部，不把正常进度累计量当作输出超限。
#[cfg(unix)]
fn drain_network_stderr(
    reader: &mut impl Read,
    output: &mut PipeOutput,
    progress: &mut NetworkProgress,
    callback: &mut dyn FnMut(u64, u64),
) -> Result<(), OperationError> {
    if output.closed {
        return Ok(());
    }
    let mut buffer = [0; 8192];
    for _ in 0..32 {
        match reader.read(&mut buffer) {
            Ok(0) => {
                progress.consume(b"\n", callback);
                output.closed = true;
                return Ok(());
            }
            Ok(size) => {
                progress.consume(&buffer[..size], callback);
                let excess = (output.bytes.len() + size).saturating_sub(STDERR_LIMIT);
                output.bytes.drain(..excess);
                output.bytes.extend_from_slice(&buffer[..size]);
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(OperationError::new("GIT_EXECUTION_FAILED")),
        }
    }
    Ok(())
}

/// 对已拥有的管道启用非阻塞访问，不改变子进程另一侧的描述符。
#[cfg(unix)]
fn nonblocking(pipe: &impl AsRawFd) -> Result<(), OperationError> {
    let descriptor = pipe.as_raw_fd();
    let result = unsafe {
        let flags = libc::fcntl(descriptor, libc::F_GETFL);
        if flags < 0 {
            -1
        } else {
            libc::fcntl(descriptor, libc::F_SETFL, flags | libc::O_NONBLOCK)
        }
    };
    if result < 0 {
        Err(OperationError::new("GIT_EXECUTION_FAILED"))
    } else {
        Ok(())
    }
}

/// 每轮排空有限批次，防止连续输出饿死另一管道或超时检查。
#[cfg(unix)]
fn drain_pipe(
    reader: &mut impl Read,
    output: &mut PipeOutput,
    limit: usize,
) -> Result<(), OperationError> {
    if output.closed || output.truncated {
        return Ok(());
    }
    let mut buffer = [0; 8192];
    for _ in 0..32 {
        match reader.read(&mut buffer) {
            Ok(0) => {
                output.closed = true;
                return Ok(());
            }
            Ok(size) => {
                let kept = size.min(limit.saturating_sub(output.bytes.len()));
                output.bytes.extend_from_slice(&buffer[..kept]);
                if kept < size {
                    output.truncated = true;
                    return Ok(());
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(OperationError::new("GIT_EXECUTION_FAILED")),
        }
    }
    Ok(())
}

/// 兼容原有读取调用，共用同一个限时执行循环。
fn run_command(
    command: Command,
    limit: usize,
    allow_truncate: bool,
    timeout: Duration,
) -> Result<ProcessOutput, OperationError> {
    run_command_with_deadline(command, &[], limit, allow_truncate, timeout, false)
}

/// 其他未支持平台保留 M1 读取兼容路径。
#[cfg(not(any(unix, windows)))]
fn run_command_with_deadline(
    command: Command,
    stdin: &[u8],
    limit: usize,
    allow_truncate: bool,
    deadline: Duration,
    _network: bool,
) -> Result<ProcessOutput, OperationError> {
    if !stdin.is_empty() {
        return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
    }
    run_command_fallback(command, limit, allow_truncate, deadline)
}

/// 非 Unix 的 M1 读取兼容路径。
#[cfg(not(any(unix, windows)))]
fn run_command_fallback(
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
#[cfg(not(any(unix, windows)))]
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

    /// 总期限已经结束时，不尝试启动另一个 Git 子查询。
    #[test]
    fn exhausted_read_budget_does_not_spawn_next_query() {
        let git = GitExecutable {
            path: "/gitmaster-does-not-exist".into(),
            version: String::new(),
            source: "test".into(),
        };
        let error = run_git_read_until(
            &git,
            Path::new("."),
            &["status".into()],
            4096,
            false,
            Instant::now(),
        )
        .unwrap_err();
        assert_eq!(error.code, "TIMEOUT");
    }

    /// stdin 必须完整传送，子进程收到 EOF 后返回真实校验结果。
    #[test]
    fn policy_writes_and_closes_stdin() {
        let output = run_command_with_deadline(
            fixture("stdin"),
            b"input",
            4096,
            false,
            Duration::from_secs(2),
            false,
        )
        .unwrap();
        assert!(output.success);
        assert!(String::from_utf8_lossy(&output.stdout).contains("stdin-verified"));
    }

    /// 未消费的大 stdin 不得让写线程越过统一期限。
    #[test]
    fn stdin_not_consumed_times_out() {
        let start = Instant::now();
        let input = vec![b'x'; STDIN_LIMIT];
        let result = run_command_with_deadline(
            fixture("timeout"),
            &input,
            4096,
            false,
            Duration::from_millis(100),
            false,
        );
        assert_eq!(result.unwrap_err().code, "TIMEOUT");
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    /// 父程序正常退出后，后代持有管道仍受期限约束。
    #[cfg(unix)]
    #[test]
    fn normal_parent_exit_with_descendant_holding_pipes() {
        let start = Instant::now();
        let result = run_command_with_deadline(
            fixture("descendant"),
            &[],
            4096,
            false,
            Duration::from_millis(100),
            false,
        );
        assert_eq!(result.unwrap_err().code, "TIMEOUT");
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    /// 新旧调用入口都必须保留允许截断的语义。
    #[test]
    fn policy_allows_bounded_truncation() {
        let output = run_command_with_deadline(
            fixture("stdout"),
            &[],
            4096,
            true,
            Duration::from_secs(2),
            false,
        )
        .unwrap();
        assert_eq!(output.stdout.len(), 4096);
        assert!(output.truncated);
    }

    /// 测试子进程入口；普通测试运行不会执行，也不包含生产后门。
    #[test]
    #[ignore]
    fn process_fixture() {
        match std::env::var("GITMASTER_PROCESS_FIXTURE").as_deref() {
            Ok("network-progress") => {
                let mut err = std::io::stderr();
                err.write_all(b"start-only-marker\n").unwrap();
                for done in 0..=5000 {
                    write!(err, "Receiving objects: {}% ({done}/5000)\r", done / 50).unwrap();
                }
                err.write_all(&[b'x'; 20000]).unwrap();
                err.write_all(b"Receiving objects: 100% (2/2)\rremote: Receiving objects: 100% (9/9)\nReceiving objects: wrong% (3/3)\nReceiving objects: 100% (6/4)\nReceiving objects: 100% (5001/5001)\rtail-only-marker\n").unwrap();
            }
            Ok("network-progress-gate") => {
                std::io::stderr()
                    .write_all(b"Receiving objects: 50% (1/2)\r")
                    .unwrap();
                let path =
                    std::path::PathBuf::from(std::env::var_os("GITMASTER_PROGRESS_GATE").unwrap());
                let until = Instant::now() + Duration::from_secs(2);
                while !path.exists() {
                    assert!(Instant::now() < until, "未收到实时进度确认");
                    thread::sleep(Duration::from_millis(2));
                }
                std::io::stderr()
                    .write_all(b"Receiving objects: 100% (2/2)\n")
                    .unwrap();
            }
            Ok("timeout") => thread::sleep(Duration::from_secs(5)),
            Ok("stdin") => {
                let mut bytes = Vec::new();
                std::io::stdin().read_to_end(&mut bytes).unwrap();
                assert_eq!(bytes, b"input");
                println!("stdin-verified");
            }
            #[cfg(unix)]
            Ok("descendant") => {
                let _child = fixture("timeout")
                    .stdin(Stdio::inherit())
                    .stdout(Stdio::inherit())
                    .stderr(Stdio::inherit())
                    .spawn()
                    .unwrap();
            }
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

    /// 上传只采用本地 Writing objects 计数，忽略压缩、远端文本和重复倒退计数。
    #[cfg(unix)]
    #[test]
    fn network_progress_accepts_push_object_counts() {
        let mut parser = NetworkProgress::default();
        let mut counts = Vec::new();
        parser.consume(b"Compressing objects: 100% (8/8)\rremote: Writing objects: 100% (8/8)\rWriting objects: 25% (2/8)\rWriting objects: 25% (2/8)\rWriting objects: 12% (1/8)\rWriting objects: 100% (8/8), 1.00 KiB | 1 MiB/s, done.\n", &mut |done, total| counts.push((done, total)));
        assert_eq!(counts, [(2, 8), (8, 8)]);
    }

    /// 超过 64 KiB 的进度和超长行持续排空，只保留尾部且能恢复解析后续有效行。
    #[cfg(unix)]
    #[test]
    fn network_progress_stream_is_bounded_and_discards_invalid_lines() {
        let mut counts = Vec::new();
        let output = run_command_with_progress(
            fixture("network-progress"),
            &[],
            1024 * 1024,
            false,
            Duration::from_secs(5),
            true,
            &mut |done, total| counts.push((done, total)),
        )
        .unwrap();
        assert!(output.success);
        assert_eq!(output.stderr.len(), STDERR_LIMIT);
        assert!(!String::from_utf8_lossy(&output.stderr).contains("start-only-marker"));
        assert!(String::from_utf8_lossy(&output.stderr).contains("tail-only-marker"));
        assert_eq!(counts.len(), 5002);
        assert_eq!(counts.last(), Some(&(5001, 5001)));
        assert!(
            !counts.contains(&(2, 2)) && !counts.contains(&(9, 9)) && !counts.contains(&(3, 3))
        );
    }

    /// 子进程必须收到进度回调的响应才退出，防止实现退化为结束后一次性解析。
    #[cfg(unix)]
    #[test]
    fn network_progress_callback_runs_before_child_exits() {
        let temporary = tempfile::tempdir().unwrap();
        let gate = temporary.path().join("continue");
        let mut cmd = fixture("network-progress-gate");
        cmd.env("GITMASTER_PROGRESS_GATE", &gate);
        let result = run_command_with_progress(
            cmd,
            &[],
            1024 * 1024,
            false,
            Duration::from_secs(4),
            true,
            &mut |done, _| {
                if done == 1 {
                    std::fs::write(&gate, b"continue").unwrap();
                }
            },
        )
        .unwrap();
        assert!(result.success);
        assert!(gate.exists());
    }

    /// 网络持续输出和后代持管道同样受输出限额及截止时间约束。
    #[cfg(unix)]
    #[test]
    fn network_stdout_and_descendants_cannot_escape_limits() {
        assert_eq!(
            run_command_with_progress(
                fixture("stdout"),
                &[],
                1024 * 1024,
                false,
                Duration::from_secs(2),
                true,
                &mut |_, _| {}
            )
            .unwrap_err()
            .code,
            "OUTPUT_LIMIT"
        );
        let start = Instant::now();
        assert_eq!(
            run_command_with_progress(
                fixture("descendant"),
                &[],
                1024 * 1024,
                false,
                Duration::from_millis(100),
                true,
                &mut |_, _| {}
            )
            .unwrap_err()
            .code,
            "TIMEOUT"
        );
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    /// 实际 Git 进程看到强制 TLS、非交互 SSH 与捕获 helper，配置来源仍可区分。
    #[cfg(unix)]
    #[test]
    fn network_git_applies_fixed_configuration_and_scoped_reader_reports_local() {
        let fixture = crate::git::repository::tests::Fixture::new();
        let deadline = Instant::now() + Duration::from_secs(10);
        let settings = vec![
            ("http.sslVerify".into(), "false".into()),
            ("credential.helper".into(), "".into()),
            ("credential.helper".into(), "test-helper".into()),
            (
                "http.extraHeader".into(),
                "Authorization: Basic test-only-secret".into(),
            ),
        ];
        let cmd = network_command(&fixture.git, &fixture.root, None, &settings);
        assert!(!cmd
            .get_args()
            .any(|arg| arg.to_string_lossy().contains("test-only-secret")));
        let output = run_network_git(
            &fixture.git,
            &fixture.root,
            &["config".into(), "--get".into(), "http.sslVerify".into()],
            None,
            &settings,
            deadline,
            &mut |_, _| {},
        )
        .unwrap();
        assert!(output.success);
        assert_eq!(output.stdout, b"true\n");
        let header = run_network_git(
            &fixture.git,
            &fixture.root,
            &["config".into(), "--get".into(), "http.extraheader".into()],
            None,
            &settings,
            deadline,
            &mut |_, _| {},
        )
        .unwrap();
        assert!(header.success);
        assert_eq!(header.stdout, b"Authorization: Basic test-only-secret\n");
        let scoped = inspect_scoped_config(&fixture.git, &fixture.root, deadline).unwrap();
        assert!(scoped
            .windows(b"local\0user.name\n".len())
            .any(|window| window == b"local\0user.name\n"));
    }
}
