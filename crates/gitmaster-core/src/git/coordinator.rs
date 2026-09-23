//! 按仓库公共目录协调后台任务，计划和结果均仅在当前进程保留。

use super::{
    OperationCounts, OperationError, OperationHandle, OperationKind, OperationPhase,
    OperationProgress, OperationRecord, OperationResult, RepositoryHandle, WriteRefresh,
};
use std::{
    collections::{HashMap, VecDeque},
    path::{Component, Path, PathBuf},
    sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, Weak},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

/// 只由后端路径构造的串行键，不接受前端传入的任意字符串。
#[derive(Debug, Clone)]
pub struct CoordinationKey(PathBuf);

impl CoordinationKey {
    /// linked worktree 的 Git 公共目录归一化为同一个队列键。
    pub fn repository(repository: &RepositoryHandle) -> Result<Self, OperationError> {
        let path = std::fs::canonicalize(&repository.common_dir)
            .map_err(|_| OperationError::new("ACCESS_DENIED"))?;
        Ok(Self(path))
    }

    /// 新目录使用已存在父目录的真实路径，不解析或覆盖已有目标。
    pub fn clone_target(parent: &Path, name: &str) -> Result<Self, OperationError> {
        let mut components = Path::new(name).components();
        if name.is_empty()
            || name.contains(['/', '\\'])
            || !matches!(components.next(), Some(Component::Normal(_)))
            || components.next().is_some()
        {
            return Err(OperationError::new("INVALID_INPUT"));
        }
        let parent =
            std::fs::canonicalize(parent).map_err(|_| OperationError::new("ACCESS_DENIED"))?;
        if !parent.is_dir() {
            return Err(OperationError::new("INVALID_INPUT"));
        }
        let target = parent.join(name);
        match std::fs::symlink_metadata(&target) {
            Ok(_) => return Err(OperationError::new("TARGET_EXISTS")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(OperationError::new("ACCESS_DENIED")),
        }
        Ok(Self(target))
    }
}

/// 队列注册表以弱引用回收空闲仓库，避免打开过的项目无限累积。
#[derive(Clone, Default)]
pub struct RepositoryQueues {
    gates: Arc<Mutex<HashMap<PathBuf, Weak<Gate>>>>,
}

/// 每个仓库一个 FIFO；active 包括已预留但线程尚未启动的任务。
#[derive(Default)]
struct Gate {
    state: Mutex<GateState>,
    changed: Condvar,
}

/// 门闩的序号和有界等待项，仅在持有 state 锁时访问。
#[derive(Default)]
struct GateState {
    next_ticket: u64,
    active: Option<u64>,
    waiting: VecDeque<u64>,
}

/// 锁内不运行业务代码；毒化时恢复内部状态用于结束及查询任务。
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl RepositoryQueues {
    /// 按调用先后预留位置；一个活动项之外最多允许十六个等待项。
    fn reserve(&self, key: &CoordinationKey) -> Result<QueueReservation, OperationError> {
        let gate = {
            let mut gates = lock(&self.gates);
            gates.retain(|_, gate| gate.strong_count() > 0);
            match gates.get(&key.0).and_then(Weak::upgrade) {
                Some(gate) => gate,
                None => {
                    let gate = Arc::new(Gate::default());
                    gates.insert(key.0.clone(), Arc::downgrade(&gate));
                    gate
                }
            }
        };
        let mut state = lock(&gate.state);
        if state.active.is_some() && state.waiting.len() >= 16 {
            return Err(OperationError::new("QUEUE_FULL"));
        }
        let ticket = state.next_ticket;
        state.next_ticket += 1;
        if state.active.is_none() {
            state.active = Some(ticket);
        } else {
            state.waiting.push_back(ticket);
        }
        drop(state);
        Ok(QueueReservation { gate, ticket })
    }

    /// 写业务准备的排队时间也计入整项预算。
    fn read_until<T>(
        &self,
        key: &CoordinationKey,
        deadline: Instant,
        read: impl FnOnce() -> Result<T, OperationError>,
    ) -> Result<T, OperationError> {
        let _guard = self.reserve(key)?.enter_until(deadline)?;
        read()
    }

    /// 读取与写入使用同一门闩，避免读取本应用写操作的中间状态。
    pub fn read<T>(
        &self,
        key: &CoordinationKey,
        read: impl FnOnce() -> Result<T, OperationError>,
    ) -> Result<T, OperationError> {
        let _guard = self.reserve(key)?.enter();
        read()
    }
}

/// 预留项在等待、执行或线程启动失败时均由析构释放。
struct QueueReservation {
    gate: Arc<Gate>,
    ticket: u64,
}

impl QueueReservation {
    /// 只等到业务期限，超时通过析构取消预留，后续请求仍可前进。
    fn enter_until(self, deadline: Instant) -> Result<Self, OperationError> {
        let mut state = lock(&self.gate.state);
        loop {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                drop(state);
                return Err(OperationError::new("TIMEOUT"));
            };
            if state.active == Some(self.ticket) {
                drop(state);
                return Ok(self);
            }
            let waited = self
                .gate
                .changed
                .wait_timeout(state, remaining)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state = waited.0;
        }
    }

