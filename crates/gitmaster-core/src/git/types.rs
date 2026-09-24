use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 已验证的系统 Git，不接受前端构造。
#[derive(Debug, Clone)]
pub struct GitExecutable {
    pub path: PathBuf,
    pub version: String,
    pub source: String,
}

/// 环境检测结果。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum GitEnvironment {
    #[serde(rename_all = "camelCase")]
    Ready {
        executable_path: String,
        version: String,
        source: String,
    },
    Unavailable {
        error: super::error::OperationError,
    },
}

/// 已识别的工作树定位信息。
#[derive(Debug, Clone)]
pub struct RepositoryHandle {
    pub id: String,
    pub root: PathBuf,
    pub git_dir: PathBuf,
    pub common_dir: PathBuf,
}

/// HEAD 状态，不假设仓库已有提交。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum HeadState {
    Branch { name: String, oid: String },
    Unborn { name: String },
    Detached { oid: String },
}

/// 单文件在当前快照中的状态。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileChange {
    pub change_id: String,
    pub path: String,
    pub original_path: Option<String>,
    pub index_status: String,
    pub worktree_status: String,
    pub kind: String,
    pub binary: String,
}

/// 完整仓库快照；超限时返回错误而非部分列表。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryState {
    /// 仅后端保存索引内容身份，前端通过 snapshot_id 绑定此快照。
    #[serde(skip)]
    pub(crate) index_identity: [u8; 32],
    pub repository_id: String,
    pub snapshot_id: String,
    pub root_path: String,
    pub head: HeadState,
    pub operations: Vec<String>,
    pub changes: Vec<FileChange>,
}

/// 用户选择的差异比较侧。
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DiffSide {
    Staged,
    Unstaged,
    Untracked,
}

/// 按需差异读取结果，不渲染可执行内容。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum FileDiff {
    Text { content: String, truncated: bool },
    Binary,
    Unsupported { reason: String },
}

/// 本地写操作请求，按 kind 区分具体参数。
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum LocalWriteRequest {
    #[serde(rename_all = "camelCase")]
    Stage { change_ids: Vec<String> },
    #[serde(rename_all = "camelCase")]
    Unstage { change_ids: Vec<String> },
    #[serde(rename_all = "camelCase")]
    Commit { message: String },
    #[serde(rename_all = "camelCase")]
    CreateBranch { name: String },
    #[serde(rename_all = "camelCase")]
    SwitchBranch { branch_id: String },
}

/// 远端写操作请求，按 kind 区分具体参数。
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RemoteWriteRequest {
    #[serde(rename_all = "camelCase")]
    PublishBranch {
        remote_id: String,
        target_branch_name: String,
    },
    #[serde(rename_all = "camelCase")]
    SetUpstream {
        remote_id: String,
        target_branch_name: String,
    },
    SyncPush,
    SyncFastForward,
    #[serde(rename_all = "camelCase")]
    FetchAll {
        remote_id: String,
    },
    #[serde(rename_all = "camelCase")]
    Fetch {
        remote_id: String,
        remote_branch_id: String,
    },
    #[serde(rename_all = "camelCase")]
    Push {
        remote_id: String,
        target_branch_name: String,
    },
    #[serde(rename_all = "camelCase")]
    Integrate {
        remote_branch_id: String,
        mode: IntegrateMode,
    },
}

/// 远端整合模式。
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum IntegrateMode {
    FastForward,
    Merge,
}

/// 冲突写操作请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ConflictWriteRequest {
    #[serde(rename_all = "camelCase")]
    SaveConflict {
        conflict_id: String,
        fingerprint: String,
        content: String,
    },
    #[serde(rename_all = "camelCase")]
    FinishMerge { message: String },
}

/// 写入能力上下文。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteContext {
    pub repository_id: String,
    pub snapshot_id: String,
    pub capabilities: WriteCapabilities,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteCapabilities {
    pub stage: WriteCapability,
    pub unstage: WriteCapability,
    pub commit: WriteCapability,
    pub create_branch: WriteCapability,
    pub switch_branch: WriteCapability,
    pub fetch: WriteCapability,
    pub push: WriteCapability,
    pub integrate: WriteCapability,
    pub save_conflict: WriteCapability,
    pub finish_merge: WriteCapability,
}

/// 能力检查结果；显示能力不能代替执行时复核。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum WriteCapability {
    Allowed,
    Error { error: super::error::OperationError },
}

