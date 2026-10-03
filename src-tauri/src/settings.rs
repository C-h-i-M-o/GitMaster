mod models;
use gitmaster_core::git::OperationError;
pub use models::*;
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Settings {
    #[serde(default)]
    pub log_level: Option<crate::logging::LogLevel>,
    pub version: u8,
    pub git_path: Option<String>,
    pub ui_preferences: UiPreferences,
    pub terminal: TerminalPreferences,
    pub editor: EditorPreferences,
    pub external_open: ExternalOpenPreferences,
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
    #[serde(default)]
    terminal: Option<TerminalPreferences>,
    #[serde(default)]
    editor: Option<EditorPreferences>,
    #[serde(default)]
    external_open: Option<ExternalOpenPreferences>,
}

/// 在全局设置锁已持有时读取并解析配置。
fn read_unlocked(path: &Path) -> Result<Settings, OperationError> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Settings {
                version: 4,
                log_level: None,
                git_path: None,
                ui_preferences: UiPreferences::default(),
                terminal: TerminalPreferences::default(),
                editor: EditorPreferences::default(),
                external_open: ExternalOpenPreferences::default(),
            })
        }
        Err(_) => return Err(OperationError::new("SETTINGS_IO")),
    };
    let stored: StoredSettings =
        serde_json::from_slice(&bytes).map_err(|_| OperationError::new("SETTINGS_IO"))?;
    let preferences = match stored.version {
        1 => UiPreferences::default(),
        2 | 3 | 4 => stored
            .ui_preferences
            .ok_or_else(|| OperationError::new("SETTINGS_IO"))?,
        _ => return Err(OperationError::new("SETTINGS_IO")),
    };
    validate(&preferences)?;
    let settings = Settings {
        version: 4,
        log_level: stored.log_level,
        git_path: stored.git_path,
        ui_preferences: preferences,
        terminal: if stored.version >= 3 {
            stored
                .terminal
                .ok_or_else(|| OperationError::new("SETTINGS_IO"))?
        } else {
            TerminalPreferences::default()
        },
        editor: if stored.version >= 3 {
            stored
                .editor
                .ok_or_else(|| OperationError::new("SETTINGS_IO"))?
        } else {
            EditorPreferences::default()
        },
        external_open: if stored.version >= 3 {
            stored
                .external_open
                .ok_or_else(|| OperationError::new("SETTINGS_IO"))?
        } else {
            ExternalOpenPreferences::default()
        },
    };
    if !models::validate(&settings).is_empty() {
        return Err(OperationError::new("SETTINGS_IO"));
    }
    Ok(settings)
}

/// 读取设置；缺失使用默认值，损坏时保留原文件并返回错误。
pub fn load(path: &Path) -> Result<Settings, OperationError> {
    let _guard = lock()?;
    read_unlocked(path)
}

/// 全量设置快照的版本仅用于防止过期覆盖，不作为授权令牌。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsSnapshot {
    pub settings: Settings,
    pub revision: String,
}

/// 表单错误只携带固定字段与脱敏原因。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsError {
    pub code: String,
    pub field_errors: Vec<FieldError>,
}
impl From<OperationError> for SettingsError {
    /// 保留稳定错误码，不透传底层错误原文。
    fn from(error: OperationError) -> Self {
        Self {
            code: error.code,
            field_errors: Vec::new(),
        }
    }
}

/// 对规范化设置内容产生进程内可比较的变更标识，不记录原文。
fn snapshot(settings: Settings) -> Result<SettingsSnapshot, OperationError> {
    use std::hash::{Hash, Hasher};
    let bytes = serde_json::to_vec(&settings).map_err(|_| OperationError::new("SETTINGS_IO"))?;
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hash);
    Ok(SettingsSnapshot {
        settings,
        revision: format!("v4-{:016x}", hash.finish()),
    })
}

/// 在同一配置锁下读取内容和变更标识。
pub fn read_snapshot(path: &Path) -> Result<SettingsSnapshot, OperationError> {
    let _guard = lock()?;
    snapshot(read_unlocked(path)?)
}

