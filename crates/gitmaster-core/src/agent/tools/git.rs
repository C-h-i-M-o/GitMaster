use super::files::sensitive_path;
use super::*;
use crate::agent::{
    policy::{decide, PolicyDecision, ToolRisk},
    provider::Cancellation,
};
use crate::git::{
    self,
    branches::BranchSession,
    coordinator::{CoordinationKey, RepositoryCoordinator},
    process::run_git,
    remote::RemoteSession,
    repository::read_repository_state,
    GitExecutable, HeadState, OperationResult, RepositoryHandle, RepositoryState, SyncUpstream,
    WritePreview, WriteRefresh,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    ffi::OsString,
    path::Path,
    time::{Duration, Instant},
};

const MAX_DIFF_PATH_LIST_BYTES: usize = 256 * 1024;
const MAX_DIFF_PATH_COUNT: usize = 4096;
const MAX_DIFF_ARGUMENT_BYTES: usize = 8 * 1024;
const MAX_DIFF_OUTPUT_BYTES: usize = 256 * 1024;

/// 桌面提供已验证 Git、仓库身份及共享写协调器，不接受模型指定程序。
#[derive(Clone)]
pub struct GitContext {
    pub git: GitExecutable,
    pub repository: RepositoryHandle,
    pub coordinator: RepositoryCoordinator,
}
/// 基础 Git 工具只通过现有安全进程和写计划执行。
pub struct GitTools {
    pub(crate) context: GitContext,
}
/// 一次性 Git 预览在批准期间固定仓库、参数和写前指纹。
pub struct PreparedGit {
    pub decision: PolicyDecision,
    pub target: String,
    pub reason: String,
    pub preview: Option<Value>,
    action: Action,
    created: Instant,
}
enum Action {
    Status,
    Log(usize),
    Diff {
        staged: bool,
        path: Option<String>,
    },
    Show {
        oid: String,
        path: Option<String>,
    },
    Write {
        plan_id: String,
        pull: Option<PullTarget>,
    },
}
struct PullTarget {
    local: String,
    remote: String,
    branch: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Empty {}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LogArgs {
    #[serde(default = "default_limit")]
    limit: usize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DiffArgs {
    #[serde(default)]
    staged: bool,
    path: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ShowArgs {
    commit_id: String,
    path: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PathsArgs {
    paths: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CommitArgs {
    message: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BranchArgs {
    name: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RemoteArgs {
    remote: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PushArgs {
    remote: String,
    branch: String,
}
/// 日志默认读取最近 20 个提交，最多 50 个。
fn default_limit() -> usize {
    20
}
/// 严格参数解析，额外字段不会被悄悄忽略。
fn args<T: serde::de::DeserializeOwned>(call: &ToolCall) -> AgentResult<T> {
    serde_json::from_value(call.arguments.clone()).map_err(|_| invalid())
}

impl GitTools {
    /// 只绑定已打开仓库，不另行创建或安装 Git。
    pub fn new(context: GitContext) -> Self {
        Self { context }
    }
    /// 只读准备写计划，网络及工作树副作用均留到批准后的 execute。
    pub fn prepare(&self, call: &ToolCall, mode: PermissionMode) -> AgentResult<PreparedGit> {
        let mut risk = ToolRisk::Git {
            changes_worktree_or_remote: false,
        };
        let mut target = self.context.repository.root.to_string_lossy().into_owned();
        let mut preview = None;
        let action = match call.name.as_str() {
            "git_status" => {
                let _: Empty = args(call)?;
                Action::Status
            }
            "git_log" => {
                let input: LogArgs = args(call)?;
                if input.limit == 0 || input.limit > 50 {
                    return Err(invalid());
                }
                Action::Log(input.limit)
            }
            "git_diff" => {
                let input: DiffArgs = args(call)?;
                validate_path(input.path.as_deref())?;
                if input
                    .path
                    .as_deref()
                    .is_some_and(|path| sensitive_path(Path::new(path)))
                {
                    return Err(sensitive_git_path_error());
                }
                Action::Diff {
                    staged: input.staged,
                    path: input.path,
                }
            }
            "git_show" => {
                let input: ShowArgs = args(call)?;
                validate_path(input.path.as_deref())?;
                if input
                    .path
                    .as_deref()
                    .is_some_and(|path| sensitive_path(Path::new(path)))
                {
                    return Err(sensitive_git_path_error());
                }
                Action::Show {
                    oid: self.resolve_commit(&input.commit_id)?,
                    path: input.path,
                }
            }
            "git_stage" | "git_unstage" | "git_commit" | "git_create_branch"
            | "git_switch_branch" | "git_fetch" | "git_push" | "git_pull" => {
                let state = self.state()?;
                let context = &self.context;
                let coordinator = &context.coordinator;
                let git = &context.git;
                let repo = &context.repository;
                let mut pull = None;
                let plan = match call.name.as_str() {
                    "git_stage" | "git_unstage" => {
                        let input: PathsArgs = args(call)?;
                        if input.paths.is_empty() || input.paths.len() > 200 {
                            return Err(invalid());
                        }
                        let ids = input
                            .paths
                            .iter()
                            .map(|path| {
                                state
                                    .changes
                                    .iter()
                                    .find(|change| &change.path == path)
                                    .map(|change| change.change_id.clone())
                                    .ok_or_else(|| {
                                        AgentError::new(
                                            "AGENT_GIT_PATH",
                                            "所选文件不在当前 Git 变更中，请重新读取 git_status。",
                                        )
                                    })
                            })
                            .collect::<AgentResult<Vec<_>>>()?;
                        git::write::prepare_index_change(
                            coordinator,
                            git,
                            repo,
                            &state,
                            &ids,
                            call.name == "git_stage",
                        )
                    }
                    "git_commit" => {
                        let input: CommitArgs = args(call)?;
                        git::write::prepare_commit(coordinator, git, repo, &state, &input.message)
                    }
                    "git_create_branch" => {
                        let input: BranchArgs = args(call)?;
                        git::branches::prepare_create_branch(
                            coordinator,
                            git,
                            repo,
                            &state,
                            &input.name,
                        )
                    }
                    "git_switch_branch" => {
                        let input: BranchArgs = args(call)?;
                        risk = ToolRisk::Git {
                            changes_worktree_or_remote: true,
                        };
                        let branches = BranchSession::new(git, repo).map_err(git_error)?;
                        let branch = branches
                            .list()
                            .branches
                            .into_iter()
                            .find(|b| b.name == input.name && b.kind == git::ReferenceKind::Local)
                            .ok_or_else(|| {
                                AgentError::new("AGENT_GIT_BRANCH", "指定的本地分支不存在。")
                            })?;
                        git::branches::prepare_switch_branch(
                            coordinator,
                            git,
                            repo,
                            &state,
                            &branches,
                            &branch.branch_id,
                        )
                    }
                    "git_fetch" => {
                        let input: RemoteArgs = args(call)?;
                        let remotes = RemoteSession::new(git, repo).map_err(git_error)?;
                        let remote = remotes
                            .state()
                            .remotes
                            .into_iter()
                            .find(|r| r.name == input.remote)
                            .ok_or_else(invalid)?;
                        target = format!("{} → {}", remote.name, remote.fetch_display_url);
                        risk = ToolRisk::Network {
                            private: mode != PermissionMode::FullAccess
                                && !public_remote(&remote.fetch_display_url),
                        };
                        git::remote::prepare_fetch_all(
                            coordinator,
                            git,
                            repo,
                            &state,
                            &remotes,
                            &remote.remote_id,
                        )
                    }
                    "git_push" => {
                        let input: PushArgs = args(call)?;
                        let remotes = RemoteSession::new(git, repo).map_err(git_error)?;
                        let remote = remotes
                            .state()
                            .remotes
                            .into_iter()
                            .find(|r| r.name == input.remote)
                            .ok_or_else(invalid)?;
                        target = format!(
                            "{} / {} → {}",
                            remote.name, input.branch, remote.push_display_url
                        );
                        risk = ToolRisk::Git {
                            changes_worktree_or_remote: true,
                        };
                        git::remote::prepare_push(
                            coordinator,
                            git,
                            repo,
                            &state,
                            &remotes,
                            &remote.remote_id,
                            &input.branch,
                        )
                    }
                    "git_pull" => {
                        let _: Empty = args(call)?;
                        let remotes = RemoteSession::new(git, repo).map_err(git_error)?;
                        let sync = remotes.sync_target(&state).map_err(git_error)?;
                        let SyncUpstream::Configured {
                            remote_id,
                            target_branch_name,
                            ..
                        } = sync.upstream
                        else {
                            return Err(AgentError::new(
                                "AGENT_GIT_UPSTREAM",
                                "当前分支尚未配置有效上游，请先在 Git 工作台设置。",
                            ));
                        };
                        let remote = remotes
                            .state()
                            .remotes
                            .into_iter()
                            .find(|r| r.remote_id == remote_id)
                            .ok_or_else(invalid)?;
                        pull = Some(PullTarget {
                            local: sync.branch_name,
                            remote: remote.name.clone(),
                            branch: target_branch_name,
                        });
                        target =
                            format!("{} → {}（仅快进）", remote.name, remote.fetch_display_url);
                        risk = ToolRisk::Git {
                            changes_worktree_or_remote: true,
                        };
                        git::remote::prepare_fetch_all(
                            coordinator,
                            git,
                            repo,
                            &state,
                            &remotes,
                            &remote_id,
                        )
                    }
                    _ => unreachable!(),
                }
                .map_err(git_error)?;
                preview = Some(serde_json::to_value(&plan).map_err(AgentError::storage)?);
                Action::Write {
                    plan_id: plan.plan_id,
                    pull,
                }
            }
            _ => {
                return Err(AgentError::new(
                    "AGENT_UNKNOWN_TOOL",
                    "该 Git 工具未注册；不支持任意 Git 参数或 shell。",
                ))
            }
        };
        let decision =
            if mode == PermissionMode::RequestApproval && matches!(action, Action::Write { .. }) {
                PolicyDecision::Ask
            } else {
                decide(mode, risk)
            };
        let reason = if matches!(risk, ToolRisk::Network { .. }) {
            "此操作会访问 Git 远端。"
        } else if matches!(
            risk,
            ToolRisk::Git {
                changes_worktree_or_remote: true
            }
        ) {
            "此操作会切换工作树、整合上游或写入远端。"
        } else {
            "操作当前仓库；Git 写入使用已核验的计划和仓库队列。"
        };
        Ok(PreparedGit {
            decision,
            target,
            reason: reason.into(),
            preview,
            action,
            created: Instant::now(),
        })
    }
    /// 批准后执行一次性计划；已开始的 Git 写入必须等待真实终态再报告。
    pub fn execute(
        &self,
        prepared: PreparedGit,
        approved: bool,
        cancellation: &Cancellation,
    ) -> AgentResult<Value> {
        cancellation.check()?;
        if prepared.decision == PolicyDecision::Deny
            || (prepared.decision == PolicyDecision::Ask && !approved)
        {
            return Err(AgentError::new(
                "AGENT_APPROVAL_REQUIRED",
                "此 Git 操作需要用户批准。",
            ));
        }
        if prepared.created.elapsed() > Duration::from_secs(300) {
            return Err(AgentError::new(
                "AGENT_APPROVAL_EXPIRED",
                "Git 操作批准已过期，请重新准备。",
            ));
        }
        match prepared.action {
            Action::Status => {
                let state = self.state()?;
                let branches = BranchSession::new(&self.context.git, &self.context.repository)
                    .map_err(git_error)?
                    .list();
                let remotes = match RemoteSession::new(&self.context.git, &self.context.repository)
                {
                    Ok(remotes) => json!(remotes.state()),
                    Err(error) => json!({"unavailable":error.code}),
                };
                Ok(json!({"state":state,"branches":branches,"remotes":remotes}))
            }
            Action::Log(limit) => self.read(
                &[
                    "log".into(),
                    format!("-{limit}"),
                    "--format=%H%x09%aI%x09%s".into(),
                    "--all".into(),
                    "--".into(),
                ],
                128 * 1024,
            ),
            Action::Diff { staged, path } => self.read_diff(staged, path.as_deref()),
            Action::Show { oid, path } => {
                if let Some(path) = path {
                    self.read(&["show".into(), format!("{oid}:{path}")], 128 * 1024)
                } else {
                    self.ensure_safe_commit(&oid)?;
                    self.read(
                        &[
                            "show".into(),
                            "--no-ext-diff".into(),
                            "--no-textconv".into(),
                            "--format=fuller".into(),
                            oid,
                            "--".into(),
                        ],
                        256 * 1024,
                    )
                }
            }
            Action::Write { plan_id, pull } => {
                let mut result = self.finish_plan(&plan_id)?;
                if let Some(target) = pull {
                    cancellation.check().map_err(|_| pull_followup_error())?;
                    let state = operation_refresh_state(&result).ok_or_else(pull_followup_error)?;
                    if !matches!(&state.head, HeadState::Branch { name, .. } if name == &target.local)
                    {
                        return Err(pull_followup_error());
                    }
                    let context = &self.context;
                    let remotes = RemoteSession::new(&context.git, &context.repository)
                        .map_err(|_| pull_followup_error())?;
                    let sync = remotes
                        .sync_target(&state)
                        .map_err(|_| pull_followup_error())?;
                    let SyncUpstream::Configured {
                        remote_id,
                        target_branch_name,
                        ..
                    } = sync.upstream
                    else {
                        return Err(pull_followup_error());
                    };
                    if target_branch_name != target.branch
                        || !remotes
                            .state()
                            .remotes
                            .iter()
                            .any(|r| r.remote_id == remote_id && r.name == target.remote)
                    {
                        return Err(pull_followup_error());
                    }
                    let plan: WritePreview = git::remote::prepare_sync_fast_forward(
                        &context.coordinator,
                        &context.git,
                        &context.repository,
                        &state,
                        &remotes,
                    )
                    .map_err(|_| pull_followup_error())?;
                    result = self
                        .finish_plan(&plan.plan_id)
                        .map_err(|_| pull_followup_error())?;
                }
                write_operation_value(result)
            }
        }
    }
    /// 通过同仓库队列读取最新状态，避免与文件/索引写入交错。
    pub(crate) fn state(&self) -> AgentResult<RepositoryState> {
        let context = &self.context;
        let key = CoordinationKey::repository(&context.repository).map_err(git_error)?;
        context
            .coordinator
            .read(&key, || {
                read_repository_state(&context.git, &context.repository)
            })
            .map_err(git_error)
    }
    /// 有界只读 Git；显式参数数组，不接受任意命令名或用户选项。
    pub(crate) fn read(&self, args: &[String], limit: usize) -> AgentResult<Value> {
        let context = &self.context;
        let key = CoordinationKey::repository(&context.repository).map_err(git_error)?;
        let args: Vec<OsString> = args.iter().map(OsString::from).collect();
        let output = context
            .coordinator
            .read(&key, || {
                run_git(&context.git, &context.repository.root, &args, limit, true)
            })
            .map_err(git_error)?;
        // 输出超限会主动终止 Git；截断事实优先于被终止的退出码。
        if !output.success && !output.truncated {
            return Err(AgentError::new(
                "AGENT_GIT_READ",
                "Git 无法读取指定对象或差异，请检查提交和路径。",
            ));
        }
        let text = match String::from_utf8(output.stdout) {
            Ok(text) => text,
            Err(error) if output.truncated && error.utf8_error().error_len().is_none() => {
                // 只移除输出上限切断的末尾字符，真正的无效编码仍拒绝。
                let end = error.utf8_error().valid_up_to();
                String::from_utf8(error.into_bytes()[..end].to_vec()).map_err(|_| {
                    AgentError::new("AGENT_GIT_BINARY", "该 Git 对象不是 UTF-8 文本。")
                })?
            }
            Err(_) => {
                return Err(AgentError::new(
                    "AGENT_GIT_BINARY",
                    "该 Git 对象不是 UTF-8 文本。",
                ))
            }
        };
        Ok(json!({"text":text,"truncated":output.truncated}))
    }
    /// 用 NUL 名称清单审查完整当前差异路径，再对冻结的普通文件名生成补丁。
    fn read_diff(&self, staged: bool, path: Option<&str>) -> AgentResult<Value> {
        let paths = self.diff_path_names(staged, path)?;
        if paths.is_empty() {
            return Ok(json!({"text":"","truncated":false}));
        }
        let mut text = String::new();
        let mut truncated = false;
        let mut batch = Vec::new();
        let mut batch_bytes = 0usize;
        for path in &paths {
            let path_bytes = path.len().saturating_add(1);
            if path_bytes > MAX_DIFF_ARGUMENT_BYTES {
                return Err(diff_path_list_error());
            }
            if !batch.is_empty() && batch_bytes.saturating_add(path_bytes) > MAX_DIFF_ARGUMENT_BYTES
            {
                self.append_diff_batch(staged, &batch, &mut text, &mut truncated)?;
                if truncated {
                    break;
                }
                batch.clear();
                batch_bytes = 0;
            }
            batch.push(path.clone());
            batch_bytes += path_bytes;
        }
        if !truncated && !batch.is_empty() {
            self.append_diff_batch(staged, &batch, &mut text, &mut truncated)?;
        }
        // 若两次读取期间增删了路径，丢弃刚生成的补丁，不把未审核路径交给调用者。
        if self.diff_path_names(staged, path)? != paths {
            return Err(diff_changed_error());
        }
        Ok(json!({"text":text,"truncated":truncated}))
    }
    /// 先完整列出差异文件名；超限或清单含敏感项时不读取任何补丁正文。
    fn diff_path_names(&self, staged: bool, path: Option<&str>) -> AgentResult<Vec<String>> {
        let mut args = vec![
            "-c".to_owned(),
            "core.quotePath=false".to_owned(),
            "--literal-pathspecs".to_owned(),
            "diff".to_owned(),
            "--no-ext-diff".to_owned(),
            "--no-textconv".to_owned(),
            "--no-renames".to_owned(),
            "--name-only".to_owned(),
            "-z".to_owned(),
        ];
        if staged {
            args.push("--cached".into());
        }
        args.push("--".into());
        if let Some(path) = path {
            args.push(path.into());
        }
        let output = self.read(&args, MAX_DIFF_PATH_LIST_BYTES)?;
        checked_path_list(&output)
    }
    /// 按已审核文件名分批读取，literal pathspec 阻止通配符扩展。
    fn append_diff_batch(
        &self,
        staged: bool,
        paths: &[String],
        text: &mut String,
        truncated: &mut bool,
    ) -> AgentResult<()> {
        let remaining = MAX_DIFF_OUTPUT_BYTES.saturating_sub(text.len());
        if remaining == 0 {
            *truncated = true;
            return Ok(());
        }
        let mut args = vec![
            "-c".to_owned(),
            "core.quotePath=false".to_owned(),
            "--literal-pathspecs".to_owned(),
            "diff".to_owned(),
            "--no-ext-diff".to_owned(),
            "--no-textconv".to_owned(),
            "--no-renames".to_owned(),
            "--src-prefix=a/".to_owned(),
            "--dst-prefix=b/".to_owned(),
            "--no-color".to_owned(),
        ];
        if staged {
            args.push("--cached".into());
        }
        args.push("--".into());
        args.extend(paths.iter().cloned());
        let output = self.read(&args, remaining)?;
        let patch = output["text"].as_str().ok_or_else(invalid)?;
        verify_patch_headers(patch, paths)?;
        text.push_str(patch);
        *truncated = output["truncated"] == true;
        Ok(())
    }
    /// 枚举不可变提交的全部变更路径，用于阻止 git_show 整提交输出敏感文件。
    fn commit_path_names(&self, oid: &str) -> AgentResult<Vec<String>> {
        let args = vec![
            "diff-tree".to_owned(),
            "--root".to_owned(),
            "--no-commit-id".to_owned(),
            "-r".to_owned(),
            "-m".to_owned(),
            "--no-renames".to_owned(),
            "--no-ext-diff".to_owned(),
            "--no-textconv".to_owned(),
            "--name-only".to_owned(),
            "-z".to_owned(),
            oid.to_owned(),
            "--".to_owned(),
        ];
        let output = self.read(&args, MAX_DIFF_PATH_LIST_BYTES)?;
        checked_path_list(&output)
    }
    /// 返回提交补丁或文件前先校验整份不可变提交路径清单。
    fn ensure_safe_commit(&self, oid: &str) -> AgentResult<()> {
        self.commit_path_names(oid).map(|_| ())
    }
    /// 只接受 SHA，不把 ref 表达式或选项当作用户指定提交。
    pub(crate) fn resolve_commit(&self, id: &str) -> AgentResult<String> {
        if !(4..=64).contains(&id.len()) || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(AgentError::new(
                "AGENT_INVALID_COMMIT",
                "请提供有效的完整或短提交 SHA。",
            ));
        }
        let value = self
            .read(
                &[
                    "rev-parse".into(),
                    "--verify".into(),
                    format!("{id}^{{commit}}"),
                ],
                1024,
            )
            .map_err(|_| {
                AgentError::new(
                    "AGENT_INVALID_COMMIT",
                    "提交 SHA 无效、存在歧义或不属于当前仓库。",
                )
            })?;
        let oid = value["text"].as_str().ok_or_else(invalid)?.trim();
        if !matches!(oid.len(), 40 | 64) || !oid.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(invalid());
        }
        Ok(oid.into())
    }
    /// 不以动画或任务受理当作成功，等待核心发布真实执行结果。
    fn finish_plan(&self, plan_id: &str) -> AgentResult<OperationResult> {
        let context = &self.context;
        let handle = context
            .coordinator
            .execute(Some(&context.repository.id), plan_id)
            .map_err(git_error)?;
        loop {
            let record = context
                .coordinator
                .read_operation(Some(&handle.operation_id))
                .map_err(|_| unknown_git_result_error())?
                .ok_or_else(unknown_git_result_error)?;
            if let Some(result) = record.result {
                return successful_operation(result);
            }
            std::thread::sleep(Duration::from_millis(40));
        }
    }
}
/// 保留操作自带的刷新结果，避免写入成功后再执行一次可超时状态查询。
fn write_operation_value(result: OperationResult) -> AgentResult<Value> {
    let result = successful_operation(result)?;
    let state = match &result {
        OperationResult::Succeeded {
            refresh: WriteRefresh::Ready { state },
            ..
        } => json!(state),
        OperationResult::Succeeded { .. } => Value::Null,
        _ => unreachable!("成功操作校验应过滤非成功结果"),
    };
    let operation = serde_json::to_value(result).map_err(AgentError::storage)?;
    Ok(json!({"operation":operation,"state":state}))
}

/// 失败保持可诊断错误，未知结果一律禁止作为可安全重试的普通失败。
fn successful_operation(result: OperationResult) -> AgentResult<OperationResult> {
    match result {
        result @ OperationResult::Succeeded { .. } => Ok(result),
        OperationResult::Failed { error, .. } => Err(git_error(error)),
        OperationResult::Unknown { .. } => Err(unknown_git_result_error()),
        OperationResult::NeedsResolution { .. } => Err(AgentError::new(
            "AGENT_GIT_CONFLICT",
            "Git 操作需要处理冲突，请在工作台中查看实际状态。",
        )),
    }
}

/// 只使用已确认操作结果中的刷新快照，不重新查询仓库。
fn operation_refresh_state(result: &OperationResult) -> Option<RepositoryState> {
    match result {
        OperationResult::Succeeded {
            refresh: WriteRefresh::Ready { state },
            ..
        } => Some(state.clone()),
        _ => None,
    }
}

/// 写操作结果未知时明确提示先核验，避免 runtime 按普通失败自动重试。
fn unknown_git_result_error() -> AgentError {
    AgentError::new(
        "AGENT_GIT_UNKNOWN",
        "Git 写操作结果未知，可能已产生副作用；请检查仓库实际状态后再决定是否继续。",
    )
}

/// 拉取首步已完成而后续核验或快进未完成，禁止把整轮误报为可重试失败。
fn pull_followup_error() -> AgentError {
    AgentError::new(
        "AGENT_GIT_UNKNOWN",
        "拉取的首步已完成，但后续状态核验或快进未完成；请检查仓库实际状态后再决定是否继续，不能自动重试。",
    )
}

/// 校验 NUL 分隔名称清单必须完整、有限，并且不包含敏感文件路径。
pub(crate) fn checked_path_list(output: &Value) -> AgentResult<Vec<String>> {
    if output["truncated"] == true {
        return Err(diff_path_list_error());
    }
    let text = output["text"].as_str().ok_or_else(diff_path_list_error)?;
    if text.is_empty() {
        return Ok(Vec::new());
    }
    let content = text.strip_suffix('\0').ok_or_else(diff_path_list_error)?;
    let paths = content.split('\0').map(str::to_owned).collect::<Vec<_>>();
    if paths.is_empty() || paths.iter().any(|path| path.is_empty()) {
        return Err(diff_path_list_error());
    }
    if paths.len() > MAX_DIFF_PATH_COUNT {
        return Err(diff_path_list_error());
    }
    if paths.iter().any(|path| sensitive_path(Path::new(path))) {
        return Err(sensitive_git_path_error());
    }
    Ok(paths)
}

/// 不暴露具体敏感路径或名称清单内容。
fn sensitive_git_path_error() -> AgentError {
    AgentError::new(
        "AGENT_GIT_SENSITIVE_PATH",
        "Git 差异涉及凭据或系统配置路径，已拒绝读取；请只选择明确的普通文件。",
    )
}

/// 不完整或过大的路径清单不能作为安全补丁范围。
fn diff_path_list_error() -> AgentError {
    AgentError::new(
        "AGENT_GIT_PATH_LIST_LIMIT",
        "Git 变更路径清单无效或超过读取上限，未读取补丁内容。",
    )
}

/// 两次路径清单不一致时，要求重新读取当前差异。
fn diff_changed_error() -> AgentError {
    AgentError::new(
        "AGENT_GIT_DIFF_CHANGED",
        "读取期间 Git 变更路径发生变化，请刷新后重试。",
    )
}

/// 仅放行能与审核文件清单逐字对应的 diff header；无法确认的形式一律拒绝。
fn verify_patch_headers(text: &str, paths: &[String]) -> AgentResult<()> {
    let allowed = paths
        .iter()
        .map(|path| format!("diff --git a/{path} b/{path}"))
        .collect::<HashSet<_>>();
    let mut found = false;
    for line in text.lines().filter(|line| line.starts_with("diff --git ")) {
        found = true;
        if !allowed.contains(line) {
            return Err(diff_header_error());
        }
    }
    if !text.is_empty() && !found {
        return Err(diff_header_error());
    }
    Ok(())
}

/// patch header 无法绑定审核路径时不返回任何正文。
fn diff_header_error() -> AgentError {
    AgentError::new(
        "AGENT_GIT_DIFF_PATHS",
        "Git 补丁包含无法核对的文件路径，已拒绝读取；请刷新差异后重试。",
    )
}

/// 仓库对象路径不允许目录穿越或路径表达式，且始终在参数分隔符之后传递。
fn validate_path(path: Option<&str>) -> AgentResult<()> {
    if let Some(path) = path {
        if path.is_empty()
            || path.len() > 4096
            || path.contains(['\0', '\\', ':'])
            || std::path::Path::new(path).is_absolute()
            || std::path::Path::new(path)
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir))
        {
            return Err(invalid());
        }
    }
    Ok(())
}
/// DNS 无法确认或任意地址不公开时按高风险网络处理。
fn public_remote(address: &str) -> bool {
    use std::net::ToSocketAddrs;
    let Ok(url) = url::Url::parse(address) else {
        return false;
    };
    let Some(host) = url.host_str() else {
        return false;
    };
    let port = url
        .port()
        .unwrap_or(if url.scheme() == "ssh" { 22 } else { 443 });
    let Ok(addresses) = (host, port).to_socket_addrs() else {
        return false;
    };
    let addresses: Vec<_> = addresses.collect();
    !addresses.is_empty() && addresses.iter().all(|address| public_ip(address.ip()))
}
/// 公网判定排除本地、私网、保留、组播和 IPv4 映射地址中的私网。
pub(crate) fn public_ip(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(ip) => {
            let octets = ip.octets();
            !ip.is_private()
                && !ip.is_loopback()
                && !ip.is_link_local()
                && !ip.is_broadcast()
                && !ip.is_documentation()
                && !ip.is_unspecified()
                && octets[0] != 0
                && octets[0] < 224
                && !(octets[0] == 100 && (64..=127).contains(&octets[1]))
                && !(octets[0] == 198 && (18..=19).contains(&octets[1]))
                && !(octets[0] == 192 && octets[1] == 0 && octets[2] == 0)
        }
        std::net::IpAddr::V6(ip) => {
            if let Some(v4) = ip.to_ipv4_mapped() {
                return public_ip(v4.into());
            }
            let first = ip.segments()[0];
            // 只自动认可常规全球单播；站点本地及转换/保留地址保守要求批准。
            first & 0xe000 == 0x2000
                && first != 0x2002
                && !(first == 0x2001 && matches!(ip.segments()[1], 0 | 0xdb8))
        }
    }
}
/// 不回显可能包含私有路径或命令片段的无效参数。
fn invalid() -> AgentError {
    AgentError::new(
        "AGENT_TOOL_ARGUMENTS",
        "Git 工具参数无效，请使用已存在的文件、分支或远端名称。",
    )
}
#[cfg(test)]
mod result_tests {
    use super::*;
    use crate::git::{error::OperationError, OperationKind, WriteRefresh};

    /// 构造不访问仓库的已成功操作，用于验证结果与刷新状态映射。
    fn succeeded(refresh: WriteRefresh) -> OperationResult {
        OperationResult::Succeeded {
            operation_id: "aa".into(),
            kind: OperationKind::Commit,
            commit_oid: None,
            branch_name: None,
            clone_path: None,
            path_results: Vec::new(),
            refresh,
        }
    }

    /// 写入成功的快照作为 state 返回，刷新失败仍保留 operation 且 state 为空。
    #[test]
    fn successful_write_uses_embedded_refresh_without_losing_refresh_failure() {
        let state = RepositoryState {
            index_identity: [0; 32],
            repository_id: "repo".into(),
            snapshot_id: "snapshot".into(),
            root_path: "C:/temp/repo".into(),
            head: HeadState::Branch {
                name: "main".into(),
                oid: "a".repeat(40),
            },
            operations: Vec::new(),
            changes: Vec::new(),
        };
        let value = write_operation_value(succeeded(WriteRefresh::Ready {
            state: state.clone(),
        }))
        .unwrap();
        assert_eq!(value["state"]["snapshotId"], "snapshot");
        assert_eq!(value["operation"]["refresh"]["status"], "ready");

        let value = write_operation_value(succeeded(WriteRefresh::Failed {
            error: OperationError::new("TIMEOUT"),
        }))
        .unwrap();
        assert!(value["state"].is_null());
        assert_eq!(value["operation"]["refresh"]["status"], "failed");
        assert_eq!(value["operation"]["refresh"]["error"]["code"], "TIMEOUT");
    }

    /// 未知结果始终作为专用未知错误，首步完成后的 pull 后续错误同样禁止重试。
    #[test]
    fn unknown_and_partial_pull_results_are_not_reported_as_ordinary_failures() {
        let unknown = successful_operation(OperationResult::Unknown {
            operation_id: "aa".into(),
            kind: OperationKind::Commit,
            error: OperationError::new("TIMEOUT"),
            clone_recovery: None,
            path_results: Vec::new(),
            refresh: WriteRefresh::NotApplicable,
        })
        .unwrap_err();
        assert_eq!(unknown.code, "AGENT_GIT_UNKNOWN");

        let partial = pull_followup_error();
        assert_eq!(partial.code, "AGENT_GIT_UNKNOWN");
        assert!(partial.message.contains("首步已完成"));
        assert!(partial.message.contains("不能自动重试"));
    }
}
