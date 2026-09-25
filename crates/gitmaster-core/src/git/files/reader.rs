//! 流式只读行索引：正文不完整驻留内存，读取页验证原始块摘要。
use super::super::{
    conflicts::{open_regular, RegularFile},
    repository::next_id,
    write_guard, OperationError,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Seek, SeekFrom},
    path::Path,
    time::{Duration, Instant},
};

const BLOCK: usize = 64 * 1024;
const MAX_BYTES: u64 = 64 * 1024 * 1024;
const MAX_LINES: usize = 1_000_000;
const SEGMENT: u64 = 16 * 1024;
const PAGE_BYTES: usize = 256 * 1024;

/// 只读文档身份独立于可保存编辑模型，正文通过页接口取得。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadDocument {
    pub document_id: String,
    pub file_id: String,
    pub path: String,
    pub byte_length: u64,
    pub encoding: &'static str,
    pub line_count: usize,
    pub bom: bool,
    pub line_ending: &'static str,
}
/// 超长行明确返回后续字节位置，不把首段冒充完整行。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadLine {
    pub line_number: usize,
    pub text: String,
    pub byte_offset: u64,
    pub next_byte_offset: Option<u64>,
}
/// 每页具有行数及总文本字节上限，保留下一页游标。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadPage {
    pub document_id: String,
    pub start_line: usize,
    pub lines: Vec<ReadLine>,
    pub next_line: Option<usize>,
}
/// 查找返回匹配行而非整份正文，同一行只列出一次。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadSearch {
    pub document_id: String,
    pub lines: Vec<usize>,
    pub next_line: Option<usize>,
}

