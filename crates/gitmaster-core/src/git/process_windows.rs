use super::{
    NetworkProgress, OperationError, PipeOutput, ProcessOutput, STDERR_LIMIT, STDIN_LIMIT,
};
use crate::git::windows_arguments::quote_argument;
use std::{
    ffi::OsStr,
    mem::{size_of, zeroed},
    os::windows::ffi::OsStrExt,
    process::Command,
    ptr::{null, null_mut},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use windows_sys::Win32::{
    Foundation::*,
    Globalization::CompareStringOrdinal,
    Security::SECURITY_ATTRIBUTES,
    Storage::FileSystem::*,
    System::{
        JobObjects::*,
        Pipes::*,
        SystemServices::{GENERIC_READ, GENERIC_WRITE},
        Threading::*,
    },
};
static PIPE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// 独占 Windows 句柄，所有提前返回路径都关闭自己创建的资源。
struct Handle(HANDLE);
impl Handle {
    /// 统一拒绝 Win32 的两种无效句柄，不读取或记录系统错误文本。
    fn owned(raw: HANDLE) -> Result<Self, OperationError> {
        if raw == 0 || raw == INVALID_HANDLE_VALUE {
            Err(failed())
        } else {
            Ok(Self(raw))
        }
    }
}
impl Drop for Handle {
    /// 仅关闭当前对象拥有的句柄。
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
/// Job 句柄不向子进程继承，关闭时终止所有受管后代。
struct Job {
    handle: Handle,
}
impl Job {
    /// 请求整树终止并限时核对 Job 内已无进程，不能仅等待直接父进程。
    fn terminate_and_wait(&self) -> Result<(), OperationError> {
        if unsafe { TerminateJobObject(self.handle.0, 1) } == 0 {
            return Err(OperationError::new("WRITE_OUTCOME_UNKNOWN"));
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let mut information: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = unsafe { zeroed() };
            if unsafe {
                QueryInformationJobObject(
                    self.handle.0,
                    JobObjectBasicAccountingInformation,
                    (&mut information as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
                    size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                    null_mut(),
                )
            } == 0
            {
                return Err(OperationError::new("WRITE_OUTCOME_UNKNOWN"));
            }
            if information.ActiveProcesses == 0 {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(OperationError::new("WRITE_OUTCOME_UNKNOWN"));
            }
            thread::sleep(Duration::from_millis(2));
        }
    }
    /// 创建禁用 breakaway 的独立 Job，进程必须先加入再恢复执行。
    fn create() -> Result<Self, OperationError> {
        let handle = Handle::owned(unsafe { CreateJobObjectW(null(), null()) })?;
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if unsafe {
            SetInformationJobObject(
                handle.0,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        } == 0
        {
            return Err(failed());
        }
        Ok(Self { handle })
    }
}
impl Drop for Job {
    /// 正常退出、超时、输出超限均回收整个进程树，句柄关闭提供第二重保障。
    fn drop(&mut self) {
        unsafe {
            TerminateJobObject(self.handle.0, 1);
        }
    }
}
/// 属性列表的内存按指针宽度对齐，句柄数组必须比此对象活得更久。
struct Attributes {
    storage: Vec<usize>,
    initialized: bool,
}
impl Attributes {
    /// 限制子进程只继承三个标准管道，不继承桌面进程的其他句柄。
    fn create(handles: &[HANDLE; 3]) -> Result<Self, OperationError> {
        let mut bytes = 0;
        unsafe {
            InitializeProcThreadAttributeList(null_mut(), 1, 0, &mut bytes);
        }
        if bytes == 0 || bytes > 65536 {
            return Err(failed());
        }
        let mut value = Self {
            storage: vec![0; bytes.div_ceil(size_of::<usize>())],
            initialized: false,
        };
        if unsafe { InitializeProcThreadAttributeList(value.pointer(), 1, 0, &mut bytes) } == 0 {
            return Err(failed());
        }
        value.initialized = true;
        if unsafe {
            UpdateProcThreadAttribute(
                value.pointer(),
                0,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                handles.as_ptr().cast(),
                size_of::<[HANDLE; 3]>(),
                null_mut(),
                null(),
            )
        } == 0
        {
            return Err(failed());
        }
        Ok(value)
    }
    /// 返回稳定堆内存的属性列表地址。
    fn pointer(&mut self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        self.storage.as_mut_ptr().cast()
    }
}
impl Drop for Attributes {
    /// 释放列表内部资源；底层分配随 Vec 一起回收。
    fn drop(&mut self) {
        if self.initialized {
            unsafe {
                DeleteProcThreadAttributeList(self.pointer());
            }
        }
    }
}
/// 返回稳定错误码，不将 Win32 消息或命令行输出到日志。
fn failed() -> OperationError {
    OperationError::new("GIT_EXECUTION_FAILED")
}
/// Windows 原生 UTF-16 输入可以保留未配对代理项，但不能嵌入 NUL。
fn wide(value: &OsStr) -> Result<Vec<u16>, OperationError> {
    let mut units: Vec<u16> = value.encode_wide().collect();
    if units.contains(&0) {
        return Err(OperationError::new("INVALID_INPUT"));
    }
    units.push(0);
    Ok(units)
}
/// 比较环境键使用 Windows 不区分大小写的 ordinal 语义。
fn compare_keys(left: &[u16], right: &[u16]) -> std::cmp::Ordering {
    let result = unsafe {
        CompareStringOrdinal(
            left.as_ptr(),
            left.len() as i32,
            right.as_ptr(),
            right.len() as i32,
            1,
        )
    };
    result.cmp(&2)
}
/// 核心命令继承系统环境，再应用每个明确设置或删除的键；不支持 env_clear 调用。
fn environment(command: &Command) -> Result<Vec<u16>, OperationError> {
    let mut values: Vec<(Vec<u16>, Vec<u16>)> = std::env::vars_os()
        .map(|(key, value)| (key.encode_wide().collect(), value.encode_wide().collect()))
        .collect();
    for (key, value) in command.get_envs() {
        let key: Vec<u16> = key.encode_wide().collect();
        values.retain(|(existing, _)| compare_keys(existing, &key) != std::cmp::Ordering::Equal);
        if let Some(value) = value {
            values.push((key, value.encode_wide().collect()));
        }
    }
    values.sort_by(|(left, _), (right, _)| compare_keys(left, right));
    let mut block = Vec::new();
    for (key, value) in values {
        if key.contains(&0) || value.contains(&0) {
            return Err(OperationError::new("INVALID_INPUT"));
        }
        block.extend(key);
        block.push(b'=' as u16);
        block.extend(value);
        block.push(0);
    }
    if block.is_empty() {
        block.push(0);
    }
    block.push(0);
    Ok(block)
}
/// 父端非阻塞、子端阻塞的字节管道，不启动无法限时 join 的读取线程。
fn pipe(parent_writes: bool) -> Result<(Handle, Handle), OperationError> {
    let sequence = PIPE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| failed())?
        .as_nanos();
    let name = wide(OsStr::new(&format!(
        r"\\.\pipe\gitmaster-{}-{time}-{sequence}",
        std::process::id()
    )))?;
    let parent = Handle::owned(unsafe {
        CreateNamedPipeW(
            name.as_ptr(),
            (if parent_writes {
                PIPE_ACCESS_OUTBOUND
            } else {
                PIPE_ACCESS_INBOUND
            }) | FILE_FLAG_FIRST_PIPE_INSTANCE,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_NOWAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            65536,
            65536,
            0,
            null(),
        )
    })?;
    let security = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: null_mut(),
        bInheritHandle: 1,
    };
    let child = Handle::owned(unsafe {
        CreateFileW(
            name.as_ptr(),
            if parent_writes {
                GENERIC_READ
            } else {
                GENERIC_WRITE
            },
            0,
            &security,
            OPEN_EXISTING,
            0,
            0,
        )
    })?;
    if unsafe { ConnectNamedPipe(parent.0, null_mut()) } == 0
        && unsafe { GetLastError() } != ERROR_PIPE_CONNECTED
    {
        return Err(failed());
    }
    Ok((parent, child))
}
/// 从非阻塞管道消费有限批次，输出和网络尾缓冲都保持明确上限。
fn drain(
    handle: &Handle,
    output: &mut PipeOutput,
    limit: usize,
    network: bool,
    progress: &mut NetworkProgress,
    callback: &mut dyn FnMut(u64, u64),
) -> Result<(), OperationError> {
    if output.closed || output.truncated {
        return Ok(());
    }
    let mut buffer = [0u8; 8192];
    for _ in 0..32 {
        let mut read = 0;
        let ok = unsafe {
            ReadFile(
                handle.0,
                buffer.as_mut_ptr().cast(),
                buffer.len() as u32,
                &mut read,
                null_mut(),
            )
        };
        if ok == 0 {
            match unsafe { GetLastError() } {
                ERROR_NO_DATA => return Ok(()),
                ERROR_BROKEN_PIPE | ERROR_PIPE_NOT_CONNECTED => {
                    output.closed = true;
                    if network {
                        progress.consume(b"\n", callback);
                    }
                    return Ok(());
                }
                _ => return Err(failed()),
            }
        }
        if read == 0 {
            return Ok(());
        }
        let bytes = &buffer[..read as usize];
        if network {
            progress.consume(bytes, callback);
            let excess = (output.bytes.len() + bytes.len()).saturating_sub(limit);
            output.bytes.drain(..excess);
            output.bytes.extend_from_slice(bytes);
        } else {
            let kept = bytes.len().min(limit.saturating_sub(output.bytes.len()));
            output.bytes.extend_from_slice(&bytes[..kept]);
            if kept < bytes.len() {
                output.truncated = true;
                return Ok(());
            }
        }
    }
    Ok(())
}
/// 挂起创建 Git、加入 Job 后才运行，统一轮询 stdin、双输出和截止时间。
pub(super) fn run(
    command: Command,
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
    let application = wide(command.get_program())?;
    let cwd = command
        .get_current_dir()
        .map(|path| wide(path.as_os_str()))
        .transpose()?;
    let mut line = Vec::new();
    for arg in std::iter::once(command.get_program()).chain(command.get_args()) {
        if !line.is_empty() {
            line.push(b' ' as u16);
        }
        line.extend(quote_argument(&arg.encode_wide().collect::<Vec<_>>())?);
    }
    line.push(0);
    if line.len() > 32767 {
        return Err(OperationError::new("INVALID_INPUT"));
    }
    let env = environment(&command)?;
    let job = Job::create()?;
    let (input, child_input) = pipe(true)?;
    let (stdout, child_output) = pipe(false)?;
    let (stderr, child_error) = pipe(false)?;
    let inherited = [child_input.0, child_output.0, child_error.0];
    let mut attributes = Attributes::create(&inherited)?;
    let mut startup: STARTUPINFOEXW = unsafe { zeroed() };
    startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = child_input.0;
    startup.StartupInfo.hStdOutput = child_output.0;
    startup.StartupInfo.hStdError = child_error.0;
    startup.lpAttributeList = attributes.pointer();
    let mut information: PROCESS_INFORMATION = unsafe { zeroed() };
    if unsafe {
        CreateProcessW(
            application.as_ptr(),
            line.as_mut_ptr(),
            null(),
            null(),
            1,
            CREATE_SUSPENDED
                | CREATE_UNICODE_ENVIRONMENT
                | CREATE_NO_WINDOW
                | EXTENDED_STARTUPINFO_PRESENT,
            env.as_ptr().cast(),
            cwd.as_ref().map_or(null(), |value| value.as_ptr()),
            &startup.StartupInfo,
            &mut information,
        )
    } == 0
    {
        return Err(failed());
    }
    let process = Handle::owned(information.hProcess)?;
    let primary_thread = Handle::owned(information.hThread)?;
    if unsafe { AssignProcessToJobObject(job.handle.0, process.0) } == 0 {
        // 尚未恢复主线程，不会产生 Git 写入；仍必须确认挂起进程已回收。
        if unsafe { TerminateProcess(process.0, 1) } == 0
            || unsafe { WaitForSingleObject(process.0, 5000) } != WAIT_OBJECT_0
        {
            return Err(OperationError::new("WRITE_OUTCOME_UNKNOWN"));
        }
        return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
    }
    drop(attributes);
    drop(child_input);
    drop(child_output);
    drop(child_error);
    if unsafe { ResumeThread(primary_thread.0) } == u32::MAX {
        job.terminate_and_wait()?;
        return Err(failed());
    }
    drop(primary_thread);
    let mut input = Some(input);
    let mut written = 0;
    let mut output = PipeOutput::default();
    let mut errors = PipeOutput::default();
    let mut progress = NetworkProgress::default();
    let output_limit = if network {
        limit.min(1024 * 1024)
    } else {
        limit
    };
    let mut exit_code = None;
    let execution = (|| -> Result<(), OperationError> {
        loop {
            if Instant::now() >= deadline {
                return Err(OperationError::new("TIMEOUT"));
            }
            if written == stdin.len() {
                input = None;
            }
            if let Some(pipe) = &input {
                let bytes = &stdin[written..stdin.len().min(written + 65536)];
                let mut count = 0;
                if unsafe {
                    WriteFile(
                        pipe.0,
                        bytes.as_ptr().cast(),
                        bytes.len() as u32,
                        &mut count,
                        null_mut(),
                    )
                } == 0
                {
                    return Err(failed());
                }
                written += count as usize;
            }
            drain(
                &stdout,
                &mut output,
                output_limit,
                false,
                &mut progress,
                on_progress,
            )?;
            drain(
                &stderr,
                &mut errors,
                STDERR_LIMIT,
                network,
                &mut progress,
                on_progress,
            )?;
            if output.truncated || errors.truncated {
                return Ok(());
            }
            if exit_code.is_none() {
                match unsafe { WaitForSingleObject(process.0, 0) } {
                    WAIT_OBJECT_0 => {
                        let mut code = 0;
                        if unsafe { GetExitCodeProcess(process.0, &mut code) } == 0 {
                            return Err(failed());
                        }
                        exit_code = Some(code);
                    }
                    WAIT_TIMEOUT => {}
                    _ => return Err(failed()),
                }
            }
            if exit_code.is_some() && output.closed && errors.closed && input.is_none() {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(2));
        }
    })();
    drop(input);
    let cleanup = job.terminate_and_wait();
    drop(job);
    cleanup?;
    let waited = unsafe { WaitForSingleObject(process.0, 0) };
    execution?;
    if waited != WAIT_OBJECT_0 {
        return Err(OperationError::new("WRITE_OUTCOME_UNKNOWN"));
    }
    if errors.truncated || (output.truncated && !allow_truncate) {
        return Err(OperationError::new("OUTPUT_LIMIT"));
    }
    if exit_code.is_none() {
        let mut code = 0;
        if unsafe { GetExitCodeProcess(process.0, &mut code) } == 0 {
            return Err(failed());
        }
        exit_code = Some(code);
    }
    Ok(ProcessOutput {
        stdout: output.bytes,
        stderr: errors.bytes,
        truncated: output.truncated,
        success: exit_code == Some(0),
        exit_code: exit_code.map(|code| code as i32),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    /// 仅测试进程通过环境变量选定固定行为，不作为应用命令入口。
    #[test]
    #[ignore = "由 Windows 父测试启动的子进程夹具"]
    fn windows_child() {
        match std::env::var("GITMASTER_WINDOWS_FIXTURE").unwrap().as_str() {
            "echo" => {
                let mut input = Vec::new();
                std::io::stdin().read_to_end(&mut input).unwrap();
                for argument in std::env::var("GITMASTER_WINDOWS_ARGUMENT").into_iter() {
                    assert!(std::env::args_os().any(|value| value == OsStr::new(&argument)));
                }
                std::io::stdout().write_all(&input).unwrap();
                std::io::stderr()
                    .write_all(b"Receiving objects: 50% (2/4)\rReceiving objects: 100% (4/4)\n")
                    .unwrap();
                std::process::exit(0);
            }
            "sleep" => thread::sleep(Duration::from_secs(30)),
            "flood" => {
                thread::spawn(|| loop {
                    std::io::stderr().write_all(&[b'e'; 8192]).unwrap();
                });
                loop {
                    std::io::stdout().write_all(&[b'o'; 8192]).unwrap();
                }
            }
            "descendant" => {
                let child = child("sleep").spawn().unwrap();
                std::fs::write(
                    std::env::var_os("GITMASTER_WINDOWS_PID").unwrap(),
                    child.id().to_string(),
                )
                .unwrap();
                std::process::exit(0);
            }
            _ => panic!("未知测试夹具"),
        }
    }
    /// 生成仅运行固定 ignored 夹具的本测试可执行文件命令。
    fn child(mode: &str) -> Command {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command.args([
            "--ignored",
            "--exact",
            "git::process::windows::tests::windows_child",
            "--nocapture",
        ]);
        command.env("GITMASTER_WINDOWS_FIXTURE", mode);
        command
    }
    /// 大输入、字面参数、双输出和进度可同时前进，且输入完整传递。
    #[test]
    fn windows_input_output_and_argument_boundaries() {
        let argument = "中文 空格\\末尾\\\"引号🧪";
        let mut command = child("echo");
        command
            .arg("--")
            .arg(argument)
            .env("GITMASTER_WINDOWS_ARGUMENT", argument);
        let input = vec![b'x'; 2 * 1024 * 1024];
        let mut progress = Vec::new();
        let output = run(
            command,
            &input,
            3 * 1024 * 1024,
            false,
            Duration::from_secs(10),
            false,
            &mut |_, _| {},
        )
        .unwrap();
        assert!(output.success);
        assert!(output.stdout.ends_with(&input));
        let output = run(
            child("echo"),
            b"example",
            1024,
            false,
            Duration::from_secs(10),
            true,
            &mut |done, total| progress.push((done, total)),
        )
        .unwrap();
        assert!(output.success);
        assert_eq!(progress, vec![(2, 4), (4, 4)]);
    }
    /// 不消费 stdin 及双输出洪泛都在预算内结束，不能卡在线程 join。
    #[test]
    fn windows_stdin_timeout_and_output_limit_are_bounded() {
        let started = Instant::now();
        let error = run(
            child("sleep"),
            &vec![0; STDIN_LIMIT],
            1024,
            false,
            Duration::from_millis(250),
            false,
            &mut |_, _| {},
        )
        .unwrap_err();
        assert_eq!(error.code, "TIMEOUT");
        assert!(started.elapsed() < Duration::from_secs(7));
        let error = run(
            child("flood"),
            &[],
            1024,
            false,
            Duration::from_secs(10),
            false,
            &mut |_, _| {},
        )
        .unwrap_err();
        assert_eq!(error.code, "OUTPUT_LIMIT");
    }
    /// 父进程提前退出而后代持管道时，超时回收 Job 中的后代。
    #[test]
    fn windows_descendant_is_reaped_after_parent_exit() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("pid");
        let mut command = child("descendant");
        command.env("GITMASTER_WINDOWS_PID", &path);
        let error = run(
            command,
            &[],
            1024,
            false,
            Duration::from_secs(2),
            false,
            &mut |_, _| {},
        )
        .unwrap_err();
        assert_eq!(error.code, "TIMEOUT");
        let pid: u32 = std::fs::read_to_string(path).unwrap().parse().unwrap();
        let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
        if let Ok(handle) = Handle::owned(handle) {
            assert_eq!(
                unsafe { WaitForSingleObject(handle.0, 5000) },
                WAIT_OBJECT_0
            );
        }
    }
}
