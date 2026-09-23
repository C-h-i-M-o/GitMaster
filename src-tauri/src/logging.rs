use crate::settings;
use gitmaster_core::git::OperationError;
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU8, Ordering},
};
use tauri::{Manager, State};
use tauri_plugin_opener::OpenerExt;

static LEVEL: AtomicU8 = AtomicU8::new(1);
static UPDATE: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// 日志详细程度，数值越大包含的信息越多。
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Error = 1,
    Warn,
    Info,
    Debug,
    Trace,
}

/// 按编译模式确定默认级别，用户设置优先。
fn effective(level: Option<LogLevel>, development: bool) -> LogLevel {
    level.unwrap_or(if development {
        LogLevel::Trace
    } else {
        LogLevel::Error
    })
}

/// 只允许自有诊断事件通过，拒绝第三方输出与前端任意文本。
fn allowed(target: &str, level: log::Level, maximum: u8) -> bool {
    target == "gitmaster::diagnostic" && level as u8 <= maximum
}

/// 同时更新插件动态过滤和 facade 快速过滤，避免低档构造详细事件。
fn apply(level: LogLevel) {
    LEVEL.store(level as u8, Ordering::Relaxed);
    log::set_max_level(match level {
        LogLevel::Error => log::LevelFilter::Error,
        LogLevel::Warn => log::LevelFilter::Warn,
        LogLevel::Info => log::LevelFilter::Info,
        LogLevel::Debug => log::LevelFilter::Debug,
        LogLevel::Trace => log::LevelFilter::Trace,
    });
}

/// 日志服务状态不影响 Git 命令的可用性。
pub struct LogState {
    directory: PathBuf,
    config: PathBuf,
    available: bool,
}

/// 返回持久化选择、实际级别与日志目录状态。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogSettings {
    level: Option<LogLevel>,
    effective_level: LogLevel,
    directory: String,
    available: bool,
}

/// 初始化官方轮转器；初始化失败仍允许应用启动并在设置页显示状态。
pub fn initialize(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let config = app.path().app_config_dir()?.join("settings.json");
    let directory = app.path().app_log_dir()?;
    let level = effective(
        settings::load(&config).ok().and_then(|s| s.log_level),
        cfg!(debug_assertions),
    );
    LEVEL.store(level as u8, Ordering::Relaxed);
    let plugin = builder(
        tauri_plugin_log::TargetKind::LogDir {
            file_name: Some("gitmaster".into()),
        },
        5 * 1024 * 1024,
    )
    .build();
    let available = app.plugin(plugin).is_ok();
    // 从打包开发工具启动时可能发生目录虚拟化，Shell 需要真实物理路径。
    let directory = dunce::canonicalize(&directory).unwrap_or(directory);
    apply(level);
    if available {
        let folder = directory.clone();
        // 插件的同秒轮转备份不参与 KeepSome，后台只维护自有归档。
        let _ = std::thread::Builder::new()
            .name("log-retention".into())
            .spawn(move || loop {
                prune_archives(&folder);
                std::thread::sleep(std::time::Duration::from_secs(30));
            });
    }
    app.manage(LogState {
        config,
        directory,
        available,
    });
    tauri::async_runtime::spawn_blocking(
        || log::info!(target: "gitmaster::diagnostic", "应用启动 os={} development={}", std::env::consts::OS, cfg!(debug_assertions)),
    );
    Ok(())
}

/// 校验插件生成的时间戳归档名，不匹配当前日志或用户其他文件。
fn is_archive(name: &str) -> bool {
    let Some(stamp) = name.strip_prefix("gitmaster_").and_then(|s| {
        s.strip_suffix(".log.bak")
            .or_else(|| s.strip_suffix(".log"))
    }) else {
        return false;
    };
    stamp.len() == 19
        && stamp.bytes().enumerate().all(|(i, b)| match i {
            4 | 7 | 13 | 16 => b == b'-',
            10 => b == b'_',
            _ => b.is_ascii_digit(),
        })
}

/// 补充插件未覆盖的同秒备份清理，失败不干扰应用操作。
fn prune_archives(directory: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    let mut archives: Vec<_> = entries
        .flatten()
        .filter(|entry| is_archive(&entry.file_name().to_string_lossy()))
        .collect();
    archives.sort_by_key(|entry| {
        std::cmp::Reverse((
            entry
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::UNIX_EPOCH),
            entry.file_name(),
        ))
    });
    for entry in archives.into_iter().skip(3) {
        let _ = std::fs::remove_file(entry.path());
    }
}

/// 共享生产与隔离测试的过滤、格式和轮转设置。
fn builder(target: tauri_plugin_log::TargetKind, size: u128) -> tauri_plugin_log::Builder {
    tauri_plugin_log::Builder::new()
        .level(log::LevelFilter::Trace)
        .filter(|metadata| {
            allowed(
                metadata.target(),
                metadata.level(),
                LEVEL.load(Ordering::Relaxed),
            )
        })
        .targets([tauri_plugin_log::Target::new(target)])
        .max_file_size(size)
        .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepSome(3))
}

/// 从配置生成设置响应，损坏的配置不会被默认值掩盖。
fn read(state: &LogState) -> Result<LogSettings, OperationError> {
    let level = settings::load(&state.config)?.log_level;
    Ok(LogSettings {
        level,
        effective_level: effective(level, cfg!(debug_assertions)),
        directory: state.directory.to_string_lossy().into_owned(),
        available: state.available,
    })
}

