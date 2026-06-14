// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // 这里的 deep_index_board_lib 取决于 Cargo.toml 中的 package name
    // 通常 Tauri 会自动处理好这个映射
    deep_index_board_lib::run();
}