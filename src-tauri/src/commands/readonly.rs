//! M2 只读业务 command：会话身份与缓存发布留在桌面层，Git 查询经共享队列执行。

use super::{blocking, DesktopState, OperationError, Session};
use gitmaster_core::git::{self, coordinator::CoordinationKey, *};
use std::sync::{Arc, Mutex};
use tauri::State;
mod cancellation;
use std::sync::OnceLock;
static READ_REGISTRY: OnceLock<cancellation::Registry> = OnceLock::new();

/// 完成第一页读取后才发布的历史图，不存在可见的半初始化缓存。
pub(super) struct HistoryCache {
    session: git::history::HistorySession,
    graph_snapshot_id: String,
}

/// 后台读取捕获的桌面代次；不持有主会话锁或接受前端路径。
pub(super) struct Context {
    pub(super) git: GitExecutable,
    pub(super) repository: RepositoryHandle,
    pub(super) state: RepositoryState,
    pub(super) epoch: u64,
    pub(super) token: u64,
    readonly_token: u64,
    pub(super) coordinator: git::coordinator::RepositoryCoordinator,
}

impl Context {
    /// 调用方持有主锁时捕获同一时刻的仓库和环境。
    pub(super) fn capture(session: &Session, repository_id: &str) -> Result<Self, OperationError> {
        let (repository, snapshot) = session
            .active
            .as_ref()
            .filter(|(repo, snapshot)| {
                repo.id == repository_id && snapshot.repository_id == repository_id
            })
            .ok_or_else(|| OperationError::new("STALE_REQUEST"))?;
        Ok(Self {
            git: session
                .git
                .clone()
                .ok_or_else(|| OperationError::new("GIT_NOT_FOUND"))?,
            repository: repository.clone(),
            state: snapshot.clone(),
            epoch: session.epoch,
            token: session.repository_request,
            readonly_token: session.readonly_request,
            coordinator: session.coordinator.clone(),
        })
    }

    /// 后台发布和缓存安装均在持有主锁时核对环境及仓库代次。
    pub(super) fn check(&self, session: &Session) -> Result<(), OperationError> {
        if session.epoch != self.epoch || session.repository_request != self.token {
            Err(OperationError::new("STALE_REQUEST"))
        } else {
            Ok(())
        }
    }

    /// 文件与冻结历史只因强制失效或切换仓库中断，普通状态刷新不使读取过期。
    pub(super) fn check_read(&self, session: &Session) -> Result<(), OperationError> {
        if session.epoch != self.epoch
            || session.readonly_request != self.readonly_token
            || !session
                .active
                .as_ref()
                .is_some_and(|(repo, _)| repo.id == self.repository.id)
        {
            Err(OperationError::new("STALE_REQUEST"))
        } else {
            Ok(())
        }
    }

    /// 文件接口另外核对当前快照，不让旧列表混入刷新后的状态。
    pub(super) fn check_snapshot(&self, snapshot_id: &str) -> Result<(), OperationError> {
        if self.state.snapshot_id == snapshot_id {
            Ok(())
        } else {
            Err(OperationError::new("STALE_REQUEST"))
        }
    }
}

/// 一次历史加载请求；同步登记代次，后台完成顺序不能改变请求先后。
struct HistoryRequest {
    ctx: Context,
    generation: u64,
    cursor: Option<String>,
    existing: Option<Arc<Mutex<HistoryCache>>>,
}

impl HistoryRequest {
    /// 新图立即使旧图失效，分页原子捕获当前图及桌面上下文。
    fn begin(
        shared: &DesktopState,
        repository_id: &str,
        cursor: Option<String>,
    ) -> Result<Self, OperationError> {
        let mut desktop = shared.lock()?;
        let ctx = Context::capture(&desktop, repository_id)?;
        if cursor.is_none() {
            desktop.history_request += 1;
            desktop.history = None;
        } else if desktop.history.is_none() {
            return Err(OperationError::new("STALE_GRAPH"));
        }
        Ok(Self {
            ctx,
            generation: desktop.history_request,
            cursor,
            existing: desktop.history.clone(),
        })
    }

