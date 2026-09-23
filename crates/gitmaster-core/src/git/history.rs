//! 固定 Git 引用图快照并提供有界历史读取。
mod files;
use super::repository::{next_id, query};
use super::types::{CommitDetail, CommitSummary, HistoryPage, HistoryRefTip, ReferenceKind};
use super::{GitExecutable, OperationError, RepositoryHandle};
use std::{
    collections::{HashMap, HashSet},
    ffi::OsString,
};

const PAGE_SIZE: usize = 50;
const MAX_COMMITS: usize = 1000;
const OUTPUT_LIMIT: usize = 8 * 1024 * 1024;

/// 绑定仓库和引用快照的历史读取会话。
pub struct HistorySession {
    git: GitExecutable,
    repository: RepositoryHandle,
    snapshot_id: String,
    tips: Vec<HistoryRefTip>,
    commits: Vec<String>,
    returned: HashSet<String>,
    issued_cursors: HashSet<String>,
    details: HashMap<String, CommitDetail>,
    file_selection: Option<files::FileSelection>,
}

impl HistorySession {
    /// 冻结本地及远端引用头，并固定最多 1000 个可达提交。
    pub fn new(git: GitExecutable, repository: RepositoryHandle) -> Result<Self, OperationError> {
        let refs = query(
            &git,
            &repository.root,
            &[
                "for-each-ref",
                "--format=%(refname)%00%(objectname)%00",
                "refs/heads",
                "refs/remotes",
            ],
            OUTPUT_LIMIT,
        )?;
        let mut tips = Vec::new();
        for line in refs.split(|b| *b == b'\n').filter(|line| !line.is_empty()) {
            let fields: Vec<&[u8]> = line.split(|b| *b == 0).collect();
            if fields.len() < 2 || fields[0].is_empty() {
                continue;
            }
            let full_name = text(fields[0])?;
            let oid = text(fields[1])?;
            let (name, kind) = if let Some(name) = full_name.strip_prefix("refs/heads/") {
                (name.to_owned(), ReferenceKind::Local)
            } else if let Some(name) = full_name.strip_prefix("refs/remotes/") {
                (name.to_owned(), ReferenceKind::Remote)
            } else {
                continue;
            };
            tips.push(HistoryRefTip {
                ref_id: full_name,
                name,
                kind,
                oid,
            });
        }
        let head = query(
            &git,
            &repository.root,
            &["rev-parse", "--verify", "HEAD"],
            4096,
        );
        if let Ok(bytes) = head {
            let oid = text(bytes.strip_suffix(b"\n").unwrap_or(&bytes))?;
            if !tips.iter().any(|tip| tip.oid == oid) {
                tips.push(HistoryRefTip {
                    ref_id: "HEAD".into(),
                    name: "HEAD".into(),
                    kind: ReferenceKind::Local,
                    oid,
                });
            }
        }
        tips.sort_by(|a, b| a.name.cmp(&b.name));
        let snapshot_id = next_id();
        for (index, tip) in tips.iter_mut().enumerate() {
            tip.ref_id = format!("{snapshot_id}:ref:{index}");
        }
        if tips.len() > MAX_COMMITS {
            return Err(OperationError::new("OUTPUT_LIMIT"));
        }
        let tip_oids: Vec<&str> = tips.iter().map(|tip| tip.oid.as_str()).collect();
        let mut commits = if tip_oids.is_empty() {
            Vec::new()
        } else {
            unique_lines(&rev_list_from_stdin(&git, &repository.root, &tip_oids)?)?
        };
        let mut seen = HashSet::new();
        commits.retain(|oid| seen.insert(oid.clone()));
        commits.truncate(MAX_COMMITS);
        Ok(Self {
            git,
            repository,
            snapshot_id,
            tips,
            commits,
            returned: HashSet::new(),
            issued_cursors: HashSet::new(),
            details: HashMap::new(),
            file_selection: None,
        })
    }

