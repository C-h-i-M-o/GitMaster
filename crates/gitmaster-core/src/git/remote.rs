//! 远端只读快照、地址脱敏与本地跟踪引用评估。

mod auth;
mod clone;
mod fetch;
mod push;
mod url;
pub use clone::prepare_clone;
pub use fetch::prepare_fetch;
pub use push::prepare_push;
use sha2::{Digest, Sha256};

use super::{
    process::{inspect_local_config, run_git_read_until},
    repository::{next_id, query_until, read_repository_state_until},
    types::{
        HeadState, RemoteAssessment, RemoteBranch, RemoteRelation, RemoteState, RemoteSummary,
        RepositoryHandle, RepositoryState,
    },
    GitExecutable, OperationError,
};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const BUDGET: Duration = Duration::from_secs(120);
const OUTPUT_LIMIT: usize = 8 * 1024 * 1024;
const MAX_REFS: usize = 1000;

/// 绑定一个仓库读取会话，所有远端和分支 ID 只在该会话中有效。
pub struct RemoteSession {
    git: GitExecutable,
    repo: RepositoryHandle,
    remotes: BTreeMap<String, RemoteRecord>,
    branches: BTreeMap<String, BranchRecord>,
    config_digest: [u8; 32],
    last_fetched_at: Arc<Mutex<Option<String>>>,
}

/// 保留全部有效配置中的地址，不将多个 push URL 隐藏成单一目标。
struct RemoteRecord {
    name: String,
    fetch: Vec<String>,
    push: Vec<String>,
}
/// 引用全名由后端保存，显示名称不直接用于后续查询。
struct BranchRecord {
    name: String,
    reference: String,
    oid: String,
}

impl RemoteSession {
    /// 读取系统、用户及仓库配置和本地跟踪引用，不执行网络操作。
    pub fn new(git: &GitExecutable, repo: &RepositoryHandle) -> Result<Self, OperationError> {
        let deadline = Instant::now() + BUDGET;
        let config = inspect_local_config(git, &repo.root, deadline)?;
        let mut records: BTreeMap<String, RemoteRecord> = BTreeMap::new();
        let mut rewrites = Vec::new();
        let mut push_rewrites = Vec::new();
        for record in config.split(|b| *b == 0).filter(|b| !b.is_empty()) {
            let separator = record.iter().position(|b| *b == b'\n');
            let (key, value) = match separator {
                Some(index) => (&record[..index], &record[index + 1..]),
                None => (record, &b""[..]),
            };
            let key = text(key)?;
            if let Some(rest) = key.strip_prefix("url.") {
                if let Some((base, kind)) = rest.rsplit_once('.') {
                    match kind {
                        "insteadof" => rewrites.push((text(value)?, base.to_owned())),
                        "pushinsteadof" => push_rewrites.push((text(value)?, base.to_owned())),
                        _ => {}
                    }
                }
            }
            let Some(rest) = key.strip_prefix("remote.") else {
                continue;
            };
            let Some((name, kind)) = rest.rsplit_once('.') else {
                continue;
            };
            if !matches!(kind, "url" | "pushurl") {
                continue;
            }
            let item = records
                .entry(name.to_owned())
                .or_insert_with(|| RemoteRecord {
                    name: name.to_owned(),
                    fetch: Vec::new(),
                    push: Vec::new(),
                });
            if kind == "url" {
                item.fetch.push(text(value)?);
            } else {
                item.push.push(text(value)?);
            }
        }
        if records.len() > MAX_REFS {
            return Err(OperationError::new("OUTPUT_LIMIT"));
        }
        for record in records.values_mut() {
            if record.push.is_empty() {
                record.push = record
                    .fetch
                    .iter()
                    .map(|value| rewrite_url(&rewrite_url(value, &push_rewrites), &rewrites))
                    .collect();
            } else {
                record.push = record
                    .push
                    .iter()
                    .map(|value| rewrite_url(value, &rewrites))
                    .collect();
            }
            record.fetch = record
                .fetch
                .iter()
                .map(|value| rewrite_url(value, &rewrites))
                .collect();
        }
        let refs = query_until(
            git,
            &repo.root,
            &[
                "for-each-ref",
                "--format=%(refname)%00%(objectname)%00%(symref)",
                "refs/heads",
                "refs/remotes",
            ],
            OUTPUT_LIMIT,
            Some(deadline),
        )?;
        let mut branches = BTreeMap::new();
        let mut total = 0;
        for row in refs.split(|b| *b == b'\n').filter(|r| !r.is_empty()) {
            total += 1;
            if total > MAX_REFS {
                return Err(OperationError::new("OUTPUT_LIMIT"));
            }
            let fields: Vec<&[u8]> = row.split(|b| *b == 0).collect();
            if fields.len() != 3 {
                return Err(OperationError::new("PARSE_FAILED"));
            }
            if !fields[2].is_empty() {
                continue;
            }
            let reference = text(fields[0])?;
            let Some(name) = reference.strip_prefix("refs/remotes/") else {
                continue;
            };
            let oid = text(fields[1])?;
            branches.insert(
                next_id(),
                BranchRecord {
                    name: name.to_owned(),
                    reference,
                    oid,
                },
            );
        }
        Ok(Self {
            git: git.clone(),
            repo: repo.clone(),
            remotes: records
                .into_values()
                .map(|record| (next_id(), record))
                .collect(),
            branches,
            config_digest: Sha256::digest(&config).into(),
            last_fetched_at: Arc::new(Mutex::new(None)),
        })
    }