/// 单文档仅保存安全句柄、行偏移和块摘要，关闭时直接释放。
pub struct PagedDocument {
    opened: RegularFile,
    document: ReadDocument,
    offsets: Vec<u64>,
    hashes: Vec<[u8; 32]>,
}
impl PagedDocument {
    /// 逐块验证 UTF-8 并索引三类换行，最多暂存一个块和不完整字符。
    pub(crate) fn open(root: &Path, path: &str, file_id: &str) -> Result<Self, OperationError> {
        let deadline = Instant::now() + Duration::from_secs(120);
        let mut opened = open_regular(root, path, deadline).map_err(read_error)?;
        if opened.len > MAX_BYTES {
            return Err(OperationError::new("FILE_READ_TOO_LARGE"));
        }
        let mut offsets = vec![0];
        let mut hashes = Vec::new();
        let mut position = 0u64;
        let mut pending = Vec::new();
        let mut bom = false;
        let (mut lf, mut cr, mut crlf) = (0usize, 0usize, 0usize);
        let mut previous_cr = false;
        while position < opened.len {
            write_guard::check_time(deadline)?;
            let length = (opened.len - position).min(BLOCK as u64) as usize;
            let mut bytes = vec![0; length];
            opened
                .file
                .read_exact(&mut bytes)
                .map_err(|_| OperationError::new("FILE_CHANGED"))?;
            if bytes.contains(&0) {
                return Err(OperationError::new("FILE_READ_BINARY"));
            }
            hashes.push(Sha256::digest(&bytes).into());
            pending.extend_from_slice(&bytes);
            match std::str::from_utf8(&pending) {
                Ok(_) => pending.clear(),
                Err(error) if error.error_len().is_none() => {
                    pending.drain(..error.valid_up_to());
                }
                Err(_) => return Err(OperationError::new("FILE_READ_ENCODING")),
            }
            let skip = if position == 0 && bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
                bom = true;
                offsets[0] = 3;
                3
            } else {
                0
            };
            for (index, byte) in bytes.iter().enumerate().skip(skip) {
                let next = position + index as u64 + 1;
                if *byte == b'\r' {
                    cr += 1;
                    offsets.push(next);
                } else if *byte == b'\n' {
                    if previous_cr {
                        cr -= 1;
                        crlf += 1;
                        if let Some(last) = offsets.last_mut() {
                            *last = next;
                        }
                    } else {
                        lf += 1;
                        offsets.push(next);
                    }
                }
                previous_cr = *byte == b'\r';
                if offsets.len() > MAX_LINES {
                    return Err(OperationError::new("FILE_READ_TOO_LARGE"));
                }
            }
            position += length as u64;
        }
        if !pending.is_empty() {
            return Err(OperationError::new("FILE_READ_ENCODING"));
        }
        opened.verify(deadline).map_err(read_error)?;
        let ending = match (lf > 0, cr > 0, crlf > 0) {
            (false, false, false) => "none",
            (true, false, false) => "lf",
            (false, true, false) => "cr",
            (false, false, true) => "crlf",
            _ => "mixed",
        };
        let document = ReadDocument {
            document_id: next_id(),
            file_id: file_id.to_owned(),
            path: path.to_owned(),
            byte_length: opened.len,
            encoding: "utf8",
            line_count: offsets.len(),
            bom,
            line_ending: ending,
        };
        Ok(Self {
            opened,
            document,
            offsets,
            hashes,
        })
    }
    /// 返回有界元数据副本，不暴露路径读取能力或底层句柄。
    pub fn metadata(&self) -> ReadDocument {
        self.document.clone()
    }
    /// 单行字面量流式查找，跨块尾部最多保留查询长度减一的字节。
    pub fn search(
        &mut self,
        query: &str,
        start: usize,
        count: usize,
    ) -> Result<ReadSearch, OperationError> {
        if query.is_empty()
            || query.len() > 1024
            || query.contains(['\r', '\n'])
            || start >= self.offsets.len()
            || count == 0
            || count > 100
        {
            return Err(OperationError::new("INVALID_INPUT"));
        }
        let deadline = Instant::now() + Duration::from_secs(120);
        self.opened.verify(deadline).map_err(read_error)?;
        let mut position = self.offsets[start];
        let mut carry = Vec::new();
        let mut cache = None;
        let mut lines = Vec::new();
        while position < self.opened.len && lines.len() < count {
            write_guard::check_time(deadline)?;
            let end = (position + BLOCK as u64).min(self.opened.len);
            let base = position - carry.len() as u64;
            carry.extend(self.range(position, end, &mut cache)?);
            for (index, bytes) in carry.windows(query.len()).enumerate() {
                if bytes != query.as_bytes() {
                    continue;
                }
                let absolute = base + index as u64;
                let line = self.offsets.partition_point(|offset| *offset <= absolute);
                if lines.last() != Some(&line) {
                    lines.push(line);
                }
                if lines.len() == count {
                    break;
                }
            }
            let keep = carry.len().min(query.len() - 1);
            carry.drain(..carry.len() - keep);
            position = end;
        }
        self.opened.verify(deadline).map_err(read_error)?;
        let next = lines
            .last()
            .copied()
            .filter(|line| lines.len() == count && *line < self.offsets.len());
        Ok(ReadSearch {
            document_id: self.document.document_id.clone(),
            lines,
            next_line: next,
        })
    }

    /// 页数据只读原索引覆盖的块；同元数据下的内容改变也会摘要不符。
    pub fn read_page(
        &mut self,
        start: usize,
        count: usize,
        byte_offset: u64,
    ) -> Result<ReadPage, OperationError> {
        if start >= self.offsets.len()
            || count == 0
            || count > 200
            || (byte_offset > 0 && count != 1)
        {
            return Err(OperationError::new("INVALID_INPUT"));
        }
        let deadline = Instant::now() + Duration::from_secs(120);
        self.opened.verify(deadline).map_err(read_error)?;
        let mut cache = None;
        let mut lines = Vec::new();
        let mut total = 0;
        for number in start..(start + count).min(self.offsets.len()) {
            write_guard::check_time(deadline)?;
            let begin = self.offsets[number];
            let mut end = self
                .offsets
                .get(number + 1)
                .copied()
                .unwrap_or(self.opened.len);
            // 只读取行尾至多两个字节，识别原换行而不加载整条长行。
            let tail = self.range(end.saturating_sub(2).max(begin), end, &mut cache)?;
            if tail.ends_with(b"\r\n") {
                end -= 2;
            } else if tail.ends_with(b"\n") || tail.ends_with(b"\r") {
                end -= 1;
            }
            if byte_offset > end - begin {
                return Err(OperationError::new("INVALID_INPUT"));
            }
            let from = begin + byte_offset;
            let to = (from + SEGMENT).min(end);
            let mut bytes = self.range(from, to, &mut cache)?;
            let valid = match std::str::from_utf8(&bytes) {
                Ok(_) => bytes.len(),
                Err(error) if error.error_len().is_none() && to < end => error.valid_up_to(),
                Err(_) => return Err(OperationError::new("INVALID_INPUT")),
            };
            bytes.truncate(valid);
            if total + bytes.len() > PAGE_BYTES {
                break;
            }
            total += bytes.len();
            let next = byte_offset + bytes.len() as u64;
            let text =
                String::from_utf8(bytes).map_err(|_| OperationError::new("FILE_READ_ENCODING"))?;
            lines.push(ReadLine {
                line_number: number + 1,
                text,
                byte_offset,
                next_byte_offset: (begin + next < end).then_some(next),
            });
        }
        self.opened.verify(deadline).map_err(read_error)?;
        let next = start + lines.len();
        Ok(ReadPage {
            document_id: self.document.document_id.clone(),
            start_line: start,
            lines,
            next_line: (next < self.offsets.len()).then_some(next),
        })
    }
    /// 每页仅缓存最近一个原始块，摘要核对后才向响应复制所需范围。
    fn range(
        &mut self,
        from: u64,
        to: u64,
        cache: &mut Option<(usize, Vec<u8>)>,
    ) -> Result<Vec<u8>, OperationError> {
        let mut output = Vec::with_capacity((to - from) as usize);
        let mut position = from;
        while position < to {
            let index = (position / BLOCK as u64) as usize;
            if cache.as_ref().map(|(key, _)| *key) != Some(index) {
                let base = index as u64 * BLOCK as u64;
                let mut bytes = vec![0; (self.opened.len - base).min(BLOCK as u64) as usize];
                self.opened
                    .file
                    .seek(SeekFrom::Start(base))
                    .map_err(|_| OperationError::new("FILE_CHANGED"))?;
                self.opened
                    .file
                    .read_exact(&mut bytes)
                    .map_err(|_| OperationError::new("FILE_CHANGED"))?;
                let hash: [u8; 32] = Sha256::digest(&bytes).into();
                if self.hashes.get(index) != Some(&hash) {
                    return Err(OperationError::new("FILE_CHANGED"));
                }
                *cache = Some((index, bytes));
            }
            if let Some((_, bytes)) = cache {
                let offset = (position % BLOCK as u64) as usize;
                let length = (to - position).min((bytes.len() - offset) as u64) as usize;
                output.extend_from_slice(&bytes[offset..offset + length]);
                position += length as u64;
            }
        }
        Ok(output)
    }
}

