//! AI 专用 Git 动作复用独立核心的预览、指纹、一次性计划和写后复读。
use crate::{ai_runtime::Binding, ai_settings::Permission};
use gitmaster_core::git::{self, OperationError, ReferenceKind};
use serde::Deserialize;
use serde_json::{json, Value};

/// 模型只能提供明确文件、提交说明或本地分支名称。
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum WriteRequest {
    Stage { paths: Vec<String> },
    Unstage { paths: Vec<String> },
    Commit { message: String },
    CreateBranch { name: String },
    SwitchBranch { name: String },
}

/// 只读权限和未保存编辑器内容在原生边界再次检查。
pub fn prepare(binding: &Binding, request: WriteRequest) -> Result<Value, OperationError> {
    if matches!(binding.permission, Permission::ReadOnly) {
        return Err(OperationError::new("AI_GIT_READ_ONLY"));
    }
    if binding.protected {
        return Err(OperationError::new("AI_UNSAVED_FILES"));
    }
    let state = binding.coordinator.read(&binding.key, || {
        git::repository::read_repository_state_until(
            &binding.git,
            &binding.repository,
            std::time::Instant::now() + std::time::Duration::from_secs(120),
        )
    })?;
    let stage = matches!(&request, WriteRequest::Stage { .. });
    let preview = match request {
        WriteRequest::Stage { paths } | WriteRequest::Unstage { paths } => {
            if paths.is_empty() || paths.len() > 200 {
                return Err(OperationError::new("AI_INVALID_INPUT"));
            }
            let ids = paths
                .iter()
                .map(|path| {
                    state
                        .changes
                        .iter()
                        .find(|change| &change.path == path)
                        .map(|change| change.change_id.clone())
                        .ok_or_else(|| OperationError::new("STALE_REQUEST"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            git::write::prepare_index_change(
                &binding.coordinator,
                &binding.git,
                &binding.repository,
                &state,
                &ids,
                stage,
            )?
        }
        WriteRequest::Commit { message } => git::write::prepare_commit(
            &binding.coordinator,
            &binding.git,
            &binding.repository,
            &state,
            &message,
        )?,
        WriteRequest::CreateBranch { name } => git::branches::prepare_create_branch(
            &binding.coordinator,
            &binding.git,
            &binding.repository,
            &state,
            &name,
        )?,
        WriteRequest::SwitchBranch { name } => {
            let branches = binding.coordinator.read(&binding.key, || {
                git::branches::BranchSession::new(&binding.git, &binding.repository)
            })?;
            let list = branches.list();
            let target = list
                .branches
                .iter()
                .find(|branch| branch.name == name && branch.kind == ReferenceKind::Local)
                .ok_or_else(|| OperationError::new("AI_GIT_LOCAL_BRANCH_REQUIRED"))?;
            git::branches::prepare_switch_branch(
                &binding.coordinator,
                &binding.git,
                &binding.repository,
                &state,
                &branches,
                &target.branch_id,
            )?
        }
    };
    serde_json::to_value(preview).map_err(|_| OperationError::new("AI_PROTOCOL"))
}

/// 已确认后等待核心任务真正结算，不能因前端取消或超时把进行中的写操作丢失。
pub fn execute(binding: &Binding, plan: &str) -> Result<Value, OperationError> {
    if matches!(binding.permission, Permission::ReadOnly) || binding.protected {
        return Err(OperationError::new("AI_GIT_WRITE_DENIED"));
    }
    let handle = binding
        .coordinator
        .execute(Some(&binding.repository.id), plan)?;
    loop {
        let record = binding
            .coordinator
            .read_operation(Some(&handle.operation_id))?
            .ok_or_else(|| OperationError::new("AI_GIT_OPERATION_LOST"))?;
        if let Some(result) = record.result {
            let mut value =
                serde_json::to_value(result).map_err(|_| OperationError::new("AI_PROTOCOL"))?;
            if let Some(state) = value.pointer("/refresh/state") {
                let summary = json!({"repositoryId":state["repositoryId"],"snapshotId":state["snapshotId"],"head":state["head"],"changeCount":state["changes"].as_array().map(Vec::len),"changesPagedVia":"git_status"});
                value["refresh"]["state"] = summary;
            }
            return Ok(json!({"result":value}));
        }
        std::thread::sleep(std::time::Duration::from_millis(40));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gitmaster_core::git::coordinator::{CoordinationKey, RepositoryCoordinator};
    /// 测试初始化只操作本测试新建的临时仓库。
    fn fixture_command(binding: &Binding, args: &[&str]) {
        let output = std::process::Command::new(&binding.git.path)
            .current_dir(&binding.root)
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success(), "测试仓库初始化失败");
    }
    /// 原生写入口实际复用暂存/提交/创建/切换，拒绝旧确认与只读模式。
    #[test]
    fn native_git_write_plans_use_real_core() {
        struct TimingLogger;
        impl log::Log for TimingLogger {
            /// 仅输出既有脱敏耗时，不包含路径、参数或 Git 内容。
            fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
                metadata.target() == "gitmaster::diagnostic"
            }
            /// 将阶段耗时交给测试日志，便于区分进程准备与 Git 执行超时。
            fn log(&self, record: &log::Record<'_>) {
                if self.enabled(record.metadata()) {
                    eprintln!("{}", record.args());
                }
            }
            /// 测试使用标准错误，不另持有输出缓冲。
            fn flush(&self) {}
        }
        static LOGGER: TimingLogger = TimingLogger;
        let _ = log::set_logger(&LOGGER);
        log::set_max_level(log::LevelFilter::Debug);
        let directory = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(directory.path()).unwrap();
        let git = git::environment::resolve_git(None).unwrap();
        assert!(std::process::Command::new(&git.path)
            .current_dir(&root)
            .args(["init", "-b", "main"])
            .output()
            .unwrap()
            .status
            .success());
        let repository = git::RepositoryHandle {
            id: "ai-write-fixture".into(),
            root: root.clone(),
            git_dir: root.join(".git"),
            common_dir: root.join(".git"),
        };
        let mut binding = Binding {
            git,
            key: CoordinationKey::repository(&repository).unwrap(),
            repository_key: root.join(".git").to_string_lossy().into_owned(),
            repository,
            root,
            coordinator: RepositoryCoordinator::new(),
            protected: false,
            permission: Permission::WorkspaceWrite,
        };
        fixture_command(&binding, &["config", "user.name", "Fixture"]);
        fixture_command(
            &binding,
            &["config", "user.email", "fixture@example.invalid"],
        );
        std::fs::write(binding.root.join("fixture.txt"), "初始内容\n").unwrap();
        let stage = prepare(
            &binding,
            WriteRequest::Stage {
                paths: vec!["fixture.txt".into()],
            },
        )
        .unwrap();
        let output = execute(&binding, stage["planId"].as_str().unwrap()).unwrap();
        assert_eq!(output["result"]["outcome"], "succeeded", "{output}");
        let commit = prepare(
            &binding,
            WriteRequest::Commit {
                message: "fixture root".into(),
            },
        )
        .unwrap();
        let output = execute(&binding, commit["planId"].as_str().unwrap()).unwrap();
        assert_eq!(output["result"]["outcome"], "succeeded", "{output}");
        let branch = prepare(
            &binding,
            WriteRequest::CreateBranch {
                name: "feature-fixture".into(),
            },
        )
        .unwrap();
        let output = execute(&binding, branch["planId"].as_str().unwrap()).unwrap();
        assert_eq!(output["result"]["outcome"], "succeeded", "{output}");
        let state = git::repository::read_repository_state_until(
            &binding.git,
            &binding.repository,
            std::time::Instant::now() + std::time::Duration::from_secs(120),
        )
        .unwrap();
        assert!(matches!(state.head,git::HeadState::Branch{name,..} if name=="main"));
        let switch = prepare(
            &binding,
            WriteRequest::SwitchBranch {
                name: "feature-fixture".into(),
            },
        )
        .unwrap();
        let output = execute(&binding, switch["planId"].as_str().unwrap()).unwrap();
        assert_eq!(output["result"]["outcome"], "succeeded", "{output}");
        binding.permission = Permission::ReadOnly;
        assert_eq!(
            prepare(
                &binding,
                WriteRequest::Commit {
                    message: "拒绝".into()
                }
            )
            .unwrap_err()
            .code,
            "AI_GIT_READ_ONLY"
        );
        assert!(serde_json::from_value::<WriteRequest>(
            json!({"action":"stage","paths":["fixture.txt"],"all":true})
        )
        .is_err());
    }
}
