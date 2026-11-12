//! 固定 Agent 子进程的私有管道；WebView 不获得任意进程或方法调用权限。
use crate::ai_settings;
use gitmaster_core::git::{
    coordinator::{CoordinationKey, ExternalWriteLease, RepositoryCoordinator},
    OperationError,
};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Read, Write},
    path::PathBuf,
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
    time::Duration,
};
use tauri::{ipc::Channel, Manager};

/// 由已打开仓库签发的上下文，不能通过模型的路径参数改变。
#[derive(Clone)]
pub struct Binding {
    pub git: gitmaster_core::git::GitExecutable,
    pub repository: gitmaster_core::git::RepositoryHandle,
    pub permission: ai_settings::Permission,
    pub root: PathBuf,
    pub repository_key: String,
    pub key: CoordinationKey,
    pub coordinator: RepositoryCoordinator,
    pub protected: bool,
}
/// 应用延迟持有一个运行时，未使用 AI 时不启动 Node。
#[derive(Default)]
pub struct AiState(pub Mutex<Option<Arc<Runtime>>>);
/// 每次管道请求的接纳结果；只通过固定 command 产生。
type Reply = Result<Value, String>;
/// 子进程拥有的全部状态，在窗口退出时显式回收。
pub struct Runtime {
    git_previews: Mutex<HashMap<String, (String, String)>>,
    owner: String,
    child: Mutex<Child>,
    input: Mutex<ChildStdin>,
    pending: Mutex<HashMap<u64, mpsc::Sender<Reply>>>,
    next: AtomicU64,
    stopped: AtomicBool,
    ended: AtomicBool,
    channels: Mutex<HashMap<String, Channel<Value>>>,
    pub bindings: Mutex<HashMap<String, Binding>>,
    leases: Mutex<HashMap<String, ExternalWriteLease>>,
    waiting: Mutex<HashMap<String, Arc<AtomicBool>>>,
    settings_path: PathBuf,
    #[cfg(windows)]
    job: WindowsJob,
}

/// 所有对外错误只携带稳定码，不包含命令、密钥或原始协议帧。
fn error(code: &str) -> OperationError {
    OperationError::new(code)
}
/// 设置路径始终属于应用目录，不能由前端传入文件名。
pub fn settings_path(app: &tauri::AppHandle) -> Result<PathBuf, OperationError> {
    app.path()
        .app_config_dir()
        .map(|path| path.join("ai/settings.json"))
        .map_err(|_| error("AI_SETTINGS_IO"))
}

impl AiState {
    /// 仅查找已启动且可接收请求的宿主，观察命令不得启动或重启进程。
    pub fn existing(&self, owner: &str) -> Result<Arc<Runtime>, OperationError> {
        let current = self.0.lock().map_err(|_| error("AI_RUNTIME_FAILED"))?;
        let runtime = current
            .as_ref()
            .ok_or_else(|| error("AI_RUNTIME_NOT_STARTED"))?;
        if runtime.owner != owner {
            return Err(error("AI_OWNER_MISMATCH"));
        }
        if runtime.stopped.load(Ordering::Acquire) {
            return Err(error("AI_RUNTIME_STOPPED"));
        }
        Ok(runtime.clone())
    }
    /// 进程终止并释放全部租约之前继续报告忙碌，缺失宿主视为空闲。
    pub fn activity(&self, owner: &str) -> Result<Value, OperationError> {
        let current = self
            .0
            .lock()
            .map_err(|_| error("AI_RUNTIME_FAILED"))?
            .clone();
        let Some(runtime) = current else {
            return Ok(json!({"busy": false}));
        };
        if runtime.owner != owner {
            return Err(error("AI_OWNER_MISMATCH"));
        }
        if runtime.stopped.load(Ordering::Acquire) {
            return Ok(json!({"busy": !runtime.ended.load(Ordering::Acquire)}));
        }
        runtime.request("activity", json!({}))
    }
    /// 活动请求仍持有旧凭据引用时，不允许替换、清除或删除其系统凭据。
    pub fn ensure_credentials_idle(&self) -> Result<(), OperationError> {
        let runtime = self
            .0
            .lock()
            .map_err(|_| error("AI_RUNTIME_FAILED"))?
            .clone();
        if let Some(runtime) = runtime {
            if runtime.stopped.load(Ordering::Acquire) && !runtime.ended.load(Ordering::Acquire) {
                return Err(error("AI_CREDENTIAL_BUSY"));
            }
            if !runtime.stopped.load(Ordering::Acquire)
                && runtime
                    .request("activity", json!({}))?
                    .get("busy")
                    .and_then(Value::as_bool)
                    == Some(true)
            {
                return Err(error("AI_CREDENTIAL_BUSY"));
            }
        }
        Ok(())
    }
    /// 取得所属窗口的延迟运行时，启动失败保持可重试。
    pub fn get(&self, app: &tauri::AppHandle, owner: &str) -> Result<Arc<Runtime>, OperationError> {
        let mut current = self.0.lock().map_err(|_| error("AI_RUNTIME_FAILED"))?;
        if let Some(runtime) = current.as_ref() {
            if runtime.owner != owner {
                return Err(error("AI_OWNER_MISMATCH"));
            }
            if !runtime.stopped.load(Ordering::Acquire) {
                return Ok(runtime.clone());
            }
            if !runtime.ended.load(Ordering::Acquire) {
                return Err(error("AI_RUNTIME_STOPPING"));
            }
        }
        let runtime = Runtime::start(app, owner)?;
        *current = Some(runtime.clone());
        Ok(runtime)
    }
    /// 配置变更只同步已启动的运行时，不因打开设置启动子进程。
    pub fn sync(&self, settings: &ai_settings::AiSettings) -> Result<(), OperationError> {
        let runtime = self
            .0
            .lock()
            .map_err(|_| error("AI_RUNTIME_FAILED"))?
            .clone();
        if let Some(runtime) = runtime {
            if !runtime.stopped.load(Ordering::Acquire) {
                runtime.request(
                    "gitmaster/configure",
                    json!({"providers":settings.providers}),
                )?;
            }
        }
        Ok(())
    }
    /// 窗口关闭先通知 Node 正常收尾，超时后终止整个进程组。
    pub fn close(&self, owner: &str) {
        let runtime = self.0.lock().ok().and_then(|mut slot| {
            if slot.as_ref().is_some_and(|r| r.owner == owner) {
                slot.take()
            } else {
                None
            }
        });
        if let Some(runtime) = runtime {
            let _ =
                runtime.request_timeout("gitmaster/shutdown", json!({}), Duration::from_secs(8));
            runtime.stop();
        }
    }
}

