use super::*;
use crate::agent::policy::{decide, PolicyDecision, ToolRisk};
use crate::git::{
    conflicts,
    coordinator::{CoordinationKey, RepositoryCoordinator},
    files::editor_text,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Component, Path, PathBuf},
    sync::Mutex,
    time::{Duration, Instant},
};

const LIMIT: usize = 2 * 1024 * 1024;
/// 文件操作上下文，保护应用自身状态并共享既有仓库写队列。
pub struct FileTools {
    root: Option<PathBuf>,
    protected: Vec<PathBuf>,
    coordination: Option<(RepositoryCoordinator, CoordinationKey)>,
    gate: Mutex<()>,
}
/// 一次性准备结果，只能整体消费；前端无法构造内部路径和操作。
pub struct PreparedFile {
    pub decision: PolicyDecision,
    pub target: String,
    pub reason: String,
    resolved: Target,
    action: Action,
    created: Instant,
}
/// 原始路径和真实目标同时绑定，批准期间换链接会导致执行失败。
struct Target {
    requested: PathBuf,
    path: PathBuf,
    anchor: PathBuf,
    relative: String,
}
enum Action {
    List,
    Read,
    Write {
        content: String,
        expected_version: Option<String>,
    },
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PathArgs {
    path: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WriteArgs {
    path: String,
    content: String,
    expected_version: Option<String>,
}

impl FileTools {
    /// 路径在后端规范化，未打开项目时只允许明确的绝对路径。
    pub fn new(
        root: Option<PathBuf>,
        protected: Vec<PathBuf>,
        coordination: Option<(RepositoryCoordinator, CoordinationKey)>,
    ) -> AgentResult<Self> {
        let root = root
            .map(|path| fs::canonicalize(path).map_err(AgentError::storage))
            .transpose()?;
        let protected = protected
            .into_iter()
            .map(|path| resolve_missing(&path))
            .collect::<AgentResult<_>>()?;
        Ok(Self {
            root,
            protected,
            coordination,
            gate: Mutex::new(()),
        })
    }
    /// 准备阶段仅处理路径和参数，不在获得必要批准前读取文件内容。
    pub fn prepare(&self, call: &ToolCall, mode: PermissionMode) -> AgentResult<PreparedFile> {
        let (path, action) = match call.name.as_str() {
            "list_files" | "read_file" => {
                let args: PathArgs =
                    serde_json::from_value(call.arguments.clone()).map_err(|_| invalid())?;
                (
                    args.path,
                    if call.name == "list_files" {
                        Action::List
                    } else {
                        Action::Read
                    },
                )
            }
            "write_file" => {
                let args: WriteArgs =
                    serde_json::from_value(call.arguments.clone()).map_err(|_| invalid())?;
                crate::agent::privacy::ensure_safe_text(&args.content, None)?;
                if args.content.len() > LIMIT || args.content.contains('\0') {
                    return Err(invalid());
                }
                (
                    args.path,
                    Action::Write {
                        content: args.content,
                        expected_version: args.expected_version,
                    },
                )
            }
            _ => return Err(AgentError::new("AGENT_UNKNOWN_TOOL", "该文件工具不存在。")),
        };
        let target = self.target(&path)?;
        let outside = self
            .root
            .as_ref()
            .is_none_or(|root| !target.path.starts_with(root));
        let sensitive = sensitive_path(&target.path);
        let decision = decide(
            mode,
            ToolRisk::File {
                outside,
                sensitive,
                write: matches!(action, Action::Write { .. }),
            },
        );
        let reason = if sensitive {
            "目标属于凭据、Git 元数据或系统配置路径。"
        } else if outside {
            "目标位于当前项目之外。"
        } else {
            "操作当前项目中的普通文本文件。"
        };
        Ok(PreparedFile {
            decision,
            target: target.path.to_string_lossy().into_owned(),
            reason: reason.into(),
            resolved: target,
            action,
            created: Instant::now(),
        })
    }
    /// 执行前重新核验路径与版本；未批准的越界操作不会产生副作用。
    pub fn execute(&self, prepared: PreparedFile, approved: bool) -> AgentResult<Value> {
        if prepared.decision == PolicyDecision::Deny
            || (prepared.decision == PolicyDecision::Ask && !approved)
        {
            return Err(AgentError::new(
                "AGENT_APPROVAL_REQUIRED",
                "此文件操作需要用户批准。",
            ));
        }
        if prepared.created.elapsed() > Duration::from_secs(300) {
            return Err(AgentError::new(
                "AGENT_APPROVAL_EXPIRED",
                "文件操作批准已过期，请重新发起。",
            ));
        }
        let _guard = self.gate.lock().map_err(AgentError::storage)?;
        let run = || self.perform(&prepared);
        if matches!(prepared.action, Action::Write { .. }) {
            if let Some((coordinator, key)) = &self.coordination {
                let mut result = None;
                coordinator
                    .write_file(key, || {
                        result = Some(run());
                        Ok(())
                    })
                    .map_err(git_error)?;
                return result
                    .ok_or_else(|| AgentError::new("AGENT_WRITE", "未获得文件写入结果。"))?;
            }
        }
        run()
    }
    /// 复用 Git 核心的能力目录、文件身份、原子替换和新文件排他创建。
    fn perform(&self, prepared: &PreparedFile) -> AgentResult<Value> {
        let target = &prepared.resolved;
        if resolve_missing(&target.requested)? != target.path {
            return Err(changed());
        }
        self.check_protected(&target.path)?;
        let deadline = Instant::now() + Duration::from_secs(10);
        match &prepared.action {
            Action::List => {
                let mut entries = vec![];
                let mut truncated = false;
                for entry in fs::read_dir(&target.path).map_err(AgentError::storage)? {
                    if entries.len() >= 200 {
                        truncated = true;
                        break;
                    }
                    let entry = entry.map_err(AgentError::storage)?;
                    let kind = entry.file_type().map_err(AgentError::storage)?;
                    entries.push(json!({"name":entry.file_name().to_string_lossy(),"kind":if kind.is_symlink() { "symlink" } else if kind.is_dir() { "directory" } else { "file" }}));
                }
                entries.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
                Ok(json!({"path":target.path,"entries":entries,"truncated":truncated}))
            }
            Action::Read => {
                let file =
                    conflicts::read_regular(&target.anchor, &target.relative, LIMIT, deadline)
                        .map_err(git_error)?;
                let text = editor_text::decode(&file.bytes).map_err(git_error)?;
                crate::agent::privacy::ensure_safe_text(&text.content, None)?;
                if resolve_missing(&target.requested)? != target.path {
                    return Err(changed());
                }
                Ok(
                    json!({"path":target.path,"content":text.content,"version":version(target, &file),"truncated":false}),
                )
            }
            Action::Write {
                content,
                expected_version,
            } => {
                if target.path.exists() {
                    let original =
                        conflicts::read_regular(&target.anchor, &target.relative, LIMIT, deadline)
                            .map_err(git_error)?;
                    if expected_version.as_deref() != Some(&version(target, &original)) {
                        return Err(changed());
                    }
                    let old = editor_text::decode(&original.bytes).map_err(git_error)?;
                    crate::agent::privacy::ensure_safe_text(&old.content, None)?;
                    let bytes = editor_text::encode(&old, content).map_err(git_error)?;
                    conflicts::replace_regular(
                        &target.anchor,
                        &target.relative,
                        &original,
                        &bytes,
                        LIMIT,
                        deadline,
                    )
                    .map_err(git_error)?;
                } else {
                    if expected_version.is_some() {
                        return Err(changed());
                    }
                    conflicts::create_regular(
                        &target.anchor,
                        &target.relative,
                        content.as_bytes(),
                        LIMIT,
                        deadline,
                    )
                    .map_err(git_error)?;
                }
                let saved =
                    conflicts::read_regular(&target.anchor, &target.relative, LIMIT, deadline)
                        .map_err(|_| {
                            AgentError::new(
                                "AGENT_WRITE_UNKNOWN",
                                "已执行写入，但未能核验落盘结果，请重新读取文件。",
                            )
                        })?;
                Ok(
                    json!({"path":target.path,"version":version(target, &saved),"bytes":saved.bytes.len(),"saved":true}),
                )
            }
        }
    }
    /// 以真实目标判断项目边界；新文件以最近存在的祖先作为能力目录。
    fn target(&self, path: &str) -> AgentResult<Target> {
        if path.is_empty() || path.len() > 4096 || path.contains('\0') {
            return Err(invalid());
        }
        let supplied = Path::new(path);
        let requested = if supplied.is_absolute() {
            supplied.to_owned()
        } else {
            self.root
                .as_ref()
                .ok_or_else(|| {
                    AgentError::new(
                        "AGENT_PROJECT_REQUIRED",
                        "请先打开项目，或提供绝对文件路径。",
                    )
                })?
                .join(supplied)
        };
        let resolved = resolve_missing(&requested)?;
        self.check_protected(&resolved)?;
        let anchor =
            if let Some(root) = self.root.as_ref().filter(|root| resolved.starts_with(root)) {
                root.clone()
            } else {
                let mut parent = resolved.parent().ok_or_else(invalid)?;
                while !parent.exists() {
                    parent = parent.parent().ok_or_else(invalid)?;
                }
                fs::canonicalize(parent).map_err(AgentError::storage)?
            };
        let relative = resolved
            .strip_prefix(&anchor)
            .map_err(|_| invalid())?
            .to_str()
            .ok_or_else(invalid)?
            .to_owned();
        Ok(Target {
            requested,
            path: resolved,
            anchor,
            relative,
        })
    }
    /// 即使完全访问也不能通过文件工具篡改 Agent 自己的权限、凭据引用或历史。
    fn check_protected(&self, path: &Path) -> AgentResult<()> {
        if self
            .protected
            .iter()
            .any(|protected| path.starts_with(protected))
        {
            return Err(AgentError::new(
                "AGENT_PROTECTED_PATH",
                "AI 自身的数据和凭据需通过设置管理，不能由文件工具读写。",
            ));
        }
        Ok(())
    }
}
/// 已存在路径完整解析符号链接，缺失路径只在真实祖先后拼接普通名称。
fn resolve_missing(path: &Path) -> AgentResult<PathBuf> {
    if !path.is_absolute() {
        return Err(invalid());
    }
    if path
        .components()
        .any(|part| matches!(part, Component::ParentDir))
    {
        return Err(invalid());
    }
    let mut ancestor = path;
    let mut missing = vec![];
    loop {
        match fs::symlink_metadata(ancestor) {
            Ok(_) => break,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                missing.push(ancestor.file_name().ok_or_else(invalid)?.to_owned());
                ancestor = ancestor.parent().ok_or_else(invalid)?;
            }
            Err(error) => return Err(AgentError::storage(error)),
        }
    }
    let mut resolved = fs::canonicalize(ancestor).map_err(AgentError::storage)?;
    for name in missing.into_iter().rev() {
        resolved.push(name);
    }
    Ok(resolved)
}
/// 文件版本同时绑定路径、字节和文件身份，不能跨文件或外部编辑复用。
fn version(target: &Target, file: &conflicts::FileBytes) -> String {
    let mut hash = Sha256::new();
    hash.update(target.path.to_string_lossy().as_bytes());
    hash.update(&file.bytes);
    hash.update(file.identity.as_bytes());
    format!("{:x}", hash.finalize())
}
/// 固定规则识别凭据和系统配置路径，不由模型自行判断风险。
fn sensitive_path(path: &Path) -> bool {
    path.components().any(|part| {
        let part = part.as_os_str().to_string_lossy().to_ascii_lowercase();
        matches!(
            part.as_str(),
            ".git"
                | ".ssh"
                | ".aws"
                | ".gnupg"
                | "keychains"
                | "credentials"
                | "id_rsa"
                | "id_ed25519"
        ) || part.starts_with(".env")
            || part.ends_with(".key")
            || part.ends_with(".pem")
    }) || ["/etc", "/private/etc", "/System", "/Library", "C:\\Windows"]
        .iter()
        .any(|root| path.starts_with(root))
}
/// 不回显未经校验的模型参数。
fn invalid() -> AgentError {
    AgentError::new(
        "AGENT_TOOL_ARGUMENTS",
        "文件工具参数无效，请使用明确路径及规定字段。",
    )
}
/// 过期版本不允许覆盖用户修改。
fn changed() -> AgentError {
    AgentError::new(
        "AGENT_FILE_CHANGED",
        "文件或路径已变化，请重新读取，再使用新的版本执行修改。",
    )
}
