//! 单分支下载先写私有引用，核实后才比较交换用户仓库的一个跟踪引用。
use super::{auth::AuthPolicy, url, RemoteSession};
use crate::git::{
    coordinator::{CoordinationKey, OperationReporter, RepositoryCoordinator},
    process::{
        inspect_local_config, run_isolated_git, run_local_git, run_network_git, ProcessOutput,
    },
    repository::{query_until, read_repository_state_until},
    write::operation_result,
    *,
};
use sha2::{Digest, Sha256};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const NETWORK_BUDGET: Duration = Duration::from_secs(15 * 60);
const PRIVATE_REF: &str = "refs/remotes/gitmaster/selected";

/// 生产入口固定使用受限网络 runner；测试替身只在本模块测试中传入。
type NetworkRunner = fn(
    &GitExecutable,
    &Path,
    &[OsString],
    Option<&Path>,
    &[(String, String)],
    Instant,
    &mut dyn FnMut(u64, u64),
) -> Result<ProcessOutput, OperationError>;

/// 准备时捕获的单一目标，不从前端接受 refspec 或可执行命令。
struct FetchPlan {
    git: GitExecutable,
    repo: RepositoryHandle,
    head: HeadState,
    auth: AuthPolicy,
    config_digest: [u8; 32],
    url: String,
    source_ref: String,
    tracking_ref: String,
    old_oid: String,
    objects: PathBuf,
    object_format: String,
    last_fetched_at: Arc<Mutex<Option<String>>>,
}

/// 只读准备显式单分支 fetch；确认执行前不会连接服务器或创建对象。
pub fn prepare_fetch(
    coordinator: &RepositoryCoordinator,
    git: &GitExecutable,
    repo: &RepositoryHandle,
    state: &RepositoryState,
    remotes: &RemoteSession,
    remote_id: &str,
    remote_branch_id: &str,
) -> Result<WritePreview, OperationError> {
    prepare_with_runner(
        coordinator,
        git,
        repo,
        state,
        remotes,
        remote_id,
        remote_branch_id,
        run_network_git,
    )
}

/// 共同准备流程；测试仅替换下载阶段，不绕过 ID、配置、refspec 或发布核验。
fn prepare_with_runner(
    coordinator: &RepositoryCoordinator,
    git: &GitExecutable,
    repo: &RepositoryHandle,
    state: &RepositoryState,
    remotes: &RemoteSession,
    remote_id: &str,
    remote_branch_id: &str,
    runner: NetworkRunner,
) -> Result<WritePreview, OperationError> {
    let generation = coordinator.begin_prepare()?;
    if !cfg!(any(unix, windows)) {
        return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
    }
    if remotes.repo.id != repo.id || state.repository_id != repo.id {
        return Err(OperationError::new("STALE_REQUEST"));
    }
    let remote = remotes
        .remotes
        .get(remote_id)
        .ok_or_else(|| OperationError::new("REMOTE_NOT_FOUND"))?;
    let branch = remotes
        .branches
        .get(remote_branch_id)
        .ok_or_else(|| OperationError::new("STALE_REQUEST"))?;
    let selected_url = remote
        .fetch
        .first()
        .ok_or_else(|| OperationError::new("REMOTE_NOT_FOUND"))?
        .clone();
    url::validate(&selected_url)?;
    let deadline = Instant::now() + super::BUDGET;
    let key = CoordinationKey::repository(repo)?;
    let plan = coordinator.read_until(&key, deadline, || {
        let fresh = read_repository_state_until(git, repo, deadline)?;
        if fresh.head != state.head
            || fresh.operations != state.operations
            || fresh.changes != state.changes
        {
            return Err(OperationError::new("STALE_REQUEST"));
        }
        let config = inspect_local_config(git, &repo.root, deadline)?;
        if <[u8; 32]>::from(Sha256::digest(config)) != remotes.config_digest {
            return Err(OperationError::new("STALE_WRITE_PLAN"));
        }
        let auth = AuthPolicy::capture(git, &repo.root, deadline)?;
        let prefix = format!("remote.{}.", remote.name);
        for suffix in ["uploadpack", "receivepack", "proxy", "vcs"] {
            if !auth.values(&format!("{prefix}{suffix}")).is_empty() {
                return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
            }
        }
        if auth
            .values(&format!("{prefix}mirror"))
            .last()
            .is_some_and(|value| {
                !matches!(
                    value.to_ascii_lowercase().as_str(),
                    "false" | "no" | "off" | "0"
                )
            })
        {
            return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
        }
        let source_ref =
            map_source_ref(&auth.values(&format!("{prefix}fetch")), &branch.reference)?;
        query_until(
            git,
            &repo.root,
            &["check-ref-format", &source_ref],
            4096,
            Some(deadline),
        )?;
        let object_format = String::from_utf8(query_until(
            git,
            &repo.root,
            &["rev-parse", "--show-object-format"],
            4096,
            Some(deadline),
        )?)
        .map_err(|_| OperationError::new("PARSE_FAILED"))?
        .trim()
        .to_owned();
        if !matches!(object_format.as_str(), "sha1" | "sha256") {
            return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
        }
        let objects = std::fs::canonicalize(repo.common_dir.join("objects"))
            .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
        let plan = FetchPlan {
            git: git.clone(),
            repo: repo.clone(),
            head: state.head.clone(),
            auth,
            config_digest: remotes.config_digest,
            url: selected_url.clone(),
            source_ref,
            tracking_ref: branch.reference.clone(),
            old_oid: branch.oid.clone(),
            objects,
            object_format,
            last_fetched_at: remotes.last_fetched_at.clone(),
        };
        plan.verify(deadline)?;
        Ok(plan)
    })?;
    let source_ref = plan.source_ref.clone();
    let old_oid = plan.old_oid.clone();
    let operation = coordinator.prepare(
        generation,
        Some(repo.id.clone()),
        key,
        OperationKind::Fetch,
        move |reporter| execute_fetch(reporter, &plan, runner),
    )?;
    Ok(WritePreview {
        plan_id: operation.plan_id,
        repository_id: repo.id.clone(),
        snapshot_id: state.snapshot_id.clone(),
        kind: OperationKind::Fetch,
        head: state.head.clone(),
        parent_oids: Vec::new(),
        paths: Vec::new(),
        author: None,
        message: None,
        target: Some(WriteTarget::Remote {
            remote_id: remote_id.to_owned(),
            display_url: url::display(&selected_url),
            ref_name: source_ref,
            oid: Some(old_oid),
            source_oid: None,
            pending_commits: Vec::new(),
        }),
        warnings: vec![
            "仅更新所选远端跟踪分支，不修改本地分支和文件。".into(),
            "可能调用系统凭据助手或 SSH agent；认证未就绪时请在外部配置后重新准备。".into(),
        ],
        expires_at: operation.expires_at,
    })
}

