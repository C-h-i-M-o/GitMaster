//! 根据已复核的仓库快照计算入口能力，目标和内容仍由 prepare 严格检查。
use super::{write_guard as guard, *};
use std::time::{Duration, Instant};
const BUDGET: Duration = Duration::from_secs(120);

/// 将只读能力检查结果转换为可展示的结构化原因。
fn capability(result: Result<(), OperationError>) -> WriteCapability {
    match result {
        Ok(()) => WriteCapability::Allowed,
        Err(error) => WriteCapability::Error { error },
    }
}
/// 根据布尔前提保留核心使用的同一错误码。
fn require(condition: bool, code: &str) -> Result<(), OperationError> {
    if condition {
        Ok(())
    } else {
        Err(OperationError::new(code))
    }
}
/// 读取当前入口能力；不创建计划、对象或索引，并返回调用方的快照身份。
pub fn read_write_context(
    git: &GitExecutable,
    repo: &RepositoryHandle,
    state: &RepositoryState,
) -> Result<WriteContext, OperationError> {
    let deadline = Instant::now() + BUDGET;
    guard::validate_snapshot(git, repo, state, deadline)?;
    let platform = require(cfg!(any(unix, windows)), "UNSUPPORTED_WRITE_CONFIGURATION");
    let idle = require(state.operations.is_empty(), "REPOSITORY_OPERATION_ACTIVE");
    let headed = require(
        !matches!(state.head, HeadState::Unborn { .. }),
        "HEAD_REQUIRED",
    );
    let named = require(
        !matches!(state.head, HeadState::Detached { .. }),
        "DETACHED_HEAD_WRITE_BLOCKED",
    );
    let clean = require(state.changes.is_empty(), "WORKTREE_DIRTY");
    // 基础本地检查复用核心配置、索引与属性规则；不检查尚未选定的目标树。
    let branch = platform
        .clone()
        .and(idle.clone())
        .and_then(|()| guard::branch_fingerprint(git, repo, deadline).map(|_| ()));
    let local = branch.clone().and(named.clone());
    let staged = state
        .changes
        .iter()
        .any(|c| c.kind == "tracked" && c.index_status != ".");
    let identity = if platform.is_ok() && (idle.is_ok() || state.operations == ["merge"]) {
        guard::identity(git, repo, deadline).map(|_| ())
    } else {
        platform.clone().and(idle.clone())
    };
    let merge = platform
        .clone()
        .and(require(
            state.operations == ["merge"],
            "REPOSITORY_OPERATION_ACTIVE",
        ))
        .and(named.clone())
        .and(headed.clone())
        .and_then(|()| guard::merge_fingerprint(git, repo, false, deadline).map(|_| ()));
    let unresolved = state.changes.iter().any(|c| c.kind == "conflicted");
    let network = platform.and(idle);
    let result = WriteContext {
        repository_id: state.repository_id.clone(),
        snapshot_id: state.snapshot_id.clone(),
        capabilities: WriteCapabilities {
            stage: capability(local.clone()),
            unstage: capability(local.clone().and(require(staged, "EMPTY_SELECTION"))),
            commit: capability(
                local
                    .and(require(staged, "NOTHING_TO_COMMIT"))
                    .and(identity.clone()),
            ),
            create_branch: capability(branch.clone().and(headed.clone())),
            switch_branch: capability(branch.and(clean.clone())),
            fetch: capability(network.clone()),
            push: capability(network.clone().and(headed.clone())),
            integrate: capability(network.and(named).and(headed).and(clean)),
            save_conflict: capability(
                merge
                    .clone()
                    .and(require(unresolved, "UNSUPPORTED_CONFLICT")),
            ),
            finish_merge: capability(
                merge
                    .and(require(!unresolved, "UNRESOLVED_CONFLICTS"))
                    .and(identity),
            ),
        },
    };
    guard::validate_snapshot(git, repo, state, deadline)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::repository::tests::Fixture;
    /// 普通仓库能力读取保持只读并返回原快照 ID。
    #[test]
    fn normal_read_only() {
        let f = Fixture::new();
        let (repo, state) = crate::git::repository::open_repository(&f.git, &f.root).unwrap();
        let context = read_write_context(&f.git, &repo, &state).unwrap();
        assert_eq!(context.snapshot_id, state.snapshot_id);
        assert!(matches!(
            context.capabilities.stage,
            WriteCapability::Allowed
        ));
    }
    /// 工作树变化后拒绝旧快照。
    #[test]
    fn stale_snapshot() {
        let f = Fixture::new();
        let (repo, state) = crate::git::repository::open_repository(&f.git, &f.root).unwrap();
        f.write("stale", b"x");
        assert_eq!(
            read_write_context(&f.git, &repo, &state).unwrap_err().code,
            "STALE_REQUEST"
        );
    }
    /// unborn 仓库没有 HEAD 但允许暂存入口。
    #[test]
    fn unborn_capabilities() {
        let f = Fixture::new();
        let (repo, state) = crate::git::repository::open_repository(&f.git, &f.root).unwrap();
        let context = read_write_context(&f.git, &repo, &state).unwrap();
        assert!(matches!(
            context.capabilities.stage,
            WriteCapability::Allowed
        ));
        assert!(matches!(
            context.capabilities.push,
            WriteCapability::Error { .. }
        ));
    }
    /// dirty 工作树禁止切换和整合。
    #[test]
    fn dirty_capabilities() {
        let f = Fixture::new();
        f.write("dirty", b"x");
        let (repo, state) = crate::git::repository::open_repository(&f.git, &f.root).unwrap();
        let context = read_write_context(&f.git, &repo, &state).unwrap();
        assert!(matches!(
            context.capabilities.switch_branch,
            WriteCapability::Error { .. }
        ));
        assert!(matches!(
            context.capabilities.integrate,
            WriteCapability::Error { .. }
        ));
    }
    /// detached HEAD 禁止提交和整合。
    #[test]
    fn detached_capabilities() {
        let f = Fixture::new();
        f.write("base", b"base");
        f.command(&["add", "."]);
        f.command(&["commit", "-m", "base"]);
        f.command(&["checkout", "--detach", "HEAD"]);
        let (repo, state) = crate::git::repository::open_repository(&f.git, &f.root).unwrap();
        let context = read_write_context(&f.git, &repo, &state).unwrap();
        assert!(matches!(
            context.capabilities.commit,
            WriteCapability::Error { .. }
        ));
        assert!(matches!(
            context.capabilities.integrate,
            WriteCapability::Error { .. }
        ));
    }

    /// 清洁合并读取身份后开放完成入口，未提交索引和合并文件保持不变。
    #[test]
    fn merge_finish_and_identity_use_real_state() {
        let f = Fixture::new();
        f.write("base", b"base\n");
        f.command(&["add", "."]);
        f.command(&["commit", "-m", "base"]);
        f.command(&["switch", "-c", "incoming"]);
        f.write("incoming", b"incoming\n");
        f.command(&["add", "."]);
        f.command(&["commit", "-m", "incoming"]);
        f.command(&["switch", "main"]);
        f.write("local", b"local\n");
        f.command(&["add", "."]);
        f.command(&["commit", "-m", "local"]);
        f.command(&["merge", "--no-ff", "--no-commit", "incoming"]);
        let (repo, state) = repository::open_repository(&f.git, &f.root).unwrap();
        let index = std::fs::read(repo.git_dir.join("index")).unwrap();
        let merge = std::fs::read(repo.git_dir.join("MERGE_HEAD")).unwrap();
        let result = read_write_context(&f.git, &repo, &state).unwrap();
        assert!(matches!(
            result.capabilities.finish_merge,
            WriteCapability::Allowed
        ));
        assert!(matches!(
            result.capabilities.stage,
            WriteCapability::Error { .. }
        ));
        assert!(matches!(
            result.capabilities.fetch,
            WriteCapability::Error { .. }
        ));
        assert_eq!(std::fs::read(repo.git_dir.join("index")).unwrap(), index);
        assert_eq!(
            std::fs::read(repo.git_dir.join("MERGE_HEAD")).unwrap(),
            merge
        );
        f.command(&["config", "user.email", ""]);
        let result = read_write_context(&f.git, &repo, &state).unwrap();
        assert!(
            matches!(result.capabilities.finish_merge,WriteCapability::Error{ref error} if error.code=="IDENTITY_REQUIRED")
        );
    }

    /// untracked 不被误判为已暂存，首次 add 后才开放提交入口。
    #[test]
    fn unborn_commit_requires_real_staged_content() {
        let f = Fixture::new();
        f.write("new", b"new\n");
        let (repo, state) = repository::open_repository(&f.git, &f.root).unwrap();
        let result = read_write_context(&f.git, &repo, &state).unwrap();
        assert!(
            matches!(result.capabilities.commit,WriteCapability::Error{ref error} if error.code=="NOTHING_TO_COMMIT")
        );
        f.command(&["add", "new"]);
        let state = repository::read_repository_state(&f.git, &repo).unwrap();
        let result = read_write_context(&f.git, &repo, &state).unwrap();
        assert!(matches!(
            result.capabilities.commit,
            WriteCapability::Allowed
        ));
    }
}
