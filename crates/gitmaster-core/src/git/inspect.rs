//! AI 专用只读查询：固定动作与参数，独立于 UI 图中加载的提交范围。
use super::{
    coordinator::{CoordinationKey, RepositoryCoordinator},
    process::run_git_read_until,
    *,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    ffi::OsString,
    time::{Duration, Instant},
};

/// 仅允许已定义的查询字段，不接收可执行文件、工作目录或任意 Git 参数。
#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Inspection {
    Status {
        cursor: Option<usize>,
    },
    Log {
        limit: Option<usize>,
        cursor: Option<usize>,
        base: Option<String>,
        head: Option<String>,
    },
    ResolveCommits {
        commits: Vec<String>,
    },
    ShowCommit {
        commit: String,
        #[serde(rename = "parentIndex")]
        parent_index: Option<usize>,
        cursor: Option<usize>,
    },
    Diff {
        scope: String,
        base: Option<String>,
        head: Option<String>,
        path: Option<String>,
        cursor: Option<usize>,
    },
    ReadFileAtCommit {
        commit: String,
        path: String,
        #[serde(rename = "startLine")]
        start_line: Option<usize>,
        #[serde(rename = "endLine")]
        end_line: Option<usize>,
    },
    Branches {
        cursor: Option<usize>,
    },
    Remotes {
        cursor: Option<usize>,
    },
}

