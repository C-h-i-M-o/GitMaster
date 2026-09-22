//! 单目标普通推送，准备和结果核验均读取真实远端引用。
use super::{auth::AuthPolicy, url, RemoteSession};
use crate::git::{
    coordinator::{CoordinationKey, OperationReporter, RepositoryCoordinator},
    process::{inspect_local_config, run_isolated_git, run_network_git, ProcessOutput},
    repository::{query_until, read_repository_state_until},
    write::operation_result,
    *,
};
use sha2::{Digest, Sha256};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const NETWORK_BUDGET: Duration = Duration::from_secs(15 * 60);
const MAX_COMMITS: usize = 1000;

/// 生产入口固定受限网络执行器，测试只替换传输层。
type NetworkRunner = fn(
    &GitExecutable,
    &Path,
    &[OsString],
    Option<&Path>,
    &[(String, String)],
    Instant,
    &mut dyn FnMut(u64, u64),
) -> Result<ProcessOutput, OperationError>;

/// 私有 bare 环境隔离推送配置和本地 refs，源 OID 与目标地址在准备阶段固定。
struct PushPlan {
    git: GitExecutable,
    repo: RepositoryHandle,
    head: HeadState,
    auth: AuthPolicy,
    config_digest: [u8; 32],
    url: String,
    target_ref: String,
    source: String,
    old_oid: Option<String>,
    objects: PathBuf,
    scratch: tempfile::TempDir,
}

/// 查询真实远端并准备单目标普通推送；不更新本地或远端引用。
pub fn prepare_push(
    coordinator: &RepositoryCoordinator,
    git: &GitExecutable,
    repo: &RepositoryHandle,
    state: &RepositoryState,
    remotes: &RemoteSession,
    remote_id: &str,
    target_branch: &str,
) -> Result<WritePreview, OperationError> {
    prepare_with_runner(
        coordinator,
        git,
        repo,
        state,
        remotes,
        remote_id,
        target_branch,
        run_network_git,
    )
}

