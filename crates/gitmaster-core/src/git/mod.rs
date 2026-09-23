//! 独立 Git 业务核心，不依赖窗口或 Tauri；写能力由确认计划控制。
pub mod environment;
pub mod error;
pub(crate) mod process;
pub mod types;

pub mod branches;
pub mod capabilities;
pub mod conflicts;
pub mod coordinator;
pub mod diff;
pub mod files;
pub mod history;
pub mod merge;
pub mod remote;
pub mod repository;
pub mod status;
pub mod watcher;

pub use error::OperationError;
pub use types::*;

#[cfg(test)]
mod acceptance_tests;

mod checkout;
mod staging;
pub mod write;
mod write_guard;

#[cfg(any(windows, test))]
mod windows_arguments;

#[cfg(windows)]
mod windows_fs;