    /// 初始化和第一页在同一个队列项执行，发布前再次核对请求代次。
    fn run(self, shared: &DesktopState) -> Result<HistoryPage, OperationError> {
        let key = CoordinationKey::repository(&self.ctx.repository)?;
        let (cache, page) = self.ctx.coordinator.read(&key, || {
            let desktop = shared.lock()?;
            self.ctx.check_read(&desktop)?;
            if desktop.history_request != self.generation {
                return Err(OperationError::new("STALE_GRAPH"));
            }
            drop(desktop);
            match &self.existing {
                Some(cache) => {
                    let page = cache
                        .lock()
                        .map_err(|_| OperationError::new("GIT_EXECUTION_FAILED"))?
                        .session
                        .read_page(self.cursor.as_deref())?;
                    Ok((cache.clone(), page))
                }
                None => {
                    let mut session = git::history::HistorySession::new(
                        self.ctx.git.clone(),
                        self.ctx.repository.clone(),
                    )?;
                    let page = session.read_page(None)?;
                    let cache = Arc::new(Mutex::new(HistoryCache {
                        session,
                        graph_snapshot_id: page.graph_snapshot_id.clone(),
                    }));
                    Ok((cache, page))
                }
            }
        })?;
        let mut desktop = shared.lock()?;
        self.ctx.check_read(&desktop)?;
        if desktop.history_request != self.generation {
            return Err(OperationError::new("STALE_GRAPH"));
        }
        if let Some(existing) = &self.existing {
            check_history_slot(&desktop, existing)?;
        } else {
            desktop.history = Some(cache);
        }
        Ok(page)
    }
}

/// 在主锁内判断后台持有的缓存是否仍是当前图，避免返回刷新前的结果。
fn check_history_slot(
    desktop: &Session,
    cache: &Arc<Mutex<HistoryCache>>,
) -> Result<(), OperationError> {
    if desktop
        .history
        .as_ref()
        .is_some_and(|current| Arc::ptr_eq(current, cache))
    {
        Ok(())
    } else {
        Err(OperationError::new("STALE_GRAPH"))
    }
}

/// 原子捕获历史缓存，所有详情与文件请求共享同一失效规则。
struct HistoryAccess {
    ctx: Context,
    cache: Arc<Mutex<HistoryCache>>,
}
impl HistoryAccess {
    /// 不在主锁内获取模块锁，后续 Git 调用只使用捕获的缓存。
    fn capture(shared: &DesktopState, repository_id: &str) -> Result<Self, OperationError> {
        let desktop = shared.lock()?;
        Ok(Self {
            ctx: Context::capture(&desktop, repository_id)?,
            cache: desktop
                .history
                .clone()
                .ok_or_else(|| OperationError::new("STALE_GRAPH"))?,
        })
    }

    /// 模块操作前后分别检查桌面身份；模块锁始终先释放再核对主会话。
    fn run<T>(
        &self,
        shared: &DesktopState,
        graph_snapshot_id: &str,
        read: impl FnOnce(&mut git::history::HistorySession) -> Result<T, OperationError>,
    ) -> Result<T, OperationError> {
        let key = CoordinationKey::repository(&self.ctx.repository)?;
        let result = self.ctx.coordinator.read(&key, || {
            let desktop = shared.lock()?;
            self.ctx.check_read(&desktop)?;
            check_history_slot(&desktop, &self.cache)?;
            drop(desktop);
            let mut history = self
                .cache
                .lock()
                .map_err(|_| OperationError::new("GIT_EXECUTION_FAILED"))?;
            if history.graph_snapshot_id != graph_snapshot_id {
                return Err(OperationError::new("STALE_GRAPH"));
            }
            read(&mut history.session)
        })?;
        let desktop = shared.lock()?;
        self.ctx.check_read(&desktop)?;
        check_history_slot(&desktop, &self.cache)?;
        Ok(result)
    }
}

