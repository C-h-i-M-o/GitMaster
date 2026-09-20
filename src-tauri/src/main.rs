#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// 启动桌面应用；发布版 Windows 不附带控制台窗口。
fn main() {
    gitmaster_desktop_lib::run();
}
