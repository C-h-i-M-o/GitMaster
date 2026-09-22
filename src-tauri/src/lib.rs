mod commands;
#[cfg(test)]
mod m2_contract_tests;
mod settings;
use gitmaster_core::AppInfo;

/// 将纯核心应用信息接口映射为桌面 command，不掺入业务规则。
#[tauri::command]
fn get_app_info() -> AppInfo {
    gitmaster_core::app_info()
}

/// 注册业务读写接口并启动 Tauri 窗口。
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(commands::DesktopState::default())
        .invoke_handler(tauri::generate_handler![
            get_app_info,
            commands::detect_git,
            commands::set_git_path,
            commands::open_repository,
            commands::read_repository_state,
            commands::read_file_diff,
            commands::read_commit_history,
            commands::read_commit_detail,
            commands::read_commit_files,
            commands::read_commit_file_diff,
            commands::read_branches,
            commands::read_project_files,
            commands::read_project_file,
            commands::choose_git_path,
            commands::choose_repository_path,
            commands::read_write_context,
            commands::prepare_local_write,
            commands::prepare_remote_write,
            commands::prepare_conflict_write,
            commands::execute_write,
            commands::choose_clone_parent,
            commands::prepare_clone,
            commands::execute_clone,
            commands::read_operation,
            commands::read_remotes,
            commands::assess_remote,
            commands::read_conflicts,
            commands::read_conflict_document,
            commands::read_ui_preferences,
            commands::set_ui_preferences,
            commands::open_git_install_page
        ])
        .run(tauri::generate_context!())
        .expect("gitMaster 桌面应用启动失败");
}