impl Runtime {
    /// 新任务接纳前重读已保存配置，不能沿用同步失败的旧路由或旧密钥引用。
    pub fn refresh_settings(&self) -> Result<(), OperationError> {
        let settings = ai_settings::read(&self.settings_path)?;
        self.request(
            "gitmaster/configure",
            json!({"providers": settings.providers}),
        )?;
        Ok(())
    }
    /// Release 只执行内置 Node 与固定入口，Debug 才使用开发机 Node。
    fn start(app: &tauri::AppHandle, owner: &str) -> Result<Arc<Self>, OperationError> {
        let home = app
            .path()
            .app_data_dir()
            .map_err(|_| error("AI_SETTINGS_IO"))?
            .join("ai");
        let (program, entry) = if cfg!(debug_assertions) {
            (
                PathBuf::from("node"),
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../services/agent-host/src/main.ts"),
            )
        } else {
            let resource = app
                .path()
                .resource_dir()
                .map_err(|_| error("AI_RUNTIME_MISSING"))?
                .join("agent");
            (
                resource.join(if cfg!(windows) { "node.exe" } else { "node" }),
                resource.join("dist/main.js"),
            )
        };
        if !entry.is_file() {
            return Err(error("AI_RUNTIME_MISSING"));
        }
        Self::launch(owner, program, entry, home, settings_path(app)?)
    }
    /// 桌面和原生回归共用实际启动、管道解码及进程树管理。
    fn launch(
        owner: &str,
        program: PathBuf,
        entry: PathBuf,
        home: PathBuf,
        settings_path: PathBuf,
    ) -> Result<Arc<Self>, OperationError> {
        // Node 的入口解析不接受 Windows 扩展路径；保留 dunce 对 UNC/长路径的兼容判断。
        let mut command = Command::new(dunce::simplified(&program));
        command
            .arg("--use-system-ca")
            .arg(dunce::simplified(&entry))
            .arg("--home")
            .arg(dunce::simplified(&home))
            .env_remove("NODE_OPTIONS")
            .env_remove("NODE_PATH")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let mut child = command.spawn().map_err(|_| error("AI_RUNTIME_START"))?;
        #[cfg(windows)]
        let job = match WindowsJob::attach(&child) {
            Ok(job) => job,
            Err(failure) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(failure);
            }
        };
        let input = child
            .stdin
            .take()
            .ok_or_else(|| error("AI_RUNTIME_START"))?;
        let output = child
            .stdout
            .take()
            .ok_or_else(|| error("AI_RUNTIME_START"))?;
        let runtime = Arc::new(Self {
            owner: owner.into(),
            child: Mutex::new(child),
            input: Mutex::new(input),
            pending: Mutex::new(HashMap::new()),
            next: AtomicU64::new(1),
            stopped: AtomicBool::new(false),
            ended: AtomicBool::new(false),
            channels: Mutex::new(HashMap::new()),
            bindings: Mutex::new(HashMap::new()),
            leases: Mutex::new(HashMap::new()),
            git_previews: Mutex::new(HashMap::new()),
            waiting: Mutex::new(HashMap::new()),
            settings_path,
            #[cfg(windows)]
            job,
        });
        let reader_runtime = runtime.clone();
        thread::spawn(move || {
            let mut reader = BufReader::new(output);
            loop {
                let mut bytes = Vec::new();
                let read = reader.by_ref().take(262146).read_until(b'\n', &mut bytes);
                if !matches!(read, Ok(size) if size > 0 && size <= 262145)
                    || bytes.last() != Some(&b'\n')
                {
                    break;
                }
                let Ok(frame) = serde_json::from_slice::<Value>(&bytes) else {
                    break;
                };
                reader_runtime.receive(frame);
            }
            reader_runtime.stop();
        });
        let settings = match ai_settings::read(&runtime.settings_path) {
            Ok(settings) => settings,
            Err(failure) => {
                runtime.stop();
                return Err(failure);
            }
        };
        if let Err(failure) = runtime.request(
            "gitmaster/configure",
            json!({"providers":settings.providers}),
        ) {
            runtime.stop();
            return Err(failure);
        }
        Ok(runtime)
    }
    /// 输出由一个互斥写端串行化；超长消息不能破坏分帧。
    fn send(&self, frame: &Value) -> Result<(), OperationError> {
        let mut bytes = serde_json::to_vec(frame).map_err(|_| error("AI_PROTOCOL"))?;
        if bytes.len() > 262144 {
            return Err(error("AI_FRAME_TOO_LARGE"));
        }
        bytes.push(b'\n');
        self.input
            .lock()
            .map_err(|_| error("AI_PROTOCOL"))?
            .write_all(&bytes)
            .map_err(|_| error("AI_RUNTIME_STOPPED"))
    }
    /// 普通请求具有上限；操作超时不代表工具没有产生影响。
    pub fn request(&self, method: &str, params: Value) -> Result<Value, OperationError> {
        self.request_timeout(method, params, Duration::from_secs(35))
    }
    /// 关闭与长连接检查可指定更短的等待预算。
    fn request_timeout(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, OperationError> {
        if self.stopped.load(Ordering::Acquire) {
            return Err(error("AI_RUNTIME_STOPPED"));
        }
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let (sender, receiver) = mpsc::channel();
        self.pending
            .lock()
            .map_err(|_| error("AI_PROTOCOL"))?
            .insert(id, sender);
        if let Err(failure) =
            self.send(&json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))
        {
            if let Ok(mut pending) = self.pending.lock() {
                pending.remove(&id);
            }
            return Err(failure);
        }
        let result = receiver
            .recv_timeout(timeout)
            .map_err(|_| error("AI_TIMEOUT"));
        if let Ok(mut pending) = self.pending.lock() {
            pending.remove(&id);
        }
        result?.map_err(|code| error(&code))
    }
    /// 控制面请求独立线程处理，等待仓库租约不能阻塞 stdout 结果接收。
    fn receive(self: &Arc<Self>, frame: Value) {
        if let Some(method) = frame.get("method").and_then(Value::as_str) {
            if method == "gitmaster/cancel-git" {
                if let Some(id) = frame.pointer("/params/requestId").and_then(Value::as_str) {
                    if let Ok(waiting) = self.waiting.lock() {
                        if let Some(cancelled) = waiting.get(id) {
                            cancelled.store(true, Ordering::Release);
                        }
                    }
                }
                return;
            }
            if method == "gitmaster/git"
                && frame.pointer("/params/phase").and_then(Value::as_str) == Some("read")
            {
                if let Some(id) = frame.get("id").and_then(Value::as_str) {
                    if let Ok(mut waiting) = self.waiting.lock() {
                        waiting.insert(id.to_owned(), Arc::new(AtomicBool::new(false)));
                    }
                }
            }
            if method == "gitmaster/event" {
                if let Some(event) = frame.get("params") {
                    if let Some(id) = event.get("sessionId").and_then(Value::as_str) {
                        if let Ok(channels) = self.channels.lock() {
                            if let Some(channel) = channels.get(id) {
                                let _ = channel.send(event.clone());
                            }
                        }
                    }
                }
                return;
            }
            if method == "gitmaster/release-write" {
                if let Some(id) = frame.pointer("/params/sessionId").and_then(Value::as_str) {
                    self.release(id);
                }
                return;
            }
            if method == "gitmaster/ready" {
                return;
            }
            // 在读取下一帧前登记取消令牌，避免“释放”先于工作线程运行而丢失。
            if method == "gitmaster/acquire-write" {
                if let Some(id) = frame.pointer("/params/sessionId").and_then(Value::as_str) {
                    if let Ok(mut waiting) = self.waiting.lock() {
                        waiting.insert(id.to_owned(), Arc::new(AtomicBool::new(false)));
                    }
                }
            }
            let runtime = self.clone();
            thread::spawn(move || {
                let result = runtime.control(&frame);
                if let Some(id) = frame.get("id") {
                    let response = match result {
                        Ok(value) => json!({"jsonrpc":"2.0","id":id,"result":value}),
                        Err(failure) => {
                            json!({"jsonrpc":"2.0","id":id,"error":{"code":-32000,"message":failure.code}})
                        }
                    };
                    let _ = runtime.send(&response);
                }
            });
        } else if let Some(id) = frame.get("id").and_then(Value::as_u64) {
            if let Ok(mut pending) = self.pending.lock() {
                if let Some(sender) = pending.remove(&id) {
                    let result = frame.get("result").cloned().ok_or_else(|| {
                        frame
                            .pointer("/error/message")
                            .and_then(Value::as_str)
                            .filter(|code| {
                                code.len() <= 64
                                    && code.bytes().all(|b| b.is_ascii_uppercase() || b == b'_')
                            })
                            .unwrap_or("AI_RUNTIME_FAILED")
                            .to_owned()
                    });
                    let _ = sender.send(result);
                }
            }
        }
    }
    /// 控制面仅有凭据解析与仓库租约，未知请求一律拒绝。
    fn control(&self, frame: &Value) -> Result<Value, OperationError> {
        match frame.get("method").and_then(Value::as_str) {
            Some("gitmaster/handoff") => {
                let id = frame
                    .pointer("/params/sessionId")
                    .and_then(Value::as_str)
                    .ok_or_else(|| error("AI_INVALID_INPUT"))?;
                self.release(id);
                Ok(Value::Null)
            }
            Some("gitmaster/git") => {
                let id = frame
                    .pointer("/params/sessionId")
                    .and_then(Value::as_str)
                    .ok_or_else(|| error("AI_INVALID_INPUT"))?;
                let binding = self
                    .bindings
                    .lock()
                    .map_err(|_| error("AI_RUNTIME_FAILED"))?
                    .get(id)
                    .cloned()
                    .ok_or_else(|| error("AI_SESSION_NOT_FOUND"))?;
                if std::fs::canonicalize(&binding.repository.common_dir)
                    .map_err(|_| error("AI_SESSION_PROJECT"))?
                    .to_string_lossy()
                    != binding.repository_key
                    || std::fs::canonicalize(&binding.repository.root)
                        .map_err(|_| error("AI_SESSION_PROJECT"))?
                        != std::fs::canonicalize(&binding.root)
                            .map_err(|_| error("AI_SESSION_PROJECT"))?
                {
                    return Err(error("AI_SESSION_PROJECT"));
                }
                if self
                    .leases
                    .lock()
                    .map_err(|_| error("AI_RUNTIME_FAILED"))?
                    .contains_key(id)
                {
                    return Err(error("AI_GIT_BUSY"));
                }
                let request = frame
                    .pointer("/params/request")
                    .cloned()
                    .ok_or_else(|| error("AI_INVALID_INPUT"))?;
                match frame.pointer("/params/phase").and_then(Value::as_str) {
                    Some("read") => {
                        let request_id = frame
                            .get("id")
                            .and_then(Value::as_str)
                            .ok_or_else(|| error("AI_PROTOCOL"))?;
                        let cancelled = self
                            .waiting
                            .lock()
                            .map_err(|_| error("AI_RUNTIME_FAILED"))?
                            .get(request_id)
                            .cloned()
                            .ok_or_else(|| error("AI_CANCELLED"))?;
                        let result = gitmaster_core::git::inspect::inspect_cancelled(
                            &binding.coordinator,
                            &binding.git,
                            &binding.repository,
                            serde_json::from_value(request)
                                .map_err(|_| error("AI_INVALID_INPUT"))?,
                            || {
                                cancelled.load(Ordering::Acquire)
                                    || self.stopped.load(Ordering::Acquire)
                            },
                        );
                        if let Ok(mut waiting) = self.waiting.lock() {
                            waiting.remove(request_id);
                        }
                        result
                    }
                    Some("prepare") => {
                        let preview = crate::ai_git::prepare(
                            &binding,
                            serde_json::from_value(request)
                                .map_err(|_| error("AI_INVALID_INPUT"))?,
                        )?;
                        if serde_json::to_vec(&preview)
                            .map_err(|_| error("AI_PROTOCOL"))?
                            .len()
                            > 48000
                        {
                            return Err(error("AI_GIT_OUTPUT_LIMIT"));
                        }
                        let plan = preview
                            .get("planId")
                            .and_then(Value::as_str)
                            .ok_or_else(|| error("AI_PROTOCOL"))?
                            .to_owned();
                        let mut previews = self
                            .git_previews
                            .lock()
                            .map_err(|_| error("AI_RUNTIME_FAILED"))?;
                        previews.clear();
                        previews.insert(plan, (id.to_owned(), binding.repository_key));
                        Ok(preview)
                    }
                    Some("execute") => {
                        let plan = request
                            .get("planId")
                            .and_then(Value::as_str)
                            .ok_or_else(|| error("AI_INVALID_INPUT"))?;
                        let owner = self
                            .git_previews
                            .lock()
                            .map_err(|_| error("AI_RUNTIME_FAILED"))?
                            .remove(plan)
                            .ok_or_else(|| error("STALE_WRITE_PLAN"))?;
                        if owner != (id.to_owned(), binding.repository_key.clone()) {
                            return Err(error("AI_OWNER_MISMATCH"));
                        }
                        crate::ai_git::execute(&binding, plan)
                    }
                    _ => Err(error("AI_INVALID_INPUT")),
                }
            }
            Some("gitmaster/sandbox-status") => {
                let id = frame
                    .pointer("/params/sessionId")
                    .and_then(Value::as_str)
                    .ok_or_else(|| error("AI_INVALID_INPUT"))?;
                let binding = self
                    .bindings
                    .lock()
                    .map_err(|_| error("AI_RUNTIME_FAILED"))?
                    .get(id)
                    .cloned()
                    .ok_or_else(|| error("AI_SESSION_NOT_FOUND"))?;
                serde_json::to_value(crate::ai_sandbox::inspect(&binding.root)?)
                    .map_err(|_| error("AI_PROTOCOL"))
            }
            Some("gitmaster/credential") => {
                let reference = frame
                    .pointer("/params/ref")
                    .and_then(Value::as_str)
                    .ok_or_else(|| error("AI_INVALID_INPUT"))?;
                let settings = ai_settings::read(&self.settings_path)?;
                if !settings
                    .providers
                    .iter()
                    .any(|provider| provider.credential_ref.as_deref() == Some(reference))
                {
                    return Err(error("AI_CREDENTIAL_DENIED"));
                }
                Ok(json!(ai_settings::read_secret(reference)?))
            }
            Some("gitmaster/acquire-write") => {
                let id = frame
                    .pointer("/params/sessionId")
                    .and_then(Value::as_str)
                    .ok_or_else(|| error("AI_INVALID_INPUT"))?
                    .to_owned();
                if self
                    .leases
                    .lock()
                    .map_err(|_| error("AI_RUNTIME_FAILED"))?
                    .contains_key(&id)
                {
                    return Ok(Value::Null);
                }
                let binding = self
                    .bindings
                    .lock()
                    .map_err(|_| error("AI_RUNTIME_FAILED"))?
                    .get(&id)
                    .cloned()
                    .ok_or_else(|| error("AI_SESSION_NOT_FOUND"))?;
                if binding.protected {
                    return Err(error("AI_UNSAVED_FILES"));
                }
                let cancelled = self
                    .waiting
                    .lock()
                    .map_err(|_| error("AI_RUNTIME_FAILED"))?
                    .get(&id)
                    .cloned()
                    .ok_or_else(|| error("AI_CANCELLED"))?;
                let lease = binding
                    .coordinator
                    .acquire_external_write_cancellable(&binding.key, cancelled.clone())?;
                let waiting = self
                    .waiting
                    .lock()
                    .map_err(|_| error("AI_RUNTIME_FAILED"))?;
                if cancelled.load(Ordering::Acquire) || self.stopped.load(Ordering::Acquire) {
                    return Err(error("AI_CANCELLED"));
                }
                self.leases
                    .lock()
                    .map_err(|_| error("AI_RUNTIME_FAILED"))?
                    .insert(id, lease);
                drop(waiting);
                Ok(Value::Null)
            }
            _ => Err(error("AI_PROTOCOL")),
        }
    }
    /// 后台任务完成才释放，取消排队请求同时撤销其后续授予。
    fn release(&self, id: &str) {
        if let Ok(mut waiting) = self.waiting.lock() {
            if let Some(cancelled) = waiting.remove(id) {
                cancelled.store(true, Ordering::Release);
            }
        }
        if let Ok(mut leases) = self.leases.lock() {
            leases.remove(id);
        }
    }
    /// 安装订阅不启动模型，不允许其他窗口读取此运行时。
    pub fn subscribe(
        &self,
        owner: &str,
        id: String,
        channel: Channel<Value>,
    ) -> Result<(), OperationError> {
        if self.owner != owner {
            return Err(error("AI_OWNER_MISMATCH"));
        }
        if !self
            .bindings
            .lock()
            .map_err(|_| error("AI_RUNTIME_FAILED"))?
            .contains_key(&id)
        {
            return Err(error("AI_SESSION_NOT_FOUND"));
        }
        let mut channels = self
            .channels
            .lock()
            .map_err(|_| error("AI_RUNTIME_FAILED"))?;
        channels.clear();
        channels.insert(id, channel);
        Ok(())
    }
    /// 进程结束后所有在途响应失败，租约随实际进程回收释放。
    pub fn stop(&self) {
        if self.stopped.swap(true, Ordering::AcqRel) {
            return;
        }
        if let Ok(mut child) = self.child.lock() {
            #[cfg(unix)]
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
            #[cfg(windows)]
            self.job.terminate();
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Ok(mut waiting) = self.waiting.lock() {
            for flag in waiting.values() {
                flag.store(true, Ordering::Release);
            }
            waiting.clear();
        }
        if let Ok(mut leases) = self.leases.lock() {
            leases.clear();
        }
        if let Ok(mut pending) = self.pending.lock() {
            for (_, sender) in pending.drain() {
                let _ = sender.send(Err("AI_RUNTIME_STOPPED".into()));
            }
        }
        self.ended.store(true, Ordering::Release);
    }
}

