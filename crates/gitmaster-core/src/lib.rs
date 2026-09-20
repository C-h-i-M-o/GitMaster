//! gitMaster 的独立核心；当前仅提供应用元数据，不执行 Git 或文件写入。

use serde::Serialize;

/// 应用采用的 Git 来源策略，不代表已检测到可用的 Git。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum GitProvider {
    System,
}

/// 面向不同客户端共享的应用信息，序列化字段保持 camelCase。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub git_provider: GitProvider,
}

/// 返回应用元数据，不访问仓库、不检查环境、不修改用户配置。
pub fn app_info() -> AppInfo {
    AppInfo {
        name: "gitMaster".to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        git_provider: GitProvider::System,
    }
}