/// 在阻塞线程读取日志设置。
#[tauri::command]
pub async fn read_log_settings(state: State<'_, LogState>) -> Result<LogSettings, OperationError> {
    let state = LogState {
        config: state.config.clone(),
        directory: state.directory.clone(),
        available: state.available,
    };
    tauri::async_runtime::spawn_blocking(move || read(&state))
        .await
        .map_err(|_| OperationError::new("SETTINGS_IO"))?
}

/// 保存成功后立即切换过滤级别；失败时保留当前级别。
#[tauri::command]
pub async fn set_log_level(
    state: State<'_, LogState>,
    level: Option<LogLevel>,
) -> Result<LogSettings, OperationError> {
    let state = LogState {
        config: state.config.clone(),
        directory: state.directory.clone(),
        available: state.available,
    };
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = UPDATE
            .lock()
            .map_err(|_| OperationError::new("SETTINGS_IO"))?;
        settings::save_log_level(&state.config, level)?;
        apply(effective(level, cfg!(debug_assertions)));
        log::info!(target: "gitmaster::diagnostic", "日志级别已更新");
        read(&state)
    })
    .await
    .map_err(|_| OperationError::new("SETTINGS_IO"))?
}

/// 只打开后端确定的日志目录，不接受任意路径。
#[tauri::command]
pub async fn open_log_directory(
    app: tauri::AppHandle,
    state: State<'_, LogState>,
) -> Result<(), OperationError> {
    let directory = state.directory.clone();
    tauri::async_runtime::spawn_blocking(move || {
        std::fs::create_dir_all(&directory).map_err(|_| OperationError::new("SETTINGS_IO"))?;
        let directory =
            dunce::canonicalize(directory).map_err(|_| OperationError::new("SETTINGS_IO"))?;
        app.opener()
            .open_path(directory.to_string_lossy(), None::<&str>)
            .map_err(|_| OperationError::new("SETTINGS_IO"))
    })
    .await
    .map_err(|_| OperationError::new("SETTINGS_IO"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    /// 使用真实插件写隔离临时目录，验证轮转和运行时过滤，不注册全局 logger。
    #[cfg(feature = "log-plugin-tests")]
    #[test]
    fn plugin_rotates_and_filters() {
        let _guard = UPDATE.lock().unwrap();
        let directory = std::env::temp_dir().join(format!(
            "gitmaster-logs-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let app = tauri::test::mock_app();
        let (_, _, logger) = builder(
            tauri_plugin_log::TargetKind::Folder {
                path: directory.clone(),
                file_name: Some("gitmaster".into()),
            },
            256,
        )
        .split(app.handle())
        .unwrap();
        for maximum in [1, 5, 1, 5] {
            LEVEL.store(maximum, Ordering::Relaxed);
            logger.log(
                &log::Record::builder()
                    .target("gitmaster::diagnostic")
                    .level(log::Level::Trace)
                    .args(format_args!("TRACE_MARKER"))
                    .build(),
            );
        }
        logger.log(
            &log::Record::builder()
                .target("thirdparty")
                .level(log::Level::Error)
                .args(format_args!("SECRET_MARKER"))
                .build(),
        );
        logger.flush();
        let first = std::fs::read_to_string(directory.join("gitmaster.log")).unwrap();
        assert_eq!(first.matches("TRACE_MARKER").count(), 2);
        assert!(!first.contains("SECRET_MARKER"));
        for _ in 0..40 {
            logger.log(
                &log::Record::builder()
                    .target("gitmaster::diagnostic")
                    .level(log::Level::Error)
                    .args(format_args!("ROTATION_MARKER"))
                    .build(),
            );
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        logger.flush();
        prune_archives(&directory);
        let files: Vec<_> = std::fs::read_dir(&directory)
            .unwrap()
            .map(|e| e.unwrap())
            .collect();
        assert!((2..=4).contains(&files.len()));
        assert!(files.iter().all(|f| f.metadata().unwrap().len() <= 256));
        drop(logger);
        std::fs::remove_dir_all(directory).unwrap();
        LEVEL.store(1, Ordering::Relaxed);
    }
    /// 只识别轮转器命名，不清理当前文件或其他文件。
    #[test]
    fn archive_names_are_scoped() {
        assert!(is_archive("gitmaster_2026-09-23_12-00-00.log.bak"));
        assert!(is_archive("gitmaster_2026-09-23_12-00-00.log"));
        for name in [
            "gitmaster.log",
            "gitmaster_notes.log",
            "other_2026-09-23_12-00-00.log",
        ] {
            assert!(!is_archive(name));
        }
    }
    /// 验证环境默认、全部级别与运行时升降过滤，以及第三方输出隔离。
    #[test]
    fn defaults_and_filtering() {
        assert_eq!(effective(None, false), LogLevel::Error);
        assert_eq!(effective(None, true), LogLevel::Trace);
        for selected in [
            LogLevel::Error,
            LogLevel::Warn,
            LogLevel::Info,
            LogLevel::Debug,
            LogLevel::Trace,
        ] {
            assert_eq!(effective(Some(selected), false), selected);
            for level in [
                log::Level::Error,
                log::Level::Warn,
                log::Level::Info,
                log::Level::Debug,
                log::Level::Trace,
            ] {
                assert_eq!(
                    allowed("gitmaster::diagnostic", level, selected as u8),
                    level as u8 <= selected as u8
                );
                assert!(!allowed("webview", level, selected as u8));
            }
        }
    }
}
