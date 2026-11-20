// 墨钥 KeyHub · 桌面程序入口
//
// Windows release 构建时隐藏控制台窗口，避免桌面应用弹出黑框。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    keyhub_lib::run()
}
