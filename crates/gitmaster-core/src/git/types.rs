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
