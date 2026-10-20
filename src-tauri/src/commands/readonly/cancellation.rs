//! 文件读取取消注册表，按仓库和读取 ID 隔离请求生命周期。
use gitmaster_core::git::OperationError;
use std::{
    collections::{HashMap, VecDeque},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, MutexGuard,
    },
};
const MAX_ACTIVE: usize = 64;
const MAX_ID: usize = 128;
const MAX_TOMBSTONES: usize = 128;
type Key = (String, String);
#[derive(Clone, Default)]
pub(super) struct Registry(Arc<Mutex<State>>);
struct State {
    active: HashMap<Key, Arc<AtomicBool>>,
    cancelled: VecDeque<Key>,
}
impl Default for State {
    /// 初始注册表不持有任何请求或乱序取消标记。
    fn default() -> Self {
        Self {
            active: HashMap::new(),
            cancelled: VecDeque::new(),
        }
    }
}
/// 毒化后仍然回收资源，避免取消请求静默失效。
fn lock(state: &Mutex<State>) -> MutexGuard<'_, State> {
    state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
pub(super) struct Registration {
    key: Key,
    flag: Arc<AtomicBool>,
    registry: Registry,
}
impl Registration {
    /// 供排队与结果发布门禁查询取消状态。
    pub(super) fn cancelled(&self) -> bool {
        self.flag.load(Ordering::Acquire)
    }
}
impl Drop for Registration {
    /// 只释放当前身份，不能移除同键的后续请求。
    fn drop(&mut self) {
        let mut state = lock(&self.registry.0);
        if state
            .active
            .get(&self.key)
            .is_some_and(|current| Arc::ptr_eq(current, &self.flag))
        {
            state.active.remove(&self.key);
        }
    }
}
impl Registry {
    /// 原子注册有界读取，重复键拒绝覆盖且只消费本键的取消标记。
    pub(super) fn register(
        &self,
        repository: String,
        read_id: String,
    ) -> Result<Registration, OperationError> {
        if repository.is_empty()
            || repository.len() > MAX_ID
            || read_id.is_empty()
            || read_id.len() > MAX_ID
        {
            return Err(OperationError::new("INVALID_INPUT"));
        }
        let key = (repository, read_id);
        let mut state = lock(&self.0);
        if state.active.contains_key(&key) || state.active.len() >= MAX_ACTIVE {
            return Err(OperationError::new("INVALID_INPUT"));
        }
        let flag = Arc::new(AtomicBool::new(false));
        if let Some(index) = state.cancelled.iter().position(|item| item == &key) {
            state.cancelled.remove(index);
            flag.store(true, Ordering::Release);
        }
        state.active.insert(key.clone(), flag.clone());
        Ok(Registration {
            key,
            flag,
            registry: self.clone(),
        })
    }
    /// 取消已注册请求或保留有界抢先取消，仓库身份参与隔离。
    pub(super) fn cancel(&self, repository: &str, read_id: &str) {
        let mut state = lock(&self.0);
        let key = (repository.to_owned(), read_id.to_owned());
        if let Some(flag) = state.active.get(&key) {
            flag.store(true, Ordering::Release);
            return;
        }
        if !state.cancelled.contains(&key) {
            if state.cancelled.len() >= MAX_TOMBSTONES {
                state.cancelled.pop_front();
            }
            state.cancelled.push_back(key);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    /// 重复身份不能覆盖，抢先取消不被其他请求消费，跨仓库取消相互隔离。
    #[test]
    fn registry_isolated_and_bounded() {
        let registry = Registry::default();
        let a = registry.register("repo-a".into(), "same".into()).unwrap();
        assert!(registry.register("repo-a".into(), "same".into()).is_err());
        registry.cancel("repo-a", "queued");
        let b = registry.register("repo-b".into(), "same".into()).unwrap();
        let queued = registry.register("repo-a".into(), "queued".into()).unwrap();
        assert!(queued.cancelled());
        registry.cancel("repo-a", "same");
        assert!(a.cancelled());
        assert!(!b.cancelled());
        drop(a);
        drop(b);
        drop(queued);
        let mut tokens: Vec<_> = (0..MAX_ACTIVE)
            .map(|n| registry.register("r".into(), n.to_string()).unwrap())
            .collect();
        assert!(registry.register("r".into(), "overflow".into()).is_err());
        tokens.pop();
        registry.register("r".into(), "replacement".into()).unwrap();
        for n in 0..256 {
            registry.cancel("other", &n.to_string());
        }
        assert_eq!(lock(&registry.0).cancelled.len(), MAX_TOMBSTONES);
    }
}