    /// 读取固定快照的一页，游标只在本会话内有效。
    pub fn read_page(&mut self, cursor: Option<&str>) -> Result<HistoryPage, OperationError> {
        let offset = match cursor {
            None => 0,
            Some(value) => {
                let offset = parse_cursor(value, &self.snapshot_id)?;
                if !self.issued_cursors.contains(value) {
                    return Err(OperationError::new("INVALID_INPUT"));
                }
                offset
            }
        };
        if offset > self.commits.len() {
            return Err(OperationError::new("INVALID_INPUT"));
        }
        let end = (offset + PAGE_SIZE).min(self.commits.len());
        let page_oids = self.commits[offset..end].to_vec();
        let summaries = self.summaries(&page_oids)?;
        self.returned.extend(page_oids);
        let next_cursor =
            (end < self.commits.len()).then(|| format!("{}:{}", self.snapshot_id, end));
        if let Some(value) = &next_cursor {
            self.issued_cursors.insert(value.clone());
        }
        Ok(HistoryPage {
            repository_id: self.repository.id.clone(),
            graph_snapshot_id: self.snapshot_id.clone(),
            tips: self.tips.clone(),
            commits: summaries,
            next_cursor,
        })
    }

    /// 读取已经由本会话分页返回的提交完整详情。
    pub fn commit_detail(&mut self, oid: &str) -> Result<CommitDetail, OperationError> {
        if !self.returned.contains(oid) {
            return Err(OperationError::new("INVALID_INPUT"));
        }
        if let Some(detail) = self.details.get(oid) {
            return Ok(detail.clone());
        }
        let detail = self.detail(oid)?;
        self.details.insert(oid.to_owned(), detail.clone());
        Ok(detail)
    }

    /// 校验父提交属于已返回提交的直接父，并返回父详情。
    pub fn parent_detail(
        &mut self,
        oid: &str,
        parent_oid: &str,
    ) -> Result<CommitDetail, OperationError> {
        let detail = self.commit_detail(oid)?;
        if !detail.parent_oids.iter().any(|parent| parent == parent_oid) {
            return Err(OperationError::new("INVALID_INPUT"));
        }
        self.commit_detail(parent_oid)
    }

    /// 一次读取整页冻结 OID，避免 Windows 逐条启动 Git 的固定开销。
    fn summaries(&self, oids: &[String]) -> Result<Vec<CommitSummary>, OperationError> {
        if oids.is_empty() {
            return Ok(Vec::new());
        }
        let mut args = vec![
            "log",
            "--no-walk=unsorted",
            "-z",
            "--no-patch",
            "--no-notes",
            "--no-show-signature",
            "--no-use-mailmap",
            "--format=%H%x00%P%x00%an%x00%aI%x00%B",
        ];
        args.extend(oids.iter().map(String::as_str));
        args.push("--");
        let bytes = query(&self.git, &self.repository.root, &args, OUTPUT_LIMIT)?;
        parse_summaries(&bytes, oids)
    }
    /// 从冻结 OID 读取原始说明和身份，格式模式不额外添加结尾换行。
    fn detail(&self, oid: &str) -> Result<CommitDetail, OperationError> {
        if !self.commits.iter().any(|value| value == oid) {
            return Err(OperationError::new("INVALID_INPUT"));
        }
        let bytes = query(
            &self.git,
            &self.repository.root,
            &[
                "show",
                "-s",
                "--no-use-mailmap",
                "--format=format:%H%x00%P%x00%an%x00%ae%x00%aI%x00%cn%x00%ce%x00%cI%x00%B",
                oid,
            ],
            OUTPUT_LIMIT,
        )?;
        let fields: Vec<&[u8]> = bytes.splitn(9, |b| *b == 0).collect();
        if fields.len() != 9 {
            return Err(OperationError::new("PARSE_FAILED"));
        }
        let message = text(fields[8])?;
        let subject = message.lines().next().unwrap_or_default().to_owned();
        let body = message.strip_prefix(&subject).unwrap_or("").to_owned();
        Ok(CommitDetail {
            oid: text(fields[0])?,
            parent_oids: if fields[1].is_empty() {
                Vec::new()
            } else {
                text(fields[1])?
                    .split_whitespace()
                    .map(str::to_owned)
                    .collect()
            },
            subject,
            author_name: text(fields[2])?,
            authored_at: text(fields[4])?,
            body,
            author_email: text(fields[3])?,
            committer_name: text(fields[5])?,
            committer_email: text(fields[6])?,
            committed_at: text(fields[7])?,
            truncated: false,
        })
    }
}

