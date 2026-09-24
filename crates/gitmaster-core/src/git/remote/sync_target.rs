//! 同步入口仅解析当前分支的真实配置，首次上游设置不能依据缺失跟踪引用推断。
use super::*;
use crate::git::{
    coordinator::{CoordinationKey, RepositoryCoordinator},
    merge::prepare_integrate,
    IntegrateMode, WritePreview,
};
use crate::git::{write_guard::validate_snapshot, SyncTarget, SyncUpstream, SyncUpstreamReason};

/// 同步快进只从当前分支上游取得目标，准备结束再次绑定原配置与快照。
pub fn prepare_sync_fast_forward(
    coordinator: &RepositoryCoordinator,
    git: &GitExecutable,
    repo: &RepositoryHandle,
    state: &RepositoryState,
    remotes: &RemoteSession,
) -> Result<WritePreview, OperationError> {
    let key = CoordinationKey::repository(repo)?;
    let target = coordinator.read(&key, || remotes.sync_target(state))?;
    let branch = match target.upstream {
        SyncUpstream::Configured {
            remote_branch_id: Some(id),
            ..
        } => id,
        SyncUpstream::Configured {
            remote_branch_id: None,
            ..
        } => return Err(OperationError::new("SYNC_TARGET_MISSING")),
        _ => return Err(OperationError::new("SYNC_UPSTREAM_REQUIRED")),
    };
    let preview = prepare_integrate(
        coordinator,
        git,
        repo,
        state,
        remotes,
        &branch,
        IntegrateMode::FastForward,
    )?;
    // 原配置变更时不向桌面交付计划；执行仍复用整合指纹和检出前干净状态核验。
    coordinator.read(&key, || {
        let deadline = Instant::now() + BUDGET;
        validate_snapshot(git, repo, state, deadline)?;
        let config = inspect_local_config(git, &repo.root, deadline)?;
        if <[u8; 32]>::from(Sha256::digest(config)) != remotes.config_digest {
            return Err(OperationError::new("REMOTE_CHANGED"));
        }
        Ok(())
    })?;
    Ok(preview)
}

/// 只保留当前分支的两个上游配置值，重复项也必须保留以识别歧义。
fn upstream_values(
    config: &[u8],
    branch: &str,
) -> Result<(Vec<String>, Vec<String>), OperationError> {
    let remote_key = format!("branch.{branch}.remote");
    let merge_key = format!("branch.{branch}.merge");
    let mut remotes = Vec::new();
    let mut merges = Vec::new();
    for record in config
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
    {
        let split = record.iter().position(|byte| *byte == b'\n');
        let (key, value) = match split {
            Some(index) => (&record[..index], &record[index + 1..]),
            None => (record, &b""[..]),
        };
        if key == remote_key.as_bytes() {
            remotes.push(text(value)?);
        }
        if key == merge_key.as_bytes() {
            merges.push(text(value)?);
        }
    }
    Ok((remotes, merges))
}

/// 配置必须同时存在且各只有一个值；本地仓库上游不属于远程同步。
fn configured_pair<'a>(
    remotes: &'a [String],
    merges: &'a [String],
) -> Result<Option<(&'a str, &'a str)>, SyncUpstreamReason> {
    if remotes.is_empty() && merges.is_empty() {
        return Ok(None);
    }
    if remotes.len() > 1 || merges.len() > 1 {
        return Err(SyncUpstreamReason::Ambiguous);
    }
    if remotes.len() != 1 || merges.len() != 1 || remotes[0].is_empty() || merges[0].is_empty() {
        return Err(SyncUpstreamReason::Incomplete);
    }
    if remotes[0] == "." {
        return Err(SyncUpstreamReason::LocalRepository);
    }
    if !merges[0].starts_with("refs/heads/") || merges[0] == "refs/heads/" {
        return Err(SyncUpstreamReason::UnsupportedTarget);
    }
    Ok(Some((&remotes[0], &merges[0])))
}

