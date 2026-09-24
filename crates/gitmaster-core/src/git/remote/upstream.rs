//! 用户明确选定目标后，仅修改当前具名分支的上游，并核验真实配置结果。
use super::{auth::AuthPolicy, *};
use crate::git::{
    coordinator::{CoordinationKey, OperationReporter, RepositoryCoordinator},
    process::run_local_git,
    write::operation_result,
    write_guard::validate_snapshot,
    *,
};

/// 参数与预览冻结，执行不能改成另一分支或另一远端。
struct UpstreamPlan {
    git: GitExecutable,
    repo: RepositoryHandle,
    state: RepositoryState,
    config: [u8; 32],
    branch: String,
    remote: String,
    target: String,
    tracking: String,
    oid: String,
}

/// 判断一个 fetch 映射是否可能拥有所选跟踪引用，排除歧义远端。
fn owns_reference(spec: &str, reference: &str) -> bool {
    let Some((_, destination)) = spec.trim_start_matches('+').split_once(':') else {
        return false;
    };
    if let Some((prefix, suffix)) = destination.split_once('*') {
        reference.starts_with(prefix)
            && reference.ends_with(suffix)
            && reference.len() >= prefix.len() + suffix.len()
    } else {
        destination == reference
    }
}

/// 准备设置已有目标的上游，不访问网络或创建远端分支。
pub fn prepare_set_upstream(
    coordinator: &RepositoryCoordinator,
    git: &GitExecutable,
    repo: &RepositoryHandle,
    state: &RepositoryState,
    remotes: &RemoteSession,
    remote_id: &str,
    target_branch: &str,
) -> Result<WritePreview, OperationError> {
    let generation = coordinator.begin_prepare()?;
    let deadline = Instant::now() + BUDGET;
    let key = CoordinationKey::repository(repo)?;
    let plan = coordinator.read_until(&key, deadline, || {
        remotes.sync_target(state)?;
        let HeadState::Branch { name, .. } = &state.head else {
            return Err(OperationError::new("HEAD_REQUIRED"));
        };
        let remote = remotes
            .remotes
            .get(remote_id)
            .ok_or_else(|| OperationError::new("REMOTE_NOT_FOUND"))?;
        if target_branch.is_empty() || target_branch.starts_with('-') {
            return Err(OperationError::new("INVALID_BRANCH_NAME"));
        }
        let target = format!("refs/heads/{target_branch}");
        query_until(
            git,
            &repo.root,
            &["check-ref-format", &target],
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
        let tracking = format!("refs/remotes/{}/{target_branch}", remote.name);
        if remotes
            .fetched_branches
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&remote.name)
            .is_some_and(|branches| !branches.iter().any(|branch| branch == target_branch))
        {
            return Err(OperationError::new("SYNC_TARGET_MISSING"));
        }
        let auth = AuthPolicy::capture(git, &repo.root, deadline)?;
        let expected = format!("+refs/heads/*:refs/remotes/{}/*", remote.name);
        let specs = auth.values(&format!("remote.{}.fetch", remote.name));
        if specs.len() != 1
            || (specs[0] != expected && specs[0] != expected.trim_start_matches('+'))
        {
            return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
        }
        if remotes.remotes.values().any(|other| {
            other.name != remote.name
                && auth
                    .values(&format!("remote.{}.fetch", other.name))
                    .iter()
                    .any(|spec| owns_reference(spec, &tracking))
        }) {
            return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
        }
        let branch = remotes
            .branches
            .values()
            .find(|branch| branch.reference == tracking)
            .ok_or_else(|| OperationError::new("SYNC_TARGET_MISSING"))?;
        let plan = UpstreamPlan {
            git: git.clone(),
            repo: repo.clone(),
            state: state.clone(),
            config: remotes.config_digest,
            branch: name.clone(),
            remote: remote.name.clone(),
            target,
            tracking,
            oid: branch.oid.clone(),
        };
        plan.verify(deadline)?;
        Ok(plan)
    })?;
    let target = WriteTarget::Remote {
        remote_id: remote_id.to_owned(),
        display_url: display_urls(&remotes.remotes[remote_id].fetch),
        ref_name: plan.target.clone(),
        oid: Some(plan.oid.clone()),
        source_oid: None,
        pending_commits: Vec::new(),
    };
    let operation = coordinator.prepare(
        generation,
        Some(repo.id.clone()),
        key,
        OperationKind::SetUpstream,
        move |reporter| plan.execute(reporter),
    )?;
    Ok(WritePreview {
        plan_id: operation.plan_id,
        repository_id: repo.id.clone(),
        snapshot_id: state.snapshot_id.clone(),
        kind: OperationKind::SetUpstream,
        head: state.head.clone(),
        parent_oids: Vec::new(),
        paths: Vec::new(),
        author: None,
        message: None,
        target: Some(target),
        warnings: Vec::new(),
        expires_at: operation.expires_at,
    })
}

