use gitmaster_core::git::OperationError;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Manager};

const MAX_PROJECTS: usize = 20;
static IO_LOCK: Mutex<()> = Mutex::new(());
static LAST_ERROR: Mutex<Option<(PathBuf, OperationError)>> = Mutex::new(None);
static NEXT: AtomicU64 = AtomicU64::new(0);

/// 最近项目写入失败只影响菜单提示，不撤回已成功打开的仓库。
pub fn remember_error(root: &Path, error: OperationError) {
    if let Ok(mut value) = LAST_ERROR.lock() {
        *value = Some((root.to_owned(), error));
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RecentProject {
    pub root_path: String,
    pub name: String,
    pub last_opened_at: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct RecentProjectsFile {
    version: u8,
    projects: Vec<RecentProject>,
}

fn file_path(app: &AppHandle) -> Result<PathBuf, OperationError> {
    app.path()
        .app_config_dir()
        .map(|dir| dir.join("recent-projects.json"))
        .map_err(|_| OperationError::new("SETTINGS_IO"))
}

fn read_file(path: &Path) -> Result<Vec<RecentProject>, OperationError> {
    let bytes = match fs::read(path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err(OperationError::new("SETTINGS_IO")),
    };
    let stored: RecentProjectsFile =
        serde_json::from_slice(&bytes).map_err(|_| OperationError::new("SETTINGS_IO"))?;
    if stored.version != 1 {
        return Err(OperationError::new("SETTINGS_IO"));
    }
    Ok(stored.projects.into_iter().take(MAX_PROJECTS).collect())
}

fn record_at_path(path: &Path, root: &Path) -> Result<(), OperationError> {
    let _lock = IO_LOCK
        .lock()
        .map_err(|_| OperationError::new("SETTINGS_IO"))?;
    let root_path = dunce::canonicalize(root).map_err(|_| OperationError::new("SETTINGS_IO"))?;
    let root_string = root_path
        .to_str()
        .ok_or_else(|| OperationError::new("UNSUPPORTED_PATH_ENCODING"))?
        .to_owned();
    let name = root_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(&root_string)
        .to_owned();
    let mut projects = read_file(path)?;
    projects.retain(|project| project.root_path != root_string);
    projects.insert(
        0,
        RecentProject {
            root_path: root_string,
            name,
            last_opened_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        },
    );
    projects.truncate(MAX_PROJECTS);
    let content = serde_json::to_vec_pretty(&RecentProjectsFile {
        version: 1,
        projects,
    })
    .map_err(|_| OperationError::new("SETTINGS_IO"))?;
    let parent = path
        .parent()
        .ok_or_else(|| OperationError::new("SETTINGS_IO"))?;
    fs::create_dir_all(parent).map_err(|_| OperationError::new("SETTINGS_IO"))?;
    let temp = path.with_extension(format!(
        "{}-{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| -> Result<(), std::io::Error> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(&content)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result.map_err(|_| OperationError::new("SETTINGS_IO"))
}

/// 读取最近成功打开的项目，不触碰项目目录。
#[tauri::command]
pub fn read_recent_projects(app: AppHandle) -> Result<Vec<RecentProject>, OperationError> {
    let pending = LAST_ERROR
        .lock()
        .map_err(|_| OperationError::new("SETTINGS_IO"))?
        .clone();
    if let Some((root, _)) = pending {
        record_opened_project(&app, &root)?;
    }
    read_file(&file_path(&app)?)
}

/// 在仓库已成功发布到活动会话后记录其规范化根目录。
pub fn record_opened_project(app: &AppHandle, root: &Path) -> Result<(), OperationError> {
    let path = file_path(app)?;
    record_at_path(&path, root)?;
    if let Ok(mut value) = LAST_ERROR.lock() {
        *value = None;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "gitmaster-recent-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn missing_file_reads_empty() {
        let dir = temp_dir();
        assert!(read_file(&dir.join("recent-projects.json"))
            .unwrap()
            .is_empty());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn records_twenty_deduplicated_projects_and_refreshes_order() {
        let dir = temp_dir();
        let file = dir.join("recent-projects.json");
        for index in 0..21 {
            let root = dir.join(format!("project-{index}"));
            fs::create_dir_all(&root).unwrap();
            record_at_path(&file, &root).unwrap();
        }
        let duplicate = dir.join("project-5");
        record_at_path(&file, &duplicate).unwrap();
        let projects = read_file(&file).unwrap();
        assert_eq!(projects.len(), 20);
        assert_eq!(
            projects[0].root_path,
            dunce::canonicalize(duplicate).unwrap().to_string_lossy()
        );
        assert!(!projects
            .iter()
            .skip(1)
            .any(|project| project.root_path == projects[0].root_path));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn corruption_is_reported_and_original_bytes_remain() {
        let dir = temp_dir();
        let file = dir.join("recent-projects.json");
        fs::write(&file, b"broken").unwrap();
        let before = fs::read(&file).unwrap();
        assert!(read_file(&file).is_err());
        assert!(record_at_path(&file, &dir).is_err());
        assert_eq!(fs::read(&file).unwrap(), before);
        fs::remove_dir_all(dir).unwrap();
    }
}
