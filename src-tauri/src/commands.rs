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
) -> Result<Vec<HistoryItem>, String> {
    let conn = state.conn.lock().unwrap();
    dbm::get_history(&conn, last_timestamp, limit, query).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn clear_history(app_handle: AppHandle, state: State<DbState>) -> Result<(), String> {
    {
        let conn = state.conn.lock().unwrap();
        dbm::clear_all_history(&conn).map_err(|e| e.to_string())?;
    }

    let app_dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let images_dir = dbm::get_images_dir(&app_dir);
    if images_dir.exists() {
        std::fs::remove_dir_all(&images_dir).map_err(|e| e.to_string())?;
    }
    std::fs::create_dir_all(&images_dir).map_err(|e| e.to_string())?;

    let _ = app_handle.emit("history-cleared", ());
    Ok(())
}

#[tauri::command]
pub fn start_window_dragging(app_handle: AppHandle) -> Result<(), String> {
    let window = app_handle
        .get_webview_window("main")
        .ok_or_else(|| "Main window not found".to_string())?;

    #[cfg(target_os = "windows")]
    crate::utils::restore_window_activation(&window).map_err(|e| e.to_string())?;

    window.start_dragging().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn prepare_window_dragging(app_handle: AppHandle) -> Result<(), String> {
    let window = app_handle
        .get_webview_window("main")
        .ok_or_else(|| "Main window not found".to_string())?;

    #[cfg(target_os = "windows")]
    crate::utils::restore_window_activation(&window).map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn get_history_semantic(
    app_handle: AppHandle,
    db_state: State<'_, DbState>,
    task_state: State<'_, crate::task_manager::HeavyWorkState>,
    query: String,
    limit: usize,
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
    dbm::get_history_semantic(&conn, &embedding, limit).map_err(|e| e.to_string())
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
            .set_text(item.content)
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
        #[cfg(target_os = "macos")]
        {
            use objc2_app_kit::NSPasteboard;
            use objc2_foundation::{NSArray, NSString};
            unsafe {
                let pb = NSPasteboard::generalPasteboard();
                pb.clearContents();
                let ns_str = NSString::from_str(&item.content);
                let array = NSArray::from_id_slice(&[ns_str]);
                let filenames_type = NSString::from_str("NSFilenamesPboardType");
                pb.setPropertyList_forType(&array, &filenames_type);
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            // 其他平台暂时回退到路径文本，因为 arboard 尚未原生支持文件列表
            clipboard
                .set_text(item.content)
                .map_err(|e| e.to_string())?;
        }
    }

    // 3. 隐藏窗口以回归焦点
    if let Some(window) = app_handle.get_webview_window("main") {
        let _ = window.hide();
    }
    #[cfg(target_os = "macos")]
    let _ = app_handle.hide();

    #[cfg(target_os = "windows")]
    {
        // Give Windows a short moment to return focus to the previous app
        // before sending Ctrl+V.
        std::thread::sleep(std::time::Duration::from_millis(80));
        send_windows_paste_shortcut();
    }

    // 5. 模拟按下 Cmd+V (macOS)
    #[cfg(target_os = "macos")]
    {
        use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation, CGKeyCode};
        use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
        use std::time::{Duration, Instant};

        // 1. 获取主窗口并强制隐藏
        if let Some(window) = app_handle.get_webview_window("main") {
            let _ = window.hide();
        }
        let _ = app_handle.hide();

        // 2. 动态等待：轮询直到应用不再处于焦点状态，或超过 500ms
        // 这比写死 sleep 更科学，能适应不同性能的机器
        let start = Instant::now();
        while start.elapsed() < Duration::from_millis(500) {
            let is_focused = app_handle
                .get_webview_window("main")
                .map(|w| w.is_focused().unwrap_or(false))
                .unwrap_or(false);

            if !is_focused {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        // 3. 额外部署一个极短的缓冲（让 OS 完成最后的上下文切换）
        std::thread::sleep(Duration::from_millis(50));

        // 4. 使用 Core Graphics 发送原生键盘事件
        if let Ok(source) = CGEventSource::new(CGEventSourceStateID::CombinedSessionState) {
            let v_key = 9 as CGKeyCode; // 'V' 键

            // Cmd + V Down
            if let Ok(event_down) = CGEvent::new_keyboard_event(source.clone(), v_key, true) {
                event_down.set_flags(CGEventFlags::CGEventFlagCommand);
                event_down.post(CGEventTapLocation::HID);
            }

            // Cmd + V Up
            if let Ok(event_up) = CGEvent::new_keyboard_event(source, v_key, false) {
                // 松开时不需要 MaskCommand
                event_up.post(CGEventTapLocation::HID);
            }
        }
    }

    Ok(())
}

#[cfg(target_os = "windows")]
fn send_windows_paste_shortcut() {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VK_CONTROL, VK_V,
    };

    fn keyboard_input(vk: u16, key_up: bool) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: if key_up { KEYEVENTF_KEYUP } else { 0 },
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    let inputs = [
        keyboard_input(VK_CONTROL, false),
        keyboard_input(VK_V, false),
        keyboard_input(VK_V, true),
        keyboard_input(VK_CONTROL, true),
    ];

    unsafe {
        let _ = SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            std::mem::size_of::<INPUT>() as i32,
        );
    }
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