impl RemoteSession {
    /// 在具名、干净且无进行中操作的当前快照上解析同步目标，不联网或修改配置。
    pub fn sync_target(&self, state: &RepositoryState) -> Result<SyncTarget, OperationError> {
        let deadline = Instant::now() + BUDGET;
        validate_snapshot(&self.git, &self.repo, state, deadline)?;
        let name = match &state.head {
            HeadState::Branch { name, .. } => name,
            HeadState::Unborn { .. } => return Err(OperationError::new("HEAD_REQUIRED")),
            HeadState::Detached { .. } => {
                return Err(OperationError::new("DETACHED_HEAD_WRITE_BLOCKED"))
            }
        };
        if !state.changes.is_empty() {
            return Err(OperationError::new("WORKTREE_DIRTY"));
        }
        if !state.operations.is_empty() {
            return Err(OperationError::new("OPERATION_IN_PROGRESS"));
        }
        let config = inspect_local_config(&self.git, &self.repo.root, deadline)?;
        if <[u8; 32]>::from(Sha256::digest(&config)) != self.config_digest {
            return Err(OperationError::new("REMOTE_CHANGED"));
        }
        let (remotes, merges) = upstream_values(&config, name)?;
        let upstream = match configured_pair(&remotes, &merges) {
            Ok(None) => SyncUpstream::Missing,
            Err(reason) => SyncUpstream::Unresolved { reason },
            Ok(Some((remote_name, target_ref))) => {
                self.resolve_sync_upstream(name, remote_name, target_ref)
            }
        };
        validate_snapshot(&self.git, &self.repo, state, deadline)?;
        if inspect_local_config(&self.git, &self.repo.root, deadline)? != config {
            return Err(OperationError::new("REMOTE_CHANGED"));
        }
        Ok(SyncTarget {
            repository_id: self.repo.id.clone(),
            snapshot_id: state.snapshot_id.clone(),
            branch_name: name.clone(),
            upstream,
        })
    }