/// 反向匹配明确 refspec，支持单个成对通配符；负向或歧义映射需要外部处理。
fn map_source_ref(specs: &[&str], tracking: &str) -> Result<String, OperationError> {
    if !tracking.starts_with("refs/remotes/") {
        return Err(OperationError::new("INVALID_INPUT"));
    }
    let mut source = None;
    for spec in specs {
        let spec = spec.strip_prefix('+').unwrap_or(spec);
        let (from, to) = spec
            .split_once(':')
            .ok_or_else(|| OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"))?;
        if !from.starts_with("refs/heads/") || !to.starts_with("refs/remotes/") {
            return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
        }
        let candidate = if from.contains('*') || to.contains('*') {
            if from.matches('*').count() != 1 || to.matches('*').count() != 1 {
                return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
            }
            let (before, after) = to.split_once('*').expect("已检查通配符");
            tracking
                .strip_prefix(before)
                .and_then(|rest| rest.strip_suffix(after))
                .map(|middle| from.replacen('*', middle, 1))
        } else {
            (to == tracking).then(|| from.to_owned())
        };
        if let Some(candidate) = candidate {
            if source
                .as_ref()
                .is_some_and(|previous| previous != &candidate)
            {
                return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
            }
            source = Some(candidate);
        }
    }
    source.ok_or_else(|| OperationError::new("INVALID_INPUT"))
}

impl FetchPlan {
    /// 发布前后使用完整引用名核对目标，符号引用不作为可更新的跟踪分支。
    fn current_oid(&self, deadline: Instant) -> Result<String, OperationError> {
        let output = query_until(
            &self.git,
            &self.repo.root,
            &[
                "for-each-ref",
                "--format=%(refname)%00%(objectname)%00%(symref)",
                &self.tracking_ref,
            ],
            4096,
            Some(deadline),
        )?;
        let text = std::str::from_utf8(&output).map_err(|_| OperationError::new("PARSE_FAILED"))?;
        let fields: Vec<_> = text
            .strip_suffix('\n')
            .unwrap_or(text)
            .split('\0')
            .collect();
        if fields.len() != 3 || fields[0] != self.tracking_ref || !fields[2].is_empty() {
            return Err(OperationError::new("REMOTE_CHANGED"));
        }
        Ok(fields[1].to_owned())
    }
    /// 配置、跟踪引用、本地 HEAD 或对象存储变化都要求重新准备，不覆盖外部修改。
    fn verify(&self, deadline: Instant) -> Result<(), OperationError> {
        self.auth.verify(&self.git, &self.repo.root, deadline)?;
        let effective = inspect_local_config(&self.git, &self.repo.root, deadline)?;
        if <[u8; 32]>::from(Sha256::digest(effective)) != self.config_digest {
            return Err(OperationError::new("STALE_WRITE_PLAN"));
        }
        if self.current_oid(deadline)? != self.old_oid {
            return Err(OperationError::new("REMOTE_CHANGED"));
        }
        let fresh = read_repository_state_until(&self.git, &self.repo, deadline)?;
        if fresh.head != self.head {
            return Err(OperationError::new("STALE_WRITE_PLAN"));
        }
        let objects = std::fs::canonicalize(self.repo.common_dir.join("objects"))
            .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
        if objects != self.objects {
            return Err(OperationError::new("STALE_WRITE_PLAN"));
        }
        Ok(())
    }
}

/// 只下载确认分支，私有引用与原对象库分离；任何网络错误都不会更新用户跟踪引用。
fn download(
    plan: &FetchPlan,
    deadline: Instant,
    reporter: &OperationReporter,
    runner: NetworkRunner,
) -> Result<String, OperationError> {
    let scratch = tempfile::Builder::new()
        .prefix("gitmaster-fetch-")
        .tempdir()
        .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
    let template = scratch.path().join("empty-template");
    std::fs::create_dir(&template).map_err(|_| OperationError::new("ACCESS_DENIED"))?;
    let mut template_argument = OsString::from("--template=");
    template_argument.push(&template);
    let args = vec![
        "init".into(),
        "--bare".into(),
        template_argument,
        format!("--object-format={}", plan.object_format).into(),
    ];
    let init = run_isolated_git(&plan.git, scratch.path(), &args, &[], None, &[], deadline)?;
    if !init.success {
        return Err(OperationError::new("GIT_EXECUTION_FAILED"));
    }
    let seed = ["update-ref", PRIVATE_REF, &plan.old_oid].map(OsString::from);
    let seeded = run_isolated_git(
        &plan.git,
        scratch.path(),
        &seed,
        &[],
        Some(&plan.objects),
        &[],
        deadline,
    )?;
    if !seeded.success {
        return Err(OperationError::new("GIT_EXECUTION_FAILED"));
    }
    let refspec = format!("+{}:{PRIVATE_REF}", plan.source_ref);
    let args = [
        "fetch",
        "--progress",
        "--no-tags",
        "--no-recurse-submodules",
        "--no-prune",
        "--no-write-fetch-head",
        "--no-auto-maintenance",
        "--no-write-commit-graph",
        "--",
        &plan.url,
        &refspec,
    ]
    .map(OsString::from);
    reporter.report(OperationPhase::Transferring, None)?;
    let result = runner(
        &plan.git,
        scratch.path(),
        &args,
        Some(&plan.objects),
        &plan.auth.settings,
        deadline,
        &mut |completed, total| {
            let _ = reporter.report(
                OperationPhase::Transferring,
                Some(OperationCounts {
                    completed,
                    total: Some(total),
                }),
            );
        },
    )?;
    if !result.success {
        return Err(OperationError::new("NETWORK_FAILED"));
    }
    let args = [
        "rev-parse",
        "--verify",
        &format!("{PRIVATE_REF}^{{commit}}"),
    ]
    .map(OsString::from);
    let actual = run_isolated_git(
        &plan.git,
        scratch.path(),
        &args,
        &[],
        Some(&plan.objects),
        &[],
        deadline,
    )?;
    if !actual.success {
        return Err(OperationError::new("GIT_EXECUTION_FAILED"));
    }
    let oid = std::str::from_utf8(&actual.stdout)
        .map_err(|_| OperationError::new("PARSE_FAILED"))?
        .trim();
    if !matches!(oid.len(), 40 | 64) || !oid.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(OperationError::new("PARSE_FAILED"));
    }
    Ok(oid.to_owned())
}