/// 读取固定图快照的一页；cursor 为 null 时开始新历史会话。
#[tauri::command]
pub async fn read_commit_history(
    state: State<'_, DesktopState>,
    repository_id: String,
    cursor: Option<String>,
) -> Result<HistoryPage, OperationError> {
    let shared = state.inner().clone();
    let request = HistoryRequest::begin(&shared, &repository_id, cursor)?;
    blocking("read_commit_history", move || request.run(&shared)).await
}

/// 读取已分页提交的详情，并核对所属图快照。
#[tauri::command]
pub async fn read_commit_detail(
    state: State<'_, DesktopState>,
    repository_id: String,
    graph_snapshot_id: String,
    oid: String,
) -> Result<CommitDetail, OperationError> {
    let shared = state.inner().clone();
    let request = HistoryAccess::capture(&shared, &repository_id)?;
    blocking("read_commit_detail", move || {
        request.run(&shared, &graph_snapshot_id, |history| {
            history.commit_detail(&oid)
        })
    })
    .await
}

/// 返回提交文件列表；核心在每次查询开始时作废旧文件映射。
#[tauri::command]
pub async fn read_commit_files(
    state: State<'_, DesktopState>,
    repository_id: String,
    graph_snapshot_id: String,
    oid: String,
    parent_oid: Option<String>,
) -> Result<CommitFileList, OperationError> {
    let shared = state.inner().clone();
    let request = HistoryAccess::capture(&shared, &repository_id)?;
    blocking("read_commit_files", move || {
        request.run(&shared, &graph_snapshot_id, |history| {
            history.commit_files(&oid, parent_oid.as_deref())
        })
    })
    .await
}

/// 读取最近提交文件列表签发的差异，不推测 fileId 的内部格式。
#[tauri::command]
pub async fn read_commit_file_diff(
    state: State<'_, DesktopState>,
    repository_id: String,
    graph_snapshot_id: String,
    file_id: String,
) -> Result<FileDiff, OperationError> {
    let shared = state.inner().clone();
    let request = HistoryAccess::capture(&shared, &repository_id)?;
    blocking("read_commit_file_diff", move || {
        request.run(&shared, &graph_snapshot_id, |history| {
            history.commit_file_diff(&file_id)
        })
    })
    .await
}

/// 新分支列表请求的同步登记与后台执行。
struct BranchRequest {
    ctx: Context,
    generation: u64,
}
impl BranchRequest {
    /// 先校验仓库身份，再作废前一次列表，避免迟到初始化覆盖。
    fn begin(shared: &DesktopState, repository_id: &str) -> Result<Self, OperationError> {
        let mut desktop = shared.lock()?;
        let ctx = Context::capture(&desktop, repository_id)?;
        desktop.branches_request += 1;
        desktop.branches = None;
        Ok(Self {
            ctx,
            generation: desktop.branches_request,
        })
    }
    /// 构造真实列表后，在同一主锁区间检查并安装。
    fn run(self, shared: &DesktopState) -> Result<BranchList, OperationError> {
        let key = CoordinationKey::repository(&self.ctx.repository)?;
        let cache = self.ctx.coordinator.read(&key, || {
            git::branches::BranchSession::new(&self.ctx.git, &self.ctx.repository)
        })?;
        let list = cache.list();
        let mut desktop = shared.lock()?;
        self.ctx.check_read(&desktop)?;
        if desktop.branches_request != self.generation {
            return Err(OperationError::new("STALE_REQUEST"));
        }
        desktop.branches = Some(Arc::new(cache));
        Ok(list)
    }
}

/// 读取本地和远端分支引用快照，保存后端 ID 映射。
#[tauri::command]
pub async fn read_branches(
    state: State<'_, DesktopState>,
    repository_id: String,
) -> Result<BranchList, OperationError> {
    let shared = state.inner().clone();
    let request = BranchRequest::begin(&shared, &repository_id)?;
    blocking("read_branches", move || request.run(&shared)).await
}

