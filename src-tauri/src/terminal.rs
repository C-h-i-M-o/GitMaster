//! 用户主动创建的交互 PTY，与 Git 命令队列独立，输入输出不写日志。
use gitmaster_core::git::OperationError;
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use std::{
    collections::HashMap,
    io::{Read, Write},
    path::Path,
    sync::{
        mpsc::{sync_channel, Receiver, SyncSender, TryRecvError},
        Arc, Mutex,
    },
};

/// 新会话返回实际 shell 与目录，不因后续设置或项目切换而变化。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSession {
    pub session_id: String,
    pub repository_id: Option<String>,
    pub display_cwd: String,
    pub shell_label: String,
}

/// 已确认序号之前的数据可释放，未确认块允许重取。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalChunk {
    pub sequence: u64,
    pub bytes: Vec<u8>,
    pub finished: bool,
    pub exit_code: Option<u32>,
}

struct Process {
    child: Box<dyn Child + Send + Sync>,
    exit: Option<u32>,
}

struct Session {
    owner: String,
    // 先断开队列，防止 Windows 控制端关闭时等待被队满阻塞的读取线程。
    output: Receiver<Vec<u8>>,
    input: SyncSender<Vec<u8>>,
    process: Arc<Mutex<Process>>,
    sequence: u64,
    pending: Vec<u8>,
    eof: bool,
    master: Box<dyn MasterPty + Send>,
}

impl Drop for Session {
    /// 关闭控制端与队列，使等待输入、输出的线程退出并回收直接子进程。
    fn drop(&mut self) {
        if let Ok(mut process) = self.process.lock() {
            if process.exit.is_some() {
                return;
            }
            // 回收与发送信号共用进程锁，已回收的 PID 不再用于终止操作。
            match process.child.try_wait() {
                Ok(Some(status)) => {
                    process.exit = Some(status.exit_code());
                    return;
                }
                Ok(None) => {}
                Err(_) => return,
            }
            #[cfg(target_os = "macos")]
            if let Some(pid) = process.child.process_id() {
                close_macos_background_jobs(pid);
            }
            #[cfg(unix)]
            if let (Some(group), Some(pid)) = (
                self.master.process_group_leader(),
                process.child.process_id(),
            ) {
                // 只终止仍属于该 shell 会话的前台组，避免旧组号复用误伤。
                if group > 0 && unsafe { libc::getsid(group) } == pid as i32 {
                    unsafe {
                        libc::kill(-group, libc::SIGKILL);
                    }
                }
            }
            let _ = process.child.kill();
        }
    }
}

/// zsh 后台作业拥有独立进程组，按原终端会话归属清理，保留其他终端。
#[cfg(target_os = "macos")]
fn close_macos_background_jobs(shell_pid: u32) {
    use libproc::processes::{pids_by_type, ProcFilter};
    let Ok(session) = i32::try_from(shell_pid) else {
        return;
    };
    // 调用方持有直接子进程锁，shell 尚未回收；先停止它继续创建或回收作业。
    if session <= 0 || unsafe { libc::getsid(session) } != session {
        return;
    }
    unsafe { libc::kill(session, libc::SIGSTOP) };
    // 先停止会话成员，再次枚举可包含首次快照期间刚创建的子任务。
    let mut candidates = Vec::new();
    for signal in [libc::SIGSTOP, libc::SIGKILL] {
        // 第二次枚举失败仍处理已停止的成员，不能将它们留在暂停状态。
        candidates.extend(pids_by_type(ProcFilter::All).unwrap_or_default());
        candidates.sort_unstable();
        candidates.dedup();
        for candidate in &candidates {
            let Ok(pid) = i32::try_from(*candidate) else {
                continue;
            };
            if pid > 0 && pid != session && unsafe { libc::getsid(pid) } == session {
                unsafe { libc::kill(pid, signal) };
            }
        }
    }
}

/// 桌面层统一持有会话；所有操作先验证调用窗口，最多保留八个会话。
#[derive(Default)]
pub struct TerminalRegistry {
    sessions: HashMap<String, Session>,
    next: u64,
}

/// 尺寸限制同时保护 PTY 分配与前端误报尺寸。
fn size(cols: u16, rows: u16) -> Result<PtySize, OperationError> {
    if !(2..=500).contains(&cols) || !(1..=300).contains(&rows) {
        return Err(OperationError::new("TERMINAL_SIZE"));
    }
    Ok(PtySize {
        cols,
        rows,
        pixel_width: 0,
        pixel_height: 0,
    })
}