/// 应用完整设置时先校验版本及所有字段，再一次持久化；失败保留原配置。
pub fn save_all(
    path: &Path,
    expected_revision: &str,
    settings: Settings,
) -> Result<SettingsSnapshot, SettingsError> {
    let _guard = lock()?;
    let old = snapshot(read_unlocked(path)?)?;
    if old.revision != expected_revision {
        return Err(SettingsError {
            code: "STALE_SETTINGS".into(),
            field_errors: Vec::new(),
        });
    }
    let field_errors = models::validate(&settings);
    if settings.version != 4 || !field_errors.is_empty() {
        return Err(SettingsError {
            code: "INVALID_INPUT".into(),
            field_errors,
        });
    }
    let next = snapshot(settings)?;
    write_unlocked(path, &next.settings)?;
    Ok(next)
}

/// 将单个逻辑设置单元合并到当前快照，校验失败时不写入文件。
pub fn save_patch(
    path: &Path,
    expected_revision: &str,
    patch: SettingsPatch,
) -> Result<SettingsSnapshot, SettingsError> {
    let _guard = lock()?;
    let current = snapshot(read_unlocked(path)?)?;
    if current.revision != expected_revision {
        return Err(SettingsError {
            code: "STALE_SETTINGS".into(),
            field_errors: Vec::new(),
        });
    }
    let mut next = current.settings;
    match patch {
        SettingsPatch::Git { git_path } => next.git_path = git_path,
        SettingsPatch::Logging { log_level } => next.log_level = log_level,
        SettingsPatch::Appearance {
            elasticity,
            show_labels,
        } => {
            if elasticity.is_none() && show_labels.is_none() {
                return Err(SettingsError {
                    code: "INVALID_INPUT".into(),
                    field_errors: Vec::new(),
                });
            }
            if let Some(value) = elasticity {
                next.ui_preferences.elasticity = value;
            }
            if let Some(value) = show_labels {
                next.ui_preferences.show_labels = value;
            }
        }
        SettingsPatch::TerminalProfiles {
            profiles,
            default_profile_id,
        } => {
            next.terminal.profiles = profiles;
            next.terminal.default_profile_id = default_profile_id;
        }
        SettingsPatch::TerminalDisplay {
            font_family,
            font_size,
            cursor_style,
            cursor_blink,
            scrollback_lines,
        } => {
            if font_family.is_none()
                && font_size.is_none()
                && cursor_style.is_none()
                && cursor_blink.is_none()
                && scrollback_lines.is_none()
            {
                return Err(SettingsError {
                    code: "INVALID_INPUT".into(),
                    field_errors: Vec::new(),
                });
            }
            if let Some(value) = font_family {
                next.terminal.font_family = value;
            }
            if let Some(value) = font_size {
                next.terminal.font_size = value;
            }
            if let Some(value) = cursor_style {
                next.terminal.cursor_style = value;
            }
            if let Some(value) = cursor_blink {
                next.terminal.cursor_blink = value;
            }
            if let Some(value) = scrollback_lines {
                next.terminal.scrollback_lines = value;
            }
        }
        SettingsPatch::Editor {
            font_family,
            font_size,
            tab_size,
            word_wrap,
            save_mode,
        } => {
            if font_family.is_none()
                && font_size.is_none()
                && tab_size.is_none()
                && word_wrap.is_none()
                && save_mode.is_none()
            {
                return Err(SettingsError {
                    code: "INVALID_INPUT".into(),
                    field_errors: Vec::new(),
                });
            }
            if let Some(value) = font_family {
                next.editor.font_family = value;
            }
            if let Some(value) = font_size {
                next.editor.font_size = value;
            }
            if let Some(value) = tab_size {
                next.editor.tab_size = value;
            }
            if let Some(value) = word_wrap {
                next.editor.word_wrap = value;
            }
            if let Some(value) = save_mode {
                next.editor.save_mode = value;
            }
        }
        SettingsPatch::ExternalOpen { external_open } => next.external_open = external_open,
    }
    let errors = models::validate(&next);
    if !errors.is_empty() || next.version != 4 {
        return Err(SettingsError {
            code: "INVALID_INPUT".into(),
            field_errors: errors,
        });
    }
    let result = snapshot(next)?;
    write_unlocked(path, &result.settings)?;
    Ok(result)
}