    /// 返回绑定会话的远端、跟踪分支和脱敏配置地址；未执行获取时不伪造时间。
    pub fn state(&self) -> RemoteState {
        let mut remotes = self
            .remotes
            .iter()
            .map(|(id, r)| RemoteSummary {
                remote_id: id.clone(),
                name: r.name.clone(),
                fetch_display_url: display_urls(&r.fetch),
                push_display_url: display_urls(if r.push.is_empty() { &r.fetch } else { &r.push }),
            })
            .collect::<Vec<_>>();
        remotes.sort_by(|a, b| a.name.cmp(&b.name));
        let mut remote_branches = self
            .branches
            .iter()
            .map(|(id, b)| RemoteBranch {
                remote_branch_id: id.clone(),
                name: b.name.clone(),
                oid: b.oid.clone(),
            })
            .collect::<Vec<_>>();
        remote_branches.sort_by(|a, b| a.name.cmp(&b.name));
        RemoteState {
            repository_id: self.repo.id.clone(),
            remotes,
            remote_branches,
            last_fetched_at: self
                .last_fetched_at
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone(),
        }
    }

    /// 刷新映射和引用时保留当前应用会话已验证的获取时间。
    pub fn refreshed(&self) -> Result<Self, OperationError> {
        let mut refreshed = Self::new(&self.git, &self.repo)?;
        refreshed.last_fetched_at = self.last_fetched_at.clone();
        Ok(refreshed)
    }

    /// 评估确认的 HEAD 与跟踪 OID，整个查询链共享期限且前后重验引用。
    pub fn assess(
        &self,
        state: &RepositoryState,
        remote_branch_id: &str,
    ) -> Result<RemoteAssessment, OperationError> {
        self.assess_until(state, remote_branch_id, Instant::now() + BUDGET)
    }

    /// 整合仅取得已签发目标及固定 OID，复用整项准备期限。
    pub(crate) fn integration_target(
        &self,
        state: &RepositoryState,
        remote_branch_id: &str,
        deadline: Instant,
    ) -> Result<(RemoteAssessment, String), OperationError> {
        let assessment = self.assess_until(state, remote_branch_id, deadline)?;
        let reference = self
            .branches
            .get(remote_branch_id)
            .ok_or_else(|| OperationError::new("STALE_REQUEST"))?
            .reference
            .clone();
        Ok((assessment, reference))
    }

