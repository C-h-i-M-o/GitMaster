//! 所选远端的完整分支下载在私有仓库完成，再事务发布跟踪引用。
use super::{auth::AuthPolicy, url, RemoteSession};
use crate::git::{
    coordinator::{CoordinationKey, OperationReporter, RepositoryCoordinator},
    process::{inspect_local_config, run_isolated_git, run_local_git, run_network_git},
    repository::{query_until, read_repository_state_until},
    write::operation_result,
    *,
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const PRIVATE: &str = "refs/remotes/gitmaster/all/";
const LIMIT: usize = 8 * 1024 * 1024;
type References = BTreeMap<String, String>;

/// 冻结单个远端和该命名空间的原始引用，不允许下载改动本地分支。
struct Plan {
    git: GitExecutable,
    repo: RepositoryHandle,
    head: HeadState,
    auth: AuthPolicy,
    config: [u8; 32],
    url: String,
    prefix: String,
    old: References,
    objects: PathBuf,
    format: String,
    fetched: Arc<Mutex<Option<String>>>,
    remote_name: String,
    fetched_branches: Arc<Mutex<BTreeMap<String, Vec<String>>>>,
}

/// 准备刷新使用的全分支 fetch，网络访问仅在执行阶段发生。
pub fn prepare_fetch_all(
    coordinator: &RepositoryCoordinator,
    git: &GitExecutable,
    repo: &RepositoryHandle,
    state: &RepositoryState,
    remotes: &RemoteSession,
    remote_id: &str,
) -> Result<WritePreview, OperationError> {
    prepare_with_runner(
        coordinator,
        git,
        repo,
        state,
        remotes,
        remote_id,
        run_network_git,
    )
}

/// 测试仅替换传输边界，其余准备、下载参数、发布和结果核验保持生产流程。
fn prepare_with_runner(
    coordinator: &RepositoryCoordinator,
    git: &GitExecutable,
    repo: &RepositoryHandle,
    state: &RepositoryState,
    remotes: &RemoteSession,
    remote_id: &str,
    runner: super::fetch::NetworkRunner,
) -> Result<WritePreview, OperationError> {
    let generation = coordinator.begin_prepare()?;
    if remotes.repo.id != repo.id || state.repository_id != repo.id {
        return Err(OperationError::new("STALE_REQUEST"));
    }
    let remote = remotes
        .remotes
        .get(remote_id)
        .ok_or_else(|| OperationError::new("REMOTE_NOT_FOUND"))?;
    if remote.fetch.len() != 1 {
        return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
    }
    let address = remote.fetch[0].clone();
    url::validate(&address)?;
    let prefix = format!("refs/remotes/{}/", remote.name);
    if remotes.remotes.values().any(|other| {
        other.name != remote.name
            && (other.name.starts_with(&format!("{}/", remote.name))
                || remote.name.starts_with(&format!("{}/", other.name)))
    }) {
        return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
    }
    let key = CoordinationKey::repository(repo)?;
    let deadline = Instant::now() + super::BUDGET;
    let plan = coordinator.read_until(&key, deadline, || {
        query_until(
            git,
            &repo.root,
            &["check-ref-format", &format!("{prefix}gitmaster-validation")],
            4096,
            Some(deadline),
        )?;
        let auth = AuthPolicy::capture(git, &repo.root, deadline)?;
        let setting = format!("remote.{}.", remote.name);
        let expected = format!("+refs/heads/*:{prefix}*");
        let specs = auth.values(&format!("{setting}fetch"));
        if specs.len() != 1
            || (specs[0] != expected && specs[0] != expected.trim_start_matches('+'))
        {
            return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
        }
        for suffix in ["uploadpack", "receivepack", "proxy", "vcs"] {
            if !auth.values(&format!("{setting}{suffix}")).is_empty() {
                return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
            }
        }
        if auth
            .values(&format!("{setting}mirror"))
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
        let format = String::from_utf8(query_until(
            git,
            &repo.root,
            &["rev-parse", "--show-object-format"],
            4096,
            Some(deadline),
        )?)
        .map_err(|_| OperationError::new("PARSE_FAILED"))?
        .trim()
        .to_owned();
        if !matches!(format.as_str(), "sha1" | "sha256") {
            return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
        }
        let old = read_refs(git, repo, &prefix, deadline)?;
        let plan = Plan {
            git: git.clone(),
            repo: repo.clone(),
            head: state.head.clone(),
            auth,
            config: remotes.config_digest,
            url: address.clone(),
            prefix,
            old,
            objects: std::fs::canonicalize(repo.common_dir.join("objects"))
                .map_err(|_| OperationError::new("ACCESS_DENIED"))?,
            format,
            fetched: remotes.last_fetched_at.clone(),
            remote_name: remote.name.clone(),
            fetched_branches: remotes.fetched_branches.clone(),
        };
        plan.verify(deadline)?;
        Ok(plan)
    })?;
    let operation = coordinator.prepare(
        generation,
        Some(repo.id.clone()),
        key,
        OperationKind::Fetch,
        move |reporter| execute(&plan, reporter, runner),
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
            display_url: url::display(&address),
            ref_name: "refs/heads/*".into(),
            oid: None,
            source_oid: None,
            pending_commits: Vec::new(),
        }),
        warnings: Vec::new(),
        expires_at: operation.expires_at,
    })
}

