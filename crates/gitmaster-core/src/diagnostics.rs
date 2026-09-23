use crate::git::OperationError;
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};
static NEXT: AtomicU64 = AtomicU64::new(1);
thread_local! { static CURRENT: std::cell::Cell<u64> = const { std::cell::Cell::new(0) }; }
thread_local! { static CURRENT_PARENT: std::cell::Cell<u64> = const { std::cell::Cell::new(0) }; }

/// 恢复嵌套诊断上下文，即使业务发生 panic 也不污染后续请求。
struct Parent(u64, u64);
impl Drop for Parent {
    /// 离开当前阶段后恢复调用方的关联编号。
    fn drop(&mut self) {
        CURRENT.set(self.0);
        CURRENT_PARENT.set(self.1);
    }
}

/// 可在线程边界安全复制的诊断关联上下文。
#[derive(Clone, Copy)]
pub struct Context(u64, u64);

/// 捕获当前线程的诊断编号，供后台工作线程延续关联关系。
pub fn capture_context() -> Context {
    Context(CURRENT.get(), CURRENT_PARENT.get())
}

/// 在给定诊断上下文中运行闭包，并在返回或 panic 后恢复原上下文。
pub fn with_context<T>(context: Context, work: impl FnOnce() -> T) -> T {
    let _parent = Parent(
        CURRENT.replace(context.0),
        CURRENT_PARENT.replace(context.1),
    );
    work()
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

/// 记录固定白名单阶段的累计耗时，并沿用当前诊断关联编号。
pub fn log_elapsed(stage: &'static str, elapsed: std::time::Duration) {
    if !matches!(
        stage,
        "repository_queue_wait"
            | "git_process_create"
            | "git_run_to_observed_exit"
            | "git_pipe_drain"
            | "git_process_reap"
    ) {
        return;
    }
    let id = CURRENT.get();
    let parent = CURRENT_PARENT.get();
    log::debug!(target: "gitmaster::diagnostic", "阶段 id={} parent={} stage={} elapsed_ms={}", id, parent, stage, elapsed.as_millis());
}

/// 记录业务阶段的开始、耗时和结果，永远不序列化业务输入输出。
pub fn measure<T>(
    stage: &'static str,
    work: impl FnOnce() -> Result<T, OperationError>,
) -> Result<T, OperationError> {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let parent_id = CURRENT.replace(id);
    let previous_parent = CURRENT_PARENT.replace(parent_id);
    let parent = Parent(parent_id, previous_parent);
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
    /// 嵌套阶段及跨线程继承不应泄漏上下文，异常退出也恢复原编号。
    #[test]
    fn nested_context_is_restored_after_panic_and_thread_handoff() {
        assert_eq!((CURRENT.get(), CURRENT_PARENT.get()), (0, 0));
        measure("outer", || {
            let outer = CURRENT.get();
            let context = capture_context();
            std::thread::spawn(move || {
                with_context(context, || {
                    assert_eq!(CURRENT.get(), outer);
                    let _ = std::panic::catch_unwind(|| {
                        let _: Result<(), OperationError> = measure("inner", || {
                            assert_eq!(CURRENT_PARENT.get(), outer);
                            panic!("测试上下文恢复");
                        });
                    });
                    assert_eq!(CURRENT.get(), outer);
                });
                assert_eq!((CURRENT.get(), CURRENT_PARENT.get()), (0, 0));
            })
            .join()
            .unwrap();
            assert_eq!(CURRENT.get(), outer);
            Ok(())
        })
        .unwrap();
        assert_eq!((CURRENT.get(), CURRENT_PARENT.get()), (0, 0));
    }

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