/// 项目文件列表请求绑定状态快照与槽代次。
struct ProjectFilesRequest {
    ctx: Context,
    generation: u64,
}
impl ProjectFilesRequest {
    /// 新列表使旧 fileId 映射立即失效，无效请求不影响有效槽。
    fn begin(
        shared: &DesktopState,
        repository_id: &str,
        snapshot_id: &str,
    ) -> Result<Self, OperationError> {
        let mut desktop = shared.lock()?;
        let ctx = Context::capture(&desktop, repository_id)?;
        ctx.check_snapshot(snapshot_id)?;
        desktop.project_files_request += 1;
        desktop.project_files = None;
        Ok(Self {
            ctx,
            generation: desktop.project_files_request,
        })
    }
    /// 创建列表时不持有主锁，迟到结果拒绝安装。
    fn run(
        self,
        shared: &DesktopState,
        include_ignored: bool,
    ) -> Result<ProjectFileList, OperationError> {
        self.run_with(shared, include_ignored, |cache| cache.list())
    }
    /// 新目录入口与旧平面入口共享同一个会话发布门禁。
    fn run_with<T>(
        self,
        shared: &DesktopState,
        include_ignored: bool,
        project: impl FnOnce(&git::files::ProjectFilesSession) -> T,
    ) -> Result<T, OperationError> {
        let key = CoordinationKey::repository(&self.ctx.repository)?;
        let cache = self.ctx.coordinator.read(&key, || {
            git::files::ProjectFilesSession::new_with_ignored(
                self.ctx.git.clone(),
                self.ctx.repository.clone(),
                self.ctx.state.clone(),
                include_ignored,
            )
        })?;
        let list = project(&cache);
        let mut desktop = shared.lock()?;
        self.ctx.check_read(&desktop)?;
        if desktop.project_files_request != self.generation {
            return Err(OperationError::new("STALE_REQUEST"));
        }
        desktop.project_files = Some(Arc::new(cache));
        Ok(list)
    }
    /// 文件树使用工作树发现，不提前读取 Git 索引全量清单。
    fn run_filesystem_with<T>(
        self,
        shared: &DesktopState,
        project: impl FnOnce(&git::files::ProjectFilesSession) -> T,
    ) -> Result<T, OperationError> {
        let key = CoordinationKey::repository(&self.ctx.repository)?;
        let cancelled = || {
            shared.lock().map_or(true, |desktop| {
                self.ctx.check_read(&desktop).is_err()
                    || desktop.project_files_request != self.generation
            })
        };
        let (cache, result) = self.ctx.coordinator.read_files(&key, cancelled, || {
            let cache = git::files::ProjectFilesSession::new_filesystem(
                self.ctx.git.clone(),
                self.ctx.repository.clone(),
                self.ctx.state.clone(),
            )?;
            let result = project(&cache);
            Ok((cache, result))
        })?;
        let mut desktop = shared.lock()?;
        self.ctx.check_read(&desktop)?;
        if desktop.project_files_request != self.generation {
            return Err(OperationError::new("STALE_REQUEST"));
        }
        desktop.project_files = Some(Arc::new(cache));
        Ok(result)
    }
}

