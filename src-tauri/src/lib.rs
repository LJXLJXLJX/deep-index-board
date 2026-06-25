use std::str::FromStr;
use std::sync::{Arc, Mutex};
use tauri::Manager;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

mod clipboard;
mod commands;
mod dbm;
mod inference;
mod platform;
mod setup;
mod task_manager;
mod tasks;
mod tray;
mod utils;

use crate::dbm::{connect_to_main_clipboard_db, DbState};
#[cfg(target_os = "macos")]
use crate::inference::CoreMLBackend;
use crate::inference::{InferenceState, SessionManager};
use crate::task_manager::{HeavyWorkManager, HeavyWorkState};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 初始化全局日志记录器
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }
    env_logger::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--autostart"]),
        ))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            commands::greet,
            commands::get_history,
            commands::clear_history,
            commands::start_window_dragging,
            commands::prepare_window_dragging,
            commands::paste_item,
            commands::get_memory_usage,
            commands::get_inference_report,
            commands::release_inference_session,
            commands::release_all_inference,
            commands::get_history_semantic,
            commands::set_favorite,
            commands::unfavorite_all,
            commands::save_text_item_copy,
            commands::overwrite_text_item,
            commands::delete_item
        ])
        .setup(|app| {
            // 1. 数据库初始化
            let app_dir = app
                .path()
                .app_data_dir()
                .expect("Failed to get app data dir");
            let sql_conn = connect_to_main_clipboard_db(app_dir.clone());
            app.manage(DbState {
                conn: Mutex::new(sql_conn),
            });

            // 1.5 推理引擎初始化
            #[cfg(target_os = "macos")]
            let mut inference_manager = SessionManager::new();
            #[cfg(not(target_os = "macos"))]
            let inference_manager = SessionManager::new();
            #[cfg(target_os = "macos")]
            inference_manager.register_backend(Box::new(CoreMLBackend));

            let inference_manager = Arc::new(inference_manager);

            app.manage(InferenceState {
                manager: inference_manager.clone(),
            });

            // 1.6 异步重度任务管理器初始化
            app.manage(HeavyWorkState {
                manager: HeavyWorkManager::init(inference_manager),
            });

            // 2. 统一的 UI 初始化
            setup::setup_app_ui(app).expect("Failed to setup UI");

            // 3. 启动剪贴板监听
            clipboard::start_monitoring(app.handle().clone());

            // 4. 注册全局快捷键
            #[cfg(target_os = "windows")]
            let shortcut = Shortcut::from_str("Alt+V").unwrap();
            #[cfg(not(target_os = "windows"))]
            let shortcut = Shortcut::from_str("Control+V").unwrap();
            app.global_shortcut()
                .on_shortcut(shortcut, move |app_handle, _shortcut, event| {
                    if event.state() == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        utils::toggle_window_no_activate(app_handle);
                    }
                })?;

            // 5. 初始化系统托盘
            tray::setup_tray(app.handle())?;

            // 5.5 启动后台向量化扫描任务
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            tasks::background_worker::start_background_worker(app.handle().clone());

            // 6. 系统自启动配置已注册到 Builder
            // 如果需要在 Rust 中手动控制，可以使用插件提供的 API

            Ok(())
        })
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::CloseRequested { api, .. } => {
                let _ = window.hide();
                api.prevent_close();
            }
            #[cfg(not(target_os = "windows"))]
            tauri::WindowEvent::Focused(false) => {
                let _ = window.hide();
            }
            _ => {}
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