/// 通过 stdin 传递冻结的引用头，避免 Windows 命令行长度限制。
fn rev_list_from_stdin(
    git: &GitExecutable,
    root: &std::path::Path,
    tip_oids: &[&str],
) -> Result<Vec<u8>, OperationError> {
    let mut input = tip_oids.join("\n").into_bytes();
    input.push(b'\n');
    let args = [
        OsString::from("rev-list"),
        OsString::from("--topo-order"),
        OsString::from("--date-order"),
        OsString::from("--max-count=1000"),
        OsString::from("--stdin"),
    ];
    let output = super::process::run_git_read_input(git, root, &args, &input, OUTPUT_LIMIT)?;
    if !output.success {
        return Err(super::repository::command_error(&output.stderr));
    }
    Ok(output.stdout)
}

/// 固定字段数并核对冻结顺序，整页有效后才允许发布；正文中的换行不作为边界。
fn parse_summaries(bytes: &[u8], oids: &[String]) -> Result<Vec<CommitSummary>, OperationError> {
    let bytes = bytes
        .strip_suffix(b"\0")
        .ok_or_else(|| OperationError::new("PARSE_FAILED"))?;
    let fields: Vec<&[u8]> = bytes.split(|byte| *byte == 0).collect();
    if fields.len() != oids.len() * 5 {
        return Err(OperationError::new("PARSE_FAILED"));
    }
    fields
        .chunks_exact(5)
        .zip(oids)
        .map(|(record, expected)| {
            let oid = text(record[0])?;
            if &oid != expected {
                return Err(OperationError::new("PARSE_FAILED"));
            }
            Ok(CommitSummary {
                oid,
                parent_oids: text(record[1])?
                    .split_whitespace()
                    .map(str::to_owned)
                    .collect(),
                author_name: text(record[2])?,
                authored_at: text(record[3])?,
                subject: text(record[4])?
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .to_owned(),
            })
        })
        .collect()
}

