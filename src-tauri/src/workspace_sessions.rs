use gitmaster_core::git::OperationError;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    sync::Mutex,
};
use tauri::{AppHandle, Manager};

const MAX_PROJECTS: usize = 20;
const MAX_EXPANDED: usize = 200;
const MAX_DOCUMENTS: usize = 50;
const MAX_SCROLL: f64 = 1_000_000_000.0;
const MAX_FILE_BYTES: usize = 2_000_000;
static IO_LOCK: Mutex<()> = Mutex::new(());
static NEXT: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct DocumentPosition {
    pub line_number: u32,
    pub column: u32,
    pub scroll_top: f64,
    pub scroll_left: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct WorkspaceDocument {
    pub path: String,
    pub position: Option<DocumentPosition>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct WorkspaceSession {
    pub root_path: String,
    pub expanded_paths: Vec<String>,
    pub documents: Vec<WorkspaceDocument>,
    pub active_path: Option<String>,
    pub active_page: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct WorkspaceSessionsFile {
    pub version: u8,
    pub last_root_path: Option<String>,
    pub projects: Vec<WorkspaceSession>,
}

/// 对外统一报告位置缓存错误，不泄漏配置内容。
fn error() -> OperationError {
    OperationError::new("SETTINGS_IO")
}
/// 工作区记录仅放入应用配置目录。
fn file_path(app: &AppHandle) -> Result<PathBuf, OperationError> {
    app.path()
        .app_config_dir()
        .map(|d| d.join("workspace-sessions.json"))
        .map_err(|_| error())
}
/// 接受文件树使用的非空相对路径，拒绝父路径和 Windows 路径别名。
fn valid_relative(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 4096
        && !value.contains(['\\', ':', '\0'])
        && value
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
        && !Path::new(value).is_absolute()
}
/// 历史记录只校验结构，已删除项目不影响其他工作区。
fn validate_structure(session: &WorkspaceSession) -> Result<(), OperationError> {
    if !Path::new(&session.root_path).is_absolute()
        || session.root_path.len() > 32768
        || session.expanded_paths.len() > MAX_EXPANDED
        || session.documents.len() > MAX_DOCUMENTS
        || !["files", "changes", "history"].contains(&session.active_page.as_str())
    {
        return Err(error());
    }
    if session.expanded_paths.iter().any(|p| !valid_relative(p))
        || session.documents.iter().any(|d| !valid_relative(&d.path))
        || session
            .active_path
            .as_ref()
            .is_some_and(|p| !session.documents.iter().any(|d| &d.path == p))
    {
        return Err(error());
    }
    if session.documents.iter().any(|d| {
        d.position.as_ref().is_some_and(|p| {
            p.line_number == 0
                || p.column == 0
                || p.line_number > 10_000_000
                || p.column > 10_000_000
                || !p.scroll_top.is_finite()
                || !p.scroll_left.is_finite()
                || p.scroll_top < 0.0
                || p.scroll_left < 0.0
                || p.scroll_top > MAX_SCROLL
                || p.scroll_left > MAX_SCROLL
        })
    }) {
        return Err(error());
    }
    Ok(())
}
/// 只对新写入的活动项目核实目录并规范化根路径。
fn validate(session: &WorkspaceSession) -> Result<WorkspaceSession, OperationError> {
    validate_structure(session)?;
    let root = dunce::canonicalize(&session.root_path).map_err(|_| error())?;
    if !root.is_dir() {
        return Err(error());
    }
    let mut normalized = session.clone();
    normalized.root_path = root.to_str().ok_or_else(error)?.to_owned();
    Ok(normalized)
}
/// 有界读取记录并拒绝未知结构，不访问历史项目磁盘。
fn read_file(path: &Path) -> Result<WorkspaceSessionsFile, OperationError> {
    let metadata = match fs::metadata(path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(WorkspaceSessionsFile {
                version: 1,
                last_root_path: None,
                projects: Vec::new(),
            })
        }
        Err(_) => return Err(error()),
    };
    if metadata.len() > MAX_FILE_BYTES as u64 {
        return Err(error());
    }
    let file = fs::File::open(path).map_err(|_| error())?;
    let mut bytes = Vec::new();
    use std::io::Read;
    file.take((MAX_FILE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| error())?;
    if bytes.len() > MAX_FILE_BYTES {
        return Err(error());
    }
    let value: WorkspaceSessionsFile = serde_json::from_slice(&bytes).map_err(|_| error())?;
    if value.version != 1
        || value.projects.len() > MAX_PROJECTS
        || value
            .projects
            .iter()
            .any(|s| validate_structure(s).is_err())
        || value
            .last_root_path
            .as_ref()
            .is_some_and(|root| !value.projects.iter().any(|s| &s.root_path == root))
    {
        return Err(error());
    }
    Ok(value)
}
/// 写入完整临时文件后原子替换，失败保留原记录。
fn write_file(path: &Path, value: &WorkspaceSessionsFile) -> Result<(), OperationError> {
    let content = serde_json::to_vec_pretty(value).map_err(|_| error())?;
    if content.len() > MAX_FILE_BYTES {
        return Err(error());
    }
    let parent = path.parent().ok_or_else(error)?;
    fs::create_dir_all(parent).map_err(|_| error())?;
    let temp = path.with_extension(format!(
        "{}-{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| -> std::io::Result<()> {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        f.write_all(&content)?;
        f.sync_all()?;
        drop(f);
        fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result.map_err(|_| error())
}

/// 读取工作区会话持久化数据。
#[tauri::command]
pub fn read_workspace_sessions(app: AppHandle) -> Result<WorkspaceSessionsFile, OperationError> {
    let _lock = IO_LOCK.lock().map_err(|_| error())?;
    read_file(&file_path(&app)?)
}
/// 校验并原子保存一个工作区会话。
#[tauri::command]
pub fn write_workspace_session(
    app: AppHandle,
    session: WorkspaceSession,
) -> Result<(), OperationError> {
    let _lock = IO_LOCK.lock().map_err(|_| error())?;
    let session = validate(&session)?;
    let path = file_path(&app)?;
    let mut file = read_file(&path)?;
    file.projects.retain(|s| s.root_path != session.root_path);
    file.projects.insert(0, session.clone());
    file.projects.truncate(MAX_PROJECTS);
    file.last_root_path = Some(session.root_path.clone());
    write_file(&path, &file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// 创建不含正文的最小位置记录。
    fn session(root: &Path) -> WorkspaceSession {
        WorkspaceSession {
            root_path: root.to_string_lossy().into(),
            expanded_paths: Vec::new(),
            documents: Vec::new(),
            active_path: None,
            active_page: "files".into(),
        }
    }
    #[test]
    fn rejects_absolute_relative_path() {
        let mut s = session(&std::env::temp_dir());
        s.expanded_paths = vec!["C:\\x".into()];
        assert!(validate(&s).is_err());
    }

    #[test]
    fn rejects_parent_unknown_and_overflow() {
        let root = std::env::temp_dir();
        let mut s = session(&root);
        s.expanded_paths = vec!["../x".into()];
        assert!(validate(&s).is_err());
        let mut s = session(&root);
        s.documents = vec![WorkspaceDocument {
            path: "a".into(),
            position: Some(DocumentPosition {
                line_number: 1,
                column: 1,
                scroll_top: f64::NAN,
                scroll_left: 0.0,
            }),
        }];
        assert!(validate(&s).is_err());
        let mut unknown = serde_json::to_value(session(&root)).unwrap();
        unknown
            .as_object_mut()
            .unwrap()
            .insert("body".into(), serde_json::Value::String("x".into()));
        assert!(serde_json::from_value::<WorkspaceSession>(unknown).is_err());
    }

    #[test]
    fn writes_twice_and_preserves_corruption_and_rejects_oversize() {
        let dir = std::env::temp_dir().join(format!(
            "gitmaster-workspace-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("workspace-sessions.json");
        let root = dir.join("repo");
        fs::create_dir_all(&root).unwrap();
        let first = validate(&session(&root)).unwrap();
        let mut file = WorkspaceSessionsFile {
            version: 1,
            last_root_path: None,
            projects: vec![validate(&first).unwrap()],
        };
        write_file(&path, &file).unwrap();
        file.last_root_path = Some(first.root_path.clone());
        write_file(&path, &file).unwrap();
        assert_eq!(
            read_file(&path).unwrap().last_root_path.as_deref(),
            Some(first.root_path.as_str())
        );
        fs::remove_dir(&root).unwrap();
        assert_eq!(read_file(&path).unwrap().projects.len(), 1);
        fs::write(&path, b"broken").unwrap();
        let original = fs::read(&path).unwrap();
        assert!(read_file(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
        fs::write(&path, vec![b'x'; MAX_FILE_BYTES + 1]).unwrap();
        assert!(read_file(&path).is_err());
        fs::remove_dir_all(dir).unwrap();
    }
}