/// 仅接受明确提交 ID 或 HEAD；相对版本语法由宿主生成，不接受任意 rev 表达式。
fn valid_commit(value: &str) -> bool {
    value == "HEAD"
        || ((4..=64).contains(&value.len()) && value.bytes().all(|b| b.is_ascii_hexdigit()))
}
/// 拒绝盘符、绝对路径、上级路径和控制字符，历史树读取不能逸出授权仓库。
fn valid_path(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with(['/', '\\'])
        && !value.contains([':', '\0', '\n', '\r'])
        && !value.split(['/', '\\']).any(|part| part == "..")
}
/// 使用既有安全进程执行器，禁止外部 diff/textconv，所有子查询共享总期限。
fn query(
    git: &GitExecutable,
    repo: &RepositoryHandle,
    args: Vec<String>,
    deadline: Instant,
) -> Result<(String, bool), OperationError> {
    // 禁止仓库通过 log.showSignature 配置在只读查询时启动额外签名验证程序。
    let args = ["-c", "log.showSignature=false"]
        .into_iter()
        .map(OsString::from)
        .chain(args.into_iter().map(OsString::from))
        .collect::<Vec<_>>();
    let output = run_git_read_until(git, &repo.root, &args, 2 * 1024 * 1024, true, deadline)?;
    if !output.success {
        return Err(OperationError::new("AI_GIT_QUERY_FAILED"));
    }
    Ok((
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.truncated,
    ))
}
/// 解析提交时区分无效、歧义和非提交对象，不沿用图缓存的写授权。
fn resolve(
    git: &GitExecutable,
    repo: &RepositoryHandle,
    value: &str,
    deadline: Instant,
) -> Result<String, OperationError> {
    if !valid_commit(value) {
        return Err(OperationError::new("AI_GIT_INVALID_COMMIT"));
    }
    if value != "HEAD" {
        let (matches, _) = query(
            git,
            repo,
            vec!["rev-parse".into(), format!("--disambiguate={value}")],
            deadline,
        )?;
        let count = matches.lines().count();
        if count == 0 {
            return Err(OperationError::new("AI_GIT_INVALID_COMMIT"));
        }
        if count > 1 {
            return Err(OperationError::new("AI_GIT_AMBIGUOUS_COMMIT"));
        }
    }
    let (oid, _) = query(
        git,
        repo,
        vec!["rev-parse".into(), "--verify".into(), value.into()],
        deadline,
    )?;
    let oid = oid.trim().to_owned();
    let (kind, _) = query(
        git,
        repo,
        vec!["cat-file".into(), "-t".into(), oid.clone()],
        deadline,
    )?;
    if kind.trim() != "commit" {
        return Err(OperationError::new("AI_GIT_NOT_COMMIT"));
    }
    Ok(oid)
}
/// 每帧最多约 48KiB 文本，游标按 UTF-8 边界推进；明确区分源查询截断。
fn page(
    text: String,
    source_truncated: bool,
    cursor: Option<usize>,
) -> Result<Value, OperationError> {
    let start = cursor.unwrap_or(0);
    if start > text.len() || !text.is_char_boundary(start) {
        return Err(OperationError::new("AI_GIT_CURSOR"));
    }
    let mut end = (start + 48 * 1024).min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    // JSON 转义会放大控制字符，按传输字节再次收紧，不能只按原文长度限额。
    while serde_json::to_vec(&text[start..end])
        .map_err(|_| OperationError::new("PARSE_FAILED"))?
        .len()
        > 44000
    {
        end = start + (end - start) / 2;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
    }
    Ok(
        json!({"text":&text[start..end],"truncated":source_truncated || end<text.len(),"nextCursor":if end<text.len(){Some(end)}else{None},"sourceTruncated":source_truncated}),
    )
}
/// 将凭据 URL 化为可展示目标；查询参数、片段及用户信息均不暴露。
fn safe_remote(value: &str) -> String {
    if let Ok(mut url) = url::Url::parse(value) {
        let _ = url.set_username("");
        let _ = url.set_password(None);
        url.set_query(None);
        url.set_fragment(None);
        url.to_string()
    } else if let Some((_, tail)) = value.split_once('@') {
        tail.split(['?', '#']).next().unwrap_or("").into()
    } else {
        value.split(['?', '#']).next().unwrap_or("").into()
    }
}
/// 列表按实际序列化大小分页，单个超大条目明确拒绝，避免私有管道溢出。
fn list_page(
    mut value: Value,
    field: &str,
    cursor: Option<usize>,
) -> Result<Value, OperationError> {
    let entries = value[field]
        .as_array_mut()
        .ok_or_else(|| OperationError::new("PARSE_FAILED"))?;
    let all = std::mem::take(entries);
    let start = cursor.unwrap_or(0);
    if start > all.len() {
        return Err(OperationError::new("AI_GIT_CURSOR"));
    }
    let mut size = 0;
    let mut count = 0;
    for entry in all.iter().skip(start) {
        let length = serde_json::to_vec(entry)
            .map_err(|_| OperationError::new("PARSE_FAILED"))?
            .len();
        if size + length > 44000 || count >= 50 {
            break;
        }
        size += length;
        count += 1;
    }
    if count == 0 && start < all.len() {
        return Err(OperationError::new("AI_GIT_OUTPUT_LIMIT"));
    }
    *entries = all.iter().skip(start).take(count).cloned().collect();
    value["truncated"] = json!(start + count < all.len());
    value["nextCursor"] = json!(if start + count < all.len() {
        Some(start + count)
    } else {
        None
    });
    Ok(value)
}