/// 捕获项目文件读取能力，刷新后旧 Arc 不再允许发布结果。
struct ProjectFileAccess {
    ctx: Context,
    cache: Arc<git::files::ProjectFilesSession>,
}
impl ProjectFileAccess {
    /// 在同一个锁区间校验状态并捕获文件映射。
    fn capture(
        shared: &DesktopState,
        repository_id: &str,
        snapshot_id: &str,
    ) -> Result<Self, OperationError> {
        let desktop = shared.lock()?;
        let ctx = Context::capture(&desktop, repository_id)?;
        let cache = desktop
            .project_files
            .clone()
            .ok_or_else(|| OperationError::new("FILE_UNAVAILABLE"))?;
        if cache.snapshot_id() != snapshot_id {
            return Err(OperationError::new("STALE_REQUEST"));
        }
        Ok(Self { ctx, cache })
    }
    /// 读取只使用核心目录能力；完成后核对缓存仍然有效。
    fn run(self, shared: &DesktopState, file_id: &str) -> Result<FileDiff, OperationError> {
        self.run_with(shared, |cache| cache.read(file_id))
    }
    /// 预览和完整编辑文档共用队列与结果发布校验。
    fn run_with<T>(
        &self,
        shared: &DesktopState,
        read: impl FnOnce(&git::files::ProjectFilesSession) -> Result<T, OperationError>,
    ) -> Result<T, OperationError> {
        self.run_cancelled(shared, || false, read)
    }
    /// 取消和能力替换在排队中生效；普通文件读取遵守写屏障而不等待 Git 查询。
    fn run_cancelled<T>(
        &self,
        shared: &DesktopState,
        cancelled: impl Fn() -> bool,
        read: impl FnOnce(&git::files::ProjectFilesSession) -> Result<T, OperationError>,
    ) -> Result<T, OperationError> {
        let key = CoordinationKey::repository(&self.ctx.repository)?;
        // 已失效的文件能力保持原错误契约；排队期间失效则由取消检查及时退出。
        {
            let desktop = shared.lock()?;
            self.ctx.check_read(&desktop)?;
            if !desktop
                .project_files
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, &self.cache))
            {
                return Err(OperationError::new("FILE_UNAVAILABLE"));
            }
        }
        let invalid = || {
            cancelled()
                || shared.lock().map_or(true, |desktop| {
                    self.ctx.check_read(&desktop).is_err()
                        || !desktop
                            .project_files
                            .as_ref()
                            .is_some_and(|current| Arc::ptr_eq(current, &self.cache))
                })
        };
        let result = self
            .ctx
            .coordinator
            .read_files(&key, invalid, || read(&self.cache))?;
        if cancelled() {
            return Err(OperationError::new("STALE_REQUEST"));
        }
        let desktop = shared.lock()?;
        self.ctx.check_read(&desktop)?;
        if !desktop
            .project_files
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, &self.cache))
        {
            return Err(OperationError::new("FILE_UNAVAILABLE"));
        }
        Ok(result)
    }
}

/// 读取当前项目文件列表，每次成功调用签发新的文件 ID。
#[tauri::command]
pub async fn read_project_files(
    state: State<'_, DesktopState>,
    repository_id: String,
    snapshot_id: String,
    include_ignored: Option<bool>,
) -> Result<ProjectFileList, OperationError> {
    let shared = state.inner().clone();
    let request = ProjectFilesRequest::begin(&shared, &repository_id, &snapshot_id)?;
    blocking("read_project_files", move || {
        request.run(&shared, include_ignored.unwrap_or(false))
    })
    .await
}

/// 建立项目树会话，仅传输根目录第一页。
#[tauri::command]
pub async fn read_project_tree(
    state: State<'_, DesktopState>,
    repository_id: String,
    snapshot_id: String,
    include_ignored: Option<bool>,
) -> Result<git::files::tree::ProjectTreePage, OperationError> {
    let shared = state.inner().clone();
    let request = ProjectFilesRequest::begin(&shared, &repository_id, &snapshot_id)?;
    let _ = include_ignored; // 兼容旧参数；真实目录始终包含忽略文件。
    blocking("read_project_tree", move || {
        request.run_filesystem_with(&shared, |cache| cache.tree_root())
    })
    .await
}

/// 展开目录只读取当前缓存，不能替换文件映射或访问任意路径。
#[tauri::command]
pub async fn read_project_directory(
    state: State<'_, DesktopState>,
    repository_id: String,
    snapshot_id: String,
    tree_id: String,
    directory_id: String,
    offset: usize,
) -> Result<git::files::tree::ProjectTreePage, OperationError> {
    let shared = state.inner().clone();
    let request = ProjectFileAccess::capture(&shared, &repository_id, &snapshot_id)?;
    blocking("read_project_directory", move || {
        request.run_with(&shared, |cache| {
            cache.tree_page(&tree_id, &directory_id, offset)
        })
    })
    .await
}

