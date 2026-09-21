//! 只读 Git 业务核心，不依赖窗口或 Tauri。
pub mod environment;
pub mod error;
pub(crate) mod process;
pub mod types;

pub mod diff;
pub mod repository;
pub mod status;

pub use error::OperationError;
pub use types::*;

#[cfg(test)]
mod acceptance_tests;
