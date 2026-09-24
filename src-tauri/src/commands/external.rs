//! 外部应用白名单：程序由后端发现，项目路径始终作为独立参数或工作目录。
use crate::settings::ExternalAppId;
use gitmaster_core::git::OperationError;
use serde::Serialize;
use std::{
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalAvailability {
    pub vs_code: bool,
    pub terminal: bool,
}

/// 查询已知安装位置，不搜索项目目录、不下载软件。
pub fn availability() -> ExternalAvailability {
    ExternalAvailability {
        vs_code: program(&ExternalAppId::VsCode).is_some(),
        terminal: program(&ExternalAppId::Terminal).is_some(),
    }
}

/// 使用系统已知安装位置查找入口，未找到时明确返回不可用。
fn program(app: &ExternalAppId) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    #[cfg(windows)]
    match app {
        ExternalAppId::VsCode => {
            for (variable, relative) in [
                ("LOCALAPPDATA", "Programs/Microsoft VS Code/Code.exe"),
                ("ProgramFiles", "Microsoft VS Code/Code.exe"),
                ("ProgramFiles(x86)", "Microsoft VS Code/Code.exe"),
            ] {
                if let Some(base) = std::env::var_os(variable) {
                    candidates.push(PathBuf::from(base).join(relative));
                }
            }
        }
        ExternalAppId::Terminal => {
            if let Some(base) = std::env::var_os("SystemRoot") {
                candidates.push(
                    PathBuf::from(base).join("System32/WindowsPowerShell/v1.0/powershell.exe"),
                );
            }
        }
        ExternalAppId::FileManager => {}
    }
    #[cfg(target_os = "macos")]
    match app {
        ExternalAppId::VsCode => {
            candidates.push(PathBuf::from("/Applications/Visual Studio Code.app"));
            if let Some(home) = std::env::var_os("HOME") {
                candidates.push(PathBuf::from(home).join("Applications/Visual Studio Code.app"));
            }
        }
        ExternalAppId::Terminal => {
            candidates.push(PathBuf::from("/System/Applications/Utilities/Terminal.app"))
        }
        ExternalAppId::FileManager => {}
    }
    candidates
        .into_iter()
        .find(|path| path.is_absolute() && path.exists())
}

/// 启动显式选择的应用；系统终端由用户请求显示交互窗口。
pub fn launch(app: &ExternalAppId, root: &Path) -> Result<(), OperationError> {
    let executable = program(app).ok_or_else(|| OperationError::new("EXTERNAL_APP_UNAVAILABLE"))?;
    let mut command = build_command(app, executable, root)?;
    let mut child = command
        .spawn()
        .map_err(|_| OperationError::new("EXTERNAL_OPEN_FAILED"))?;
    // 独立等待以回收子进程，不让外部编辑器生命周期阻塞 IPC。
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

/// 构造参数数组供启动和边界测试共用，不执行任何进程。
fn build_command(
    app: &ExternalAppId,
    executable: PathBuf,
    root: &Path,
) -> Result<Command, OperationError> {
    #[cfg(windows)]
    let command = {
        use std::os::windows::process::CommandExt;
        let mut command = Command::new(executable);
        match app {
            ExternalAppId::VsCode => {
                command.arg("--new-window").arg(root);
            }
            ExternalAppId::Terminal => {
                command
                    .arg("-NoExit")
                    .current_dir(root)
                    .creation_flags(0x00000010);
            }
            ExternalAppId::FileManager => return Err(OperationError::new("INVALID_INPUT")),
        }
        command
    };
    #[cfg(target_os = "macos")]
    let command = {
        let mut command = Command::new("/usr/bin/open");
        command.arg("-a").arg(executable).arg(root);
        command
    };
    #[cfg(not(any(windows, target_os = "macos")))]
    return Err(OperationError::new("EXTERNAL_APP_UNAVAILABLE"));
    #[cfg(any(windows, target_os = "macos"))]
    Ok(command)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    /// 特殊字符路径必须作为一个参数，不进入命令解释器。
    #[test]
    fn vscode_path_is_one_literal_argument() {
        let root = Path::new(r"C:\项目 空格\a&b$(x)");
        let command = build_command(
            &ExternalAppId::VsCode,
            PathBuf::from(r"C:\Apps\Code.exe"),
            root,
        )
        .unwrap();
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            [std::ffi::OsStr::new("--new-window"), root.as_os_str()]
        );
    }
    /// 终端起始目录使用进程属性，不构造 Set-Location 或其他脚本。
    #[test]
    fn terminal_path_is_only_working_directory() {
        let root = Path::new(r"C:\项目 空格\a&b$(x)");
        let command = build_command(
            &ExternalAppId::Terminal,
            PathBuf::from(r"C:\Windows\powershell.exe"),
            root,
        )
        .unwrap();
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            [std::ffi::OsStr::new("-NoExit")]
        );
        assert_eq!(command.get_current_dir(), Some(root));
    }
}