/// 只读查询经公共目录队列执行，不改变当前 UI 图、写计划或取消代次。
pub fn inspect(
    coordinator: &RepositoryCoordinator,
    git: &GitExecutable,
    repo: &RepositoryHandle,
    request: Inspection,
) -> Result<Value, OperationError> {
    inspect_cancelled(coordinator, git, repo, request, || false)
}
/// AI 读取拥有独立取消回调，取消排队不影响 UI 历史图的读请求。
pub fn inspect_cancelled(
    coordinator: &RepositoryCoordinator,
    git: &GitExecutable,
    repo: &RepositoryHandle,
    request: Inspection,
    cancelled: impl Fn() -> bool,
) -> Result<Value, OperationError> {
    let key = CoordinationKey::repository(repo)?;
    coordinator.read_files(&key, &cancelled, || {
        if cancelled() {
            return Err(OperationError::new("AI_CANCELLED"));
        }
        let result = inspect_unlocked(git, repo, request);
        if cancelled() {
            return Err(OperationError::new("AI_CANCELLED"));
        }
        result
    })
}
/// 单次读取已经取得队列位置，内部不得再次进入同一队列。
fn inspect_unlocked(
    git: &GitExecutable,
    repo: &RepositoryHandle,
    request: Inspection,
) -> Result<Value, OperationError> {
    let deadline = Instant::now() + Duration::from_secs(30);
    match request {
        Inspection::Status { cursor } => list_page(
            serde_json::to_value(repository::read_repository_state_until(
                git, repo, deadline,
            )?)
            .map_err(|_| OperationError::new("PARSE_FAILED"))?,
            "changes",
            cursor,
        ),
        Inspection::Branches { cursor } => list_page(
            serde_json::to_value(branches::BranchSession::new(git, repo)?.list())
                .map_err(|_| OperationError::new("PARSE_FAILED"))?,
            "branches",
            cursor,
        ),
        Inspection::ResolveCommits { commits } => {
            if commits.is_empty() || commits.len() > 20 {
                return Err(OperationError::new("AI_INVALID_INPUT"));
            }
            let values = commits
                .iter()
                .map(|value| match resolve(git, repo, value, deadline) {
                    Ok(oid) => json!({"input":value,"oid":oid}),
                    Err(error) => json!({"input":value,"error":error.code}),
                })
                .collect::<Vec<_>>();
            Ok(json!({"commits":values}))
        }
        Inspection::Log {
            limit,
            cursor,
            base,
            head,
        } => {
            let limit = limit.unwrap_or(50).clamp(1, 50);
            let offset = cursor.unwrap_or(0);
            if offset > 1_000_000 {
                return Err(OperationError::new("AI_GIT_CURSOR"));
            }
            let mut args = vec![
                "log".into(),
                "--no-decorate".into(),
                "--format=%H%x00%P%x00%an%x00%aI%x00%s".into(),
                format!("--max-count={}", limit + 1),
                format!("--skip={offset}"),
            ];
            if let Some(head) = head {
                let end = resolve(git, repo, &head, deadline)?;
                args.push(if let Some(base) = base {
                    format!("{}..{end}", resolve(git, repo, &base, deadline)?)
                } else {
                    end
                });
            } else if base.is_some() {
                return Err(OperationError::new("AI_INVALID_INPUT"));
            }
            args.push("--".into());
            let (text, truncated) = query(git, repo, args, deadline)?;
            let lines = text.lines().collect::<Vec<_>>();
            let mut selected = Vec::new();
            let mut bytes = 0;
            let mut entry_text_truncated = false;
            for line in lines.iter().take(limit) {
                let bounded = line.chars().take(2048).collect::<String>();
                let size = serde_json::to_vec(&bounded)
                    .map_err(|_| OperationError::new("PARSE_FAILED"))?
                    .len();
                if bytes + size > 44000 {
                    break;
                }
                bytes += size;
                entry_text_truncated |= bounded.len() < line.len();
                selected.push(bounded);
            }
            let count = selected.len();
            Ok(
                json!({"text":selected.join("\n"),"truncated":truncated || lines.len()>count || entry_text_truncated,"entryTextTruncated":entry_text_truncated,"nextCursor":if lines.len()>count {Some(offset+count)}else{None}}),
            )
        }
        Inspection::ShowCommit {
            commit,
            parent_index,
            cursor,
        } => {
            let oid = resolve(git, repo, &commit, deadline)?;
            let (parents, _) = query(
                git,
                repo,
                vec![
                    "rev-list".into(),
                    "--parents".into(),
                    "-n".into(),
                    "1".into(),
                    oid.clone(),
                ],
                deadline,
            )?;
            let parents = parents.split_whitespace().skip(1).collect::<Vec<_>>();
            let index = parent_index.unwrap_or(1);
            let mut args = vec![
                "show".into(),
                "--format=fuller".into(),
                "--no-patch".into(),
                "--no-ext-diff".into(),
                "--no-textconv".into(),
                oid.clone(),
                "--".into(),
            ];
            let comparison = if parents.is_empty() {
                "root".to_owned()
            } else {
                if index == 0 || index > parents.len() {
                    return Err(OperationError::new("AI_GIT_PARENT"));
                }
                parents[index - 1].to_owned()
            };
            let (header, truncated) = query(git, repo, args.clone(), deadline)?;
            args = vec![
                "diff-tree".into(),
                "--root".into(),
                "--no-commit-id".into(),
                "-r".into(),
                "-p".into(),
                "--stat".into(),
                "--no-ext-diff".into(),
                "--no-textconv".into(),
            ];
            if comparison != "root" {
                args.push(comparison.clone());
            }
            args.push(oid.clone());
            args.push("--".into());
            let (patch, cut) = query(git, repo, args, deadline)?;
            let mut result = page(format!("{header}\n{patch}"), truncated || cut, cursor)?;
            result["oid"] = json!(oid);
            result["comparison"] = json!(comparison);
            result["parents"] = json!(parents);
            Ok(result)
        }
        Inspection::Diff {
            scope,
            base,
            head,
            path,
            cursor,
        } => {
            let mut args = vec![
                "diff".into(),
                "--no-ext-diff".into(),
                "--no-textconv".into(),
            ];
            match scope.as_str() {
                "worktree" => {}
                "staged" => args.push("--cached".into()),
                "commits" => {
                    args.push(resolve(
                        git,
                        repo,
                        base.as_deref()
                            .ok_or_else(|| OperationError::new("AI_INVALID_INPUT"))?,
                        deadline,
                    )?);
                    args.push(resolve(
                        git,
                        repo,
                        head.as_deref()
                            .ok_or_else(|| OperationError::new("AI_INVALID_INPUT"))?,
                        deadline,
                    )?);
                }
                _ => return Err(OperationError::new("AI_INVALID_INPUT")),
            }
            args.push("--".into());
            if let Some(path) = path {
                if !valid_path(&path) {
                    return Err(OperationError::new("AI_GIT_PATH"));
                }
                args.push(format!(":(literal){path}"));
            }
            let (text, cut) = query(git, repo, args, deadline)?;
            let mut result = page(text, cut, cursor)?;
            result["untrackedIncluded"] = json!(false);
            Ok(result)
        }
        Inspection::ReadFileAtCommit {
            commit,
            path,
            start_line,
            end_line,
        } => {
            if !valid_path(&path) {
                return Err(OperationError::new("AI_GIT_PATH"));
            }
            let oid = resolve(git, repo, &commit, deadline)?;
            let (text, cut) = query(
                git,
                repo,
                vec!["cat-file".into(), "blob".into(), format!("{oid}:{path}")],
                deadline,
            )?;
            if text.contains(['\0', '\u{fffd}']) {
                return Ok(json!({"binary":true,"oid":oid,"path":path}));
            }
            let start = start_line.unwrap_or(1);
            let end = end_line.unwrap_or(start.saturating_add(999));
            if start == 0 || end < start || end - start > 2000 {
                return Err(OperationError::new("AI_INVALID_INPUT"));
            }
            let mut result = page(
                text.lines()
                    .skip(start - 1)
                    .take(end - start + 1)
                    .collect::<Vec<_>>()
                    .join("\n"),
                cut,
                None,
            )?;
            result["oid"] = json!(oid);
            result["path"] = json!(path);
            result["startLine"] = json!(start);
            result["endLine"] = json!(end);
            Ok(result)
        }
        Inspection::Remotes { cursor } => {
            let (text, cut) = query(git, repo, vec!["remote".into(), "-v".into()], deadline)?;
            let rows=text.lines().map(|line|{let mut fields=line.split_whitespace();json!({"name":fields.next().unwrap_or(""),"url":safe_remote(fields.next().unwrap_or("")),"direction":fields.next().unwrap_or("")})}).collect::<Vec<_>>();
            let mut result = list_page(json!({"remotes":rows}), "remotes", cursor)?;
            result["sourceTruncated"] = json!(cut);
            Ok(result)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// 实际临时仓库覆盖根提交、历史文件、未跟踪边界和非提交对象。
    #[test]
    fn inspect_real_commits() {
        let f = repository::tests::Fixture::new();
        f.write("说明.txt", "根提交内容\n".as_bytes());
        f.command(&["add", "."]);
        f.command(&["commit", "-m", "root"]);
        let root = std::fs::canonicalize(&f.root).unwrap();
        let repo = RepositoryHandle {
            id: "inspection-fixture".into(),
            git_dir: root.join(".git"),
            common_dir: root.join(".git"),
            root,
        };
        let coordinator = RepositoryCoordinator::new();
        let resolved = inspect(
            &coordinator,
            &f.git,
            &repo,
            Inspection::ResolveCommits {
                commits: vec!["HEAD".into(), "deadbeef".into()],
            },
        )
        .unwrap();
        let oid = resolved["commits"][0]["oid"].as_str().unwrap().to_owned();
        assert_eq!(resolved["commits"][1]["error"], "AI_GIT_INVALID_COMMIT");
        let show = inspect(
            &coordinator,
            &f.git,
            &repo,
            Inspection::ShowCommit {
                commit: oid.clone(),
                parent_index: None,
                cursor: None,
            },
        )
        .unwrap();
        assert_eq!(show["comparison"], "root");
        assert!(show["text"].as_str().unwrap().contains("根提交内容"));
        f.write("说明.txt", "工作区改动不能计入历史\n".as_bytes());
        f.write("未跟踪.txt", b"untracked");
        let file = inspect(
            &coordinator,
            &f.git,
            &repo,
            Inspection::ReadFileAtCommit {
                commit: oid,
                path: "说明.txt".into(),
                start_line: None,
                end_line: None,
            },
        )
        .unwrap();
        assert_eq!(file["text"], "根提交内容");
        let diff = inspect(
            &coordinator,
            &f.git,
            &repo,
            Inspection::Diff {
                scope: "worktree".into(),
                base: None,
                head: None,
                path: None,
                cursor: None,
            },
        )
        .unwrap();
        assert_eq!(diff["untrackedIncluded"], false);
        assert!(!diff["text"].as_str().unwrap().contains("untracked"));
    }
    /// 参数边界拒绝选项注入和目录逃逸，分页保持中文完整。
    #[test]
    fn inspection_boundaries() {
        let escaped = page("\u{1}".repeat(50000), false, None).unwrap();
        assert!(serde_json::to_vec(&escaped).unwrap().len() < 48000);
        assert!(escaped["nextCursor"].is_number());
        assert!(!valid_commit("--help"));
        assert!(!valid_commit("HEAD~1"));
        assert!(valid_commit("a123"));
        for path in ["../secret", "C:\\secret", "/etc/passwd", "a/../b"] {
            assert!(!valid_path(path));
        }
        assert!(valid_path("文档/说明.md"));
        let text = "中文".repeat(10000);
        let first = page(text.clone(), false, None).unwrap();
        let cursor = first["nextCursor"].as_u64().unwrap() as usize;
        let second = page(text, false, Some(cursor)).unwrap();
        assert_eq!(
            format!(
                "{}{}",
                first["text"].as_str().unwrap(),
                second["text"].as_str().unwrap()
            ),
            "中文".repeat(10000)
        );
        assert_eq!(
            safe_remote("https://user:password@host/repo?token=secret#fragment"),
            "https://host/repo"
        );
    }
}
