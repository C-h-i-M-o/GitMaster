use gitmaster_core::git::OperationError;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex, OnceLock,
    },
};

static NEXT: AtomicU64 = AtomicU64::new(0);
static SETTINGS_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// 应用级设置，不保存仓库内容或凭据。
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Settings {
    #[serde(default)]
    pub log_level: Option<crate::logging::LogLevel>,
    pub version: u8,
    pub git_path: Option<String>,
    pub ui_preferences: UiPreferences,
}

/// 用户界面偏好；弹性值限定在 1 到 10。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UiPreferences {
    #[serde(default = "default_elasticity")]
    pub elasticity: u8,
    #[serde(default = "default_show_labels")]
    pub show_labels: bool,
}
/// 提供界面偏好的稳定默认值。
impl Default for UiPreferences {
    fn default() -> Self {
        Self {
            elasticity: 6,
            show_labels: true,
        }
    }
}
/// 返回界面弹性默认值。
fn default_elasticity() -> u8 {
    6
}
/// 返回是否显示标签的默认值。
fn default_show_labels() -> bool {
    true
}
/// 获取设置读改写使用的全局串行锁。
fn lock() -> Result<std::sync::MutexGuard<'static, ()>, OperationError> {
    SETTINGS_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| OperationError::new("SETTINGS_IO"))
}
/// 校验界面偏好的可持久化取值范围。
fn validate(p: &UiPreferences) -> Result<(), OperationError> {
    if (1..=10).contains(&p.elasticity) {
        Ok(())
    } else {
        Err(OperationError::new("SETTINGS_IO"))
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredSettings {
    #[serde(default)]
    log_level: Option<crate::logging::LogLevel>,
    version: u8,
    git_path: Option<String>,
    #[serde(default)]
    ui_preferences: Option<UiPreferences>,
}

/// 在全局设置锁已持有时读取并解析配置。
fn read_unlocked(path: &Path) -> Result<Settings, OperationError> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Settings {
                version: 2,
                log_level: None,
                git_path: None,
                ui_preferences: UiPreferences::default(),
            })
        }
        Err(_) => return Err(OperationError::new("SETTINGS_IO")),
    };
    let stored: StoredSettings =
        serde_json::from_slice(&bytes).map_err(|_| OperationError::new("SETTINGS_IO"))?;
    let preferences = match stored.version {
        1 => UiPreferences::default(),
        2 => stored
            .ui_preferences
            .ok_or_else(|| OperationError::new("SETTINGS_IO"))?,
        _ => return Err(OperationError::new("SETTINGS_IO")),
    };
    validate(&preferences)?;
    Ok(Settings {
        version: 2,
        log_level: stored.log_level,
        git_path: stored.git_path,
        ui_preferences: preferences,
    })
}

/// 读取设置；缺失使用默认值，损坏时保留原文件并返回错误。
pub fn load(path: &Path) -> Result<Settings, OperationError> {
    let _guard = lock()?;
    read_unlocked(path)
}