    /// 远端评估内部查询共享调用方剩余预算。
    fn assess_until(
        &self,
        state: &RepositoryState,
        remote_branch_id: &str,
        deadline: Instant,
    ) -> Result<RemoteAssessment, OperationError> {
        if state.repository_id != self.repo.id {
            return Err(OperationError::new("STALE_REQUEST"));
        }
        let branch = self
            .branches
            .get(remote_branch_id)
            .ok_or_else(|| OperationError::new("STALE_REQUEST"))?;
        let local = match &state.head {
            HeadState::Branch { oid, .. } | HeadState::Detached { oid } => oid,
            HeadState::Unborn { .. } => return Err(OperationError::new("HEAD_REQUIRED")),
        };
        self.validate_state(state, branch, deadline)?;
        let shallow = query_until(
            &self.git,
            &self.repo.root,
            &["rev-parse", "--is-shallow-repository"],
            4096,
            Some(deadline),
        )?;
        let (relation, ahead, behind) = match shallow.as_slice() {
            b"true\n" => (RemoteRelation::Unknown, 0, 0),
            b"false\n" => self.relation(local, &branch.oid, deadline)?,
            _ => return Err(OperationError::new("PARSE_FAILED")),
        };
        self.validate_state(state, branch, deadline)?;
        Ok(RemoteAssessment {
            repository_id: self.repo.id.clone(),
            snapshot_id: state.snapshot_id.clone(),
            remote_branch_id: remote_branch_id.to_owned(),
            local_oid: local.clone(),
            remote_oid: branch.oid.clone(),
            ahead,
            behind,
            relation,
            observed_at: unix_millis(),
        })
    }

    /// 核对本地状态与远端完整引用，删除和 OID 移动均使旧选择失效。
    fn validate_state(
        &self,
        state: &RepositoryState,
        branch: &BranchRecord,
        deadline: Instant,
    ) -> Result<(), OperationError> {
        let refs = query_until(
            &self.git,
            &self.repo.root,
            &[
                "for-each-ref",
                "--format=%(refname)%00%(objectname)%00%(symref)",
                &branch.reference,
            ],
            OUTPUT_LIMIT,
            Some(deadline),
        )?;
        let expected = format!("{}\0{}\0\n", branch.reference, branch.oid);
        if refs != expected.as_bytes() {
            return Err(OperationError::new("REMOTE_CHANGED"));
        }
        let fresh = read_repository_state_until(&self.git, &self.repo, deadline)?;
        if fresh.head != state.head
            || fresh.operations != state.operations
            || fresh.changes != state.changes
        {
            return Err(OperationError::new("STALE_REQUEST"));
        }
        Ok(())
    }

    /// 只把 merge-base 的明确无共同祖先退出码归类为 unrelated，其他失败保留错误。
    fn relation(
        &self,
        local: &str,
        remote: &str,
        deadline: Instant,
    ) -> Result<(RemoteRelation, u64, u64), OperationError> {
        let args = ["merge-base", local, remote].map(OsString::from);
        let merge = run_git_read_until(&self.git, &self.repo.root, &args, 4096, false, deadline)?;
        if !merge.success && merge.exit_code != Some(1) {
            return Err(OperationError::new("GIT_EXECUTION_FAILED"));
        }
        let range = format!("{local}...{remote}");
        let bytes = query_until(
            &self.git,
            &self.repo.root,
            &["rev-list", "--left-right", "--count", &range],
            4096,
            Some(deadline),
        )?;
        let counts = text(&bytes)?;
        let parts = counts.split_whitespace().collect::<Vec<_>>();
        if parts.len() != 2 {
            return Err(OperationError::new("PARSE_FAILED"));
        }
        let ahead: u64 = parts[0]
            .parse()
            .map_err(|_| OperationError::new("PARSE_FAILED"))?;
        let behind: u64 = parts[1]
            .parse()
            .map_err(|_| OperationError::new("PARSE_FAILED"))?;
        let relation = if !merge.success {
            return Ok((RemoteRelation::Unrelated, 0, 0));
        } else if ahead == 0 && behind == 0 {
            RemoteRelation::Equal
        } else if behind == 0 {
            RemoteRelation::Ahead
        } else if ahead == 0 {
            RemoteRelation::Behind
        } else {
            RemoteRelation::Diverged
        };
        Ok((relation, ahead, behind))
    }
}