impl UpstreamPlan {
    /// 外部配置、工作区或跟踪引用变化时不写入旧计划。
    fn verify(&self, deadline: Instant) -> Result<(), OperationError> {
        validate_snapshot(&self.git, &self.repo, &self.state, deadline)?;
        if <[u8; 32]>::from(Sha256::digest(inspect_local_config(
            &self.git,
            &self.repo.root,
            deadline,
        )?)) != self.config
        {
            return Err(OperationError::new("REMOTE_CHANGED"));
        }
        let refs = query_until(
            &self.git,
            &self.repo.root,
            &[
                "for-each-ref",
                "--format=%(refname)%00%(objectname)%00%(symref)",
                &self.tracking,
            ],
            4096,
            Some(deadline),
        )?;
        if refs != format!("{}\0{}\0\n", self.tracking, self.oid).as_bytes() {
            return Err(OperationError::new("REMOTE_CHANGED"));
        }
        Ok(())
    }
    /// 使用 Git 自身配置写入并核对上游；写后不确定时保留未知结果。
    fn execute(self, reporter: &OperationReporter) -> OperationResult {
        let deadline = reporter.deadline(BUDGET);
        let mut attempted = false;
        let result = (|| {
            self.verify(deadline)?;
            reporter.report(OperationPhase::Writing, None)?;
            let args = [
                "-c".into(),
                "branch.autoSetupRebase=never".into(),
                "branch".into(),
                format!("--set-upstream-to={}", self.tracking).into(),
                "--".into(),
                self.branch.clone().into(),
            ];
            attempted = true;
            let output = run_local_git(&self.git, &self.repo.root, &args, &[], None, deadline)?;
            if !output.success {
                return Err(OperationError::new("GIT_EXECUTION_FAILED"));
            }
            reporter.report(OperationPhase::Verifying, None)?;
            validate_snapshot(&self.git, &self.repo, &self.state, deadline)?;
            let refreshed = RemoteSession::new(&self.git, &self.repo)?;
            let actual = refreshed.sync_target(&self.state)?;
            let SyncUpstream::Configured {
                remote_id,
                target_branch_name,
                ..
            } = actual.upstream
            else {
                return Err(OperationError::new("WRITE_OUTCOME_UNKNOWN"));
            };
            if refreshed.remotes[&remote_id].name != self.remote
                || format!("refs/heads/{target_branch_name}") != self.target
            {
                return Err(OperationError::new("WRITE_OUTCOME_UNKNOWN"));
            }
            Ok(())
        })();
        operation_result(
            reporter,
            &self.git,
            &self.repo,
            result,
            attempted,
            None,
            Some(self.branch.clone()),
            deadline,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 设置不同名上游只更改当前分支配置，准备后外部配置变化必须拒绝。
    #[test]
    fn sets_only_current_upstream_and_rejects_stale_configuration() {
        use crate::git::repository::{open_repository, tests::Fixture};
        let f = Fixture::new();
        f.write("file", b"base");
        f.command(&["add", "."]);
        f.command(&["commit", "-m", "base"]);
        f.command(&["branch", "other"]);
        f.command(&["config", "branch.autoSetupRebase", "always"]);
        f.command(&["config", "branch.main.rebase", "false"]);
        f.command(&[
            "remote",
            "add",
            "team/origin",
            "https://fixture.invalid/project",
        ]);
        f.command(&["update-ref", "refs/remotes/team/origin/release", "HEAD"]);
        f.command(&["config", "branch.other.remote", "preserved"]);
        f.command(&["config", "branch.other.merge", "refs/heads/other"]);
        let (repo, state) = open_repository(&f.git, &f.root).unwrap();
        let before_head = std::fs::read(repo.git_dir.join("HEAD")).unwrap();
        let before_index = std::fs::read(repo.git_dir.join("index")).unwrap();
        let coordinator = RepositoryCoordinator::new();
        let session = RemoteSession::new(&f.git, &repo).unwrap();
        let remote_id = session.state().remotes[0].remote_id.clone();
        let preview = prepare_set_upstream(
            &coordinator,
            &f.git,
            &repo,
            &state,
            &session,
            &remote_id,
            "release",
        )
        .unwrap();
        let result = finish(&coordinator, &repo, &preview);
        assert!(
            matches!(result, OperationResult::Succeeded { .. }),
            "{result:?}"
        );
        let config = query_until(
            &f.git,
            &repo.root,
            &["config", "--get-regexp", "^branch\\."],
            4096,
            None,
        )
        .unwrap();
        let config = String::from_utf8(config).unwrap();
        assert!(config.contains("branch.main.remote team/origin"));
        assert!(config.contains("branch.main.merge refs/heads/release"));
        assert!(config.contains("branch.main.rebase false"));
        assert!(config.contains("branch.other.remote preserved"));
        assert!(config.contains("branch.other.merge refs/heads/other"));
        assert_eq!(
            std::fs::read(repo.git_dir.join("HEAD")).unwrap(),
            before_head
        );
        assert_eq!(
            std::fs::read(repo.git_dir.join("index")).unwrap(),
            before_index
        );
        let session = RemoteSession::new(&f.git, &repo).unwrap();
        let remote_id = session.state().remotes[0].remote_id.clone();
        let preview = prepare_set_upstream(
            &coordinator,
            &f.git,
            &repo,
            &state,
            &session,
            &remote_id,
            "release",
        )
        .unwrap();
        f.command(&["config", "branch.main.merge", "refs/heads/external"]);
        assert!(
            matches!(finish(&coordinator, &repo, &preview), OperationResult::Failed { error, .. } if error.code == "REMOTE_CHANGED")
        );
        assert_eq!(
            query_until(
                &f.git,
                &repo.root,
                &["config", "branch.main.merge"],
                4096,
                None
            )
            .unwrap(),
            b"refs/heads/external\n"
        );
    }

    /// 等待同一个真实任务的终态，不重复执行设置配置。
    fn finish(
        coordinator: &RepositoryCoordinator,
        repo: &RepositoryHandle,
        preview: &WritePreview,
    ) -> OperationResult {
        let handle = coordinator
            .execute(Some(&repo.id), &preview.plan_id)
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(180);
        loop {
            if let Some(result) = coordinator
                .read_operation(Some(&handle.operation_id))
                .unwrap()
                .unwrap()
                .result
            {
                return result;
            }
            assert!(Instant::now() < deadline, "设置上游验证超时");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