/// 写入预览及一次性计划。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WritePreview {
    pub plan_id: String,
    pub repository_id: String,
    pub snapshot_id: String,
    pub kind: OperationKind,
    pub head: HeadState,
    pub parent_oids: Vec<String>,
    pub paths: Vec<String>,
    pub author: Option<String>,
    pub message: Option<String>,
    pub target: Option<WriteTarget>,
    pub warnings: Vec<String>,
    pub expires_at: u64,
}

/// 写入目标的可辨别联合。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum WriteTarget {
    #[serde(rename_all = "camelCase")]
    LocalBranch { name: String, oid: Option<String> },
    #[serde(rename_all = "camelCase")]
    Remote {
        remote_id: String,
        display_url: String,
        ref_name: String,
        oid: Option<String>,
        source_oid: Option<String>,
        pending_commits: Vec<CommitSummary>,
    },
    #[serde(rename_all = "camelCase")]
    Merge {
        remote_branch_id: String,
        oid: String,
    },
}

/// clone 请求与预览。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneRequest {
    pub url: String,
    pub parent_directory_id: String,
    pub directory_name: String,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClonePreview {
    pub plan_id: String,
    pub display_url: String,
    pub target_path: String,
    pub expires_at: u64,
}

/// 后台操作句柄。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationHandle {
    pub operation_id: String,
    pub repository_id: Option<String>,
}

/// 后台操作类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OperationKind {
    SetUpstream,
    SaveFile,
    Stage,
    Unstage,
    Commit,
    CreateBranch,
    SwitchBranch,
    Clone,
    Fetch,
    Push,
    Integrate,
    SaveConflict,
    FinishMerge,
}

/// 操作阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OperationPhase {
    Queued,
    Checking,
    Transferring,
    Writing,
    Verifying,
    Completed,
    Failed,
    NeedsResolution,
    Unknown,
}

/// 可选的阶段计数。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationCounts {
    pub completed: u64,
    pub total: Option<u64>,
}

/// 可轮询的操作进度。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationProgress {
    pub handle: OperationHandle,
    pub sequence: u64,
    pub kind: OperationKind,
    pub phase: OperationPhase,
    pub counts: Option<OperationCounts>,
    pub started_at: u64,
}

/// 写入后刷新状态。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum WriteRefresh {
    Ready { state: RepositoryState },
    Failed { error: super::error::OperationError },
    NotApplicable,
}

/// 冲突摘要。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictSummary {
    pub merge_session_id: String,
    pub unresolved_count: u32,
    pub paths: Vec<String>,
}

/// 克隆失败后可继续处理的工作区上下文。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneRecovery {
    pub path: String,
    pub stage: CloneStage,
}

/// 克隆流程中产生可恢复状态的阶段。
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CloneStage {
    Download,
    Checkout,
    Publish,
}

/// 操作结果联合。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "outcome", rename_all = "camelCase")]
pub enum OperationResult {
    #[serde(rename_all = "camelCase")]
    Succeeded {
        operation_id: String,
        kind: OperationKind,
        commit_oid: Option<String>,
        branch_name: Option<String>,
        clone_path: Option<String>,
        refresh: WriteRefresh,
    },
    #[serde(rename_all = "camelCase")]
    Failed {
        operation_id: String,
        kind: OperationKind,
        error: super::error::OperationError,
        clone_recovery: Option<CloneRecovery>,
        refresh: WriteRefresh,
    },
    #[serde(rename_all = "camelCase")]
    Unknown {
        operation_id: String,
        kind: OperationKind,
        error: super::error::OperationError,
        clone_recovery: Option<CloneRecovery>,
        refresh: WriteRefresh,
    },
    #[serde(rename_all = "camelCase")]
    NeedsResolution {
        operation_id: String,
        kind: OperationKind,
        conflict: ConflictSummary,
        refresh: WriteRefresh,
    },
}

/// 操作记录，将进度与最终结果绑定。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationRecord {
    pub progress: OperationProgress,
    pub result: Option<OperationResult>,
}