/// 搜索使用已签发清单及有界分页，不读取文件正文。
#[tauri::command]
pub async fn search_project_files(
    state: State<'_, DesktopState>,
    repository_id: String,
    snapshot_id: String,
    tree_id: String,
    query: String,
    offset: usize,
) -> Result<git::files::tree::ProjectTreePage, OperationError> {
    let shared = state.inner().clone();
    let request = ProjectFileAccess::capture(&shared, &repository_id, &snapshot_id)?;
    blocking("search_project_files", move || {
        request.run_with(&shared, |cache| cache.tree_search(&tree_id, &query, offset))
    })
    .await
}

/// 按本次列表 fileId 读取项目文件内容，列表刷新后旧 ID 拒绝。
#[tauri::command]
pub async fn read_project_file(
    state: State<'_, DesktopState>,
    repository_id: String,
    snapshot_id: String,
    file_id: String,
) -> Result<FileDiff, OperationError> {
    let shared = state.inner().clone();
    let request = ProjectFileAccess::capture(&shared, &repository_id, &snapshot_id)?;
    blocking("read_project_file", move || request.run(&shared, &file_id)).await
}

/// 获取完整可编辑文档，不能把截断预览升级为可保存内容。
#[tauri::command]
pub async fn read_editable_file(
    state: State<'_, DesktopState>,
    repository_id: String,
    snapshot_id: String,
    file_id: String,
    read_id: Option<String>,
) -> Result<git::files::EditableFile, OperationError> {
    let shared = state.inner().clone();
    let request = ProjectFileAccess::capture(&shared, &repository_id, &snapshot_id)?;
    let registration = read_id
        .map(|id| {
            READ_REGISTRY
                .get_or_init(Default::default)
                .register(repository_id.clone(), id)
        })
        .transpose()?;
    blocking("read_editable_file", move || {
        let document = if let Some(registration) = registration.as_ref() {
            request.run_cancelled(
                &shared,
                || registration.cancelled(),
                |cache| cache.read_editable_cancellable(&file_id, || registration.cancelled()),
            )
        } else {
            request.run_with(&shared, |cache| cache.read_editable(&file_id))
        }?;
        register_editor_document(&shared, &request, &document)?;
        Ok(document)
    })
    .await
}

/// 成功打开文档才登记最小保存能力；重读不撤销尚有草稿的旧能力，数量有界。
fn register_editor_document(
    shared: &DesktopState,
    request: &ProjectFileAccess,
    document: &git::files::EditableFile,
) -> Result<(), OperationError> {
    let editor = Arc::new(request.cache.editor_session(&document.file_id)?);
    let mut desktop = shared.lock()?;
    request.ctx.check_read(&desktop)?;
    desktop
        .editor_files
        .retain(|(id, _)| id != &document.file_id);
    desktop
        .editor_files
        .push_back((document.file_id.clone(), editor));
    while desktop.editor_files.len() > 512 {
        desktop.editor_files.pop_front();
    }
    Ok(())
}

/// 文档能力跨目录刷新有效，真实保存仍核实工作区身份、原版本并持有写屏障。
fn save_open_document(
    shared: &DesktopState,
    repository_id: &str,
    file_id: &str,
    expected_version: &str,
    content: &str,
) -> Result<git::files::EditableFile, OperationError> {
    let (ctx, cache) = {
        let mut desktop = shared.lock()?;
        let ctx = Context::capture(&desktop, repository_id)?;
        let index = desktop
            .editor_files
            .iter()
            .position(|(id, _)| id == file_id)
            .ok_or_else(|| OperationError::new("FILE_UNAVAILABLE"))?;
        let entry = desktop
            .editor_files
            .remove(index)
            .ok_or_else(|| OperationError::new("FILE_UNAVAILABLE"))?;
        let cache = entry.1.clone();
        desktop.editor_files.push_back(entry);
        (ctx, cache)
    };
    let key = CoordinationKey::repository(&ctx.repository)?;
    ctx.coordinator.write_file(&key, || {
        {
            let desktop = shared.lock()?;
            if desktop.epoch != ctx.epoch
                || !desktop
                    .active
                    .as_ref()
                    .is_some_and(|(repo, _)| repo.id == ctx.repository.id)
            {
                return Err(OperationError::new("STALE_REQUEST"));
            }
        }
        cache.save_editor_file(file_id, expected_version, content)
    })
}