impl Plan {
    /// 配置、HEAD 和跟踪引用变化时拒绝发布网络结果。
    fn verify(&self, deadline: Instant) -> Result<(), OperationError> {
        let state = read_repository_state_until(&self.git, &self.repo, deadline)?;
        if state.head != self.head
            || Sha256::digest(inspect_local_config(&self.git, &self.repo.root, deadline)?)
                .as_slice()
                != self.config
            || read_refs(&self.git, &self.repo, &self.prefix, deadline)? != self.old
        {
            return Err(OperationError::new("REMOTE_CHANGED"));
        }
        Ok(())
    }
}

/// 保留符号引用标记；下载分支与符号引用撞名时事务会拒绝写入。
fn parse_refs(bytes: &[u8], prefix: &str) -> Result<References, OperationError> {
    let mut refs = References::new();
    for row in bytes
        .split(|byte| *byte == b'\n')
        .filter(|row| !row.is_empty())
    {
        let fields = row.split(|byte| *byte == 0).collect::<Vec<_>>();
        if fields.len() != 3 {
            return Err(OperationError::new("PARSE_FAILED"));
        }
        let name =
            std::str::from_utf8(fields[0]).map_err(|_| OperationError::new("PARSE_FAILED"))?;
        let oid =
            std::str::from_utf8(fields[1]).map_err(|_| OperationError::new("PARSE_FAILED"))?;
        if !name.starts_with(prefix)
            || name.chars().any(|c| c.is_whitespace() || c.is_control())
            || !matches!(oid.len(), 40 | 64)
            || !oid.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(OperationError::new("PARSE_FAILED"));
        }
        let value = if fields[2].is_empty() {
            oid.to_owned()
        } else {
            format!("symbolic:{oid}")
        };
        if refs.insert(name.to_owned(), value).is_some() || refs.len() > 10000 {
            return Err(OperationError::new("OUTPUT_LIMIT"));
        }
    }
    Ok(refs)
}

/// 查询精确命名空间下的真实引用，包含符号 HEAD 以检测并发变化。
fn read_refs(
    git: &GitExecutable,
    repo: &RepositoryHandle,
    prefix: &str,
    deadline: Instant,
) -> Result<References, OperationError> {
    parse_refs(
        &query_until(
            git,
            &repo.root,
            &[
                "for-each-ref",
                "--format=%(refname)%00%(objectname)%00%(symref)",
                prefix,
            ],
            LIMIT,
            Some(deadline),
        )?,
        prefix,
    )
}

/// 事务只创建或更新下载到的分支，不删除本地残留跟踪引用。
fn transaction(
    old: &References,
    fetched: &References,
    prefix: &str,
) -> Result<Vec<u8>, OperationError> {
    let mut input = String::from("start\noption no-deref\n");
    for (source, oid) in fetched {
        let name = source
            .strip_prefix(PRIVATE)
            .ok_or_else(|| OperationError::new("PARSE_FAILED"))?;
        let target = format!("{prefix}{name}");
        if oid.starts_with("symbolic:") {
            return Err(OperationError::new("UNSUPPORTED_WRITE_CONFIGURATION"));
        }
        match old.get(&target) {
            Some(previous) if previous.starts_with("symbolic:") => {
                return Err(OperationError::new("REMOTE_CHANGED"))
            }
            Some(previous) => input.push_str(&format!("update {target} {oid} {previous}\n")),
            None => input.push_str(&format!("create {target} {oid}\n")),
        }
    }
    input.push_str("prepare\ncommit\n");
    Ok(input.into_bytes())
}