/// 单一跟踪引用采用旧 OID 比较交换，发布后的结果以真实引用为准。
fn execute_fetch(
    reporter: &OperationReporter,
    plan: &FetchPlan,
    runner: NetworkRunner,
) -> OperationResult {
    let deadline = reporter.deadline(NETWORK_BUDGET);
    let mut attempted = false;
    let result = (|| {
        plan.verify(deadline)?;
        let oid = download(plan, deadline, reporter, runner)?;
        reporter.report(OperationPhase::Verifying, None)?;
        plan.verify(deadline)?;
        attempted = true;
        let output = publish_tracking(
            &plan.git,
            &plan.repo,
            &plan.tracking_ref,
            &oid,
            &plan.old_oid,
            deadline,
        );
        let actual = plan.current_oid(deadline)?;
        if actual == oid {
            *plan
                .last_fetched_at
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(super::unix_millis());
            return Ok(());
        }
        if output.as_ref().is_ok_and(|out| !out.success) {
            attempted = false;
            return Err(OperationError::new("REMOTE_CHANGED"));
        }
        Err(OperationError::new("WRITE_OUTCOME_UNKNOWN"))
    })();
    operation_result(
        reporter, &plan.git, &plan.repo, result, attempted, None, None, deadline,
    )
}

/// 比较交换只操作该引用自身，不跟随执行前被外部替换的符号引用。
fn publish_tracking(
    git: &GitExecutable,
    repo: &RepositoryHandle,
    reference: &str,
    new_oid: &str,
    old_oid: &str,
    deadline: Instant,
) -> Result<ProcessOutput, OperationError> {
    let args = ["update-ref", "--no-deref", reference, new_oid, old_oid].map(OsString::from);
    run_local_git(git, &repo.root, &args, &[], None, deadline)
}

#[cfg(test)]
mod tests;