#[cfg(windows)]
struct WindowsJob(windows_sys::Win32::Foundation::HANDLE);
#[cfg(windows)]
impl WindowsJob {
    /// 子进程在接受任何任务前加入 KILL_ON_JOB_CLOSE 的受控进程树。
    fn attach(child: &Child) -> Result<Self, OperationError> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::JobObjects::*;
        unsafe {
            let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if handle == 0 {
                return Err(error("AI_RUNTIME_START"));
            }
            let job = Self(handle);
            let mut information: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            information.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &information as *const _ as *const _,
                std::mem::size_of_val(&information) as u32,
            ) == 0
                || AssignProcessToJobObject(handle, child.as_raw_handle() as isize) == 0
            {
                return Err(error("AI_RUNTIME_START"));
            }
            Ok(job)
        }
    }
    /// 回收整个运行时进程树，避免只终止 Node 遗留命令。
    fn terminate(&self) {
        unsafe {
            windows_sys::Win32::System::JobObjects::TerminateJobObject(self.0, 1);
        }
    }
}
#[cfg(windows)]
impl Drop for WindowsJob {
    /// 关闭 Job 句柄同时终止崩溃后残留的子进程。
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 真实验证无论成功或断言失败都回收私有宿主。
    struct StopRuntime(Arc<Runtime>);
    impl Drop for StopRuntime {
        /// 测试作用域结束时终止宿主进程树。
        fn drop(&mut self) {
            self.0.stop();
        }
    }

