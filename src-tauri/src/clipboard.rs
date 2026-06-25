#[cfg(any(target_os = "macos", target_os = "windows"))]
use crate::dbm::update_content_text;
#[cfg(target_os = "macos")]
use crate::dbm::{clear_content_text, delete_vector, get_item_by_hash};
use crate::dbm::{get_path_by_hash, upsert_item, DbState};
#[cfg(any(target_os = "macos", target_os = "windows"))]
use crate::task_manager::{HeavyTask, HeavyWorkManager, TaskPriority};
use arboard::Clipboard;
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, Runtime};

use sha2::{Digest, Sha256};
use std::path::Path;

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn resolve_ocr_model_root<R: Runtime>(app_handle: &AppHandle<R>) -> Option<std::path::PathBuf> {
    app_handle
        .path()
        .resolve("resources/models/ocr", tauri::path::BaseDirectory::Resource)
        .ok()
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn submit_ocr_task<R: Runtime>(
    app_handle: &AppHandle<R>,
    image_path: String,
    mut item_to_emit: crate::dbm::HistoryItem,
) {
    let manager = HeavyWorkManager::global();
    let handle_clone = app_handle.clone();
    let db_path_clone = image_path.clone();
    let db_path_for_submit_log = image_path.clone();
    let model_root = resolve_ocr_model_root(app_handle);

    item_to_emit.content_text = Some("OCR 正在识别...".to_string());
    let _ = app_handle.emit("history-item-updated", item_to_emit.clone());

    let task = HeavyTask::Ocr {
        image_path: std::path::PathBuf::from(image_path),
        model_root,
        callback: Box::new(move |res| {
            let text = match res {
                Ok(text) => text,
                Err(err) => {
                    log::error!("OCR failed for {}: {}", db_path_clone, err);
                    format!("OCR 识别失败：{}", err)
                }
            };

            let db = handle_clone.state::<DbState>();
            let conn = db.conn.lock().unwrap();
            let _ = update_content_text(&conn, &db_path_clone, &text);
            let mut updated_item = item_to_emit;
            updated_item.content_text = Some(text);
            let _ = handle_clone.emit("history-item-updated", updated_item);
        }),
    };

    if let Err(err) = manager.execute(task, TaskPriority::Normal) {
        log::error!(
            "Failed to submit OCR task for {}: {}",
            db_path_for_submit_log,
            err
        );
    }
}

pub fn start_monitoring<R: Runtime>(app_handle: AppHandle<R>) {
    thread::spawn(move || {
        let mut clipboard = Clipboard::new().expect("Failed to initialize arboard");

        #[cfg(target_os = "macos")]
        {
            let mut last_count = crate::platform::clipboard_change_count();

            loop {
                let current_count = crate::platform::clipboard_change_count();
                if current_count != last_count {
                    last_count = current_count;
                    process_clipboard(&app_handle, &mut clipboard);
                }
                thread::sleep(Duration::from_millis(200));
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            let mut last_text = String::new();
            let mut last_image_hash = String::new();
            loop {
                if let Ok(text) = clipboard.get_text() {
                    if text != last_text && !text.is_empty() {
                        last_text = text.clone();
                        process_clipboard(&app_handle, &mut clipboard);
                    }
                }

                if let Some(png_bytes) = read_clipboard_image_as_png(&mut clipboard) {
                    let image_hash = calculate_hash(&png_bytes);
                    if image_hash != last_image_hash {
                        last_image_hash = image_hash;
                        process_clipboard_image(&app_handle, &png_bytes);
                    }
                }

                thread::sleep(Duration::from_millis(500));
            }
        }
    });
}

fn calculate_hash(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

#[cfg(target_os = "macos")]
fn is_image_file(path: &Path) -> bool {
    if let Some(ext) = path.extension() {
        if let Some(ext_str) = ext.to_str() {
            return matches!(
                ext_str.to_lowercase().as_str(),
                "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "heic" | "heif"
            );
        }
    }
    false
}

#[cfg(target_os = "macos")]
fn is_text_file(path: &Path) -> bool {
    if let Some(ext) = path.extension() {
        if let Some(ext_str) = ext.to_str() {
            return matches!(
                ext_str.to_lowercase().as_str(),
                "txt"
                    | "md"
                    | "json"
                    | "js"
                    | "ts"
                    | "tsx"
                    | "jsx"
                    | "rs"
                    | "py"
                    | "html"
                    | "css"
                    | "yml"
                    | "yaml"
                    | "xml"
                    | "log"
                    | "csv"
                    | "sql"
                    | "go"
                    | "c"
                    | "cpp"
                    | "h"
                    | "hpp"
            );
        }
    }
    false
}

#[cfg(target_os = "macos")]
fn extract_text_preview(path: &Path) -> Option<String> {
    use std::fs::File;
    use std::io::Read;

    let mut file = File::open(path).ok()?;
    let metadata = file.metadata().ok()?;
    if metadata.is_dir() {
        return None;
    }

    let limit = 256 * 1024; // 提取前 256KB
    let mut buffer = vec![0; limit];
    let n = file.read(&mut buffer).ok()?;
    buffer.truncate(n);

    // 工业级防御：如果在前 1KB 数据中发现了 null 字节 (\0)，则判定为二进制文件
    // 即使它的后缀是 .txt，我们也不提取预览内容
    let check_size = std::cmp::min(buffer.len(), 1024);
    if buffer[..check_size].contains(&0) {
        return None;
    }

    // 转换为字符串，忽略无效的 UTF-8 字符
    let text = String::from_utf8_lossy(&buffer).to_string();

    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn process_clipboard<R: Runtime>(app_handle: &AppHandle<R>, clipboard: &mut Clipboard) {
    let db = app_handle.state::<DbState>();
    let conn = db.conn.lock().unwrap();
    let app_dir = app_handle
        .path()
        .app_data_dir()
        .expect("Failed to get app data dir");

    // 0. 尝试获取文件 (macOS 专有)
    #[cfg(target_os = "macos")]
    {
        let file_paths = crate::platform::read_clipboard_file_paths();
        if !file_paths.is_empty() {
            let mut captured_files = false;
            for path_str in file_paths {
                let path = Path::new(&path_str);

                if path.exists() {
                    captured_files = true;
                    let item_type = if path.is_dir() { "directory" } else { "file" };

                    // 采取路径唯一性策略：同一路径永远对应同一个 ID
                    let hash = calculate_hash(path_str.as_bytes());

                    // 变更检测：如果是已存在的路径，且文件已被修改，则清除旧的预览/向量以触发更新
                    if let Ok(Some(existing)) = get_item_by_hash(&conn, &hash) {
                        if let Ok(meta) = path.metadata() {
                            use std::os::unix::fs::MetadataExt;
                            let _mtime = meta.mtime();

                            // 简单解析原有的 timestamp 进行大致判定
                            // 只要磁盘上的 mtime 显著新于记录的时间（考虑到时区，这里保留一定宽容度）
                            // 或者为了稳妥，直接强制清除旧解析结果（因为用户提到“覆盖之前的”）
                            let _ = clear_content_text(&conn, existing.id);
                            let _ = delete_vector(&conn, existing.id);
                        }
                    }

                    let mtime = crate::utils::get_mtime(path);
                    let source_app = crate::platform::get_frontmost_app();
                    if let Err(e) = upsert_item(
                        &conn,
                        &path_str,
                        item_type,
                        &hash,
                        mtime,
                        source_app.as_deref(),
                    ) {
                        eprintln!("Failed to save file path: {}", e);
                    } else {
                        // 无论哪种文件，尝试获取最新的历史记录项以更新 content_text
                        if let Ok(items) = crate::dbm::get_history(&conn, None, 1, None) {
                            if let Some(item) = items.first() {
                                if item.content_text.is_none() {
                                    if is_image_file(path) {
                                        // 对图片文件触发 OCR
                                        submit_ocr_task(app_handle, path_str.clone(), item.clone());
                                    } else if is_text_file(path) {
                                        // 对文本文件直接提取
                                        if let Some(text) = extract_text_preview(path) {
                                            let _ = update_content_text(&conn, &path_str, &text);
                                            let mut updated_item = item.clone();
                                            updated_item.content_text = Some(text);
                                            let _ = app_handle
                                                .emit("history-item-updated", updated_item);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if captured_files {
                if let Ok(items) = crate::dbm::get_history(&conn, None, 1, None) {
                    if let Some(item) = items.first() {
                        let _ = app_handle.emit("clipboard-updated", item);
                    }
                }
                return;
            }
        }
    }

    // 1. 尝试获取文本
    if let Ok(text) = clipboard.get_text() {
        if !text.is_empty() {
            let hash = calculate_hash(text.as_bytes());
            let source_app = crate::platform::get_frontmost_app();
            if let Err(e) = upsert_item(&conn, &text, "text", &hash, 0, source_app.as_deref()) {
                eprintln!("Failed to save clipboard text: {}", e);
            } else if let Ok(items) = crate::dbm::get_history(&conn, None, 1, None) {
                if let Some(item) = items.first() {
                    let _ = app_handle.emit("clipboard-updated", item);
                }
                return;
            }
        }
    }

    // 2. 尝试获取位图图片
    if let Some(png_bytes) = read_clipboard_image_as_png(clipboard) {
        process_image_data(app_handle, &conn, &png_bytes, &app_dir);
    }
}

#[cfg(not(target_os = "macos"))]
fn process_clipboard_image<R: Runtime>(app_handle: &AppHandle<R>, png_bytes: &[u8]) {
    let db = app_handle.state::<DbState>();
    let conn = db.conn.lock().unwrap();
    let app_dir = app_handle
        .path()
        .app_data_dir()
        .expect("Failed to get app data dir");

    process_image_data(app_handle, &conn, png_bytes, &app_dir);
}

fn read_clipboard_image_as_png(clipboard: &mut Clipboard) -> Option<Vec<u8>> {
    let image = clipboard.get_image().ok()?;

    // arboard 返回的是原始 RGBA 位图，先编码成 PNG 后再交给统一图片保存路径。
    use image::{ImageBuffer, Rgba};
    let buffer: ImageBuffer<Rgba<u8>, _> = ImageBuffer::from_raw(
        image.width as u32,
        image.height as u32,
        image.bytes.into_owned(),
    )?;

    let mut png_bytes = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut png_bytes);
    if let Err(e) =
        image::DynamicImage::ImageRgba8(buffer).write_to(&mut cursor, image::ImageFormat::Png)
    {
        eprintln!("Failed to encode clipboard bitmap to PNG: {}", e);
        return None;
    }

    Some(png_bytes)
}

fn process_image_data<R: Runtime>(
    app_handle: &AppHandle<R>,
    conn: &rusqlite::Connection,
    bytes: &[u8],
    app_dir: &std::path::Path,
) {
    let hash = calculate_hash(bytes);

    // 先检查哈希是否存在
    if let Ok(Some(existing_path)) = get_path_by_hash(conn, &hash) {
        // 已存在，保持原逻辑：更新时间戳
        let source_app = crate::platform::get_frontmost_app();
        if let Err(e) = upsert_item(
            conn,
            &existing_path,
            "image",
            &hash,
            crate::utils::get_mtime(Path::new(&existing_path)),
            source_app.as_deref(),
        ) {
            eprintln!("Failed to update duplicate image timestamp: {}", e);
        } else if let Ok(items) = crate::dbm::get_history(conn, None, 1, None) {
            if let Some(item) = items.first() {
                let item_to_emit = item.clone();
                let _ = app_handle.emit("clipboard-updated", &item_to_emit);

                // 如果该记录还没有 OCR 文本，则触发 OCR
                #[cfg(any(target_os = "macos", target_os = "windows"))]
                if item_to_emit.content_text.is_none() {
                    submit_ocr_task(app_handle, existing_path, item_to_emit);
                }
            }
        }
        return;
    }

    // 不存在，尝试解析图片并保存
    let img = image::load_from_memory(bytes).ok();

    if let Some(dynamic_img) = img {
        let images_dir = crate::dbm::get_images_dir(app_dir);
        let id = uuid::Uuid::new_v4().to_string();
        let file_name = format!("{}.png", id);
        let file_path = images_dir.join(&file_name);

        if let Err(e) = dynamic_img.save(&file_path) {
            eprintln!("Failed to save image file: {}", e);
        } else {
            let db_path = file_path.to_string_lossy().to_string();
            let source_app = crate::platform::get_frontmost_app();
            if let Err(e) = upsert_item(
                conn,
                &db_path,
                "image",
                &hash,
                crate::utils::get_mtime(&file_path),
                source_app.as_deref(),
            ) {
                eprintln!("Failed to save image path to DB: {}", e);
            } else if let Ok(items) = crate::dbm::get_history(conn, None, 1, None) {
                if let Some(item) = items.first() {
                    let item_to_emit = item.clone();
                    let _ = app_handle.emit("clipboard-updated", &item_to_emit);

                    // 提交 OCR 任务
                    #[cfg(any(target_os = "macos", target_os = "windows"))]
                    {
                        submit_ocr_task(app_handle, db_path, item_to_emit);
                    }
                }
            }
        }
    }
}
