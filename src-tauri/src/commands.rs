use crate::dbm::{self, DbState, HistoryItem};
use crate::inference::InferenceState;
use arboard::{Clipboard, ImageData};
use std::borrow::Cow;
use std::collections::HashMap;
use tauri::{AppHandle, Emitter, Manager, State};

#[tauri::command]
pub fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
pub fn get_history(
    state: State<DbState>,
    last_timestamp: Option<String>,
    limit: usize,
    query: Option<String>,
    favorites_only: bool,
) -> Result<Vec<HistoryItem>, String> {
    let conn = state.conn.lock().unwrap();
    dbm::get_history(&conn, last_timestamp, limit, query, Some(favorites_only))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn clear_history(
    app_handle: AppHandle,
    state: State<DbState>,
    favorites_only: bool,
) -> Result<(), String> {
    let image_paths = {
        let conn = state.conn.lock().unwrap();
        dbm::get_clearable_image_paths(&conn, favorites_only).map_err(|e| e.to_string())?
    };

    {
        let conn = state.conn.lock().unwrap();
        dbm::clear_history(&conn, favorites_only).map_err(|e| e.to_string())?;
    }

    let app_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;
    let images_dir = dbm::get_images_dir(&app_dir);
    for image_path in image_paths {
        let path = std::path::PathBuf::from(image_path);
        if path.starts_with(&images_dir) && path.exists() {
            let _ = std::fs::remove_file(path);
        }
    }

    let _ = app_handle.emit("history-cleared", ());
    Ok(())
}

#[tauri::command]
pub fn start_window_dragging(app_handle: AppHandle) -> Result<(), String> {
    let window = app_handle
        .get_webview_window("main")
        .ok_or_else(|| "Main window not found".to_string())?;

    crate::platform::restore_window_activation(&window).map_err(|e| e.to_string())?;

    window.start_dragging().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn prepare_window_dragging(app_handle: AppHandle) -> Result<(), String> {
    let window = app_handle
        .get_webview_window("main")
        .ok_or_else(|| "Main window not found".to_string())?;

    crate::platform::restore_window_activation(&window).map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn get_history_semantic(
    app_handle: AppHandle,
    db_state: State<'_, DbState>,
    task_state: State<'_, crate::task_manager::HeavyWorkState>,
    query: String,
    limit: usize,
    favorites_only: bool,
) -> Result<Vec<HistoryItem>, String> {
    if query.trim().is_empty() {
        return Ok(Vec::new());
    }

    // 1. 准备模型路径
    #[cfg(target_os = "macos")]
    let text_model_resource = "resources/models/clips/vit-b-16.text.mlpackage";
    #[cfg(target_os = "windows")]
    let text_model_resource = "resources/models/clips/vit-b-16.txt.int8.onnx";
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let text_model_resource = "resources/models/clips/vit-b-16.text.mlpackage";

    let model_path = crate::utils::resolve_resource_path(&app_handle, text_model_resource)?;

    let vocab_path =
        crate::utils::resolve_resource_path(&app_handle, "resources/models/clips/vocab.txt")?;

    // 2. 发起异步向量化任务
    let (tx, rx) = tokio::sync::oneshot::channel();
    task_state
        .manager
        .execute(
            crate::task_manager::HeavyTask::ClipTextEmbedding {
                text: query,
                model_path,
                vocab_path,
                callback: Box::new(move |res| {
                    let _ = tx.send(res);
                }),
            },
            crate::task_manager::TaskPriority::High,
        )
        .map_err(|e| e.to_string())?;

    // 等待结果
    let embedding = rx
        .await
        .map_err(|_| "Internal task timeout".to_string())?
        .map_err(|e| e.to_string())?;

    // 3. 数据库检索
    let conn = db_state.conn.lock().unwrap();
    dbm::get_history_semantic(&conn, &embedding, limit, Some(favorites_only))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_favorite(
    app_handle: AppHandle,
    state: State<DbState>,
    id: i64,
    is_favorite: bool,
) -> Result<HistoryItem, String> {
    let item = {
        let conn = state.conn.lock().unwrap();
        dbm::set_favorite(&conn, id, is_favorite).map_err(|e| e.to_string())?
    }
    .ok_or_else(|| "Item not found".to_string())?;

    let _ = app_handle.emit("history-item-updated", &item);
    Ok(item)
}

#[tauri::command]
pub fn unfavorite_all(state: State<DbState>) -> Result<(), String> {
    let conn = state.conn.lock().unwrap();
    dbm::unfavorite_all(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_text_item_copy(
    app_handle: AppHandle,
    state: State<DbState>,
    content: String,
) -> Result<HistoryItem, String> {
    if content.is_empty() {
        return Err("Content is empty".to_string());
    }

    let item = {
        let conn = state.conn.lock().unwrap();
        dbm::save_text_copy(&conn, &content).map_err(|e| e.to_string())?
    };

    let _ = app_handle.emit("clipboard-updated", &item);
    Ok(item)
}

#[tauri::command]
pub fn overwrite_text_item(
    app_handle: AppHandle,
    state: State<DbState>,
    id: i64,
    content: String,
) -> Result<HistoryItem, String> {
    if content.is_empty() {
        return Err("Content is empty".to_string());
    }

    let item = {
        let conn = state.conn.lock().unwrap();
        dbm::overwrite_text_item(&conn, id, &content).map_err(|e| e.to_string())?
    };

    let _ = app_handle.emit("history-item-updated", &item);
    Ok(item)
}

#[tauri::command]
pub fn delete_item(app_handle: AppHandle, state: State<DbState>, id: i64) -> Result<(), String> {
    let item = {
        let conn = state.conn.lock().unwrap();
        dbm::delete_item(&conn, id).map_err(|e| e.to_string())?
    }
    .ok_or_else(|| "Item not found".to_string())?;

    if item.r#type == "image" {
        let app_dir = app_handle
            .path()
            .app_data_dir()
            .map_err(|e| e.to_string())?;
        let images_dir = dbm::get_images_dir(&app_dir);
        let path = std::path::PathBuf::from(&item.content);
        if path.starts_with(&images_dir) && path.exists() {
            let _ = std::fs::remove_file(path);
        }
    }

    let _ = app_handle.emit("history-item-deleted", id);
    Ok(())
}

#[tauri::command]
pub async fn paste_item(
    app_handle: AppHandle,
    state: State<'_, DbState>,
    id: i64,
) -> Result<(), String> {
    // 1. 获取项目数据
    let item = {
        let conn = state.conn.lock().unwrap();
        dbm::get_item_by_id(&conn, id).map_err(|e| e.to_string())?
    };

    let item = item.ok_or_else(|| "Item not found".to_string())?;

    // 3. 即时发起 UI 更新（不必等待后台轮询周期）
    // 必须在写入剪贴板（可能会 move 掉 item.content）之前进行 emit
    let _ = app_handle.emit("clipboard-updated", &item);

    // 4. 写入剪贴板
    let mut clipboard = Clipboard::new().map_err(|e| e.to_string())?;

    if item.r#type == "text" {
        clipboard
            .set_text(item.content.clone())
            .map_err(|e| e.to_string())?;
    } else if item.r#type == "image" {
        let img = image::open(&item.content).map_err(|e| e.to_string())?;
        let rgba = img.to_rgba8();
        let (width, height) = rgba.dimensions();
        let image_data = ImageData {
            width: width as usize,
            height: height as usize,
            bytes: Cow::from(rgba.into_raw()),
        };
        clipboard.set_image(image_data).map_err(|e| e.to_string())?;
    } else if item.r#type == "file" || item.r#type == "directory" {
        crate::platform::write_file_path_to_clipboard(&mut clipboard, &item.content)?;
    }

    // 3. 隐藏窗口以回归焦点
    if let Some(window) = app_handle.get_webview_window("main") {
        crate::platform::hide_quick_window(&window);
    }

    crate::platform::paste_clipboard_item(
        &app_handle,
        &item.r#type,
        (item.r#type == "text").then_some(item.content.as_str()),
    );

    Ok(())
}

#[tauri::command]
pub fn get_memory_usage() -> u64 {
    use sysinfo::System;
    let mut sys = System::new_all();
    sys.refresh_all();

    let pid = sysinfo::get_current_pid().unwrap();
    let mut total_memory = 0;

    // 获取当前进程及其所有子进程的内存占用 (RSS)
    for (p_pid, process) in sys.processes() {
        if *p_pid == pid || process.parent() == Some(pid) {
            total_memory += process.memory();
        }
    }

    total_memory
}

#[tauri::command]
pub fn get_inference_report(state: State<InferenceState>) -> HashMap<String, usize> {
    state.manager.get_memory_report()
}

#[tauri::command]
pub fn release_inference_session(state: State<InferenceState>, model_id: String) {
    state.manager.release_session(&model_id);
}

#[tauri::command]
pub fn release_all_inference(state: State<InferenceState>) {
    state.manager.release_all();
}