    #[test]
    /// 状态观察不依赖桌面句柄，也不能自行启动宿主。
    fn observation_does_not_start_runtime() {
        let state = AiState::default();
        assert_eq!(state.activity("main").unwrap(), json!({"busy": false}));
        assert!(state.existing("main").is_err());
        assert!(state.0.lock().unwrap().is_none());
    }

    #[test]
    #[ignore = "先构建 Release 并启动 services/agent-host/fixtures/release-acceptance.ts，仅连接本机模拟模型"]
    /// 实际内置 Node 经 Rust 桥接运行终端、Git 查询、技能和确认暂存，全程只操作夹具目录。
    fn release_tools_with_local_fixture() {
        let manifest_path = std::env::var_os("GITMASTER_RELEASE_ACCEPTANCE_MANIFEST")
            .expect("缺少本机验收夹具清单");
        let manifest: Value =
            serde_json::from_slice(&std::fs::read(manifest_path).unwrap()).unwrap();
        let root = std::fs::canonicalize(manifest["root"].as_str().unwrap()).unwrap();
        assert!(root
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("gitmaster-release-acceptance-"));
        assert!(root.join(".gitmaster-acceptance-owner").is_file());
        let project = std::fs::canonicalize(root.join("project")).unwrap();
        assert_eq!(project.parent(), Some(root.as_path()));
        let settings_path = root.join("settings.json");
        let settings = ai_settings::read(&settings_path).unwrap();
        assert_eq!(settings.providers.len(), 1);
        assert!(settings.providers[0].credential_ref.is_none());
        let url = url::Url::parse(&settings.providers[0].base_url).unwrap();
        assert_eq!(url.host_str(), Some("127.0.0.1"));
        let resources = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../target/release/agent");
        let runtime = Runtime::launch(
            "release-acceptance",
            resources.join(if cfg!(windows) { "node.exe" } else { "node" }),
            resources.join("dist/main.js"),
            root.join("home"),
            settings_path,
        )
        .unwrap();
        let _stop = StopRuntime(runtime.clone());
        let git = gitmaster_core::git::environment::resolve_git(None).unwrap();
        let repository =
            gitmaster_core::git::repository::identify_repository(&git, &project).unwrap();
        let repository_key = std::fs::canonicalize(&repository.common_dir)
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let coordinator = RepositoryCoordinator::new();
        let session=runtime.request("create",json!({"workspaceRoot":project,"repositoryKey":repository_key,"model":settings.default_model,"permission":"workspace-write"})).unwrap();
        let id = session["sessionId"].as_str().unwrap().to_owned();
        runtime.bindings.lock().unwrap().insert(
            id.clone(),
            Binding {
                git: git.clone(),
                repository: repository.clone(),
                permission: ai_settings::Permission::WorkspaceWrite,
                root: project.clone(),
                repository_key,
                key: CoordinationKey::repository(&repository).unwrap(),
                coordinator,
                protected: false,
            },
        );
        for (phase, (input, expected)) in [
            ("RELEASE_PIPELINE", "RELEASE_ACCEPTANCE_OK"),
            ("RELEASE_STAGE", "RELEASE_STAGE_OK"),
        ]
        .into_iter()
        .enumerate()
        {
            eprintln!("开始 Release 本机工具验收：阶段 {}", phase + 1);
            runtime.request("send",json!({"sessionId":id,"clientMessageId":format!("00000000-0000-4000-8000-{:012}",phase+1),"text":input})).unwrap();
            let deadline = std::time::Instant::now() + Duration::from_secs(600);
            let mut approved = false;
            loop {
                let view = runtime.request("read", json!({"sessionId":id})).unwrap();
                let events = view["events"].as_array().unwrap();
                if phase == 1 && !approved {
                    if let Some(event) = events.iter().find_map(|item| {
                        item.get("event").filter(|event| {
                            event["kind"] == "approval" && event["withdrawn"] == false
                        })
                    }) {
                        assert_eq!(event["title"], "Git stage");
                        assert!(event["detail"].as_str().unwrap().contains("fixture.txt"));
                        let before = std::process::Command::new(&git.path)
                            .current_dir(&project)
                            .args(["diff", "--cached", "--name-only"])
                            .output()
                            .unwrap();
                        assert!(before.status.success());
                        assert!(before.stdout.is_empty(), "用户确认之前不得暂存");
                        runtime.request("decision",json!({"sessionId":id,"decisionId":event["approvalId"],"value":"allowed-once"})).unwrap();
                        approved = true;
                    }
                }
                if view["status"] == "idle" {
                    assert!(
                        events.iter().any(|item| item
                            .pointer("/event/text")
                            .and_then(Value::as_str)
                            .is_some_and(|text| text.contains(expected))),
                        "Release 模拟模型未确认工具结果"
                    );
                    for name in if phase == 0 {
                        vec!["pwsh", "git_status", "skill"]
                    } else {
                        vec!["git_stage"]
                    } {
                        let name = if name == "pwsh" && !cfg!(windows) {
                            "bash"
                        } else {
                            name
                        };
                        assert!(
                            events.iter().any(|item| item
                                .pointer("/event/name")
                                .and_then(Value::as_str)
                                == Some(name)
                                && item.pointer("/event/failed") == Some(&Value::Bool(false))
                                && item
                                    .pointer("/event/output")
                                    .is_some_and(|value| !value.is_null())),
                            "缺少成功工具结果：{name}"
                        );
                    }
                    if phase == 1 {
                        assert!(approved);
                    }
                    break;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "Release 工具验收超时，不自动重发"
                );
                thread::sleep(Duration::from_millis(200));
            }
        }
        let staged = std::process::Command::new(&git.path)
            .current_dir(&project)
            .args(["diff", "--cached", "--name-only"])
            .output()
            .unwrap();
        assert!(staged.status.success());
        assert_eq!(
            String::from_utf8(staged.stdout).unwrap().trim(),
            "fixture.txt"
        );
        assert_eq!(
            runtime.request("activity", json!({})).unwrap()["busy"],
            false
        );
        eprintln!(
            "Release 内置运行时验收通过：终端、Git 查询、内置 Skill、先预览后确认暂存、租约交接。"
        );
    }