/// 只读错误沿用安全文件边界，但不向界面暴露冲突编辑术语。
fn read_error(error: OperationError) -> OperationError {
    match error.code.as_str() {
        "STALE_CONFLICT" => OperationError::new("FILE_CHANGED"),
        "UNSUPPORTED_CONFLICT" => OperationError::new("FILE_UNAVAILABLE"),
        _ => error,
    }
}

impl super::ProjectFilesSession {
    /// 只有当前清单签发的普通文件才能建立只读文档，每个会话最多八份。
    pub fn open_read_document(&self, file_id: &str) -> Result<ReadDocument, OperationError> {
        let deadline = Instant::now() + Duration::from_secs(120);
        write_guard::validate_snapshot(&self.git, &self.repository, &self.state, deadline)?;
        let (_, path, _) = self
            .files
            .iter()
            .find(|(id, _, _)| id == file_id)
            .ok_or_else(|| OperationError::new("FILE_UNAVAILABLE"))?;
        if self
            .readers
            .lock()
            .map_err(|_| OperationError::new("FILE_UNAVAILABLE"))?
            .len()
            >= 8
        {
            return Err(OperationError::new("FILE_READ_LIMIT"));
        }
        let reader = PagedDocument::open(&self.repository.root, path, file_id)?;
        let document = reader.metadata();
        let mut readers = self
            .readers
            .lock()
            .map_err(|_| OperationError::new("FILE_UNAVAILABLE"))?;
        if readers.len() >= 8 {
            return Err(OperationError::new("FILE_READ_LIMIT"));
        }
        readers.insert(document.document_id.clone(), reader);
        Ok(document)
    }
    /// 文档 ID 只在本文件会话内有效；读取不能升级为保存能力。
    pub fn read_document_page(
        &self,
        document_id: &str,
        start: usize,
        count: usize,
        byte_offset: u64,
    ) -> Result<ReadPage, OperationError> {
        let mut readers = self
            .readers
            .lock()
            .map_err(|_| OperationError::new("FILE_UNAVAILABLE"))?;
        readers
            .get_mut(document_id)
            .ok_or_else(|| OperationError::new("FILE_UNAVAILABLE"))?
            .read_page(start, count, byte_offset)
    }
    /// 关闭立即释放句柄与索引，重复关闭不产生文件写入。
    pub fn close_read_document(&self, document_id: &str) -> Result<(), OperationError> {
        self.readers
            .lock()
            .map_err(|_| OperationError::new("FILE_UNAVAILABLE"))?
            .remove(document_id);
        Ok(())
    }
    /// 查找仅针对当前会话已打开文档，仍不能传任意路径。
    pub fn search_read_document(
        &self,
        document_id: &str,
        query: &str,
        start: usize,
        count: usize,
    ) -> Result<ReadSearch, OperationError> {
        let mut readers = self
            .readers
            .lock()
            .map_err(|_| OperationError::new("FILE_UNAVAILABLE"))?;
        readers
            .get_mut(document_id)
            .ok_or_else(|| OperationError::new("FILE_UNAVAILABLE"))?
            .search(query, start, count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// 查找跨越块边界，分页按行推进且不会重复同一行。
    #[test]
    fn search_crosses_block_boundary_and_resumes_by_line() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("text"),
            format!("{}目标目标\n中间\n目标", "a".repeat(BLOCK - 2)),
        )
        .unwrap();
        let mut reader = PagedDocument::open(dir.path(), "text", "f").unwrap();
        let first = reader.search("目标", 0, 1).unwrap();
        assert_eq!(first.lines, [1]);
        assert_eq!(first.next_line, Some(1));
        let second = reader.search("目标", first.next_line.unwrap(), 1).unwrap();
        assert_eq!(second.lines, [3]);
        assert_eq!(second.next_line, None);
        assert_eq!(reader.search("\n", 0, 1).unwrap_err().code, "INVALID_INPUT");
    }

