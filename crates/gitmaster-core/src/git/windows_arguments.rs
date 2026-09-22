use super::OperationError;

/// 按 Windows CRT 的引号和反斜杠规则编码单个参数，始终保持一个 argv 元素。
pub(super) fn quote_argument(value: &[u16]) -> Result<Vec<u16>, OperationError> {
    if value.contains(&0) {
        return Err(OperationError::new("INVALID_INPUT"));
    }
    let mut result = vec![b'"' as u16];
    let mut slashes = 0;
    for &unit in value {
        if unit == b'\\' as u16 {
            slashes += 1;
            continue;
        }
        let count = if unit == b'"' as u16 {
            slashes * 2 + 1
        } else {
            slashes
        };
        result.extend(std::iter::repeat_n(b'\\' as u16, count));
        result.push(unit);
        slashes = 0;
    }
    result.extend(std::iter::repeat_n(b'\\' as u16, slashes * 2));
    result.push(b'"' as u16);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    /// 空参数、空格、双引号、尾反斜杠和非 BMP 字符按原字节语义传递。
    #[test]
    fn windows_argument_quoting_preserves_boundaries() {
        for (input, expected) in [
            ("", "\"\""),
            ("中文 空格.txt", "\"中文 空格.txt\""),
            ("a\"b", "\"a\\\"b\""),
            ("C:\\尾部\\", "\"C:\\尾部\\\\\""),
            ("a\\\"b", "\"a\\\\\\\"b\""),
            ("🧪", "\"🧪\""),
        ] {
            let encoded = quote_argument(&input.encode_utf16().collect::<Vec<_>>()).unwrap();
            assert_eq!(String::from_utf16(&encoded).unwrap(), expected);
        }
        assert!(quote_argument(&[0]).is_err());
        assert_eq!(quote_argument(&[0xd800]).unwrap(), vec![34, 0xd800, 34]);
    }
}
