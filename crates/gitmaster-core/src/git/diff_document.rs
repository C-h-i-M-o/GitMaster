//! 有界差异正文留在 Rust，前端仅请求可见行片段。
use super::{read_with_limits, DiffLimits};
use crate::git::{
    DiffSide, FileDiff, GitExecutable, OperationError, RepositoryHandle, RepositoryState,
};
use serde::Serialize;
use std::ops::Range;

const LIMITS: DiffLimits = DiffLimits {
    bytes: 16 * 1024 * 1024,
    lines: 100_000,
};
const SEGMENT: usize = 16 * 1024;
const PAGE_BYTES: usize = 256 * 1024;

/// 文档只保留一份原始正文和索引，不在各行复制文本。
#[derive(Debug)]
pub struct DiffDocument {
    content: String,
    rows: Vec<IndexedRow>,
    summary: DiffSummary,
}
/// 核心读取结果，桌面层负责签发文档能力与生命周期。
#[derive(Debug)]
pub enum DiffDocumentResult {
    Text(DiffDocument),
    Binary,
    Unsupported { reason: String },
}
/// 统计只针对当前有界快照，截断时明确标示为部分统计。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffSummary {
    pub folds: Vec<DiffFold>,
    pub row_count: usize,
    pub hunk_count: usize,
    pub additions: usize,
    pub deletions: usize,
    pub truncated: bool,
}
/// 可折叠连续上下文的位置，不携带正文且不跨变化或 hunk。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffFold {
    pub start_row: usize,
    pub count: usize,
}
/// 对应真实 patch 的行语义，不把文件元数据误认为内容。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DiffRowKind {
    Hunk,
    Context,
    Add,
    Delete,
    Note,
}
/// 单行带原始双行号，长行段偏移按 UTF-8 字节计算。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffRow {
    pub kind: DiffRowKind,
    pub old_line: Option<usize>,
    pub new_line: Option<usize>,
    pub text: String,
    pub byte_offset: usize,
    pub next_byte_offset: Option<usize>,
}
/// 有界行页；nextRow 供短页继续加载，不改变正文行身份。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffPage {
    pub start_row: usize,
    pub rows: Vec<DiffRow>,
    pub next_row: Option<usize>,
}
/// 索引只存储正文范围及行号，内存受原始行数上限约束。
#[derive(Debug)]
struct IndexedRow {
    kind: DiffRowKind,
    old_line: Option<usize>,
    new_line: Option<usize>,
    range: Range<usize>,
}

/// 基于签发文件身份获取只读差异；保留原 Git 执行与路径安全边界。
pub fn open_diff_document(
    git: &GitExecutable,
    repository: &RepositoryHandle,
    snapshot: &RepositoryState,
    change_id: &str,
    side: DiffSide,
    context_lines: u16,
) -> Result<DiffDocumentResult, OperationError> {
    match read_with_limits(
        git,
        repository,
        snapshot,
        change_id,
        side,
        context_lines,
        LIMITS,
    )? {
        FileDiff::Text { content, truncated } => Ok(DiffDocumentResult::Text(DiffDocument::parse(
            content,
            side == DiffSide::Untracked,
            truncated,
        )?)),
        FileDiff::Binary => Ok(DiffDocumentResult::Binary),
        FileDiff::Unsupported { reason } => Ok(DiffDocumentResult::Unsupported { reason }),
    }
}