/// 在同目录同步临时文件后原子替换，只清理本次创建的文件。
fn write_unlocked(path: &Path, settings: &Settings) -> Result<(), OperationError> {
    validate(&settings.ui_preferences)?;
    if !models::validate(settings).is_empty() {
        return Err(OperationError::new("INVALID_INPUT"));
    }
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
            version: 4,
            log_level: current.log_level,
            git_path,
            ui_preferences: current.ui_preferences,
            terminal: current.terminal,
            editor: current.editor,
            external_open: current.external_open,
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
            version: 4,
            log_level: current.log_level,
            git_path: current.git_path,
            ui_preferences: preferences.clone(),
            terminal: current.terminal,
            editor: current.editor,
            external_open: current.external_open,
        },
    )?;
    Ok(preferences)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    /// v3 读取补齐手动保存，首次按项保存才将配置升级为 v4。
    fn v3_migrates_manual_save_without_rewriting_on_read() {
        let p = path();
        let original = read_snapshot(&p).unwrap();
        let mut stored = serde_json::to_value(original.settings).unwrap();
        stored["version"] = 3.into();
        stored["editor"].as_object_mut().unwrap().remove("saveMode");
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        let bytes = serde_json::to_vec(&stored).unwrap();
        fs::write(&p, &bytes).unwrap();
        let loaded = read_snapshot(&p).unwrap();
        assert_eq!(loaded.settings.editor.save_mode, SaveMode::Manual);
        assert_eq!(fs::read(&p).unwrap(), bytes);
        let patch = serde_json::from_str(r#"{"kind":"editor","saveMode":"auto"}"#).unwrap();
        save_patch(&p, &loaded.revision, patch).unwrap();
        let reloaded = load(&p).unwrap();
        assert_eq!(reloaded.version, 4);
        assert_eq!(reloaded.editor.save_mode, SaveMode::Auto);
        cleanup(&p);
    }
    #[test]
    /// 非法字段不污染磁盘，也不阻止后续无关设置成功写入。
    fn invalid_patch_preserves_bytes_and_other_fields_can_save() {
        let p = path();
        save(&p, None).unwrap();
        let original = read_snapshot(&p).unwrap();
        let bytes = fs::read(&p).unwrap();
        let invalid = serde_json::from_str(r#"{"kind":"editor","fontSize":1}"#).unwrap();
        let error = save_patch(&p, &original.revision, invalid).unwrap_err();
        assert_eq!(error.code, "INVALID_INPUT");
        assert!(error
            .field_errors
            .iter()
            .any(|error| error.field == "editor.fontSize"));
        assert_eq!(fs::read(&p).unwrap(), bytes);
        let valid = serde_json::from_str(r#"{"kind":"editor","wordWrap":"on"}"#).unwrap();
        let saved = save_patch(&p, &original.revision, valid).unwrap();
        assert_eq!(saved.settings.editor.word_wrap, WordWrap::On);
        assert_eq!(
            saved.settings.editor.font_size,
            original.settings.editor.font_size
        );
        assert_eq!(saved.settings.terminal, original.settings.terminal);
        cleanup(&p);
    }
    #[test]
    /// 并发旧版本和空 patch 均不产生新的配置写入。
    fn stale_and_empty_patches_preserve_saved_settings() {
        let p = path();
        let original = read_snapshot(&p).unwrap();
        let first = serde_json::from_str(r#"{"kind":"appearance","elasticity":9}"#).unwrap();
        let saved = save_patch(&p, &original.revision, first).unwrap();
        let bytes = fs::read(&p).unwrap();
        let stale = serde_json::from_str(r#"{"kind":"appearance","showLabels":false}"#).unwrap();
        assert_eq!(
            save_patch(&p, &original.revision, stale).unwrap_err().code,
            "STALE_SETTINGS"
        );
        let empty = serde_json::from_str(r#"{"kind":"editor"}"#).unwrap();
        assert_eq!(
            save_patch(&p, &saved.revision, empty).unwrap_err().code,
            "INVALID_INPUT"
        );
        assert_eq!(fs::read(&p).unwrap(), bytes);
        cleanup(&p);
    }
    #[test]
    /// 过期草稿不能覆盖其他分类已保存的设置。
    fn stale_settings_draft_is_rejected() {
        let p = path();
        let first = read_snapshot(&p).unwrap();
        let mut draft = first.settings.clone();
        draft.editor.font_size = 18;
        let saved = save_all(&p, &first.revision, draft).unwrap();
        assert_eq!(saved.settings.editor.font_size, 18);
        assert!(save_all(&p, &first.revision, first.settings).is_err());
        assert_eq!(load(&p).unwrap().editor.font_size, 18);
        cleanup(&p);
    }
    #[test]
    /// 非法终端引用拒绝整个设置事务且返回对应字段。
    fn invalid_profile_preserves_all_categories() {
        let p = path();
        save(&p, None).unwrap();
        let old = fs::read(&p).unwrap();
        let snapshot = read_snapshot(&p).unwrap();
        let mut draft = snapshot.settings;
        draft.ui_preferences.elasticity = 9;
        draft.terminal.default_profile_id = "missing".into();
        let error = save_all(&p, &snapshot.revision, draft).unwrap_err();
        assert!(error
            .field_errors
            .iter()
            .any(|e| e.field == "terminal.defaultProfileId"));
        assert_eq!(fs::read(&p).unwrap(), old);
        cleanup(&p);
    }
    #[test]
    /// 旧分类保存接口必须保留新版本终端及编辑器字段。
    fn legacy_setters_preserve_new_preferences() {
        let p = path();
        let snapshot = read_snapshot(&p).unwrap();
        let mut draft = snapshot.settings;
        draft.terminal.profiles[0].args = vec!["-NoLogo".into()];
        draft.editor.font_size = 20;
        save_all(&p, &snapshot.revision, draft).unwrap();
        save_preferences(
            &p,
            UiPreferences {
                elasticity: 5,
                show_labels: true,
            },
        )
        .unwrap();
        save_log_level(&p, None).unwrap();
        save(&p, None).unwrap();
        let result = load(&p).unwrap();
        assert_eq!(result.terminal.profiles[0].args, vec!["-NoLogo"]);
        assert_eq!(result.editor.font_size, 20);
        cleanup(&p);
    }
    #[test]
    /// 旧配置升级必须保留用户设置并补齐终端及编辑器默认值。
    fn v2_migrates_to_complete_settings() {
        let p = path();
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, br#"{"version":2,"gitPath":"C:/Git/git.exe","uiPreferences":{"elasticity":8,"showLabels":false},"logLevel":"warn"}"#).unwrap();
        let value = serde_json::to_value(load(&p).unwrap()).unwrap();
        assert_eq!(value["version"], 4);
        assert_eq!(value["gitPath"], "C:/Git/git.exe");
        assert_eq!(value["uiPreferences"]["elasticity"], 8);
        assert_eq!(value["logLevel"], "warn");
        assert_eq!(value["terminal"]["defaultProfileId"], "system");
        assert_eq!(value["editor"]["tabSize"], 4);
        assert_eq!(value["externalOpen"]["defaultAppId"], "fileManager");
        // 读取不改写旧文件，只有应用后才持久化迁移。
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&fs::read(&p).unwrap()).unwrap()["version"],
            2
        );
        cleanup(&p);
    }
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
    /// 验证 v1 配置升级后使用 v3 默认偏好。
    fn v1_migrates_and_defaults() {
        let p = path();
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, b"{\"version\":1,\"gitPath\":\"/git\"}").unwrap();
        let s = load(&p).unwrap();
        assert_eq!(s.version, 4);
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
        fs::write(&p, b"{\"version\":99,\"gitPath\":null}").unwrap();
        assert!(load(&p).is_err());
        assert!(save_preferences(&p, UiPreferences::default()).is_err());
        assert_eq!(fs::read(&p).unwrap(), b"{\"version\":99,\"gitPath\":null}");
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
