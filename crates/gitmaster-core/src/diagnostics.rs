use crate::git::OperationError;
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};
static NEXT: AtomicU64 = AtomicU64::new(1);
thread_local! { static CURRENT: std::cell::Cell<u64> = const { std::cell::Cell::new(0) }; }

/// 恢复嵌套诊断上下文，即使业务发生 panic 也不污染后续请求。
struct Parent(u64);
impl Drop for Parent {
    /// 离开当前阶段后恢复调用方的关联编号。
    fn drop(&mut self) {
        CURRENT.set(self.0);
    }
}

/// 只允许内部错误码字符进入日志，拒绝路径、URL 与任意文本。
pub fn safe_code(value: &str) -> &str {
    if !value.is_empty()
        && value.len() <= 64
        && value.bytes().all(|b| b.is_ascii_uppercase() || b == b'_')
    {
        value
    } else {
        "UNKNOWN"
    }
}

/// 记录业务阶段的开始、耗时和结果，永远不序列化业务输入输出。
pub fn measure<T>(
    stage: &'static str,
    work: impl FnOnce() -> Result<T, OperationError>,
) -> Result<T, OperationError> {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let parent = Parent(CURRENT.replace(id));
    let start = Instant::now();
    log::trace!(target: "gitmaster::diagnostic", "开始 id={} parent={} stage={}", id, parent.0, stage);
    let result = work();
    match &result {
        Ok(_) => {
            log::debug!(target: "gitmaster::diagnostic", "完成 id={} parent={} stage={} elapsed_ms={}", id, parent.0, stage, start.elapsed().as_millis())
        }
        Err(error) => {
            let os = error.diagnostic.as_ref().and_then(|d| d.os_code);
            let exit = error.diagnostic.as_ref().and_then(|d| d.exit_code);
            let level = if matches!(
                error.code.as_str(),
                "STALE_REQUEST" | "STALE_SNAPSHOT" | "STALE_WRITE_PLAN"
            ) {
                log::Level::Warn
            } else {
                log::Level::Error
            };
            log::log!(target: "gitmaster::diagnostic", level, "失败 id={} parent={} stage={} code={} os_code={:?} exit_code={:?} elapsed_ms={}", id, parent.0, stage, safe_code(&error.code), os, exit, start.elapsed().as_millis());
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    /// 验证凭据、路径、换行和超长文本不进入错误码字段。
    #[test]
    fn rejects_untrusted_text() {
        for value in [
            "https://user:password@host",
            "C:\\secret",
            "ERROR\nTOKEN",
            "",
            &"A".repeat(65),
        ] {
            assert_eq!(safe_code(value), "UNKNOWN");
        }
        assert_eq!(safe_code("TIMEOUT"), "TIMEOUT");
    }
}