/// 历史与引用 DTO。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitSummary {
    pub oid: String,
    pub parent_oids: Vec<String>,
    pub subject: String,
    pub author_name: String,
    pub authored_at: String,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitDetail {
    pub oid: String,
    pub parent_oids: Vec<String>,
    pub subject: String,
    pub author_name: String,
    pub authored_at: String,
    pub body: String,
    pub author_email: String,
    pub committer_name: String,
    pub committer_email: String,
    pub committed_at: String,
    pub truncated: bool,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryRefTip {
    pub ref_id: String,
    pub name: String,
    pub kind: ReferenceKind,
    pub oid: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ReferenceKind {
    Local,
    Remote,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryPage {
    pub repository_id: String,
    pub graph_snapshot_id: String,
    pub tips: Vec<HistoryRefTip>,
    pub commits: Vec<CommitSummary>,
    pub next_cursor: Option<String>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchList {
    pub repository_id: String,
    pub branches: Vec<BranchSummary>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchSummary {
    pub branch_id: String,
    pub name: String,
    pub oid: String,
    pub kind: ReferenceKind,
    pub current: bool,
    pub occupied_by_other_worktree: bool,
    pub upstream_ref_id: Option<String>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitFileList {
    pub repository_id: String,
    pub graph_snapshot_id: String,
    pub oid: String,
    pub parent_oid: Option<String>,
    pub files: Vec<CommitFile>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitFile {
    pub file_id: String,
    pub path: String,
    pub original_path: Option<String>,
    pub status: String,
    pub additions: Option<u64>,
    pub deletions: Option<u64>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectFileList {
    pub repository_id: String,
    pub snapshot_id: String,
    pub files: Vec<ProjectFile>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectFile {
    pub file_id: String,
    pub path: String,
    pub kind: ProjectFileKind,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProjectFileKind {
    Tracked,
    Untracked,
    Ignored,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteState {
    pub repository_id: String,
    pub remotes: Vec<RemoteSummary>,
    pub remote_branches: Vec<RemoteBranch>,
    pub branch_upstreams: Vec<BranchUpstream>,
    pub last_fetched_at: Option<String>,
}
/// 本地分支的上游映射由 Git 解析，不从展示名称拆分远端和目标分支。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchUpstream {
    pub branch_name: String,
    pub remote_name: String,
    pub remote_id: Option<String>,
    pub target_ref: String,
    pub tracking_ref: String,
}
/// 当前分支同步入口的只读目标；不代表已获取最新远端或已获写入计划。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncTarget {
    pub repository_id: String,
    pub snapshot_id: String,
    pub branch_name: String,
    pub upstream: SyncUpstream,
}
/// 区分从未配置与配置无法唯一解析，防止首次同步误覆盖已有配置。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum SyncUpstream {
    Missing,
    #[serde(rename_all = "camelCase")]
    Configured {
        remote_id: String,
        target_branch_name: String,
        remote_branch_id: Option<String>,
    },
    Unresolved {
        reason: SyncUpstreamReason,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SyncUpstreamReason {
    Incomplete,
    Ambiguous,
    LocalRepository,
    UnsupportedTarget,
    RemoteMissing,
    MappingMissing,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSummary {
    pub remote_id: String,
    pub name: String,
    pub fetched_branch_names: Option<Vec<String>>,
    pub fetch_display_url: String,
    pub push_display_url: String,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteBranch {
    pub remote_branch_id: String,
    pub name: String,
    pub oid: String,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteAssessment {
    pub repository_id: String,
    pub snapshot_id: String,
    pub remote_branch_id: String,
    pub local_oid: String,
    pub remote_oid: String,
    pub ahead: u64,
    pub behind: u64,
    pub relation: RemoteRelation,
    pub observed_at: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RemoteRelation {
    Equal,
    Ahead,
    Behind,
    Diverged,
    Unrelated,
    Unknown,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictState {
    pub repository_id: String,
    pub merge_session_id: String,
    pub head_oid: String,
    pub merge_head_oids: Vec<String>,
    pub files: Vec<ConflictFile>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictFile {
    pub conflict_id: String,
    pub path: String,
    pub stage_oids: ConflictStageOids,
    pub editor_support: ConflictEditorSupport,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictStageOids {
    pub base: Option<String>,
    pub local: Option<String>,
    pub incoming: Option<String>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum ConflictEditorSupport {
    Supported,
    Unsupported { reason: String },
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictDocument {
    pub merge_session_id: String,
    pub conflict_id: String,
    pub base: Option<String>,
    pub local: Option<String>,
    pub incoming: Option<String>,
    pub result: String,
    pub encoding: String,
    pub line_ending: String,
    pub fingerprint: String,
}