/// 独立文件落盘入口：持有写屏障并复读成功后立即返回，Git 刷新由监听器处理。
#[tauri::command]
pub async fn save_editor_file(
    state: State<'_, DesktopState>,
    repository_id: String,
    file_id: String,
    expected_version: String,
    content: String,
) -> Result<git::files::EditableFile, OperationError> {
    let shared = state.inner().clone();
    blocking("save_editor_file", move || {
        save_open_document(
            &shared,
            &repository_id,
            &file_id,
            &expected_version,
            &content,
        )
    })
    .await
}

/// 取消指定仓库中的文件读取；读取尚未注册时保留一次有界 tombstone。
#[tauri::command]
pub async fn cancel_project_read(
    repository_id: String,
    read_id: String,
) -> Result<(), OperationError> {
    if repository_id.is_empty()
        || repository_id.len() > 128
        || read_id.is_empty()
        || read_id.len() > 128
    {
        return Err(OperationError::new("INVALID_INPUT"));
    }
    READ_REGISTRY
        .get_or_init(Default::default)
        .cancel(&repository_id, &read_id);
    Ok(())
}

#[cfg(test)]
mod tests;

/// 单文档字面量查找按匹配行分页，不返回未请求的正文。
#[tauri::command]
pub async fn search_read_document(
    state: State<'_, DesktopState>,
    repository_id: String,
    snapshot_id: String,
    document_id: String,
    query: String,
    start_line: usize,
    count: usize,
) -> Result<git::files::reader::ReadSearch, OperationError> {
    let shared = state.inner().clone();
    let request = ProjectFileAccess::capture(&shared, &repository_id, &snapshot_id)?;
    blocking("search_read_document", move || {
        request.run_with(&shared, |cache| {
            cache.search_read_document(&document_id, &query, start_line, count)
        })
    })
    .await
}

/// 建立流式只读行索引，正文仍保留在工作文件中。
#[tauri::command]
pub async fn open_read_document(
    state: State<'_, DesktopState>,
    repository_id: String,
    snapshot_id: String,
    file_id: String,
) -> Result<git::files::reader::ReadDocument, OperationError> {
    let shared = state.inner().clone();
    let request = ProjectFileAccess::capture(&shared, &repository_id, &snapshot_id)?;
    blocking("open_read_document", move || {
        request.run_with(&shared, |cache| cache.open_read_document(&file_id))
    })
    .await
}
/// 页读取复用原仓库队列与 Arc 发布门禁，旧快照不能回填新项目。
#[tauri::command]
pub async fn read_document_page(
    state: State<'_, DesktopState>,
    repository_id: String,
    snapshot_id: String,
    document_id: String,
    start_line: usize,
    count: usize,
    byte_offset: Option<u64>,
) -> Result<git::files::reader::ReadPage, OperationError> {
    let shared = state.inner().clone();
    let request = ProjectFileAccess::capture(&shared, &repository_id, &snapshot_id)?;
    blocking("read_document_page", move || {
        request.run_with(&shared, |cache| {
            cache.read_document_page(&document_id, start_line, count, byte_offset.unwrap_or(0))
        })
    })
    .await
}
/// 释放只读文档，不删除或修改磁盘内容。
#[tauri::command]
pub async fn close_read_document(
    state: State<'_, DesktopState>,
    repository_id: String,
    snapshot_id: String,
    document_id: String,
) -> Result<(), OperationError> {
    let shared = state.inner().clone();
    let request = ProjectFileAccess::capture(&shared, &repository_id, &snapshot_id)?;
    blocking("close_read_document", move || {
        request.run_with(&shared, |cache| cache.close_read_document(&document_id))
    })
    .await
}