    /// 使用 Git 解析的完整跟踪引用匹配后端 ID，不按斜杠拆远端或分支名。
    fn resolve_sync_upstream(
        &self,
        branch: &str,
        remote_name: &str,
        target_ref: &str,
    ) -> SyncUpstream {
        let Some((remote_id, _)) = self
            .remotes
            .iter()
            .find(|(_, remote)| remote.name == remote_name)
        else {
            return SyncUpstream::Unresolved {
                reason: SyncUpstreamReason::RemoteMissing,
            };
        };
        let mapping = self.branch_upstreams.iter().find(|upstream| {
            upstream.branch_name == branch
                && upstream.remote_name == remote_name
                && upstream.target_ref == target_ref
                && upstream.tracking_ref.starts_with("refs/remotes/")
        });
        let Some(mapping) = mapping else {
            return SyncUpstream::Unresolved {
                reason: SyncUpstreamReason::MappingMissing,
            };
        };
        let mut remote_branch_id = self
            .branches
            .iter()
            .find(|(_, record)| record.reference == mapping.tracking_ref)
            .map(|(id, _)| id.clone());
        let names = self
            .fetched_branches
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if names.get(remote_name).is_some_and(|branches| {
            !branches
                .iter()
                .any(|branch| Some(branch.as_str()) == target_ref.strip_prefix("refs/heads/"))
        }) {
            remote_branch_id = None;
        }
        SyncUpstream::Configured {
            remote_id: remote_id.clone(),
            target_branch_name: target_ref
                .strip_prefix("refs/heads/")
                .unwrap_or(target_ref)
                .to_owned(),
            remote_branch_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// 当前 main 跟踪 release 时，快进必须读取上游映射而非远端列表的首项。
    #[test]
    fn sync_fast_forward_uses_current_upstream_only() {
        use crate::git::{
            repository::{open_repository, tests::Fixture},
            OperationResult,
        };
        let f = Fixture::new();
        f.write("file", b"base");
        f.command(&["add", "."]);
        f.command(&["commit", "-m", "base"]);
        f.command(&["branch", "unselected"]);
        f.command(&["update-ref", "refs/remotes/origin/aaa", "HEAD"]);
        f.command(&["switch", "-c", "incoming"]);
        f.write("file", b"incoming");
        f.command(&["add", "."]);
        f.command(&["commit", "-m", "incoming"]);
        f.command(&["update-ref", "refs/remotes/origin/release", "HEAD"]);
        f.command(&["switch", "main"]);
        f.command(&["remote", "add", "origin", "https://fixture.invalid/project"]);
        f.command(&["config", "branch.main.remote", "origin"]);
        f.command(&["config", "branch.main.merge", "refs/heads/release"]);
        let (repo, state) = open_repository(&f.git, &f.root).unwrap();
        let remotes = RemoteSession::new(&f.git, &repo).unwrap();
        let coordinator = RepositoryCoordinator::new();
        let preview =
            prepare_sync_fast_forward(&coordinator, &f.git, &repo, &state, &remotes).unwrap();
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
                assert!(
                    matches!(result, OperationResult::Succeeded { .. }),
                    "{result:?}"
                );
                break;
            }
            assert!(Instant::now() < deadline, "同步快进验证超时");
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(std::fs::read(f.root.join("file")).unwrap(), b"incoming");
        let heads = query_until(
            &f.git,
            &f.root,
            &[
                "rev-parse",
                "HEAD",
                "refs/remotes/origin/release",
                "unselected",
                "refs/remotes/origin/aaa",
            ],
            4096,
            Some(deadline),
        )
        .unwrap();
        let rows = std::str::from_utf8(&heads)
            .unwrap()
            .lines()
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0], rows[1]);
        assert_eq!(rows[2], rows[3]);
        assert_ne!(rows[0], rows[2]);
    }
    /// 不完整、重复及其他分支配置不能被误判为当前分支首次设置。
    #[test]
    fn upstream_config_distinguishes_missing_partial_and_ambiguous() {
        let config = b"branch.Main.remote\nteam/origin\0branch.Main.merge\nrefs/heads/release/stable\0branch.main.remote\nother\0";
        let (remotes, merges) = upstream_values(config, "Main").unwrap();
        assert_eq!(
            configured_pair(&remotes, &merges),
            Ok(Some(("team/origin", "refs/heads/release/stable")))
        );
        assert_eq!(configured_pair(&[], &[]), Ok(None));
        assert_eq!(
            configured_pair(&["origin".into()], &[]),
            Err(SyncUpstreamReason::Incomplete)
        );
        assert_eq!(
            configured_pair(
                &["origin".into(), "origin".into()],
                &["refs/heads/main".into()]
            ),
            Err(SyncUpstreamReason::Ambiguous)
        );
        assert_eq!(
            configured_pair(&[".".into()], &["refs/heads/main".into()]),
            Err(SyncUpstreamReason::LocalRepository)
        );
        assert_eq!(
            configured_pair(&["origin".into()], &["refs/tags/v1".into()]),
            Err(SyncUpstreamReason::UnsupportedTarget)
        );
    }

    /// 真实配置在尚未 fetch 时仍属于已有上游；外部修改配置使旧会话失效。
    #[test]
    fn sync_target_preserves_unfetched_upstream_and_rejects_changed_config() {
        use crate::git::repository::{open_repository, tests::Fixture};
        let fixture = Fixture::new();
        fixture.write("file", b"base");
        fixture.command(&["add", "."]);
        fixture.command(&["commit", "-m", "base"]);
        fixture.command(&[
            "remote",
            "add",
            "team/origin",
            "https://fixture.invalid/project",
        ]);
        fixture.command(&["config", "branch.main.remote", "team/origin"]);
        fixture.command(&["config", "branch.main.merge", "refs/heads/release/stable"]);
        let (repo, state) = open_repository(&fixture.git, &fixture.root).unwrap();
        let session = RemoteSession::new(&fixture.git, &repo).unwrap();
        let target = session.sync_target(&state).unwrap();
        assert_eq!(target.branch_name, "main");
        assert!(
            matches!(target.upstream, SyncUpstream::Configured { target_branch_name, remote_branch_id: None, .. } if target_branch_name == "release/stable")
        );
        fixture.command(&[
            "update-ref",
            "refs/remotes/team/origin/release/stable",
            "HEAD",
        ]);
        let session = session.refreshed().unwrap();
        assert!(matches!(
            session.sync_target(&state).unwrap().upstream,
            SyncUpstream::Configured {
                remote_branch_id: Some(_),
                ..
            }
        ));
        session
            .fetched_branches
            .lock()
            .unwrap()
            .insert("team/origin".into(), vec!["main".into()]);
        assert_eq!(session.state().remote_branches.len(), 1);
        assert!(matches!(
            session.sync_target(&state).unwrap().upstream,
            SyncUpstream::Configured {
                remote_branch_id: None,
                ..
            }
        ));
        fixture.command(&["config", "branch.main.merge", "refs/heads/other"]);
        assert_eq!(
            session.sync_target(&state).unwrap_err().code,
            "REMOTE_CHANGED"
        );
    }

    /// 同步入口区分首次设置与残缺配置，并拒绝工作区变化后的旧快照和脏状态。
    #[test]
    fn sync_target_checks_missing_partial_and_dirty_state() {
        use crate::git::repository::{open_repository, tests::Fixture};
        let fixture = Fixture::new();
        fixture.write("file", b"base");
        fixture.command(&["add", "."]);
        fixture.command(&["commit", "-m", "base"]);
        let (repo, state) = open_repository(&fixture.git, &fixture.root).unwrap();
        let session = RemoteSession::new(&fixture.git, &repo).unwrap();
        assert!(matches!(
            session.sync_target(&state).unwrap().upstream,
            SyncUpstream::Missing
        ));
        fixture.command(&["config", "branch.main.remote", "origin"]);
        let session = RemoteSession::new(&fixture.git, &repo).unwrap();
        assert!(matches!(
            session.sync_target(&state).unwrap().upstream,
            SyncUpstream::Unresolved {
                reason: SyncUpstreamReason::Incomplete
            }
        ));
        fixture.write("untracked", b"local");
        assert_eq!(
            session.sync_target(&state).unwrap_err().code,
            "STALE_REQUEST"
        );
        let dirty =
            read_repository_state_until(&fixture.git, &repo, Instant::now() + BUDGET).unwrap();
        assert_eq!(
            session.sync_target(&dirty).unwrap_err().code,
            "WORKTREE_DIRTY"
        );
    }
}