/// 机器输出必须无损解码，错误中不回显仓库配置内容。
fn text(v: &[u8]) -> Result<String, OperationError> {
    String::from_utf8(v.to_vec()).map_err(|_| OperationError::new("PARSE_FAILED"))
}
/// 遵循 Git 的最长前缀优先规则；同长时使用首次配置，不递归展开。
fn rewrite_url(value: &str, rules: &[(String, String)]) -> String {
    let mut matched: Option<&(String, String)> = None;
    for rule in rules {
        if value.starts_with(&rule.0)
            && matched.is_none_or(|previous| rule.0.len() > previous.0.len())
        {
            matched = Some(rule);
        }
    }
    match matched {
        Some((prefix, base)) => format!("{base}{}", &value[prefix.len()..]),
        None => value.to_owned(),
    }
}
/// 展示时间采用 Unix 毫秒十进制字符串，不参与计划过期判断。
fn unix_millis() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .to_string()
}
/// 多地址全部按配置顺序展示，不将它们折叠成一个执行目标。
fn display_urls(values: &[String]) -> String {
    if values.is_empty() {
        "未配置远端地址".to_owned()
    } else {
        values
            .iter()
            .map(|v| url::display(v))
            .collect::<Vec<_>>()
            .join("；")
    }
}

#[cfg(test)]
mod tests {
    use super::url::display;
    use super::*;
    use crate::git::repository::query;
    use crate::git::repository::{open_repository, read_repository_state, tests::Fixture};

    /// 在隔离仓库创建可辨别提交，并返回其真实 OID。
    fn commit(f: &Fixture, message: &str) -> String {
        f.write("file", message.as_bytes());
        f.command(&["add", "."]);
        f.command(&["commit", "-m", message]);
        String::from_utf8(query(&f.git, &f.root, &["rev-parse", "HEAD"], 4096).unwrap())
            .unwrap()
            .trim()
            .to_owned()
    }

    /// 五种完整历史关系均由真实对象图计算，分叉两侧计数不能颠倒。
    #[test]
    fn assesses_real_graph_relations() {
        let f = Fixture::new();
        let base = commit(&f, "base");
        let local = commit(&f, "local");
        f.command(&["switch", "-c", "other", &base]);
        let remote = commit(&f, "remote");
        f.command(&["switch", "--orphan", "unrelated"]);
        let unrelated = commit(&f, "unrelated");
        f.command(&["switch", "main"]);
        for (oid, relation, ahead, behind) in [
            (&local, RemoteRelation::Equal, 0, 0),
            (&base, RemoteRelation::Ahead, 1, 0),
            (&remote, RemoteRelation::Diverged, 1, 1),
            (&unrelated, RemoteRelation::Unrelated, 0, 0),
        ] {
            f.command(&["update-ref", "refs/remotes/origin/main", oid]);
            let (repo, state) = open_repository(&f.git, &f.root).unwrap();
            let session = RemoteSession::new(&f.git, &repo).unwrap();
            let id = &session.state().remote_branches[0].remote_branch_id;
            let assessment = session.assess(&state, id).unwrap();
            assert_eq!(
                (assessment.relation, assessment.ahead, assessment.behind),
                (relation, ahead, behind)
            );
            assert_eq!(assessment.local_oid, local);
            assert_eq!(assessment.remote_oid, *oid);
            assert!(assessment.observed_at.parse::<u128>().unwrap() > 0);
        }
        f.command(&["switch", "--detach", &base]);
        f.command(&["update-ref", "refs/remotes/origin/main", &local]);
        let (repo, state) = open_repository(&f.git, &f.root).unwrap();
        let session = RemoteSession::new(&f.git, &repo).unwrap();
        let result = session
            .assess(&state, &session.state().remote_branches[0].remote_branch_id)
            .unwrap();
        assert_eq!(
            (result.relation, result.ahead, result.behind),
            (RemoteRelation::Behind, 0, 1)
        );
    }

    /// 远端会话拒绝外部移动、跨会话 ID 和已经过期的本地 HEAD。
    #[test]
    fn rejects_moved_refs_and_old_state() {
        let f = Fixture::new();
        let base = commit(&f, "base");
        f.command(&["update-ref", "refs/remotes/origin/main", &base]);
        let (repo, state) = open_repository(&f.git, &f.root).unwrap();
        let session = RemoteSession::new(&f.git, &repo).unwrap();
        let id = session.state().remote_branches[0].remote_branch_id.clone();
        let other = RemoteSession::new(&f.git, &repo).unwrap();
        assert_eq!(other.assess(&state, &id).unwrap_err().code, "STALE_REQUEST");
        let next = commit(&f, "next");
        assert_eq!(
            session.assess(&state, &id).unwrap_err().code,
            "STALE_REQUEST"
        );
        let fresh = read_repository_state(&f.git, &repo).unwrap();
        f.command(&["update-ref", "refs/remotes/origin/main", &next]);
        assert_eq!(
            session.assess(&fresh, &id).unwrap_err().code,
            "REMOTE_CHANGED"
        );
        f.command(&["update-ref", "-d", "refs/remotes/origin/main"]);
        assert_eq!(
            session.assess(&fresh, &id).unwrap_err().code,
            "REMOTE_CHANGED"
        );
    }