impl DiffDocument {
    /// 后台扫描原文一次建立索引，解析时不复制每行内容。
    fn parse(
        mut content: String,
        untracked: bool,
        truncated: bool,
    ) -> Result<Self, OperationError> {
        // 字节预算可能落在 hunk 头或正文中间；部分末行不能参与编号和统计。
        if truncated && !content.ends_with('\n') {
            content.truncate(content.rfind('\n').map_or(0, |at| at + 1));
        }
        let mut rows = Vec::new();
        let mut summary = DiffSummary {
            folds: Vec::new(),
            row_count: 0,
            hunk_count: 0,
            additions: 0,
            deletions: 0,
            truncated,
        };
        let (mut offset, mut old_line, mut new_line) = (0, 0, 0);
        let mut inside = false;
        for raw in content.split_inclusive('\n') {
            let text = raw.strip_suffix('\n').unwrap_or(raw);
            let text = text.strip_suffix('\r').unwrap_or(text);
            let mut row = IndexedRow {
                kind: DiffRowKind::Note,
                old_line: None,
                new_line: None,
                range: offset..offset + text.len(),
            };
            offset += raw.len();
            if untracked {
                row.kind = DiffRowKind::Add;
                new_line += 1;
                row.new_line = Some(new_line);
            } else if text.starts_with("@@ ") {
                let (old, new) =
                    hunk_start(text).ok_or_else(|| OperationError::new("PARSE_FAILED"))?;
                old_line = old;
                new_line = new;
                inside = true;
                row.kind = DiffRowKind::Hunk;
                summary.hunk_count += 1;
            } else if inside {
                row.kind = match text.as_bytes().first() {
                    Some(b'+') => DiffRowKind::Add,
                    Some(b'-') => DiffRowKind::Delete,
                    Some(b' ') => DiffRowKind::Context,
                    Some(b'\\') => DiffRowKind::Note,
                    _ => continue,
                };
                if row.kind != DiffRowKind::Note {
                    row.range.start += 1;
                    if row.kind != DiffRowKind::Add {
                        row.old_line = Some(old_line);
                        old_line = old_line
                            .checked_add(1)
                            .ok_or_else(|| OperationError::new("PARSE_FAILED"))?;
                    }
                    if row.kind != DiffRowKind::Delete {
                        row.new_line = Some(new_line);
                        new_line = new_line
                            .checked_add(1)
                            .ok_or_else(|| OperationError::new("PARSE_FAILED"))?;
                    }
                }
            } else {
                continue;
            }
            summary.additions += usize::from(row.kind == DiffRowKind::Add);
            summary.deletions += usize::from(row.kind == DiffRowKind::Delete);
            rows.push(row);
        }
        summary.row_count = rows.len();
        let mut cursor = 0;
        while cursor < rows.len() {
            let start = cursor;
            while cursor < rows.len() && rows[cursor].kind == DiffRowKind::Context {
                cursor += 1;
            }
            if cursor - start > 8 {
                summary.folds.push(DiffFold {
                    start_row: start,
                    count: cursor - start,
                });
            }
            if cursor == start {
                cursor += 1;
            }
        }
        Ok(Self {
            content,
            rows,
            summary,
        })
    }

    /// 摘要不含正文，首屏前无需把整个 patch 传输至 WebView。
    pub fn summary(&self) -> DiffSummary {
        self.summary.clone()
    }

    /// 读取不可变快照的一页；每行最多一段，续段请求仅允许单行。
    pub fn page(
        &self,
        start_row: usize,
        count: usize,
        byte_offset: usize,
    ) -> Result<DiffPage, OperationError> {
        if !(1..=200).contains(&count)
            || start_row > self.rows.len()
            || (byte_offset != 0 && (count != 1 || start_row == self.rows.len()))
        {
            return Err(OperationError::new("INVALID_INPUT"));
        }
        let mut rows = Vec::new();
        let mut bytes = 0;
        for row in self.rows.iter().skip(start_row).take(count) {
            let text = &self.content[row.range.clone()];
            if byte_offset > text.len() || !text.is_char_boundary(byte_offset) {
                return Err(OperationError::new("INVALID_INPUT"));
            }
            let mut end = text.len().min(byte_offset.saturating_add(SEGMENT));
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            if bytes + end - byte_offset > PAGE_BYTES {
                break;
            }
            bytes += end - byte_offset;
            rows.push(DiffRow {
                kind: row.kind,
                old_line: row.old_line,
                new_line: row.new_line,
                text: text[byte_offset..end].to_owned(),
                byte_offset,
                next_byte_offset: (end < text.len()).then_some(end),
            });
        }
        let next = start_row + rows.len();
        Ok(DiffPage {
            start_row,
            rows,
            next_row: (next < self.rows.len()).then_some(next),
        })
    }
}