    /// 阻塞至本预留项位于队首；持有返回对象期间拥有仓库执行权。
    fn enter(self) -> Self {
        let mut state = lock(&self.gate.state);
        while state.active != Some(self.ticket) {
            state = self
                .gate
                .changed
                .wait(state)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        drop(state);
        self
    }
}

impl Drop for QueueReservation {
    /// 活动任务结束时唤醒下一项，取消等待项时仅移除自身。
    fn drop(&mut self) {
        let mut state = lock(&self.gate.state);
        if state.active == Some(self.ticket) {
            state.active = state.waiting.pop_front();
        } else {
            state.waiting.retain(|ticket| *ticket != self.ticket);
        }
        self.gate.changed.notify_all();
    }
}

/// 当前应用共享的队列注册表，所有窗口和仓库会话共用同一实例。
static SHARED_QUEUES: OnceLock<RepositoryQueues> = OnceLock::new();
const PLAN_LIFETIME: Duration = Duration::from_secs(5 * 60);

/// 由业务准备函数返回的计划标识，展示时间不参与有效期判断。
#[derive(Debug, Clone)]
pub struct PreparedOperation {
    pub plan_id: String,
    pub expires_at: u64,
}

/// 业务执行闭包持有不可变请求和指纹，进入队列后先重验再写入。
type Work = Box<dyn FnOnce(&OperationReporter) -> OperationResult + Send + 'static>;

/// 一次性计划绑定会话代次和仓库身份，刷新或重新准备时整体替换。
struct PendingPlan {
    id: String,
    repository_id: Option<String>,
    key: CoordinationKey,
    kind: OperationKind,
    deadline: Instant,
    work: Work,
}

/// 终态保留计划 ID，以便重复 execute 只返回原任务。
struct CompletedOperation {
    plan_id: String,
    record: OperationRecord,
}

/// 会话锁只保护内存状态，不在其临界区读取 Git 或等待队列。
#[derive(Default)]
struct Session {
    generation: u64,
    pending: Option<PendingPlan>,
    current: Option<(String, OperationRecord)>,
    completed: VecDeque<CompletedOperation>,
}

/// 与界面生命周期独立的会话协调器；同仓库队列由整个进程共享。
#[derive(Clone)]
pub struct RepositoryCoordinator {
    session: Arc<Mutex<Session>>,
    queues: RepositoryQueues,
}

impl Default for RepositoryCoordinator {
    /// 默认构造一个独立会话，同时复用全应用队列。
    fn default() -> Self {
        Self::new()
    }
}

impl RepositoryCoordinator {
    /// 创建不依赖 Tauri、窗口或前端组件的仓库操作会话。
    pub fn new() -> Self {
        Self {
            session: Arc::new(Mutex::new(Session::default())),
            queues: SHARED_QUEUES.get_or_init(RepositoryQueues::default).clone(),
        }
    }

    /// 开始准备前取得代次；写操作中禁止新的准备或环境切换。
    pub fn begin_prepare(&self) -> Result<u64, OperationError> {
        let mut session = lock(&self.session);
        ensure_idle(&session)?;
        session.generation += 1;
        session.pending = None;
        Ok(session.generation)
    }

    /// 刷新、换仓库或环境改变时使旧准备及迟到准备失效。
    pub fn invalidate(&self) -> Result<(), OperationError> {
        let mut session = lock(&self.session);
        ensure_idle(&session)?;
        session.generation += 1;
        session.pending = None;
        Ok(())
    }

