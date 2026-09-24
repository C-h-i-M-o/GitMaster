//! 编辑文档的字节契约：BOM 独立保存，正文保留原始换行，不转换编码。
use crate::git::OperationError;
use serde::Serialize;
use sha2::{Digest, Sha256};

pub const MAX_EDIT_BYTES: usize = 2 * 1024 * 1024;

/// 完整 UTF-8 文本模型，不使用截断预览作为可保存文档。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorText {
    pub content: String,
    pub bom: bool,
    pub line_ending: &'static str,
    pub content_version: String,
}

/// 完整解码后才允许编辑；二进制、错误编码及超限均显式拒绝。
pub fn decode(bytes: &[u8]) -> Result<EditorText, OperationError> {
    if bytes.len() > MAX_EDIT_BYTES {
        return Err(OperationError::new("FILE_EDIT_TOO_LARGE"));
    }
    let bom = bytes.starts_with(&[0xef, 0xbb, 0xbf]);
    let body = if bom { &bytes[3..] } else { bytes };
    if body.contains(&0) {
        return Err(OperationError::new("FILE_EDIT_BINARY"));
    }
    let content =
        std::str::from_utf8(body).map_err(|_| OperationError::new("FILE_EDIT_ENCODING"))?;
    let mut lf = false;
    let mut crlf = false;
    let mut cr = false;
    for (index, byte) in body.iter().enumerate() {
        if *byte == b'\n' {
            if index > 0 && body[index - 1] == b'\r' {
                crlf = true;
            } else {
                lf = true;
            }
        }
        if *byte == b'\r' && body.get(index + 1) != Some(&b'\n') {
            cr = true;
        }
    }
    let line_ending = match (lf, crlf, cr) {
        (false, false, false) => "none",
        (true, false, false) => "lf",
        (false, true, false) => "crlf",
        (false, false, true) => "cr",
        _ => "mixed",
    };
    // 代码编辑器会统一模型换行，混合换行只能保留在只读预览中。
    if line_ending == "mixed" {
        return Err(OperationError::new("FILE_EDIT_LINE_ENDING"));
    }
    Ok(EditorText {
        content: content.to_owned(),
        bom,
        line_ending,
        content_version: format!("{:x}", Sha256::digest(bytes)),
    })
}

/// 编码只恢复原文件 BOM，正文按完整草稿原样输出；调用方负责文件身份复核。
pub fn encode(original: &EditorText, content: &str) -> Result<Vec<u8>, OperationError> {
    let size = content
        .len()
        .checked_add(if original.bom { 3 } else { 0 })
        .ok_or_else(|| OperationError::new("FILE_EDIT_TOO_LARGE"))?;
    if size > MAX_EDIT_BYTES {
        return Err(OperationError::new("FILE_EDIT_TOO_LARGE"));
    }
    if content.contains('\0') {
        return Err(OperationError::new("FILE_EDIT_BINARY"));
    }
    let mut bytes = Vec::with_capacity(size);
    if original.bom {
        bytes.extend_from_slice(&[0xef, 0xbb, 0xbf]);
    }
    bytes.extend_from_slice(content.as_bytes());
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    /// 支持的文本逐字节还原；混合换行拒绝进入可保存模型。
    #[test]
    fn exact_roundtrip_preserves_bom_and_every_line_ending() {
        for bytes in [
            b"".as_slice(),
            b"\xef\xbb\xbf",
            b"a\r\nb\r\n",
            b"a\nb",
            b"a\rb",
        ] {
            let document = decode(bytes).unwrap();
            assert_eq!(encode(&document, &document.content).unwrap(), bytes);
        }
        assert_eq!(
            decode(b"a\r\nb\nc\r").unwrap_err().code,
            "FILE_EDIT_LINE_ENDING"
        );
    }
    /// 编辑模型不得接受二进制、不可解码或被截断的大文件。
    #[test]
    fn rejects_non_text_and_oversized_documents() {
        assert_eq!(decode(b"a\0b").unwrap_err().code, "FILE_EDIT_BINARY");
        assert_eq!(decode(&[0xff]).unwrap_err().code, "FILE_EDIT_ENCODING");
        assert_eq!(
            decode(&vec![b'a'; MAX_EDIT_BYTES + 1]).unwrap_err().code,
            "FILE_EDIT_TOO_LARGE"
        );
        let document = decode(b"\xef\xbb\xbf").unwrap();
        assert_eq!(
            encode(&document, &"a".repeat(MAX_EDIT_BYTES))
                .unwrap_err()
                .code,
            "FILE_EDIT_TOO_LARGE"
        );
        assert_ne!(
            decode(b"one").unwrap().content_version,
            decode(b"two").unwrap().content_version
        );
    }
}