/// 将 Git 输出无损转换为 UTF-8 文本。
fn text(bytes: &[u8]) -> Result<String, OperationError> {
    std::str::from_utf8(bytes)
        .map(str::to_owned)
        .map_err(|_| OperationError::new("PARSE_FAILED"))
}
/// 解析固定快照游标，拒绝跨会话或非数字偏移。
fn parse_cursor(cursor: &str, snapshot: &str) -> Result<usize, OperationError> {
    let (id, offset) = cursor
        .split_once(':')
        .ok_or_else(|| OperationError::new("INVALID_INPUT"))?;
    if id != snapshot {
        return Err(OperationError::new("STALE_GRAPH"));
    }
    offset
        .parse()
        .map_err(|_| OperationError::new("INVALID_INPUT"))
}
/// 解析换行分隔的 OID 并去重。
fn unique_lines(bytes: &[u8]) -> Result<Vec<String>, OperationError> {
    bytes
        .split(|b| *b == b'\n')
        .filter(|line| !line.is_empty())
        .map(text)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::repository::open_repository;
    use crate::git::repository::tests::Fixture;

    /// 批量记录保留中文、多行与空说明，拒绝缺失字段、额外 NUL 和错误顺序。
    #[test]
    fn batch_summary_boundaries() {
        let oids = vec!["a".to_owned(), "b".to_owned()];
        let bytes = "a\0p q\0作者\02026-09-23\0标题\n正文\n\0b\0\0作者\02026-09-22\0\0".as_bytes();
        let parsed = parse_summaries(bytes, &oids).unwrap();
        assert_eq!(parsed[0].subject, "标题");
        assert_eq!(parsed[0].parent_oids, vec!["p", "q"]);
        assert_eq!(parsed[1].subject, "");
        assert!(parse_summaries(&bytes[..bytes.len() - 1], &oids).is_err());
        assert!(parse_summaries(bytes, &["b".into(), "a".into()]).is_err());
        assert!(parse_summaries(b"a\0\0name\0date\0body\0extra\0", &["a".into()]).is_err());
    }

    /// 显式指定仓库时执行只读性能验收，避免日常测试依赖用户路径与机器速度。
    #[test]
    #[ignore = "需要 GITMASTER_HISTORY_BENCH_REPOSITORY 指定只读验收仓库"]
    fn history_page_readonly_performance() {
        let path = std::env::var_os("GITMASTER_HISTORY_BENCH_REPOSITORY").unwrap();
        let git = crate::git::environment::resolve_git(None).unwrap();
        let started = std::time::Instant::now();
        let (repository, _) = open_repository(&git, std::path::Path::new(&path)).unwrap();
        println!("打开仓库：{:?}", started.elapsed());
        let started = std::time::Instant::now();
        let mut session = HistorySession::new(git, repository).unwrap();
        println!("冻结历史：{:?}", started.elapsed());
        let started = std::time::Instant::now();
        let page = session.read_page(None).unwrap();
        println!("首页 {} 条：{:?}", page.commits.len(), started.elapsed());
        assert_eq!(page.commits.len(), 50);
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
    }

    /// 真实临时仓库验证超过一页时游标只读取固定提交集合。
    #[test]
    fn paginates_fixed_snapshot_and_rejects_unknown_detail() {
        let fixture = Fixture::new();
        for index in 0..55 {
            fixture.write("history.txt", index.to_string().as_bytes());
            fixture.command(&["add", "history.txt"]);
            fixture.command(&["commit", "-m", "提交\n恶意"]);
        }
        let (repository, _) = open_repository(&fixture.git, &fixture.root).unwrap();
        let mut session = HistorySession::new(fixture.git.clone(), repository).unwrap();
        let observed = [
            ".git/index",
            ".git/config",
            ".git/HEAD",
            ".git/refs/heads/main",
            "history.txt",
        ];
        let before: Vec<_> = observed
            .iter()
            .map(|path| std::fs::read(fixture.root.join(path)).unwrap())
            .collect();
        let first = session.read_page(None).unwrap();
        let detail = session.commit_detail(&first.commits[0].oid).unwrap();
        assert_eq!(format!("{}{}", detail.subject, detail.body), "提交\n恶意\n");
        for (path, expected) in observed.iter().zip(&before) {
            assert_eq!(&std::fs::read(fixture.root.join(path)).unwrap(), expected);
        }
        assert_eq!(first.commits.len(), 50);
        assert!(first.next_cursor.is_some());
        assert!(session
            .read_page(Some(&format!("{}:49", first.graph_snapshot_id)))
            .is_err());
        let forged = format!("{}:999", first.graph_snapshot_id);
        assert_eq!(
            session.read_page(Some(&forged)).unwrap_err().code,
            "INVALID_INPUT"
        );
        let (other_repository, _) = open_repository(&fixture.git, &fixture.root).unwrap();
        let mut other = HistorySession::new(fixture.git.clone(), other_repository).unwrap();
        assert_eq!(
            other
                .read_page(first.next_cursor.as_deref())
                .unwrap_err()
                .code,
            "STALE_GRAPH"
        );
        assert!(session.commit_detail("deadbeef").is_err());
        fixture.write("new", b"new");
        fixture.command(&["add", "."]);
        fixture.command(&["commit", "-m", "new tip"]);
        let new_oid = String::from_utf8(
            query(&fixture.git, &fixture.root, &["rev-parse", "HEAD"], 4096).unwrap(),
        )
        .unwrap()
        .trim()
        .to_owned();
        let second = session.read_page(first.next_cursor.as_deref()).unwrap();
        assert!(!first
            .commits
            .iter()
            .chain(&second.commits)
            .any(|commit| commit.oid == new_oid));
        let (repository, _) = open_repository(&fixture.git, &fixture.root).unwrap();
        let mut fresh = HistorySession::new(fixture.git.clone(), repository).unwrap();
        assert_eq!(fresh.read_page(None).unwrap().commits[0].oid, new_oid);
        assert_eq!(second.commits.len(), 5);
        assert!(session.read_page(first.next_cursor.as_deref()).is_ok());
    }

    /// 验证 unborn、root 和 detached HEAD 的历史边界。
    #[test]
    fn unborn_root_and_detached() {
        let fixture = Fixture::new();
        let (repository, state) = open_repository(&fixture.git, &fixture.root).unwrap();
        assert!(matches!(
            state.head,
            crate::git::types::HeadState::Unborn { .. }
        ));
        let session = HistorySession::new(fixture.git.clone(), repository).unwrap();
        assert!(session.commits.is_empty());
        fixture.write("root", b"root");
        fixture.command(&["add", "."]);
        fixture.command(&["commit", "-m", "root"]);
        fixture.command(&["checkout", "--detach", "HEAD"]);
        let (repository, _) = open_repository(&fixture.git, &fixture.root).unwrap();
        let mut session = HistorySession::new(fixture.git.clone(), repository).unwrap();
        let page = session.read_page(None).unwrap();
        assert_eq!(page.commits.len(), 1);
        assert!(page.commits[0].parent_oids.is_empty());
    }

    /// 验证两页之间新增提交不会进入既有快照，新会话可见。
    #[test]
    fn snapshot_does_not_mix_new_commit() {
        let fixture = Fixture::new();
        fixture.write("a", b"a");
        fixture.command(&["add", "."]);
        fixture.command(&["commit", "-m", "a"]);
        let (repository, _) = open_repository(&fixture.git, &fixture.root).unwrap();
        let mut old = HistorySession::new(fixture.git.clone(), repository).unwrap();
        let first = old.read_page(None).unwrap();
        assert!(first.next_cursor.is_none());
        fixture.write("b", b"b");
        fixture.command(&["add", "."]);
        fixture.command(&["commit", "-m", "b"]);
        assert_eq!(old.commits.len(), 1);
        let (repository, _) = open_repository(&fixture.git, &fixture.root).unwrap();
        let mut fresh = HistorySession::new(fixture.git.clone(), repository).unwrap();
        assert_eq!(fresh.read_page(None).unwrap().commits.len(), 2);
    }

    /// 真实建立 1001 个 refs 时拒绝超出会话引用上限。
    #[test]
    fn rejects_more_than_1000_refs() {
        let fixture = Fixture::new();
        fixture.write("root", b"root");
        fixture.command(&["add", "."]);
        fixture.command(&["commit", "-m", "root"]);
        let (repository, _) = open_repository(&fixture.git, &fixture.root).unwrap();
        let oid = query(&fixture.git, &fixture.root, &["rev-parse", "HEAD"], 4096).unwrap();
        let oid = String::from_utf8(oid).unwrap().trim().to_owned();
        let commands = (0..999)
            .map(|index| format!("create refs/heads/ref-{index} {oid}\n"))
            .collect::<Vec<_>>();
        // 测试准备分批创建真实 refs，避免把慢磁盘上的千文件写入当成只读查询超时。
        for batch in commands.chunks(100) {
            crate::git::write_guard::local_query(
                &fixture.git,
                &repository,
                &["update-ref", "--stdin"],
                batch.concat().as_bytes(),
                None,
                std::time::Instant::now() + std::time::Duration::from_secs(60),
            )
            .unwrap();
        }
        assert_eq!(
            HistorySession::new(fixture.git.clone(), repository.clone())
                .unwrap()
                .tips
                .len(),
            1000
        );
        crate::git::write_guard::local_query(
            &fixture.git,
            &repository,
            &["update-ref", "--stdin"],
            format!("create refs/heads/ref-over-limit {oid}\n").as_bytes(),
            None,
            std::time::Instant::now() + std::time::Duration::from_secs(60),
        )
        .unwrap();
        let result = HistorySession::new(fixture.git.clone(), repository);
        assert!(matches!(result, Err(error) if error.code == "OUTPUT_LIMIT"));
    }

    /// 验证 merge 双亲以及多个引用指向同一 OID 时的引用类型和名称。
    #[test]
    fn merge_and_multiple_refs() {
        let fixture = Fixture::new();
        fixture.write("base", b"base");
        fixture.command(&["add", "."]);
        fixture.command(&["commit", "-m", "base"]);
        fixture.command(&["checkout", "-b", "side"]);
        fixture.write("side", b"side");
        fixture.command(&["add", "."]);
        fixture.command(&["commit", "-m", "side"]);
        fixture.command(&["checkout", "main"]);
        fixture.write("main", b"main");
        fixture.command(&["add", "."]);
        fixture.command(&["commit", "-m", "main"]);
        fixture.command(&["merge", "--no-edit", "side"]);
        fixture.command(&["branch", "same"]);
        let (repository, _) = open_repository(&fixture.git, &fixture.root).unwrap();
        let mut session = HistorySession::new(fixture.git.clone(), repository).unwrap();
        let page = session.read_page(None).unwrap();
        let merge = page
            .commits
            .iter()
            .find(|commit| commit.parent_oids.len() == 2)
            .unwrap();
        assert_eq!(merge.parent_oids.len(), 2);
        let refs: Vec<_> = page
            .tips
            .iter()
            .filter(|tip| tip.oid == merge.oid)
            .collect();
        assert!(refs
            .iter()
            .any(|tip| tip.name == "main" && tip.kind == ReferenceKind::Local));
        assert!(refs
            .iter()
            .any(|tip| tip.name == "same" && tip.kind == ReferenceKind::Local));
    }
}