/// 共同准备流程只允许替换网络传输层，保留真实仓库、配置和结果校验。
fn prepare_with_runner(
    coordinator: &RepositoryCoordinator,
    git: &GitExecutable,
    repo: &RepositoryHandle,
    state: &RepositoryState,
    remotes: &RemoteSession,
    remote_id: &str,
    target_branch: &str,
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
    if remote.push.len() != 1 {
        return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
    }
    let selected_url = remote.push[0].clone();
    url::validate(&selected_url)?;
    if target_branch.is_empty() || target_branch.starts_with('-') || target_branch == "HEAD" {
        return Err(OperationError::new("INVALID_BRANCH_NAME"));
    }
    let target_ref = format!("refs/heads/{target_branch}");
    let source = match &state.head {
        HeadState::Branch { oid, .. } | HeadState::Detached { oid } => oid.clone(),
        HeadState::Unborn { .. } => return Err(OperationError::new("HEAD_REQUIRED")),
    };
    let deadline = Instant::now() + NETWORK_BUDGET;
    let key = CoordinationKey::repository(repo)?;
    let (plan, pending_commits) = coordinator.read_until(&key, deadline, || {
        query_until(
            git,
            &repo.root,
            &["check-ref-format", &target_ref],
            4096,
            Some(deadline),
        )
        .map_err(|error| {
            if error.code == "GIT_EXECUTION_FAILED" {
                OperationError::new("INVALID_BRANCH_NAME")
            } else {
                error
            }
        })?;
        let effective = inspect_local_config(git, &repo.root, deadline)?;
        if <[u8; 32]>::from(Sha256::digest(effective)) != remotes.config_digest {
            return Err(OperationError::new("STALE_WRITE_PLAN"));
        }
        let auth = AuthPolicy::capture(git, &repo.root, deadline)?;
        validate_push_config(&auth, &remote.name)?;
        let objects = std::fs::canonicalize(repo.common_dir.join("objects"))
            .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
        let format = query_until(
            git,
            &repo.root,
            &["rev-parse", "--show-object-format"],
            4096,
            Some(deadline),
        )?;
        let format = super::text(&format)?;
        let format = format.trim();
        if !matches!(format, "sha1" | "sha256") {
            return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
        }
        let scratch = tempfile::Builder::new()
            .prefix("gitmaster-push-")
            .tempdir()
            .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
        let template = scratch.path().join("empty-template");
        std::fs::create_dir(&template).map_err(|_| OperationError::new("ACCESS_DENIED"))?;
        let mut template_arg = OsString::from("--template=");
        template_arg.push(template);
        let args = vec![
            "init".into(),
            "--bare".into(),
            template_arg,
            format!("--object-format={format}").into(),
        ];
        let init = run_isolated_git(git, scratch.path(), &args, &[], None, &[], deadline)?;
        if !init.success {
            return Err(OperationError::new("GIT_EXECUTION_FAILED"));
        }
        let mut plan = PushPlan {
            git: git.clone(),
            repo: repo.clone(),
            head: state.head.clone(),
            auth,
            config_digest: remotes.config_digest,
            url: selected_url.clone(),
            target_ref: target_ref.clone(),
            source: source.clone(),
            old_oid: None,
            objects,
            scratch,
        };
        plan.verify(deadline)?;
        plan.old_oid = plan.remote_oid(runner, deadline)?;
        let pending = plan.pending_commits(deadline)?;
        // 网络预览期间本地可以被外部改动，返回确认框前再次验证固定来源。
        plan.verify(deadline)?;
        Ok((plan, pending))
    })?;
    let old_oid = plan.old_oid.clone();
    let is_new = old_oid.is_none();
    let operation = coordinator.prepare(
        generation,
        Some(repo.id.clone()),
        key,
        OperationKind::Push,
        move |reporter| execute_push(reporter, &plan, runner),
    )?;
    let mut warnings = vec![
        "仅普通推送所列提交到一个分支，不设置上游或上传标签。".into(),
        "将连接所示推送地址，可能调用系统凭据助手或 SSH agent。".into(),
    ];
    if is_new {
        warnings.push("目标分支不存在，将新建该分支。".into());
    }
    Ok(WritePreview {
        plan_id: operation.plan_id,
        repository_id: repo.id.clone(),
        snapshot_id: state.snapshot_id.clone(),
        kind: OperationKind::Push,
        head: state.head.clone(),
        parent_oids: Vec::new(),
        paths: Vec::new(),
        author: None,
        message: None,
        target: Some(WriteTarget::Remote {
            remote_id: remote_id.into(),
            display_url: url::display(&selected_url),
            ref_name: target_ref,
            oid: old_oid,
            source_oid: Some(source),
            pending_commits,
        }),
        warnings,
        expires_at: operation.expires_at,
    })
}