    /// 读取保留多地址、跳过符号 HEAD，并不改变仓库及工作文件字节。
    #[test]
    fn remote_metadata_is_read_only_and_session_bound() {
        let f = Fixture::new();
        let oid = commit(&f, "base");
        f.command(&[
            "config",
            "--add",
            "remote.origin.url",
            "https://example.invalid/a",
        ]);
        f.command(&[
            "config",
            "--add",
            "remote.origin.url",
            "ssh://user@example.invalid/b",
        ]);
        f.command(&[
            "config",
            "--add",
            "remote.origin.pushurl",
            "ssh://push-one.invalid/a",
        ]);
        f.command(&[
            "config",
            "--add",
            "remote.origin.pushurl",
            "ssh://push-two.invalid/b",
        ]);
        f.command(&["update-ref", "refs/remotes/origin/main", &oid]);
        f.command(&[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/main",
        ]);
        let paths = [
            ".git/config",
            ".git/index",
            ".git/HEAD",
            ".git/refs/heads/main",
            ".git/refs/remotes/origin/main",
            "file",
        ];
        let before: Vec<_> = paths
            .iter()
            .map(|p| std::fs::read(f.root.join(p)).unwrap())
            .collect();
        let (repo, state) = open_repository(&f.git, &f.root).unwrap();
        let session = RemoteSession::new(&f.git, &repo).unwrap();
        let metadata = session.state();
        assert_eq!(metadata.remote_branches.len(), 1);
        assert_eq!(metadata.last_fetched_at, None);
        assert_eq!(
            metadata.remotes[0].fetch_display_url,
            "https://example.invalid/a；ssh://example.invalid/b"
        );
        assert_eq!(
            metadata.remotes[0].push_display_url,
            "ssh://push-one.invalid/a；ssh://push-two.invalid/b"
        );
        assert_ne!(metadata.remotes[0].remote_id, "origin");
        assert_ne!(
            metadata.remotes[0].remote_id,
            RemoteSession::new(&f.git, &repo).unwrap().state().remotes[0].remote_id
        );
        session
            .assess(&state, &metadata.remote_branches[0].remote_branch_id)
            .unwrap();
        for (path, original) in paths.iter().zip(before) {
            assert_eq!(std::fs::read(f.root.join(path)).unwrap(), original);
        }
    }

    /// 浅历史不能推断无共同祖先，未出生的 HEAD 不能发起整合评估。
    #[test]
    fn shallow_is_unknown_and_unborn_requires_head() {
        let f = Fixture::new();
        let base = commit(&f, "base");
        let tip = commit(&f, "tip");
        f.command(&["update-ref", "refs/remotes/origin/main", &base]);
        f.write(".git/shallow", format!("{tip}\n").as_bytes());
        let (repo, state) = open_repository(&f.git, &f.root).unwrap();
        let session = RemoteSession::new(&f.git, &repo).unwrap();
        let id = &session.state().remote_branches[0].remote_branch_id;
        assert_eq!(
            session.assess(&state, id).unwrap().relation,
            RemoteRelation::Unknown
        );
        std::fs::remove_file(f.root.join(".git/shallow")).unwrap();
        f.command(&["switch", "--orphan", "empty"]);
        let state = read_repository_state(&f.git, &repo).unwrap();
        assert_eq!(
            session.assess(&state, id).unwrap_err().code,
            "HEAD_REQUIRED"
        );
    }

