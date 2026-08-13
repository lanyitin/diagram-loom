// Windows 的 release build 不要跳出主控台視窗。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    diagram_loom_lib::run()
}