impl PushPlan {
    /// 私有环境执行本地对象查询，既不继承 replace refs，也不写入原索引或引用。
    fn local(&self, args: &[&str], deadline: Instant) -> Result<ProcessOutput, OperationError> {
        run_isolated_git(
            &self.git,
            self.scratch.path(),
            &args.iter().map(OsString::from).collect::<Vec<_>>(),
            &[],
            Some(&self.objects),
            &[],
            deadline,
        )
    }
    /// 固定配置、对象目录和 HEAD，不因无关工作文件脏而禁止上传已提交内容。
    fn verify(&self, deadline: Instant) -> Result<(), OperationError> {
        self.auth.verify(&self.git, &self.repo.root, deadline)?;
        validate_push_hook(&self.repo, &self.auth)?;
        let shallow = query_until(
            &self.git,
            &self.repo.root,
            &["rev-parse", "--is-shallow-repository"],
            4096,
            Some(deadline),
        )?;
        if shallow != b"false\n" {
            return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
        }
        let effective = inspect_local_config(&self.git, &self.repo.root, deadline)?;
        let objects = std::fs::canonicalize(self.repo.common_dir.join("objects"))
            .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
        let fresh = read_repository_state_until(&self.git, &self.repo, deadline)?;
        if <[u8; 32]>::from(Sha256::digest(effective)) != self.config_digest
            || objects != self.objects
            || fresh.head != self.head
        {
            return Err(OperationError::new("STALE_WRITE_PLAN"));
        }
        Ok(())
    }
    /// 精确读取完整目标引用，空输出才表示不存在；失败不能冒充新分支。
    fn remote_oid(
        &self,
        runner: NetworkRunner,
        deadline: Instant,
    ) -> Result<Option<String>, OperationError> {
        let args = ["ls-remote", "--refs", "--", &self.url, &self.target_ref].map(OsString::from);
        let output = runner(
            &self.git,
            self.scratch.path(),
            &args,
            Some(&self.objects),
            &self.auth.settings,
            deadline,
            &mut |_, _| {},
        )?;
        if !output.success {
            return Err(OperationError::new("NETWORK_FAILED"));
        }
        parse_remote_oid(&output.stdout, &self.target_ref, self.source.len())
    }
    /// 完整展示源提交相对已确认远端祖先的历史，检测第 1001 条而非静默截断。
    fn pending_commits(&self, deadline: Instant) -> Result<Vec<CommitSummary>, OperationError> {
        if let Some(old) = &self.old_oid {
            let ancestor = self.local(
                &["merge-base", "--is-ancestor", old, &self.source],
                deadline,
            )?;
            if !ancestor.success {
                return Err(OperationError::new(if ancestor.exit_code == Some(1) {
                    "NON_FAST_FORWARD"
                } else {
                    "REMOTE_CHANGED"
                }));
            }
        }
        let excluded = self.old_oid.as_ref().map(|oid| format!("^{oid}"));
        let mut args = vec![
            "log",
            "-z",
            "--topo-order",
            "--max-count=1001",
            "--no-use-mailmap",
            "--no-decorate",
            "--no-notes",
            "--encoding=UTF-8",
            "--format=format:%H%x00%P%x00%an%x00%aI%x00%s",
            &self.source,
        ];
        if let Some(excluded) = &excluded {
            args.push(excluded);
        }
        args.push("--");
        let output = self.local(&args, deadline)?;
        if !output.success {
            return Err(OperationError::new("GIT_EXECUTION_FAILED"));
        }
        if output.stdout.is_empty() {
            return Ok(Vec::new());
        }
        let fields: Vec<_> = output.stdout.split(|b| *b == 0).collect();
        if fields.len() % 5 != 0 {
            return Err(OperationError::new("PARSE_FAILED"));
        }
        if fields.len() / 5 > MAX_COMMITS {
            return Err(OperationError::new("OUTPUT_LIMIT"));
        }
        fields
            .chunks_exact(5)
            .map(|part| {
                let oid = super::text(part[0])?;
                let parents = super::text(part[1])?;
                if !valid_oid(&oid, self.source.len())
                    || parents
                        .split_whitespace()
                        .any(|oid| !valid_oid(oid, self.source.len()))
                {
                    return Err(OperationError::new("PARSE_FAILED"));
                }
                Ok(CommitSummary {
                    oid,
                    parent_oids: parents.split_whitespace().map(str::to_owned).collect(),
                    author_name: super::text(part[2])?,
                    authored_at: super::text(part[3])?,
                    subject: super::text(part[4])?,
                })
            })
            .collect()
    }
}

/// 推送自己的额外程序、签名和选项要求必须显式拒绝，不把它们静默忽略。
fn validate_push_config(auth: &AuthPolicy, remote: &str) -> Result<(), OperationError> {
    for key in [
        format!("remote.{remote}.uploadpack"),
        format!("remote.{remote}.receivepack"),
        format!("remote.{remote}.proxy"),
        format!("remote.{remote}.vcs"),
        "push.pushoption".into(),
    ] {
        if !auth.values(&key).is_empty() {
            return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
        }
    }
    for key in [format!("remote.{remote}.mirror"), "push.gpgsign".into()] {
        if auth.values(&key).last().is_some_and(|value| {
            !matches!(
                value.to_ascii_lowercase().as_str(),
                "false" | "no" | "off" | "0"
            )
        }) {
            return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
        }
    }
    Ok(())
}

