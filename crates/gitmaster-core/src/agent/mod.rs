//! 独立 Agent 核心；平台凭据和窗口事件由桌面适配层提供。
pub mod context;
mod migration;
mod models;
pub mod references;
pub use store::tool_results::ToolResultPage;
pub mod policy;
pub mod privacy;
mod profiles;
pub mod provider;
mod run_summary;
pub mod runtime;
mod store;
pub mod summary;
pub mod tools;
mod workspace;
pub use migration::{ModelConfig, ProviderConnection};
pub use models::*;
pub use profiles::*;
pub use provider::{ModelCapabilities, ModelParameters, ParameterStyle, ThinkingMode};
pub use run_summary::{FileChangeArtifact, FileChangeContent};
pub use store::chat_search::{ChatSearchResult, ChatSearchResults};
pub use store::directory_bytes;
pub use store::memory::{
    MemoryEntry, MemoryExcluded, MemoryExcludedReason, MemoryInput, MemoryScope, MemorySelection,
    MemorySource,
};
pub use store::AgentStore;
pub use workspace::workspace_id;