impl TerminalRegistry {
    /// 只从后端解析后的程序和目录创建；调用方不允许传入仓库命令正文。
    pub fn create(
        &mut self,
        owner: &str,
        repository_id: Option<String>,
        shell: &str,
        args: &[String],
        cwd: &Path,
        cols: u16,
        rows: u16,
    ) -> Result<TerminalSession, OperationError> {
        let dimensions = size(cols, rows)?;
        if self.sessions.len() >= 8 {
            return Err(OperationError::new("TERMINAL_LIMIT"));
        }
        let pair = native_pty_system()
            .openpty(dimensions)
            .map_err(|_| OperationError::new("TERMINAL_START"))?;
        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|_| OperationError::new("TERMINAL_START"))?;
        let mut writer = pair
            .master
            .take_writer()
            .map_err(|_| OperationError::new("TERMINAL_START"))?;
        let mut command = CommandBuilder::new(shell);
        command.args(args);
        command.cwd(cwd);
        command.env("TERM", "xterm-256color");
        let child = pair
            .slave
            .spawn_command(command)
            .map_err(|_| OperationError::new("TERMINAL_START"))?;
        drop(pair.slave);
        let (output_tx, output) = sync_channel(16);
        let (input, input_rx) = sync_channel::<Vec<u8>>(16);
        let process = Arc::new(Mutex::new(Process { child, exit: None }));
        let process_result = process.clone();
        std::thread::spawn(move || {
            let mut bytes = [0u8; 4096];
            loop {
                match reader.read(&mut bytes) {
                    Ok(0) => break,
                    Ok(count) => {
                        if output_tx.send(bytes[..count].to_vec()).is_err() {
                            break;
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => break,
                }
            }
        });
        std::thread::spawn(move || {
            while let Ok(bytes) = input_rx.recv() {
                if writer
                    .write_all(&bytes)
                    .and_then(|_| writer.flush())
                    .is_err()
                {
                    break;
                }
            }
        });
        std::thread::spawn(move || loop {
            if let Ok(mut value) = process_result.lock() {
                if value.exit.is_some() {
                    break;
                }
                match value.child.try_wait() {
                    Ok(Some(status)) => {
                        value.exit = Some(status.exit_code());
                        break;
                    }
                    Ok(None) => {}
                    Err(_) => {
                        value.exit = Some(1);
                        break;
                    }
                }
            } else {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        });
        self.next += 1;
        let id = format!("terminal-{}", self.next);
        self.sessions.insert(
            id.clone(),
            Session {
                owner: owner.into(),
                master: pair.master,
                output,
                input,
                process,
                sequence: 0,
                pending: Vec::new(),
                eof: false,
            },
        );
        Ok(TerminalSession {
            session_id: id,
            repository_id,
            display_cwd: cwd.to_string_lossy().into_owned(),
            shell_label: shell.into(),
        })
    }

    /// 窗口只可使用自己创建的会话，不接受其他窗口的标识。
    fn session(&mut self, owner: &str, id: &str) -> Result<&mut Session, OperationError> {
        self.sessions
            .get_mut(id)
            .filter(|session| session.owner == owner)
            .ok_or_else(|| OperationError::new("TERMINAL_UNAVAILABLE"))
    }

    /// 单次读取最多四个块；未确认块重取不会重复消费 PTY 数据。
    pub fn read(
        &mut self,
        owner: &str,
        id: &str,
        ack: u64,
    ) -> Result<TerminalChunk, OperationError> {
        let session = self.session(owner, id)?;
        if ack > session.sequence || session.sequence.saturating_sub(ack) > 1 {
            return Err(OperationError::new("TERMINAL_SEQUENCE"));
        }
        if ack == session.sequence {
            session.pending.clear();
            for _ in 0..4 {
                match session.output.try_recv() {
                    Ok(bytes) => session.pending.extend(bytes),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        session.eof = true;
                        break;
                    }
                }
            }
            if !session.pending.is_empty() {
                session.sequence += 1;
            }
        }
        let exit_code = session
            .process
            .lock()
            .map_err(|_| OperationError::new("TERMINAL_IO"))?
            .exit;
        Ok(TerminalChunk {
            sequence: session.sequence,
            bytes: session.pending.clone(),
            finished: session.eof && session.pending.is_empty() && exit_code.is_some(),
            exit_code,
        })
    }

    /// 输入有界排队，队满拒绝而非阻塞桌面线程或悄悄丢失字符。
    pub fn write(&mut self, owner: &str, id: &str, bytes: Vec<u8>) -> Result<(), OperationError> {
        if bytes.len() > 4096 {
            return Err(OperationError::new("TERMINAL_INPUT_LIMIT"));
        }
        self.session(owner, id)?
            .input
            .try_send(bytes)
            .map_err(|_| OperationError::new("TERMINAL_INPUT_BUSY"))
    }

    /// 改变 PTY 尺寸，不重启 shell。
    pub fn resize(
        &mut self,
        owner: &str,
        id: &str,
        cols: u16,
        rows: u16,
    ) -> Result<(), OperationError> {
        self.session(owner, id)?
            .master
            .resize(size(cols, rows)?)
            .map_err(|_| OperationError::new("TERMINAL_IO"))
    }

    /// 确认关闭后移除会话，释放 PTY 和有界队列。
    pub fn close(&mut self, owner: &str, id: &str) -> Result<(), OperationError> {
        if !self.sessions.contains_key(id) {
            return Ok(());
        }
        self.session(owner, id)?;
        self.sessions.remove(id);
        Ok(())
    }

    /// 窗口销毁时只回收该窗口的会话。
    pub fn close_window(&mut self, owner: &str) {
        self.sessions.retain(|_, session| session.owner != owner);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// 作业控制将后台任务放到独立进程组，关闭终端也必须结束它。
    #[cfg(unix)]
    #[test]
    fn close_terminates_background_job_group() {
        check_background_close("/bin/sh", &["-m".into()]);
    }

    /// 交互 zsh 不会替应用清理后台作业，覆盖原生验收发现的遗留进程。
    #[cfg(target_os = "macos")]
    #[test]
    fn close_terminates_interactive_zsh_background_job() {
        check_background_close("/bin/zsh", &["-f".into(), "-i".into(), "-m".into()]);
    }

    /// 关闭一个会话不得结束另一个仍在运行的 shell。
    #[cfg(target_os = "macos")]
    #[test]
    fn closing_session_preserves_other_terminal() {
        let mut registry = TerminalRegistry::default();
        let first = registry
            .create(
                "main",
                None,
                "/bin/zsh",
                &["-f".into(), "-i".into()],
                &std::env::temp_dir(),
                80,
                24,
            )
            .unwrap();
        let second = registry
            .create(
                "main",
                None,
                "/bin/zsh",
                &["-f".into(), "-i".into()],
                &std::env::temp_dir(),
                80,
                24,
            )
            .unwrap();
        let survivor = registry.sessions[&second.session_id].process.clone();
        registry.close("main", &first.session_id).unwrap();
        std::thread::sleep(Duration::from_millis(50));
        assert!(survivor.lock().unwrap().child.try_wait().unwrap().is_none());
        registry.close("main", &second.session_id).unwrap();
    }

    /// 只启动测试进程；失败也清理后台任务，避免污染验收环境。
    #[cfg(unix)]
    fn check_background_close(shell: &str, initial_args: &[String]) {
        let mut args = initial_args.to_vec();
        args.extend([
            "-c".into(),
            "sleep 60 & printf 'CHILD:%s\\n' \"$!\"; wait".into(),
        ]);
        let mut registry = TerminalRegistry::default();
        let session = registry
            .create("main", None, shell, &args, &std::env::temp_dir(), 80, 24)
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut output = Vec::new();
        let mut ack = 0;
        let child = loop {
            let chunk = registry.read("main", &session.session_id, ack).unwrap();
            ack = chunk.sequence;
            output.extend(chunk.bytes);
            let text = String::from_utf8_lossy(&output);
            if let Some(line) = text.lines().find(|line| line.starts_with("CHILD:")) {
                break line
                    .trim()
                    .strip_prefix("CHILD:")
                    .unwrap()
                    .parse::<i32>()
                    .unwrap();
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(10));
        };
        registry.close("main", &session.session_id).unwrap();
        std::thread::sleep(Duration::from_millis(150));
        let status = std::process::Command::new("/bin/ps")
            .args(["-o", "stat=", "-p", &child.to_string()])
            .output()
            .unwrap();
        let state = String::from_utf8_lossy(&status.stdout);
        let stopped = state.trim().is_empty() || state.trim().starts_with('Z');
        // 即使断言失败也先清理本测试独立创建的 sleep，避免遗留测试进程。
        if !stopped {
            unsafe {
                libc::kill(child, libc::SIGKILL);
            }
        }
        assert!(stopped, "后台任务关闭后仍在运行：{state}");
    }

    /// 输出超过队列容量时仍有界，关闭后直接子进程实际退出并被回收。
    #[cfg(unix)]
    #[test]
    fn saturated_output_can_be_closed_and_reaped() {
        let mut registry = TerminalRegistry::default();
        let session = registry
            .create(
                "main",
                None,
                "/bin/sh",
                &[
                    "-c".into(),
                    "while :; do printf '0123456789012345678901234567890123456789'; done".into(),
                ],
                &std::env::temp_dir(),
                80,
                24,
            )
            .unwrap();
        let process = registry.sessions[&session.session_id].process.clone();
        std::thread::sleep(Duration::from_millis(100));
        let first = registry.read("main", &session.session_id, 0).unwrap();
        assert!(!first.bytes.is_empty());
        assert!(first.bytes.len() <= 16384);
        assert_eq!(
            registry.read("main", &session.session_id, 0).unwrap().bytes,
            first.bytes
        );
        assert!(registry
            .read("main", &session.session_id, first.sequence + 1)
            .is_err());
        registry.close("main", &session.session_id).unwrap();
        registry.close("main", &session.session_id).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        while process.lock().unwrap().exit.is_none() {
            assert!(Instant::now() < deadline, "关闭终端后子进程未被回收");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// 真实 PTY 输出可重取，确认后才消费，窗口身份不能跨用。
    #[cfg(unix)]
    #[test]
    fn pty_output_is_acknowledged_and_window_bound() {
        let mut terminals = TerminalRegistry::default();
        let dir = std::env::temp_dir();
        let session = terminals
            .create("main", None, "/bin/sh", &[], &dir, 80, 24)
            .unwrap();
        assert!(terminals.read("other", &session.session_id, 0).is_err());
        terminals
            .write(
                "main",
                &session.session_id,
                b"printf 'PTY_OK\\n'; exit\n".to_vec(),
            )
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut ack = 0;
        let mut output = Vec::new();
        loop {
            let chunk = terminals.read("main", &session.session_id, ack).unwrap();
            if !chunk.bytes.is_empty() {
                let repeated = terminals.read("main", &session.session_id, ack).unwrap();
                assert_eq!(repeated.bytes, chunk.bytes);
                assert_eq!(repeated.sequence, chunk.sequence);
                output.extend(&chunk.bytes);
                ack = chunk.sequence;
            }
            if chunk.finished {
                assert_eq!(chunk.exit_code, Some(0));
                break;
            }
            assert!(Instant::now() < deadline, "PTY 没有结束");
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(String::from_utf8_lossy(&output).contains("PTY_OK"));
        terminals.close("main", &session.session_id).unwrap();
        assert!(terminals.read("main", &session.session_id, ack).is_err());
    }

    /// 非法尺寸、超限输入与第九个会话均被拒绝，不越过资源边界。
    #[cfg(unix)]
    #[test]
    fn pty_limits_and_close_release_sessions() {
        let mut terminals = TerminalRegistry::default();
        let dir = std::env::temp_dir();
        assert!(terminals
            .create("main", None, "/bin/sh", &[], &dir, 0, 24)
            .is_err());
        let mut ids = Vec::new();
        for _ in 0..8 {
            ids.push(
                terminals
                    .create("main", None, "/bin/sh", &[], &dir, 80, 24)
                    .unwrap()
                    .session_id,
            );
        }
        assert!(terminals
            .create("main", None, "/bin/sh", &[], &dir, 80, 24)
            .is_err());
        assert!(terminals.write("main", &ids[0], vec![b'x'; 4097]).is_err());
        terminals.resize("main", &ids[0], 100, 30).unwrap();
        assert!(terminals.resize("main", &ids[0], 1000, 30).is_err());
        terminals.close_window("other");
        assert_eq!(terminals.sessions.len(), 8);
        terminals.close_window("main");
        assert!(terminals.sessions.is_empty());
    }
}
