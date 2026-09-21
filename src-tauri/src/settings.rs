use gitmaster_core::git::OperationError;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);

/// 应用级设置，不保存仓库内容或凭据。
#[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Settings {
    pub version: u8,
    pub git_path: Option<String>,
}

/// 读取设置；缺失使用默认值，损坏时保留原文件并返回可恢复错误。
pub fn load(path: &Path) -> Result<Settings, OperationError> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Settings {
                version: 1,
                git_path: None,
            })
        }
        Err(_) => return Err(OperationError::new("SETTINGS_IO")),
    };
    let settings: Settings =
        serde_json::from_slice(&bytes).map_err(|_| OperationError::new("SETTINGS_IO"))?;
    if settings.version != 1 {
        return Err(OperationError::new("SETTINGS_IO"));
    }
    Ok(settings)
}

/// 通过同目录临时文件原子替换，失败不改变旧设置。
pub fn save(path: &Path, git_path: Option<String>) -> Result<(), OperationError> {
    let parent = path
        .parent()
        .ok_or_else(|| OperationError::new("SETTINGS_IO"))?;
    fs::create_dir_all(parent).map_err(|_| OperationError::new("SETTINGS_IO"))?;
    let temporary = parent.join(format!(
        ".settings-{}-{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        let bytes = serde_json::to_vec_pretty(&Settings {
            version: 1,
            git_path,
        })?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
        return Err(OperationError::new("SETTINGS_IO"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    /// 创建当前测试独有的配置路径。
    fn path() -> std::path::PathBuf {
        std::env::temp_dir()
            .join(format!(
                "gitmaster-settings-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ))
            .join("settings.json")
    }
    /// 验证首次读取、保存、替换和重启恢复。
    #[test]
    fn settings_roundtrip() {
        let p = path();
        assert_eq!(load(&p).unwrap().git_path, None);
        save(&p, Some("/中文 空格/git".into())).unwrap();
        assert_eq!(load(&p).unwrap().git_path, Some("/中文 空格/git".into()));
        save(&p, None).unwrap();
        assert_eq!(load(&p).unwrap().git_path, None);
        fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }
    /// 损坏配置不能在检测过程中静默覆盖。
    #[test]
    fn corrupt_is_preserved() {
        let p = path();
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, b"broken").unwrap();
        assert_eq!(load(&p).unwrap_err().code, "SETTINGS_IO");
        assert_eq!(fs::read(&p).unwrap(), b"broken");
        fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }
    /// 不支持的配置版本和不可写路径返回稳定错误。
    #[test]
    fn unsupported_and_invalid_destination() {
        let p = path();
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, b"{\"version\":2,\"gitPath\":null}").unwrap();
        assert!(load(&p).is_err());
        assert!(save(&p.join("child"), None).is_err());
        fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }
}