/// 在同目录同步临时文件后原子替换，只清理本次创建的文件。
fn write_unlocked(path: &Path, settings: &Settings) -> Result<(), OperationError> {
    validate(&settings.ui_preferences)?;
    let parent = path
        .parent()
        .ok_or_else(|| OperationError::new("SETTINGS_IO"))?;
    fs::create_dir_all(parent).map_err(|_| OperationError::new("SETTINGS_IO"))?;
    let temporary = parent.join(format!(
        ".settings-{}-{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut created = false;
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        created = true;
        file.write_all(&serde_json::to_vec_pretty(settings)?)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        if created {
            let _ = fs::remove_file(&temporary);
        }
        return Err(OperationError::new("SETTINGS_IO"));
    }
    Ok(())
}

/// 保存 Git 路径并保留现有界面偏好。
pub fn save(path: &Path, git_path: Option<String>) -> Result<(), OperationError> {
    let _guard = lock()?;
    let current = read_unlocked(path)?;
    write_unlocked(
        path,
        &Settings {
            version: 2,
            log_level: current.log_level,
            git_path,
            ui_preferences: current.ui_preferences,
        },
    )
}
/// 保存界面偏好并保留现有 Git 路径。
pub fn save_preferences(
    path: &Path,
    preferences: UiPreferences,
) -> Result<UiPreferences, OperationError> {
    let _guard = lock()?;
    validate(&preferences)?;
    let current = read_unlocked(path)?;
    write_unlocked(
        path,
        &Settings {
            version: 2,
            log_level: current.log_level,
            git_path: current.git_path,
            ui_preferences: preferences.clone(),
        },
    )?;
    Ok(preferences)
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
    /// 删除当前测试生成的临时配置目录。
    fn cleanup(p: &Path) {
        let _ = fs::remove_dir_all(p.parent().unwrap());
    }
    #[test]
    /// 验证 v1 配置升级后使用 v2 默认偏好。
    fn v1_migrates_and_defaults() {
        let p = path();
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, b"{\"version\":1,\"gitPath\":\"/git\"}").unwrap();
        let s = load(&p).unwrap();
        assert_eq!(s.version, 2);
        assert_eq!(s.git_path.as_deref(), Some("/git"));
        assert_eq!(s.ui_preferences, UiPreferences::default());
        cleanup(&p);
    }
    #[test]
    /// 验证保存 Git 路径和偏好时互相保留。
    fn saves_preserve_each_other() {
        let p = path();
        assert_eq!(load(&p).unwrap().log_level, None);
        save_log_level(&p, Some(crate::logging::LogLevel::Trace)).unwrap();
        save(&p, Some("/git".into())).unwrap();
        save_preferences(
            &p,
            UiPreferences {
                elasticity: 9,
                show_labels: false,
            },
        )
        .unwrap();
        assert_eq!(load(&p).unwrap().git_path.as_deref(), Some("/git"));
        save(&p, None).unwrap();
        assert_eq!(load(&p).unwrap().ui_preferences.elasticity, 9);
        assert_eq!(
            load(&p).unwrap().log_level,
            Some(crate::logging::LogLevel::Trace)
        );
        save_log_level(&p, None).unwrap();
        assert_eq!(load(&p).unwrap().log_level, None);
        assert_eq!(load(&p).unwrap().ui_preferences.elasticity, 9);
        cleanup(&p);
    }
    #[test]
    /// 验证默认值以及弹性值边界。
    fn defaults_and_boundaries() {
        assert_eq!(UiPreferences::default().elasticity, 6);
        let p = path();
        assert!(save_preferences(
            &p,
            UiPreferences {
                elasticity: 1,
                show_labels: true
            }
        )
        .is_ok());
        assert!(save_preferences(
            &p,
            UiPreferences {
                elasticity: 10,
                show_labels: true
            }
        )
        .is_ok());
        assert!(save_preferences(
            &p,
            UiPreferences {
                elasticity: 0,
                show_labels: true
            }
        )
        .is_err());
        assert!(save_preferences(
            &p,
            UiPreferences {
                elasticity: 11,
                show_labels: true
            }
        )
        .is_err());
        cleanup(&p);
    }
    #[test]
    /// 验证损坏和未知版本配置不会被读取或覆盖。
    fn corrupt_and_unknown_are_preserved() {
        let p = path();
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, b"broken").unwrap();
        assert!(load(&p).is_err());
        assert_eq!(fs::read(&p).unwrap(), b"broken");
        assert!(save(&p, Some("/git".into())).is_err());
        assert!(save_log_level(&p, Some(crate::logging::LogLevel::Error)).is_err());
        assert_eq!(fs::read(&p).unwrap(), b"broken");
        fs::write(&p, b"{\"version\":3,\"gitPath\":null}").unwrap();
        assert!(load(&p).is_err());
        assert!(save_preferences(&p, UiPreferences::default()).is_err());
        assert_eq!(fs::read(&p).unwrap(), b"{\"version\":3,\"gitPath\":null}");
        cleanup(&p);
    }

    /// 验证非法偏好不会改写已有配置字节。
    #[test]
    fn invalid_preferences_preserve_bytes() {
        let p = path();
        save(&p, Some("/git".into())).unwrap();
        let before = fs::read(&p).unwrap();
        assert!(save_preferences(
            &p,
            UiPreferences {
                elasticity: 0,
                show_labels: true
            }
        )
        .is_err());
        assert_eq!(fs::read(&p).unwrap(), before);
        cleanup(&p);
    }

    /// 拒绝未知日志级别并完整保留原始配置。
    #[test]
    fn invalid_log_level_preserves_bytes() {
        let p = path();
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        let bytes = br#"{"version":2,"gitPath":null,"uiPreferences":{"elasticity":6,"showLabels":true},"logLevel":"verbose"}"#;
        fs::write(&p, bytes).unwrap();
        assert!(save_log_level(&p, None).is_err());
        assert_eq!(fs::read(&p).unwrap(), bytes);
        cleanup(&p);
    }
}

/// 保存日志级别，保留其他设置并拒绝覆盖损坏配置。
pub fn save_log_level(
    path: &Path,
    level: Option<crate::logging::LogLevel>,
) -> Result<(), OperationError> {
    let _guard = lock()?;
    let mut current = read_unlocked(path)?;
    current.log_level = level;
    write_unlocked(path, &current)
}