    #[test]
    #[ignore = "需先构建 target/release/agent 内置资源，再显式运行此集成测试"]
    /// 真实 Release 内置宿主必须经过 Rust 管道握手，不能只验证 Node 客户端。
    fn bundled_host_survives_native_bridge() {
        let root = std::fs::canonicalize(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../target/release/agent"),
        )
        .unwrap();
        let temporary = tempfile::tempdir().unwrap();
        let runtime = Runtime::launch(
            "main",
            root.join(if cfg!(windows) { "node.exe" } else { "node" }),
            root.join("dist/main.js"),
            temporary.path().join("中文历史"),
            temporary.path().join("settings.json"),
        )
        .unwrap();
        let result = runtime.request("activity", json!({}));
        runtime.stop();
        assert_eq!(result.unwrap(), json!({"busy": false}));
        assert!(runtime.ended.load(Ordering::Acquire));
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "仅在用户授权后执行：使用系统凭据发出一次连接测试与一次真实只读会话请求"]
    /// 原生凭据桥接驱动已保存模型，历史和工作区仅使用本测试目录。
    fn saved_model_real_connection_and_readonly_reply() {
        let settings_path = PathBuf::from(std::env::var_os("APPDATA").expect("缺少应用配置目录"))
            .join("dev.gitmaster.desktop/ai/settings.json");
        let settings = ai_settings::read(&settings_path).expect("读取已保存模型配置失败");
        let selection = settings.default_model.clone().unwrap_or_else(|| {
            let provider = settings.providers.first().expect("未配置提供商");
            ai_settings::ModelRef {
                provider_id: provider.provider_id.clone(),
                model_id: provider
                    .models
                    .first()
                    .expect("未配置模型")
                    .model_id
                    .clone(),
                reasoning_effort: None,
            }
        });
        let root = std::fs::canonicalize(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../target/release/agent"),
        )
        .unwrap();
        let temporary = tempfile::tempdir().unwrap();
        let project = temporary.path().join("project");
        std::fs::create_dir(&project).unwrap();
        let guard = StopRuntime(
            Runtime::launch(
                "validation",
                root.join("node.exe"),
                root.join("dist/main.js"),
                temporary.path().join("history"),
                settings_path,
            )
            .expect("启动原生宿主失败"),
        );
        let runtime = &guard.0;
        let connection = runtime
            .request(
                "testModel",
                json!({"providerId":selection.provider_id,"modelId":selection.model_id}),
            )
            .expect("连接测试桥接失败");
        assert_eq!(
            connection.get("ok").and_then(Value::as_bool),
            Some(true),
            "连接测试失败：{}",
            connection
        );
        let session=runtime.request("create",json!({"workspaceRoot":project,"repositoryKey":"isolated-validation","model":selection,"permission":"read-only"})).expect("创建隔离会话失败");
        let id = session
            .get("sessionId")
            .and_then(Value::as_str)
            .expect("会话未返回 ID");
        runtime.request("send",json!({"sessionId":id,"clientMessageId":id,"text":"这是一次连通性验证。不要使用任何工具，不要读取或修改文件，不要执行命令。请只回复：验证通过。"})).expect("发送验证消息失败");
        let deadline = std::time::Instant::now() + Duration::from_secs(90);
        loop {
            let view = runtime
                .request("read", json!({"sessionId":id}))
                .expect("读取验证结果失败");
            let events = view
                .get("events")
                .and_then(Value::as_array)
                .expect("缺少事件列表");
            if events
                .iter()
                .any(|e| e.pointer("/event/kind").and_then(Value::as_str) == Some("tool"))
            {
                let _ = runtime.request("cancel", json!({"sessionId":id,"runId":id}));
                panic!("真实验证意外请求工具，已取消任务");
            }
            if let Some(error) = events
                .iter()
                .find(|e| e.pointer("/event/kind").and_then(Value::as_str) == Some("error"))
            {
                panic!("真实会话失败：{}", error.get("event").unwrap());
            }
            if view.get("status").and_then(Value::as_str) == Some("idle") {
                let length: usize = events
                    .iter()
                    .filter(|e| {
                        e.pointer("/event/kind").and_then(Value::as_str) == Some("assistant")
                    })
                    .filter_map(|e| e.pointer("/event/text").and_then(Value::as_str))
                    .map(str::len)
                    .sum();
                assert!(length > 0, "模型未返回最终文本");
                println!(
                    "真实连接测试成功；只读会话收到最终文本，共 {} 字节；未调用工具。",
                    length
                );
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "真实会话等待超时，不自动重发"
            );
            thread::sleep(Duration::from_millis(200));
        }
    }
}