    /// 业务层完成只读预检后登记一次性计划；不向 IPC 暴露执行闭包。
    pub(crate) fn prepare(
        &self,
        generation: u64,
        repository_id: Option<String>,
        key: CoordinationKey,
        kind: OperationKind,
        work: impl FnOnce(&OperationReporter) -> OperationResult + Send + 'static,
    ) -> Result<PreparedOperation, OperationError> {
        let mut session = lock(&self.session);
        ensure_idle(&session)?;
        if generation != session.generation {
            return Err(OperationError::new("STALE_WRITE_PLAN"));
        }
        let plan_id = operation_id();
        session.pending = Some(PendingPlan {
            id: plan_id.clone(),
            repository_id,
            key,
            kind,
            deadline: Instant::now() + PLAN_LIFETIME,
            work: Box::new(work),
        });
        Ok(PreparedOperation {
            plan_id,
            expires_at: unix_millis() + 300_000,
        })
    }

    /// 消费一次性计划并立即返回句柄；重复请求在保留期内返回同一任务。
    pub fn execute(
        &self,
        repository_id: Option<&str>,
        plan_id: &str,
    ) -> Result<OperationHandle, OperationError> {
        let mut session = lock(&self.session);
        if let Some((id, record)) = &session.current {
            if id == plan_id && record.progress.handle.repository_id.as_deref() == repository_id {
                return Ok(record.progress.handle.clone());
            }
        }
        if let Some(previous) = session.completed.iter().find(|item| {
            item.plan_id == plan_id
                && item.record.progress.handle.repository_id.as_deref() == repository_id
        }) {
            return Ok(previous.record.progress.handle.clone());
        }
        ensure_idle(&session)?;
        let pending = session
            .pending
            .as_ref()
            .filter(|plan| plan.id == plan_id && plan.repository_id.as_deref() == repository_id)
            .ok_or_else(|| OperationError::new("STALE_WRITE_PLAN"))?;
        if Instant::now() >= pending.deadline {
            session.pending = None;
            return Err(OperationError::new("STALE_WRITE_PLAN"));
        }
        // 预留在启动线程前完成，因此高频点击不能改变 FIFO 顺序。
        let reservation = self.queues.reserve(&pending.key)?;
        let pending = session.pending.take().expect("已验证一次性计划");
        let handle = OperationHandle {
            operation_id: operation_id(),
            repository_id: pending.repository_id,
        };
        session.current = Some((
            pending.id,
            OperationRecord {
                progress: OperationProgress {
                    handle: handle.clone(),
                    sequence: 0,
                    kind: pending.kind,
                    phase: OperationPhase::Queued,
                    counts: None,
                    started_at: unix_millis(),
                },
                result: None,
            },
        ));
        session.generation += 1;
        let reporter = OperationReporter {
            session: Arc::clone(&self.session),
            handle: handle.clone(),
            kind: pending.kind,
            started: Instant::now(),
        };
        let failure_reporter = reporter.clone();
        drop(session);
        let spawned = std::thread::Builder::new()
            .name(format!("gitmaster-operation-{}", handle.operation_id))
            .spawn(move || {
                let entered = reservation.enter_until(pending.deadline);
                // 排队等待也可能跨过计划期限，出队后不得执行已经过期的确认。
                let result = if entered.is_err() || Instant::now() >= pending.deadline {
                    OperationResult::Failed {
                        operation_id: reporter.handle.operation_id.clone(),
                        kind: reporter.kind,
                        error: OperationError::new("STALE_WRITE_PLAN"),
                        clone_recovery: None,
                        refresh: WriteRefresh::Failed {
                            error: OperationError::new("STALE_REQUEST"),
                        },
                    }
                } else {
                    let _ = reporter.report(OperationPhase::Checking, None);
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        (pending.work)(&reporter)
                    }))
                    .unwrap_or_else(|_| reporter.unknown())
                };
                reporter.finish(result);
            });
        if spawned.is_err() {
            failure_reporter.finish(OperationResult::Failed {
                operation_id: handle.operation_id.clone(),
                kind: failure_reporter.kind,
                error: OperationError::new("GIT_EXECUTION_FAILED"),
                clone_recovery: None,
                refresh: WriteRefresh::Failed {
                    error: OperationError::new("STALE_REQUEST"),
                },
            });
        }
        Ok(handle)
    }

    /// null 查询当前或最近一次任务；未知和已淘汰的 ID 返回 null。
    pub fn read_operation(
        &self,
        operation_id: Option<&str>,
    ) -> Result<Option<OperationRecord>, OperationError> {
        let session = lock(&self.session);
        if let Some((_, current)) = &session.current {
            if operation_id.is_none_or(|id| id == current.progress.handle.operation_id) {
                return Ok(Some(current.clone()));
            }
        }
        Ok(match operation_id {
            Some(id) => session
                .completed
                .iter()
                .find(|item| item.record.progress.handle.operation_id == id),
            None => session.completed.back(),
        }
        .map(|item| item.record.clone()))
    }

    /// 准备操作的排队及查询共用调用者的总期限。
    pub(crate) fn read_until<T>(
        &self,
        key: &CoordinationKey,
        deadline: Instant,
        read: impl FnOnce() -> Result<T, OperationError>,
    ) -> Result<T, OperationError> {
        self.queues.read_until(key, deadline, read)
    }

    /// 业务读取和准备共享写队列；不得在持有桌面会话锁时调用。
    pub fn read<T>(
        &self,
        key: &CoordinationKey,
        read: impl FnOnce() -> Result<T, OperationError>,
    ) -> Result<T, OperationError> {
        self.queues.read(key, read)
    }
}

