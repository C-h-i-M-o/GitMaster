mod commands;
mod logging;
#[cfg(test)]
mod m2_contract_tests;
mod settings;
mod terminal;
use gitmaster_core::AppInfo;
use tauri::Manager;

/// macOS 默认退出项直接终止 AppKit；替换为经过文档保护的窗口关闭请求。
fn guarded_menu(app: &tauri::AppHandle) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    let menu = tauri::menu::Menu::default(app)?;
    #[cfg(target_os = "macos")]
    {
        use tauri::menu::{MenuItem, PredefinedMenuItem, Submenu};
        // Tauri 默认菜单首项是应用菜单，其他编辑、窗口等菜单原样保留。
        menu.remove_at(0)?;
        let application = Submenu::with_items(
            app,
            "gitMaster",
            true,
            &[
                &PredefinedMenuItem::about(app, Some("关于 gitMaster"), None)?,
                &PredefinedMenuItem::separator(app)?,
                &PredefinedMenuItem::services(app, Some("服务"))?,
                &PredefinedMenuItem::separator(app)?,
                &PredefinedMenuItem::hide(app, Some("隐藏 gitMaster"))?,
                &PredefinedMenuItem::hide_others(app, Some("隐藏其他应用"))?,
                &PredefinedMenuItem::separator(app)?,
                &MenuItem::with_id(
                    app,
                    "guarded-quit",
                    "退出 gitMaster",
                    true,
                    Some("CmdOrCtrl+Q"),
                )?,
            ],
        )?;
        menu.insert(&application, 0)?;
    }
    Ok(menu)
}

/// 将纯核心应用信息接口映射为桌面 command，不掺入业务规则。
#[tauri::command]
fn get_app_info() -> AppInfo {
    gitmaster_core::app_info()
}

/// 注册业务读写接口并启动 Tauri 窗口。
pub fn run() {
    tauri::Builder::default()
        .menu(guarded_menu)
        .on_menu_event(|app, event| {
            if event.id().as_ref() == "guarded-quit" {
                for window in app.webview_windows().values() {
                    let _ = window.close();
                }
            }
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(commands::DesktopState::default())
        .manage(commands::TerminalState::default())
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::Destroyed) {
                let app = window.app_handle().clone();
                let owner = window.label().to_owned();
                tauri::async_runtime::spawn_blocking(move || {
                    if let Ok(mut registry) = app.state::<commands::TerminalState>().0.lock() {
                        registry.close_window(&owner);
                    }
                });
            }
        })
        .setup(|app| logging::initialize(app.handle()))
        .invoke_handler(tauri::generate_handler![
            get_app_info,
            commands::read_app_settings,
            commands::apply_app_settings,
            commands::choose_terminal_path,
            commands::create_terminal,
            commands::read_terminal,
            commands::write_terminal,
            commands::resize_terminal,
            commands::close_terminal,
            logging::read_log_settings,
            logging::set_log_level,
            logging::open_log_directory,
            commands::detect_git,
            commands::set_git_path,
            commands::open_repository,
            commands::read_repository_state,
            commands::read_repository_watch,
            commands::read_file_diff,
            commands::open_diff_document,
            commands::read_diff_page,
            commands::close_diff_document,
            commands::read_commit_history,
            commands::read_commit_detail,
            commands::read_commit_files,
            commands::read_commit_file_diff,
            commands::read_branches,
            commands::read_project_files,
            commands::read_project_tree,
            commands::read_project_directory,
            commands::search_project_files,
            commands::read_project_file,
            commands::read_editable_file,
            commands::open_read_document,
            commands::read_document_page,
            commands::close_read_document,
            commands::search_read_document,
            commands::prepare_file_save,
            commands::open_project_folder,
            commands::read_external_availability,
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
            commands::read_sync_target,
            commands::assess_remote,
            commands::read_conflicts,
            commands::read_conflict_document,
            commands::read_ui_preferences,
            commands::set_ui_preferences,
            commands::open_git_install_page
        ])
        .build(tauri::generate_context!())
        .expect("gitMaster 桌面应用启动失败")
        .run(|app, event| {
            // 菜单退出同样走窗口关闭确认；最后一个窗口销毁后允许进程退出。
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                let windows = app.webview_windows();
                if !windows.is_empty() {
                    api.prevent_exit();
                    for window in windows.values() {
                        let _ = window.close();
                    }
                }
            }
        });
}