/// Git 固定统一格式的 hunk 头只解析起始行，函数标题不参与编号。
fn hunk_start(text: &str) -> Option<(usize, usize)> {
    let mut fields = text.split_whitespace();
    if fields.next()? != "@@" {
        return None;
    }
    let old = fields
        .next()?
        .strip_prefix('-')?
        .split(',')
        .next()?
        .parse()
        .ok()?;
    let new = fields
        .next()?
        .strip_prefix('+')?
        .split(',')
        .next()?
        .parse()
        .ok()?;
    if fields.next()? != "@@" {
        return None;
    }
    Some((old, new))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::{
        repository::{open_repository, tests::Fixture},
        DiffSide,
    };

    /// 折叠范围只包含连续上下文，不吞入变化或下一个 hunk。
    #[test]
    fn fold_ranges_stop_at_changes() {
        let patch = format!(
            "@@ -1,20 +1,20 @@\n{}-old\n+new\n{}",
            " same\n".repeat(9),
            " tail\n".repeat(10)
        );
        let doc = DiffDocument::parse(patch, false, false).unwrap();
        let summary = doc.summary();
        assert_eq!(
            summary
                .folds
                .iter()
                .map(|f| (f.start_row, f.count))
                .collect::<Vec<_>>(),
            vec![(1, 9), (12, 10)]
        );
        assert_eq!((summary.additions, summary.deletions), (1, 1));
    }

    /// 万行真实差异必须能读至末尾，统计及行号不能受旧预览上限影响。
    #[test]
    fn reads_ten_thousand_changes_without_legacy_truncation() {
        let f = Fixture::new();
        let source = (0..10_000)
            .map(|i| format!("old {i}\n"))
            .collect::<String>();
        f.write("large.txt", source.as_bytes());
        f.command(&["add", "."]);
        f.command(&["commit", "-m", "base"]);
        f.write("large.txt", source.replace("old", "new").as_bytes());
        let (repo, state) = open_repository(&f.git, &f.root).unwrap();
        let DiffDocumentResult::Text(doc) = open_diff_document(
            &f.git,
            &repo,
            &state,
            &state.changes[0].change_id,
            DiffSide::Unstaged,
            3,
        )
        .unwrap() else {
            panic!("预期文本差异");
        };
        let summary = doc.summary();
        assert_eq!(
            (summary.additions, summary.deletions, summary.hunk_count),
            (10_000, 10_000, 1)
        );
        assert!(!summary.truncated);
        let page = doc.page(summary.row_count - 1, 1, 0).unwrap();
        assert_eq!(page.rows[0].new_line, Some(10_000));
        assert_eq!(page.rows[0].text, "new 9999");
        assert!(page.next_row.is_none());
    }

    /// 中文长行分段无损，单页正文有界，非法字节位置和数量直接拒绝。
    #[test]
    fn unicode_segments_and_page_limits() {
        let text = "中文".repeat(10_000);
        let doc = DiffDocument::parse(format!("@@ -0,0 +1,1 @@\n+{text}\n"), false, false).unwrap();
        let mut output = String::new();
        let mut offset = 0;
        loop {
            let page = doc.page(1, 1, offset).unwrap();
            let row = &page.rows[0];
            assert!(row.text.len() <= 16 * 1024);
            output.push_str(&row.text);
            if let Some(next) = row.next_byte_offset {
                offset = next;
            } else {
                break;
            }
        }
        assert_eq!(output, text);
        assert!(doc.page(1, 1, 1).is_err());
        assert!(doc.page(1, 2, 3).is_err());
        assert!(doc.page(0, 0, 0).is_err());
        assert!(doc.page(0, 201, 0).is_err());
        assert!(doc.page(3, 1, 0).is_err());
        let doc = DiffDocument::parse(format!("{}\n", "中".repeat(6_000)).repeat(200), true, false)
            .unwrap();
        let page = doc.page(0, 200, 0).unwrap();
        assert!(page.rows.iter().map(|r| r.text.len()).sum::<usize>() <= 256 * 1024);
        assert!(page.next_row.is_some());
    }

    /// 未跟踪文件的 patch 样式内容是原文；普通 patch 保留注释及多个 hunk 行号。
    #[test]
    fn raw_preview_and_hunk_numbering() {
        let raw = "@@ -1 +2 @@\n+++file\n\\ no newline";
        let doc = DiffDocument::parse(raw.into(), true, false).unwrap();
        assert_eq!(doc.summary().additions, 3);
        assert_eq!(doc.page(0, 3, 0).unwrap().rows[0].text, "@@ -1 +2 @@");
        let patch = "diff --git a/a b/a\n--- a/a\n+++ b/a\n@@ -2,2 +4,2 @@ title\n same\n-before\n+after\n\\ No newline at end of file\n@@ -20 +30 @@\n-end\n+new\n";
        let doc = DiffDocument::parse(patch.into(), false, true).unwrap();
        let page = doc.page(0, 200, 0).unwrap();
        assert_eq!(doc.summary().hunk_count, 2);
        assert!(doc.summary().truncated);
        assert_eq!(
            (page.rows[1].old_line, page.rows[1].new_line),
            (Some(2), Some(4))
        );
        assert_eq!(page.rows.last().unwrap().new_line, Some(30));
        assert_eq!(page.rows[4].kind, DiffRowKind::Note);
        let empty =
            DiffDocument::parse("old mode 100644\nnew mode 100755\n".into(), false, false).unwrap();
        assert_eq!(empty.summary().row_count, 0);
        assert!(empty.page(0, 1, 0).unwrap().rows.is_empty());
    }

    /// 字节截断到 hunk 头或正文时丢弃半行，不伪造新增统计或解析失败。
    #[test]
    fn partial_tail_is_not_counted() {
        let doc =
            DiffDocument::parse("@@ -0,0 +1,2 @@\n+complete\n+part".into(), false, true).unwrap();
        assert_eq!(doc.summary().additions, 1);
        let doc = DiffDocument::parse("@@ -1".into(), false, true).unwrap();
        assert_eq!(doc.summary().row_count, 0);
        assert!(doc.summary().truncated);
    }

    /// 分块入口沿用暂存/工作区隔离、二进制及无尾换行语义。
    #[test]
    fn document_entry_preserves_sides_and_binary() {
        let f = Fixture::new();
        f.write("text", b"staged");
        f.write("binary", b"a\0b");
        f.command(&["add", "."]);
        f.write("text", b"working");
        let (repo, state) = open_repository(&f.git, &f.root).unwrap();
        let change = state.changes.iter().find(|c| c.path == "text").unwrap();
        for (side, yes, no) in [
            (DiffSide::Staged, "staged", "working"),
            (DiffSide::Unstaged, "working", "missing"),
        ] {
            let DiffDocumentResult::Text(doc) =
                open_diff_document(&f.git, &repo, &state, &change.change_id, side, 3).unwrap()
            else {
                panic!("预期文本");
            };
            let page = doc.page(0, 200, 0).unwrap();
            assert!(page
                .rows
                .iter()
                .any(|r| r.kind == DiffRowKind::Add && r.text == yes));
            assert!(!page.rows.iter().any(|r| r.text == no));
            assert!(page.rows.iter().any(|r| r.kind == DiffRowKind::Note));
        }
        let binary = state.changes.iter().find(|c| c.path == "binary").unwrap();
        assert!(matches!(
            open_diff_document(
                &f.git,
                &repo,
                &state,
                &binary.change_id,
                DiffSide::Staged,
                3
            )
            .unwrap(),
            DiffDocumentResult::Binary
        ));
        assert!(open_diff_document(
            &f.git,
            &repo,
            &state,
            &change.change_id,
            DiffSide::Untracked,
            3
        )
        .is_err());
    }
}