/// 活动任务存在时阻止会话变更；读取任务进度不受此限制。
fn ensure_idle(session: &Session) -> Result<(), OperationError> {
    if session.current.is_some() {
        Err(OperationError::new("OPERATION_IN_PROGRESS"))
    } else {
        Ok(())
    }
}

/// 进程启动标识避免重启后计数器归零使旧确认碰到新计划。
fn operation_id() -> String {
    static BOOT_ID: OnceLock<String> = OnceLock::new();
    let boot = BOOT_ID.get_or_init(|| {
        format!(
            "{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        )
    });
    format!("{boot}-{}", super::repository::next_id())
}

/// 仅用于展示的 Unix 毫秒；业务期限使用 Instant。
fn unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// 后台业务任务的受限进度发布器，不能修改另一个任务或已完成结果。
#[derive(Clone)]
pub(crate) struct OperationReporter {
    session: Arc<Mutex<Session>>,
    pub(crate) handle: OperationHandle,
    pub(crate) kind: OperationKind,
    started: Instant,
}

impl OperationReporter {
    /// 业务截止时间从任务入队开始计算，不能在出队后重置整项预算。
    pub(crate) fn deadline(&self, budget: Duration) -> Instant {
        self.started + budget
    }
    /// 发布有可靠来源的阶段及计数；终态只能通过 finish 写入。
    pub(crate) fn report(
        &self,
        phase: OperationPhase,
        counts: Option<OperationCounts>,
    ) -> Result<(), OperationError> {
        if matches!(
            phase,
            OperationPhase::Completed
                | OperationPhase::Failed
                | OperationPhase::Unknown
                | OperationPhase::NeedsResolution
        ) || counts
            .as_ref()
            .is_some_and(|count| count.total.is_some_and(|total| count.completed > total))
        {
            return Err(OperationError::new("INVALID_INPUT"));
        }
        let mut session = lock(&self.session);
        let (_, record) = session
            .current
            .as_mut()
            .filter(|(_, record)| record.progress.handle.operation_id == self.handle.operation_id)
            .ok_or_else(|| OperationError::new("STALE_REQUEST"))?;
        if record.progress.phase != phase {
            log::debug!(target: "gitmaster::diagnostic", "写入阶段 operation_id={} kind={:?} phase={:?} elapsed_ms={}", self.handle.operation_id, self.kind, phase, self.started.elapsed().as_millis());
        }
        record.progress.sequence += 1;
        record.progress.phase = phase;
        record.progress.counts = counts;
        Ok(())
    }

    /// 异常中断时保守标记影响未知，等待后续读取真实 Git 状态。
    fn unknown(&self) -> OperationResult {
        OperationResult::Unknown {
            operation_id: self.handle.operation_id.clone(),
            kind: self.kind,
            error: OperationError::new("WRITE_OUTCOME_UNKNOWN"),
            clone_recovery: None,
            refresh: WriteRefresh::Failed {
                error: OperationError::new("WRITE_OUTCOME_UNKNOWN"),
            },
        }
    }

    /// 结果和终态一同提交到有界历史，后台业务不能伪造操作身份。
    fn finish(&self, mut result: OperationResult) {
        match &result {
            OperationResult::Failed { error, .. } | OperationResult::Unknown { error, .. } => {
                log::error!(target: "gitmaster::diagnostic", "写入失败 operation_id={} kind={:?} code={} elapsed_ms={}", self.handle.operation_id, self.kind, crate::diagnostics::safe_code(&error.code), self.started.elapsed().as_millis())
            }
            _ => {
                log::info!(target: "gitmaster::diagnostic", "写入结束 operation_id={} kind={:?} elapsed_ms={}", self.handle.operation_id, self.kind, self.started.elapsed().as_millis())
            }
        }
        let phase = match &mut result {
            OperationResult::Succeeded {
                operation_id, kind, ..
            } => {
                operation_id.clone_from(&self.handle.operation_id);
                *kind = self.kind;
                OperationPhase::Completed
            }
            OperationResult::Failed {
                operation_id, kind, ..
            } => {
                operation_id.clone_from(&self.handle.operation_id);
                *kind = self.kind;
                OperationPhase::Failed
            }
            OperationResult::Unknown {
                operation_id, kind, ..
            } => {
                operation_id.clone_from(&self.handle.operation_id);
                *kind = self.kind;
                OperationPhase::Unknown
            }
            OperationResult::NeedsResolution {
                operation_id, kind, ..
            } => {
                operation_id.clone_from(&self.handle.operation_id);
                *kind = self.kind;
                OperationPhase::NeedsResolution
            }
        };
        let mut session = lock(&self.session);
        if !session.current.as_ref().is_some_and(|(_, record)| {
            record.progress.handle.operation_id == self.handle.operation_id
        }) {
            return;
        }
        let (plan_id, mut record) = session.current.take().expect("已验证当前任务身份");
        if record.progress.phase != phase {
            log::debug!(target: "gitmaster::diagnostic", "写入阶段 operation_id={} kind={:?} phase={:?} elapsed_ms={}", self.handle.operation_id, self.kind, phase, self.started.elapsed().as_millis());
        }
        record.progress.sequence += 1;
        record.progress.phase = phase;
        record.progress.counts = None;
        record.result = Some(result);
        session
            .completed
            .push_back(CompletedOperation { plan_id, record });
        while session.completed.len() > 16 {
            session.completed.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::repository::{open_repository, tests::Fixture};
    use std::sync::mpsc;

    /// 确认丢失响应后可查询当前任务，重复 execute 不产生第二次写入。
    #[test]
    fn execute_is_once_and_current_operation_is_recoverable() {
        let fixture = Fixture::new();
        let (repo, _) = open_repository(&fixture.git, &fixture.root).unwrap();
        let coordinator = RepositoryCoordinator::new();
        let marker = fixture.root.join("执行次数");
        let captured = marker.clone();
        let (release, wait) = mpsc::channel();
        let plan = coordinator
            .prepare(
                coordinator.begin_prepare().unwrap(),
                Some(repo.id.clone()),
                CoordinationKey::repository(&repo).unwrap(),
                OperationKind::Stage,
                move |reporter| {
                    wait.recv().unwrap();
                    std::fs::write(captured, b"once").unwrap();
                    success(reporter)
                },
            )
            .unwrap();
        let handle = coordinator.execute(Some(&repo.id), &plan.plan_id).unwrap();
        let duplicate = coordinator.execute(Some(&repo.id), &plan.plan_id).unwrap();
        assert_eq!(handle.operation_id, duplicate.operation_id);
        assert_eq!(
            coordinator
                .read_operation(None)
                .unwrap()
                .unwrap()
                .progress
                .handle
                .operation_id,
            handle.operation_id
        );
        assert_eq!(
            coordinator.invalidate().unwrap_err().code,
            "OPERATION_IN_PROGRESS"
        );
        assert_eq!(
            coordinator.begin_prepare().unwrap_err().code,
            "OPERATION_IN_PROGRESS"
        );
        release.send(()).unwrap();
        let completed = wait_for_result(&coordinator, &handle.operation_id);
        assert!(matches!(
            completed.result,
            Some(OperationResult::Succeeded { .. })
        ));
        assert_eq!(std::fs::read(&marker).unwrap(), b"once");
        std::fs::write(&marker, b"external").unwrap();
        assert_eq!(
            coordinator
                .execute(Some(&repo.id), &plan.plan_id)
                .unwrap()
                .operation_id,
            handle.operation_id
        );
        assert_eq!(std::fs::read(&marker).unwrap(), b"external");
    }

    /// 新计划、刷新和过期都会拒绝旧确认，旧准备不能覆盖新会话。
    #[test]
    fn expired_replaced_and_invalidated_plans_cannot_execute() {
        let fixture = Fixture::new();
        let (repo, _) = open_repository(&fixture.git, &fixture.root).unwrap();
        let coordinator = RepositoryCoordinator::new();
        let prepare = || {
            coordinator
                .prepare(
                    coordinator.begin_prepare().unwrap(),
                    Some(repo.id.clone()),
                    CoordinationKey::repository(&repo).unwrap(),
                    OperationKind::Stage,
                    success,
                )
                .unwrap()
        };
        let old = prepare();
        let current = prepare();
        assert_eq!(
            coordinator
                .execute(Some(&repo.id), &old.plan_id)
                .unwrap_err()
                .code,
            "STALE_WRITE_PLAN"
        );
        lock(&coordinator.session)
            .pending
            .as_mut()
            .unwrap()
            .deadline = Instant::now() - Duration::from_secs(1);
        assert_eq!(
            coordinator
                .execute(Some(&repo.id), &current.plan_id)
                .unwrap_err()
                .code,
            "STALE_WRITE_PLAN"
        );
        let old_generation = coordinator.begin_prepare().unwrap();
        let invalidated = prepare();
        coordinator.invalidate().unwrap();
        assert_eq!(
            coordinator
                .execute(Some(&repo.id), &invalidated.plan_id)
                .unwrap_err()
                .code,
            "STALE_WRITE_PLAN"
        );
        assert_eq!(
            coordinator
                .prepare(
                    old_generation,
                    Some(repo.id.clone()),
                    CoordinationKey::repository(&repo).unwrap(),
                    OperationKind::Stage,
                    success,
                )
                .unwrap_err()
                .code,
            "STALE_WRITE_PLAN"
        );
    }

    /// 开始新准备立即废除旧确认，并拒绝先开始后返回的准备结果。
    #[test]
    fn later_preparation_invalidates_inflight_preparation() {
        let fixture = Fixture::new();
        let (repo, _) = open_repository(&fixture.git, &fixture.root).unwrap();
        let coordinator = RepositoryCoordinator::new();
        let old_generation = coordinator.begin_prepare().unwrap();
        let latest = coordinator.begin_prepare().unwrap();
        assert_eq!(
            coordinator
                .prepare(
                    old_generation,
                    Some(repo.id.clone()),
                    CoordinationKey::repository(&repo).unwrap(),
                    OperationKind::Stage,
                    success,
                )
                .unwrap_err()
                .code,
            "STALE_WRITE_PLAN"
        );
        let plan = coordinator
            .prepare(
                latest,
                Some(repo.id.clone()),
                CoordinationKey::repository(&repo).unwrap(),
                OperationKind::Stage,
                success,
            )
            .unwrap();
        coordinator.begin_prepare().unwrap();
        assert_eq!(
            coordinator
                .execute(Some(&repo.id), &plan.plan_id)
                .unwrap_err()
                .code,
            "STALE_WRITE_PLAN"
        );
    }

    /// 排队期间超过有效期时，出队后不运行写入闭包。
    #[test]
    fn queued_plan_expiry_does_not_run_work() {
        let fixture = Fixture::new();
        let (repo, _) = open_repository(&fixture.git, &fixture.root).unwrap();
        let coordinator = RepositoryCoordinator::new();
        let key = CoordinationKey::repository(&repo).unwrap();
        let running = coordinator.queues.reserve(&key).unwrap().enter();
        let marker = fixture.root.join("不能写入");
        let captured = marker.clone();
        let plan = coordinator
            .prepare(
                coordinator.begin_prepare().unwrap(),
                Some(repo.id.clone()),
                key,
                OperationKind::Commit,
                move |reporter| {
                    std::fs::write(captured, b"unexpected").unwrap();
                    success(reporter)
                },
            )
            .unwrap();
        lock(&coordinator.session)
            .pending
            .as_mut()
            .unwrap()
            .deadline = Instant::now() + Duration::from_millis(50);
        let handle = coordinator.execute(Some(&repo.id), &plan.plan_id).unwrap();
        std::thread::sleep(Duration::from_millis(75));
        // 前方任务仍持有队列时，过期项已经退出，不能无限等待其释放。
        assert!(
            matches!(wait_for_result(&coordinator, &handle.operation_id).result,
            Some(OperationResult::Failed { error, .. }) if error.code == "STALE_WRITE_PLAN")
        );
        drop(running);
        assert!(!marker.exists());
    }

    /// 任务 ID 只能在所属会话和仓库使用，进度序号递增且终态不可覆写。
    #[test]
    fn session_binding_and_monotonic_terminal_progress() {
        let fixture = Fixture::new();
        let (repo, _) = open_repository(&fixture.git, &fixture.root).unwrap();
        let coordinator = RepositoryCoordinator::new();
        let other_session = RepositoryCoordinator::new();
        let (send, receive) = mpsc::channel();
        let plan = coordinator
            .prepare(
                coordinator.begin_prepare().unwrap(),
                Some(repo.id.clone()),
                CoordinationKey::repository(&repo).unwrap(),
                OperationKind::Fetch,
                move |reporter| {
                    reporter
                        .report(
                            OperationPhase::Transferring,
                            Some(OperationCounts {
                                completed: 2,
                                total: Some(4),
                            }),
                        )
                        .unwrap();
                    send.send(reporter.clone()).unwrap();
                    success(reporter)
                },
            )
            .unwrap();
        assert_eq!(
            coordinator
                .execute(Some("other"), &plan.plan_id)
                .unwrap_err()
                .code,
            "STALE_WRITE_PLAN"
        );
        assert_eq!(
            other_session
                .execute(Some(&repo.id), &plan.plan_id)
                .unwrap_err()
                .code,
            "STALE_WRITE_PLAN"
        );
        let handle = coordinator.execute(Some(&repo.id), &plan.plan_id).unwrap();
        let reporter = receive.recv_timeout(Duration::from_secs(2)).unwrap();
        let record = wait_for_result(&coordinator, &handle.operation_id);
        assert!(record.progress.sequence >= 3);
        assert_eq!(record.progress.phase, OperationPhase::Completed);
        assert_eq!(
            reporter
                .report(OperationPhase::Writing, None)
                .unwrap_err()
                .code,
            "STALE_REQUEST"
        );
        assert!(other_session
            .read_operation(Some(&handle.operation_id))
            .unwrap()
            .is_none());
    }

    /// 任务异常不能留在运行态，也不能对写入影响作无根据的失败结论。
    #[test]
    fn panic_becomes_unknown_and_releases_queue() {
        let fixture = Fixture::new();
        let (repo, _) = open_repository(&fixture.git, &fixture.root).unwrap();
        let coordinator = RepositoryCoordinator::new();
        let plan = coordinator
            .prepare(
                coordinator.begin_prepare().unwrap(),
                Some(repo.id.clone()),
                CoordinationKey::repository(&repo).unwrap(),
                OperationKind::Commit,
                |_| panic!("测试后台任务异常"),
            )
            .unwrap();
        let handle = coordinator.execute(Some(&repo.id), &plan.plan_id).unwrap();
        assert!(
            matches!(wait_for_result(&coordinator, &handle.operation_id).result,
            Some(OperationResult::Unknown { error, .. }) if error.code == "WRITE_OUTCOME_UNKNOWN")
        );
        coordinator.invalidate().unwrap();
        coordinator
            .queues
            .read(&CoordinationKey::repository(&repo).unwrap(), || Ok(()))
            .unwrap();
    }

    /// 查询历史有界，但最后一次响应和当前任务始终可恢复。
    #[test]
    fn retains_only_recent_sixteen_completed_operations() {
        let fixture = Fixture::new();
        let (repo, _) = open_repository(&fixture.git, &fixture.root).unwrap();
        let coordinator = RepositoryCoordinator::new();
        let mut ids = Vec::new();
        for _ in 0..18 {
            let plan = coordinator
                .prepare(
                    coordinator.begin_prepare().unwrap(),
                    Some(repo.id.clone()),
                    CoordinationKey::repository(&repo).unwrap(),
                    OperationKind::Stage,
                    success,
                )
                .unwrap();
            let handle = coordinator.execute(Some(&repo.id), &plan.plan_id).unwrap();
            wait_for_result(&coordinator, &handle.operation_id);
            ids.push(handle.operation_id);
        }
        assert!(coordinator.read_operation(Some(&ids[0])).unwrap().is_none());
        assert!(coordinator.read_operation(Some(&ids[1])).unwrap().is_none());
        assert!(coordinator.read_operation(Some(&ids[2])).unwrap().is_some());
        assert_eq!(
            coordinator
                .read_operation(None)
                .unwrap()
                .unwrap()
                .progress
                .handle
                .operation_id,
            ids[17]
        );
    }

    /// 仅为调度测试生成无需 Git 写入的终态。
    fn success(reporter: &OperationReporter) -> OperationResult {
        OperationResult::Succeeded {
            operation_id: reporter.handle.operation_id.clone(),
            kind: reporter.kind,
            commit_oid: None,
            branch_name: None,
            clone_path: None,
            refresh: WriteRefresh::NotApplicable,
        }
    }

    /// 在测试期限内查询后台任务，避免无期限等待掩盖死锁。
    fn wait_for_result(coordinator: &RepositoryCoordinator, operation: &str) -> OperationRecord {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let record = coordinator
                .read_operation(Some(operation))
                .unwrap()
                .unwrap();
            if record.result.is_some() {
                return record;
            }
            assert!(Instant::now() < deadline, "后台任务未在期限内结束");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    /// 有界读取超时取消排队，不等待前方长任务也不执行回调。
    #[test]
    fn read_queue_deadline_cancels_reservation() {
        let fixture = Fixture::new();
        let (repo, _) = open_repository(&fixture.git, &fixture.root).unwrap();
        let queues = RepositoryQueues::default();
        let key = CoordinationKey::repository(&repo).unwrap();
        let running = queues.reserve(&key).unwrap().enter();
        let started = Instant::now();
        let result = queues.read_until(
            &key,
            Instant::now() + Duration::from_millis(30),
            || -> Result<(), OperationError> {
                panic!("过期准备不应执行");
            },
        );
        assert_eq!(result.unwrap_err().code, "TIMEOUT");
        assert!(started.elapsed() < Duration::from_secs(1));
        drop(running);
        queues.read(&key, || Ok(())).unwrap();
    }

    /// linked worktree 必须共用 FIFO，其他仓库仍可独立取得执行权。
    #[test]
    fn linked_worktrees_share_queue_and_other_repositories_do_not() {
        let fixture = Fixture::new();
        fixture.write("a", b"base");
        fixture.command(&["add", "."]);
        fixture.command(&["commit", "-m", "base"]);
        fixture.command(&["worktree", "add", "--detach", "linked"]);
        let (main, _) = open_repository(&fixture.git, &fixture.root).unwrap();
        let (linked, _) = open_repository(&fixture.git, &fixture.root.join("linked")).unwrap();
        let other = Fixture::new();
        let (other_repo, _) = open_repository(&other.git, &other.root).unwrap();
        let queues = RepositoryQueues::default();
        let first = queues
            .reserve(&CoordinationKey::repository(&main).unwrap())
            .unwrap()
            .enter();
        let second = queues
            .reserve(&CoordinationKey::repository(&linked).unwrap())
            .unwrap();
        let independent = queues
            .reserve(&CoordinationKey::repository(&other_repo).unwrap())
            .unwrap();
        let (send, receive) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let _guard = second.enter();
            send.send("linked").unwrap();
        });
        let _other = independent.enter();
        assert!(receive.recv_timeout(Duration::from_millis(50)).is_err());
        drop(first);
        assert_eq!(
            receive.recv_timeout(Duration::from_secs(2)).unwrap(),
            "linked"
        );
        worker.join().unwrap();
    }

    /// 一个运行任务加十六个等待者；超额请求拒绝，取消预留不会卡住队列。
    #[test]
    fn queue_limit_and_cancelled_reservation() {
        let fixture = Fixture::new();
        let (repo, _) = open_repository(&fixture.git, &fixture.root).unwrap();
        let queues = RepositoryQueues::default();
        let key = CoordinationKey::repository(&repo).unwrap();
        let running = queues.reserve(&key).unwrap().enter();
        let mut waiting: Vec<_> = (0..16).map(|_| queues.reserve(&key).unwrap()).collect();
        assert_eq!(queues.reserve(&key).err().unwrap().code, "QUEUE_FULL");
        drop(waiting.remove(0));
        let replacement = queues.reserve(&key).unwrap();
        drop(running);
        for reservation in waiting {
            drop(reservation.enter());
        }
        drop(replacement.enter());
    }
}
