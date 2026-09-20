use gitmaster_core::AppInfo;

/// 将纯核心应用信息接口映射为桌面 command，不掺入业务规则。
#[tauri::command]
fn get_app_info() -> AppInfo {
    gitmaster_core::app_info()
}

/// 注册最小只读接口并启动 Tauri 窗口。
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![get_app_info])
        .run(tauri::generate_context!())
        .expect("gitMaster 桌面应用启动失败");
}