    /// 十万行目标不生成完整正文模型，首尾页与末尾空行保持真实位置。
    #[test]
    fn indexes_ten_megabytes_and_reads_tail_without_full_body() {
        let dir = tempfile::tempdir().unwrap();
        let line = format!("{}\n", "a".repeat(104));
        std::fs::write(dir.path().join("large"), line.repeat(100_000)).unwrap();
        let mut reader = PagedDocument::open(dir.path(), "large", "f").unwrap();
        assert!(reader.metadata().byte_length >= 10 * 1024 * 1024);
        assert_eq!(reader.metadata().line_count, 100_001);
        assert_eq!(reader.metadata().line_ending, "lf");
        assert!(reader.offsets.len() * 8 + reader.hashes.len() * 32 < 1024 * 1024);
        let page = reader.read_page(99_990, 20, 0).unwrap();
        assert_eq!(page.lines.len(), 11);
        assert_eq!(page.lines[0].line_number, 99_991);
        assert_eq!(page.lines[0].text.len(), 104);
        assert_eq!(page.lines.last().unwrap().text, "");
        assert_eq!(page.next_line, None);
    }

    /// 跨块 UTF-8、长行字节续读、BOM 与混合换行不能丢字符。
    #[test]
    fn segments_utf8_and_preserves_line_boundaries() {
        let dir = tempfile::tempdir().unwrap();
        let body = format!("{}中文", "a".repeat(BLOCK - 5));
        std::fs::write(
            dir.path().join("long"),
            format!("\u{feff}{body}\r\n第二\r第三\n"),
        )
        .unwrap();
        let mut reader = PagedDocument::open(dir.path(), "long", "f").unwrap();
        assert!(reader.metadata().bom);
        assert_eq!(reader.metadata().line_count, 4);
        assert_eq!(reader.metadata().line_ending, "mixed");
        let mut cursor = 0;
        let mut result = String::new();
        loop {
            let page = reader.read_page(0, 1, cursor).unwrap();
            let line = &page.lines[0];
            assert!(line.text.len() <= SEGMENT as usize);
            result.push_str(&line.text);
            if let Some(next) = line.next_byte_offset {
                assert!(next > cursor);
                cursor = next;
            } else {
                break;
            }
        }
        assert_eq!(result, body);
        let page = reader.read_page(1, 3, 0).unwrap();
        assert_eq!(
            page.lines
                .iter()
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>(),
            ["第二", "第三", ""]
        );
        assert_eq!(reader.read_page(1, 1, 1).unwrap_err().code, "INVALID_INPUT");
    }

