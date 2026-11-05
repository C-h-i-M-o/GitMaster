use sha2::{Digest, Sha256};

/// 从已保存的仓库根生成稳定历史身份；目录失联时也能阅读旧聊天。
/// 实际执行仍须由桌面工作区注册表重新验证 canonical 路径和运行代次。
pub fn workspace_id(repository_path: Option<&str>) -> String {
    match repository_path {
        None => "default".into(),
        Some(root) => format!("folder-{:x}", Sha256::digest(root.as_bytes())),
    }
}