/// 网络下载与发布分离；失败时按可能已发布状态返回标准任务结果。
fn execute(
    plan: &Plan,
    reporter: &OperationReporter,
    runner: super::fetch::NetworkRunner,
) -> OperationResult {
    let deadline = reporter.deadline(Duration::from_secs(15 * 60));
    let mut attempted = false;
    let result = (|| {
        plan.verify(deadline)?;
        let scratch = tempfile::Builder::new()
            .prefix("gitmaster-fetch-all-")
            .tempdir()
            .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
        let template = scratch.path().join("empty-template");
        std::fs::create_dir(&template).map_err(|_| OperationError::new("ACCESS_DENIED"))?;
        let mut template_arg = OsString::from("--template=");
        template_arg.push(template);
        let init = run_isolated_git(
            &plan.git,
            scratch.path(),
            &[
                "init".into(),
                "--bare".into(),
                template_arg,
                format!("--object-format={}", plan.format).into(),
            ],
            &[],
            None,
            &[],
            deadline,
        )?;
        if !init.success {
            return Err(OperationError::new("GIT_EXECUTION_FAILED"));
        }
        let spec = format!("+refs/heads/*:{PRIVATE}*");
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
            &spec,
        ]
        .map(OsString::from);
        reporter.report(OperationPhase::Transferring, None)?;
        let output = runner(
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
        if !output.success {
            return Err(OperationError::new("NETWORK_FAILED"));
        }
        let args = [
            "for-each-ref",
            "--format=%(refname)%00%(objectname)%00%(symref)",
            PRIVATE,
        ]
        .map(OsString::from);
        let output = run_isolated_git(
            &plan.git,
            scratch.path(),
            &args,
            &[],
            Some(&plan.objects),
            &[],
            deadline,
        )?;
        if !output.success {
            return Err(OperationError::new("GIT_EXECUTION_FAILED"));
        }
        let fetched = parse_refs(&output.stdout, PRIVATE)?;
        let input = transaction(&plan.old, &fetched, &plan.prefix)?;
        plan.verify(deadline)?;
        reporter.report(OperationPhase::Writing, None)?;
        attempted = true;
        let args = ["update-ref", "--stdin"].map(OsString::from);
        let output = run_local_git(&plan.git, &plan.repo.root, &args, &input, None, deadline)?;
        if !output.success {
            attempted = false;
            return Err(OperationError::new("REMOTE_CHANGED"));
        }
        let actual = read_refs(&plan.git, &plan.repo, &plan.prefix, deadline)?;
        for (source, oid) in &fetched {
            let target = format!(
                "{}{}",
                plan.prefix,
                source
                    .strip_prefix(PRIVATE)
                    .ok_or_else(|| OperationError::new("PARSE_FAILED"))?
            );
            if actual.get(&target) != Some(oid) {
                return Err(OperationError::new("WRITE_OUTCOME_UNKNOWN"));
            }
        }
        let names = fetched
            .keys()
            .map(|reference| {
                reference
                    .strip_prefix(PRIVATE)
                    .map(str::to_owned)
                    .ok_or_else(|| OperationError::new("PARSE_FAILED"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        plan.fetched_branches
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(plan.remote_name.clone(), names);
        *plan
            .fetched
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(super::unix_millis());
        Ok(())
    })();
    operation_result(
        reporter, &plan.git, &plan.repo, result, attempted, None, None, deadline,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    /// 下载替身只将保留测试域映射为本地仓库，不连接真实网络。
    fn local_download(
        git: &GitExecutable,
        cwd: &std::path::Path,
        args: &[OsString],
        objects: Option<&std::path::Path>,
        _settings: &[(String, String)],
        _deadline: Instant,
        progress: &mut dyn FnMut(u64, u64),
    ) -> Result<crate::git::process::ProcessOutput, OperationError> {
        let mut args = args.to_vec();
        let index = args.len() - 2;
        let address = ::url::Url::parse(args[index].to_str().unwrap()).unwrap();
        assert_eq!(address.host_str(), Some("fixture.invalid"));
        args[index] = ::url::Url::parse(&format!("file://{}", address.path()))
            .unwrap()
            .to_file_path()
            .unwrap()
            .into_os_string();
        let mut command = std::process::Command::new(&git.path);
        for (key, _) in
            std::env::vars_os().filter(|(key, _)| key.to_string_lossy().starts_with("GIT_"))
        {
            command.env_remove(key);
        }
        command
            .current_dir(cwd)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env(
                "GIT_CONFIG_GLOBAL",
                if cfg!(windows) { "NUL" } else { "/dev/null" },
            )
            .env("GIT_ALLOW_PROTOCOL", "file")
            .args([
                "-c",
                "core.hooksPath=",
                "-c",
                "maintenance.auto=false",
                "-c",
                "gc.auto=0",
            ]);
        if let Some(objects) = objects {
            command.env("GIT_OBJECT_DIRECTORY", dunce::simplified(objects));
        }
        let output = command.args(args).output().unwrap();
        assert!(
            output.status.success(),
            "本地测试传输失败：{}",
            String::from_utf8_lossy(&output.stderr)
        );
        progress(1, 1);
        Ok(crate::git::process::ProcessOutput {
            stdout: output.stdout,
            stderr: output.stderr,
            success: output.status.success(),
            exit_code: output.status.code(),
            truncated: false,
        })
    }

    /// 完整私有下载获取新分支而不抓标签、不改变本地 HEAD、索引或工作文件。
    #[test]
    fn downloads_all_branches_without_tags_or_local_changes() {
        use crate::git::repository::{open_repository, tests::Fixture};
        let source = Fixture::new();
        source.write("file", b"source");
        source.command(&["add", "."]);
        source.command(&["commit", "-m", "base"]);
        source.command(&["branch", "feature/new"]);
        source.command(&["tag", "not-requested"]);
        let local = Fixture::new();
        local.write("untouched", b"local content");
        let mut address = ::url::Url::parse("https://fixture.invalid/").unwrap();
        address.set_path(source.root.to_str().unwrap());
        local.command(&["remote", "add", "origin", address.as_str()]);
        let (repo, state) = open_repository(&local.git, &local.root).unwrap();
        let head = std::fs::read(repo.git_dir.join("HEAD")).unwrap();
        let index_before = std::fs::read(repo.git_dir.join("index")).ok();
        let remotes = RemoteSession::new(&local.git, &repo).unwrap();
        let remote_id = remotes.state().remotes[0].remote_id.clone();
        let coordinator = RepositoryCoordinator::new();
        let preview = prepare_with_runner(
            &coordinator,
            &local.git,
            &repo,
            &state,
            &remotes,
            &remote_id,
            local_download,
        )
        .unwrap();
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
            assert!(Instant::now() < deadline, "全分支下载测试超时");
            std::thread::sleep(Duration::from_millis(20));
        }
        let refs = read_refs(&local.git, &repo, "refs/remotes/origin/", deadline).unwrap();
        assert_eq!(refs.len(), 2);
        assert!(refs.contains_key("refs/remotes/origin/main"));
        assert!(refs.contains_key("refs/remotes/origin/feature/new"));
        assert!(query_until(
            &local.git,
            &local.root,
            &["for-each-ref", "refs/tags"],
            LIMIT,
            Some(deadline)
        )
        .unwrap()
        .is_empty());
        assert_eq!(std::fs::read(repo.git_dir.join("HEAD")).unwrap(), head);
        assert_eq!(std::fs::read(repo.git_dir.join("index")).ok(), index_before);
        assert_eq!(
            std::fs::read(local.root.join("untouched")).unwrap(),
            b"local content"
        );
        assert!(remotes.state().last_fetched_at.is_some());
        assert_eq!(
            remotes.state().remotes[0].fetched_branch_names.as_deref(),
            Some(["feature/new".to_owned(), "main".to_owned()].as_slice())
        );
        local.command(&[
            "update-ref",
            "refs/remotes/origin/deleted",
            &refs["refs/remotes/origin/main"],
        ]);
        let refreshed = remotes.refreshed().unwrap().state();
        assert!(refreshed
            .remote_branches
            .iter()
            .any(|branch| branch.name == "origin/deleted"));
        assert!(!refreshed.remotes[0]
            .fetched_branch_names
            .as_ref()
            .unwrap()
            .iter()
            .any(|name| name == "deleted"));
    }
    /// 真实 Git 事务遇到旧 OID 不匹配时，新增引用也不能部分发布。
    #[test]
    fn stale_transaction_publishes_no_new_branch() {
        use crate::git::repository::{open_repository, tests::Fixture};
        let f = Fixture::new();
        f.write("file", b"first");
        f.command(&["add", "."]);
        f.command(&["commit", "-m", "first"]);
        let (repo, state) = open_repository(&f.git, &f.root).unwrap();
        let first = match state.head {
            HeadState::Branch { oid, .. } => oid,
            _ => panic!("测试需要分支"),
        };
        f.write("file", b"second");
        f.command(&["add", "."]);
        f.command(&["commit", "-m", "second"]);
        let next = String::from_utf8(
            query_until(&f.git, &f.root, &["rev-parse", "HEAD"], 4096, None).unwrap(),
        )
        .unwrap()
        .trim()
        .to_owned();
        f.command(&["update-ref", "refs/remotes/origin/main", &next]);
        let old = References::from([("refs/remotes/origin/main".into(), first)]);
        let fetched = References::from([
            (format!("{PRIVATE}main"), next.clone()),
            (format!("{PRIVATE}fresh"), next.clone()),
        ]);
        let input = transaction(&old, &fetched, "refs/remotes/origin/").unwrap();
        let args = ["update-ref", "--stdin"].map(OsString::from);
        let output = run_local_git(
            &f.git,
            &f.root,
            &args,
            &input,
            None,
            Instant::now() + Duration::from_secs(120),
        )
        .unwrap();
        assert!(!output.success);
        let actual = read_refs(
            &f.git,
            &repo,
            "refs/remotes/origin/",
            Instant::now() + Duration::from_secs(120),
        )
        .unwrap();
        assert_eq!(
            actual,
            References::from([("refs/remotes/origin/main".into(), next)])
        );
    }
    /// 发布包含新增和更新，不产生删除指令，未知符号引用必须拒绝。
    #[test]
    fn transaction_updates_all_fetched_without_pruning() {
        let first = "1".repeat(40);
        let next = "2".repeat(40);
        let old = References::from([
            ("refs/remotes/origin/main".into(), first.clone()),
            ("refs/remotes/origin/retained".into(), first.clone()),
        ]);
        let fetched = References::from([
            (format!("{PRIVATE}main"), next.clone()),
            (format!("{PRIVATE}feature/new"), next.clone()),
        ]);
        let input = String::from_utf8(transaction(&old, &fetched, "refs/remotes/origin/").unwrap())
            .unwrap();
        assert!(input.contains(&format!("update refs/remotes/origin/main {next} {first}\n")));
        assert!(input.contains(&format!("create refs/remotes/origin/feature/new {next}\n")));
        assert!(!input.contains("retained"));
        assert!(!input.contains("delete"));
        let symbolic = References::from([(
            "refs/remotes/origin/main".into(),
            format!("symbolic:{first}"),
        )]);
        assert_eq!(
            transaction(&symbolic, &fetched, "refs/remotes/origin/")
                .unwrap_err()
                .code,
            "REMOTE_CHANGED"
        );
    }
    /// 解析保留符号标记，拒绝前缀外引用和不完整 OID。
    #[test]
    fn reference_parser_rejects_untrusted_records() {
        let oid = "a".repeat(40);
        let input = format!("refs/remotes/origin/HEAD\0{oid}\0refs/remotes/origin/main\n");
        let parsed = parse_refs(input.as_bytes(), "refs/remotes/origin/").unwrap();
        assert_eq!(
            parsed["refs/remotes/origin/HEAD"],
            format!("symbolic:{oid}")
        );
        assert!(parse_refs(input.as_bytes(), PRIVATE).is_err());
        assert!(parse_refs(b"refs/remotes/origin/main\0bad\0\n", "refs/remotes/origin/").is_err());
    }
}