    /// 页文本字节受限；超长行仍给出显式续读位置。
    #[test]
    fn page_budget_and_invalid_requests_are_bounded() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("long"),
            format!("{}\n", "x".repeat(20_000)).repeat(30),
        )
        .unwrap();
        let mut reader = PagedDocument::open(dir.path(), "long", "f").unwrap();
        let page = reader.read_page(0, 200, 0).unwrap();
        assert_eq!(page.lines.len(), 16);
        assert_eq!(page.next_line, Some(16));
        assert_eq!(
            page.lines.iter().map(|line| line.text.len()).sum::<usize>(),
            PAGE_BYTES
        );
        assert_eq!(page.lines[0].next_byte_offset, Some(SEGMENT));
        assert_eq!(
            reader.read_page(0, 201, 0).unwrap_err().code,
            "INVALID_INPUT"
        );
        assert_eq!(reader.read_page(0, 0, 0).unwrap_err().code, "INVALID_INPUT");
        assert_eq!(
            reader.read_page(100, 1, 0).unwrap_err().code,
            "INVALID_INPUT"
        );
    }

    /// 同长度且恢复 mtime 的修改仍被块摘要拒绝，不拼接不同内容版本。
    #[test]
    fn detects_modified_blocks_even_when_mtime_is_restored() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("text");
        std::fs::write(&path, b"original\n").unwrap();
        let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
        let mut reader = PagedDocument::open(dir.path(), "text", "f").unwrap();
        let mut file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
        file.write_all(b"modified\n").unwrap();
        file.set_modified(modified).unwrap();
        assert_eq!(reader.read_page(0, 1, 0).unwrap_err().code, "FILE_CHANGED");
    }

    /// 非文本、编码及索引上限在创建文档时明确拒绝。
    #[test]
    fn rejects_binary_encoding_and_oversized_index() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("binary"), b"a\0b").unwrap();
        std::fs::write(dir.path().join("encoding"), [0xff]).unwrap();
        assert_eq!(
            PagedDocument::open(dir.path(), "binary", "f")
                .err()
                .unwrap()
                .code,
            "FILE_READ_BINARY"
        );
        assert_eq!(
            PagedDocument::open(dir.path(), "encoding", "f")
                .err()
                .unwrap()
                .code,
            "FILE_READ_ENCODING"
        );
        let file = std::fs::File::create(dir.path().join("huge")).unwrap();
        file.set_len(MAX_BYTES + 1).unwrap();
        assert_eq!(
            PagedDocument::open(dir.path(), "huge", "f")
                .err()
                .unwrap()
                .code,
            "FILE_READ_TOO_LARGE"
        );
        std::fs::write(dir.path().join("lines"), vec![b'\n'; MAX_LINES]).unwrap();
        assert_eq!(
            PagedDocument::open(dir.path(), "lines", "f")
                .err()
                .unwrap()
                .code,
            "FILE_READ_TOO_LARGE"
        );
    }

    /// 父目录被换成仓库外符号链接时，保留的旧句柄也不能继续发布正文。
    #[cfg(unix)]
    #[test]
    fn rejects_replaced_ancestor_after_open() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("inner")).unwrap();
        std::fs::write(dir.path().join("inner/text"), b"original").unwrap();
        std::fs::write(outside.path().join("text"), b"outside").unwrap();
        let mut reader = PagedDocument::open(dir.path(), "inner/text", "f").unwrap();
        std::fs::rename(dir.path().join("inner"), dir.path().join("previous")).unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join("inner")).unwrap();
        assert_eq!(reader.read_page(0, 1, 0).unwrap_err().code, "FILE_CHANGED");
    }
}
