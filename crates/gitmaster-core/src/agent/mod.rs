//! 独立 Agent 核心；平台凭据和窗口事件由桌面适配层提供。
mod models;
pub mod policy;
pub mod privacy;
mod profiles;
pub mod provider;
pub mod runtime;
mod store;
pub mod summary;
pub mod tools;
pub use models::*;
pub use profiles::*;
pub use store::directory_bytes;
pub use store::AgentStore;
