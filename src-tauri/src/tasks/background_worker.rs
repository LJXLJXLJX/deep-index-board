use crate::dbm::{get_item_by_id_with_conn, upsert_vector, DbState};
use crate::task_manager::{HeavyTask, HeavyWorkState, TaskPriority};
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, Runtime};

/// 在后台常驻扫描数据库
/// 1. 查找所有类型为 image 或以图片后缀结尾的 file
/// 2. 检查向量是否存在，或磁盘文件是否已被修改
/// 3. 异步提交计算任务到 HeavyWorkManager
pub fn start_background_worker<R: Runtime>(app_handle: AppHandle<R>) {
    thread::spawn(move || {
        log::info!("Background worker started: Scanning for missing or stale embeddings...");

        loop {
            // 获取数据库状态
            let db_state = match app_handle.try_state::<DbState>() {
                Some(state) => state,
                None => {
                    thread::sleep(Duration::from_secs(1));
                    continue;
                }
            };

            // 获取 CLIP 模型路径
            #[cfg(target_os = "macos")]
            let image_model_resource = "resources/models/clips/vit-b-16.image.mlpackage";
            #[cfg(target_os = "windows")]
            let image_model_resource = "resources/models/clips/vit-b-16.img.int8.onnx";

            let model_path = match crate::utils::resolve_resource_path(&app_handle, image_model_resource) {
                Ok(path) => path,
                Err(e) => {
                    log::error!("Failed to resolve CLIP model path: {}", e);
                    thread::sleep(Duration::from_secs(30));
                    continue;
                }
            };

            // 1. 查询候选条目及其向量状态和记录的 mtime
            let candidates = {
                let conn = match db_state.conn.lock() {
                    Ok(conn) => conn,
                    Err(_) => {
                        thread::sleep(Duration::from_secs(1));
                        continue;
                    }
                };

                let mut stmt = match conn.prepare(
                    "SELECT id, content, mtime, 
                     (SELECT 1 FROM clipboard_vec WHERE id = clipboard.id) as has_vec 
                     FROM clipboard 
                     WHERE (type = 'image' OR (type = 'file' AND (
                        content LIKE '%.png' OR content LIKE '%.jpg' OR content LIKE '%.jpeg' OR 
                        content LIKE '%.gif' OR content LIKE '%.webp' OR content LIKE '%.bmp' OR 
                        content LIKE '%.heic' OR content LIKE '%.heif'
                     )))
                     ORDER BY timestamp DESC",
                ) {
                    Ok(stmt) => stmt,
                    Err(e) => {
                        log::error!("Failed to prepare candidate query: {}", e);
                        thread::sleep(Duration::from_secs(10));
                        continue;
                    }
                };

                let rows = stmt.query_map([], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, Option<i32>>(3)?.is_some(),
                    ))
                });

                match rows {
                    Ok(r) => r
                        .collect::<Result<Vec<(i64, String, i64, bool)>, rusqlite::Error>>()
                        .unwrap_or_default(),
                    Err(_) => Vec::new(),
                }
            };

            // 2. 筛选真正需要处理的条目
            let mut items_to_process: Vec<(i64, String, i64)> = Vec::new();
            for (id, content, db_mtime, has_vec) in candidates {
                let path = std::path::Path::new(&content);
                if !path.exists() {
                    continue;
                }

                let disk_mtime = crate::utils::get_mtime(path);
                if !has_vec || disk_mtime > db_mtime {
                    items_to_process.push((id, content, disk_mtime));
                }
            }

            if items_to_process.is_empty() {
                thread::sleep(Duration::from_secs(30));
                continue;
            }

            log::info!(
                "Found {} images/files to (re)process for embeddings.",
                items_to_process.len()
            );

            for (id, content, disk_mtime) in items_to_process {
                let image_path = std::path::PathBuf::from(content);
                if !image_path.exists() {
                    continue;
                }

                let task_manager_state = match app_handle.try_state::<HeavyWorkState>() {
                    Some(s) => s,
                    None => break,
                };
                let task_manager = task_manager_state.manager;

                let handle_clone = app_handle.clone();
                let model_path_clone = model_path.clone();

                let _ = task_manager.execute(
                    HeavyTask::ClipImageEmbedding {
                        image_path: image_path.clone(),
                        model_path: model_path_clone,
                        callback: Box::new(move |res| {
                            if let Ok(embedding) = res {
                                let db = handle_clone.state::<DbState>();
                                let conn = db.conn.lock().unwrap();

                                if let Err(e) = upsert_vector(&conn, id, &embedding) {
                                    log::error!("Failed to save embedding for id {}: {}", id, e);
                                } else {
                                    log::info!("Successfully saved embedding for id {}", id);

                                    let _ = conn.execute(
                                        "UPDATE clipboard SET mtime = ?1 WHERE id = ?2",
                                        rusqlite::params![disk_mtime, id],
                                    );

                                    if let Ok(Some(updated_item)) =
                                        get_item_by_id_with_conn(&conn, id)
                                    {
                                        let _ =
                                            handle_clone.emit("history-item-updated", updated_item);
                                    }
                                }
                            } else if let Err(e) = res {
                                log::error!("Failed to generate embedding for id {}: {}", id, e);
                            }
                        }),
                    },
                    TaskPriority::Low,
                );

                thread::sleep(Duration::from_millis(1000));
            }

            thread::sleep(Duration::from_secs(10));
        }
    });
}