/// 客户端 pre-push 的相对路径以工作树为基准，准备后新增 hook 也使执行拒绝。
fn validate_push_hook(repo: &RepositoryHandle, auth: &AuthPolicy) -> Result<(), OperationError> {
    let configured = auth.values("core.hookspath");
    let hooks = match configured.last().copied() {
        Some("") => return Ok(()),
        Some(path) if path.starts_with('~') => {
            return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"))
        }
        Some(path) => repo.root.join(path),
        None => repo.common_dir.join("hooks"),
    };
    match std::fs::metadata(hooks.join("pre-push")) {
        Ok(metadata) => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if metadata.permissions().mode() & 0o111 != 0 {
                    return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
                }
            }
            #[cfg(not(unix))]
            if metadata.is_file() {
                return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
            }
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(OperationError::new("ACCESS_DENIED")),
    }
}

/// 目标 OID 必须匹配本地对象格式，不能把错误输出当引用使用。
fn valid_oid(value: &str, length: usize) -> bool {
    matches!(length, 40 | 64)
        && value.len() == length
        && value.bytes().all(|b| b.is_ascii_hexdigit())
}

/// ls-remote 模式会按尾部匹配，必须再次确认返回的是唯一完整目标。
fn parse_remote_oid(
    bytes: &[u8],
    target: &str,
    length: usize,
) -> Result<Option<String>, OperationError> {
    if bytes.is_empty() {
        return Ok(None);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| OperationError::new("PARSE_FAILED"))?;
    let line = text.strip_suffix('\n').unwrap_or(text);
    let (oid, name) = line
        .split_once('\t')
        .ok_or_else(|| OperationError::new("PARSE_FAILED"))?;
    if name != target || !valid_oid(oid, length) {
        return Err(OperationError::new("PARSE_FAILED"));
    }
    Ok(Some(oid.into()))
}

/// 仅识别机器格式的单目标拒绝行，不依赖 stderr 或本地化说明来猜测成功。
fn explicitly_rejected(output: &ProcessOutput, source: &str, target: &str) -> bool {
    let expected = format!("{source}:{target}");
    output.stdout.split(|b| *b == b'\n').any(|line| {
        let fields: Vec<_> = line.split(|b| *b == b'\t').collect();
        fields.len() == 3 && fields[0] == b"!" && fields[1] == expected.as_bytes()
    })
}

/// 上传只能执行一次；传输后查询实际目标，不能将连接失败推断成没有远端影响。
fn execute_push(
    reporter: &OperationReporter,
    plan: &PushPlan,
    runner: NetworkRunner,
) -> OperationResult {
    let deadline = reporter.deadline(NETWORK_BUDGET);
    let mut attempted = false;
    let result = (|| {
        plan.verify(deadline)?;
        if plan.remote_oid(runner, deadline)? != plan.old_oid {
            return Err(OperationError::new("REMOTE_CHANGED"));
        }
        // 远端查询可能等待认证，本地固定来源在真正上传前再核对一次。
        plan.verify(deadline)?;
        reporter.report(OperationPhase::Transferring, None)?;
        let refspec = format!("{}:{}", plan.source, plan.target_ref);
        let args = [
            "push",
            "--porcelain",
            "--progress",
            "--no-follow-tags",
            "--recurse-submodules=no",
            "--no-signed",
            "--",
            &plan.url,
            &refspec,
        ]
        .map(OsString::from);
        attempted = true;
        let output = runner(
            &plan.git,
            plan.scratch.path(),
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
        );
        let _ = reporter.report(OperationPhase::Verifying, None);
        let actual = plan.remote_oid(runner, deadline)?;
        if actual.as_deref() == Some(plan.source.as_str()) {
            return Ok(());
        }
        if output
            .as_ref()
            .is_ok_and(|out| explicitly_rejected(out, &plan.source, &plan.target_ref))
        {
            attempted = false;
            return Err(OperationError::new("REMOTE_REJECTED"));
        }
        Err(OperationError::new("WRITE_OUTCOME_UNKNOWN"))
    })();
    let success = result.is_ok();
    operation_result(
        reporter,
        &plan.git,
        &plan.repo,
        result,
        attempted,
        success.then(|| plan.source.clone()),
        success.then(|| plan.target_ref.trim_start_matches("refs/heads/").to_owned()),
        deadline,
    )
}

#[cfg(test)]
mod tests;