    /// URL 校验拒绝辅助程序、路径及空 userinfo，不能把它们误认成 SSH。
    #[test]
    fn rejects_transport_ambiguity() {
        for raw in [
            "ext::sh -c secret",
            "helper::repo",
            "-oProxyCommand=secret:x",
            "C:/repo",
            "./path:repo",
            "https://@example.invalid/a",
            "ssh://user:secret@example.invalid/a",
            "https://example.invalid/a?token=secret",
            "ssh://example.invalid/a#secret",
            "https://example.invalid/\nsecret",
        ] {
            assert_eq!(display(raw), "远端地址不可安全显示", "错误接受 {raw:?}");
        }
        assert_eq!(
            display("host.invalid:org/repo"),
            "ssh://host.invalid/org/repo"
        );
    }

    /// 配置地址重写与原生 Git 对照，并验证明确 pushurl 优先及凭据脱敏。
    #[test]
    fn rewritten_urls_match_git_and_explicit_push_target() {
        let f = Fixture::new();
        f.command(&["config", "remote.origin.url", "alias:team/project"]);
        f.command(&["config", "url.https://read.invalid/.insteadOf", "alias:"]);
        f.command(&[
            "config",
            "url.https://specific.invalid/.insteadOf",
            "alias:team/",
        ]);
        f.command(&["config", "url.ssh://write.invalid/.pushInsteadOf", "alias:"]);
        let (repo, _) = open_repository(&f.git, &f.root).unwrap();
        let session = RemoteSession::new(&f.git, &repo).unwrap();
        let native_fetch = query(&f.git, &f.root, &["remote", "get-url", "origin"], 4096).unwrap();
        let native_push = query(
            &f.git,
            &f.root,
            &["remote", "get-url", "--push", "origin"],
            4096,
        )
        .unwrap();
        assert_eq!(
            session.state().remotes[0].fetch_display_url,
            String::from_utf8(native_fetch).unwrap().trim()
        );
        assert_eq!(
            session.state().remotes[0].push_display_url,
            String::from_utf8(native_push).unwrap().trim()
        );
        f.command(&[
            "config",
            "remote.origin.pushurl",
            "https://explicit.invalid/project",
        ]);
        let explicit = RemoteSession::new(&f.git, &repo).unwrap().state();
        assert_eq!(
            explicit.remotes[0].push_display_url,
            "https://explicit.invalid/project"
        );
        f.command(&[
            "config",
            "url.https://user:secret@private.invalid/.insteadOf",
            "alias:team/project",
        ]);
        let secret = RemoteSession::new(&f.git, &repo).unwrap().state();
        assert_eq!(secret.remotes[0].fetch_display_url, "远端地址不可安全显示");
    }

    /// 缺失祖先或损坏配置必须返回读取错误，不能伪装成无共同祖先或空列表。
    #[test]
    fn corrupt_repository_data_is_not_a_valid_empty_result() {
        let f = Fixture::new();
        let base = commit(&f, "base");
        let tip = commit(&f, "tip");
        f.command(&["update-ref", "refs/remotes/origin/main", &base]);
        let (repo, state) = open_repository(&f.git, &f.root).unwrap();
        let session = RemoteSession::new(&f.git, &repo).unwrap();
        let base_object = f
            .root
            .join(".git/objects")
            .join(&base[..2])
            .join(&base[2..]);
        std::fs::remove_file(base_object).unwrap();
        assert!(session
            .assess(&state, &session.state().remote_branches[0].remote_branch_id)
            .is_err());
        assert!(session
            .relation(&tip, &base, Instant::now() + BUDGET)
            .is_err());
        f.write(".git/config", b"[broken");
        assert!(RemoteSession::new(&f.git, &repo).is_err());
    }

    /// 验证远端地址脱敏和不安全传输固定提示。
    #[test]
    fn url_display_redacts_and_rejects() {
        assert_eq!(
            display("https://user:secret@example.com/a"),
            "远端地址不可安全显示"
        );
        assert_eq!(display("http://example.com/a"), "远端地址不可安全显示");
        assert!(display("git@example.com:org/repo.git").contains("ssh://"));
        assert_eq!(
            display("ssh://user@example.com/org/repo.git"),
            "ssh://example.com/org/repo.git"
        );
    }

    /// 验证多个推送地址按配置顺序保留展示，不折叠成单一目标。
    #[test]
    fn multiple_urls_keep_order() {
        assert_eq!(
            super::display_urls(&["ssh://a/x".into(), "ssh://b/x".into()]),
            "ssh://a/x；ssh://b/x"
        );
    }
}
