use rusqlite::Connection;
use std::sync::Mutex;

const SQLITE_FILE_NAME: &str = "clipboard.db";

// 定义一个包装类，处理多线程同步
pub struct DbState {
    pub(crate) conn: Mutex<Connection>,
}

pub fn connect_to_main_clipboard_db(app_dir: std::path::PathBuf) -> Connection {
    if !app_dir.exists() {
        std::fs::create_dir_all(&app_dir).expect("failed to create app data dir");
    }
    let db_path = app_dir.join(SQLITE_FILE_NAME);

    // 加载 sqlite-vec 扩展
    unsafe {
        let extension = std::mem::transmute::<
            *const (),
            unsafe extern "C" fn(
                *mut rusqlite::ffi::sqlite3,
                *mut *mut i8,
                *const rusqlite::ffi::sqlite3_api_routines,
            ) -> i32,
        >(sqlite_vec::sqlite3_vec_init as *const ());
        let _ = rusqlite::ffi::sqlite3_auto_extension(Some(extension));
    }

    let conn = Connection::open(db_path).expect("failed to open database");

    // 可以在这里初始化表结构
    conn.execute_batch(include_str!("schema.sql"))
        .expect("failed to initialize database schema");

    // FTS5 初始同步：如果 FTS 表为空但主表有数据，进行同步
    let needs_sync: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM clipboard WHERE id NOT IN (SELECT rowid FROM clipboard_fts) LIMIT 1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(0)
        > 0;

    if needs_sync {
        log::info!("Manager: Syncing FTS index...");
        let _ = conn.execute(
            "INSERT INTO clipboard_fts(rowid, content, content_text) SELECT id, content, content_text FROM clipboard",
            [],
        );
    }

    conn
}

pub fn upsert_item(
    conn: &Connection,
    content: &str,
    item_type: &str,
    hash: &str,
    mtime: i64,
    source_app: Option<&str>,
) -> rusqlite::Result<i64> {
    conn.query_row(
        "INSERT INTO clipboard (content, type, content_hash, mtime, source_app) 
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(content_hash) DO UPDATE SET timestamp = CURRENT_TIMESTAMP, mtime = ?4, source_app = ?5
         RETURNING id",
        rusqlite::params![content, item_type, hash, mtime, source_app],
        |row| row.get(0),
    )
}
pub fn update_content_text(conn: &Connection, content: &str, text: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE clipboard SET content_text = ?1 WHERE content = ?2",
        [text, content],
    )?;
    Ok(())
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn upsert_vector(conn: &Connection, id: i64, embedding: &[f32]) -> rusqlite::Result<()> {
    // sqlite-vec requires the embedding to be a blob
    // We can use vec_f32() or just pass the vector as a blob if it's already in the right format.
    // However, the most direct way is to use the vec0 virtual table.

    let embedding_blob = unsafe {
        std::slice::from_raw_parts(
            embedding.as_ptr() as *const u8,
            std::mem::size_of_val(embedding),
        )
    };

    conn.execute("DELETE FROM clipboard_vec WHERE id = ?1", [id])?;
    conn.execute(
        "INSERT INTO clipboard_vec(id, embedding) VALUES (?1, ?2)",
        rusqlite::params![id, embedding_blob],
    )?;
    Ok(())
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn delete_vector(conn: &Connection, id: i64) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM clipboard_vec WHERE id = ?1", [id])?;
    Ok(())
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn clear_content_text(conn: &Connection, id: i64) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE clipboard SET content_text = NULL WHERE id = ?1",
        [id],
    )?;
    Ok(())
}

pub fn clear_all_history(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM clipboard_vec", [])?;
    conn.execute("DELETE FROM clipboard", [])?;
    Ok(())
}

#[allow(dead_code)]
pub fn get_id_by_hash(conn: &Connection, hash: &str) -> rusqlite::Result<Option<i64>> {
    conn.query_row(
        "SELECT id FROM clipboard WHERE content_hash = ?1",
        [hash],
        |row| row.get(0),
    )
    .optional()
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn get_item_by_hash(conn: &Connection, hash: &str) -> rusqlite::Result<Option<HistoryItem>> {
    conn.query_row(
        "SELECT id, content, type, content_text, timestamp, 
         (SELECT 1 FROM clipboard_vec WHERE id = clipboard.id) as has_vec,
         mtime, source_app
         FROM clipboard WHERE content_hash = ?1",
        [hash],
        |row| {
            Ok(HistoryItem {
                id: row.get(0)?,
                content: row.get(1)?,
                r#type: row.get(2)?,
                content_text: row.get(3)?,
                timestamp: row.get(4)?,
                has_vec: row.get::<_, Option<i32>>(5)?.is_some(),
                mtime: row.get(6)?,
                source_app: row.get(7)?,
            })
        },
    )
    .optional()
}

pub fn get_path_by_hash(conn: &Connection, hash: &str) -> rusqlite::Result<Option<String>> {
    conn.query_row(
        "SELECT content FROM clipboard WHERE content_hash = ?1 AND type = 'image'",
        [hash],
        |row| row.get(0),
    )
    .optional()
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct HistoryItem {
    pub id: i64,
    pub content: String,
    pub r#type: String,
    pub content_text: Option<String>,
    pub timestamp: String,
    pub has_vec: bool,
    pub mtime: i64,
    pub source_app: Option<String>,
}

pub fn get_history(
    conn: &Connection,
    last_timestamp: Option<String>,
    limit: usize,
    query: Option<String>,
) -> rusqlite::Result<Vec<HistoryItem>> {
    let mut sql = "SELECT id, content, type, content_text, timestamp,
         (SELECT 1 FROM clipboard_vec WHERE id = clipboard.id) as has_vec,
         mtime, source_app
         FROM clipboard WHERE 1=1"
        .to_string();
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(ref q) = query {
        if !q.is_empty() {
            // 采用混合搜索策略：
            // 1. FTS5 MATCH 用于高性能单词/短语匹配 (支持前缀匹配 *)
            // 2. LIKE 用于万能模糊匹配 (解决中文分词瓶颈)
            sql.push_str(
                " AND (
                id IN (SELECT rowid FROM clipboard_fts WHERE clipboard_fts MATCH ?) 
                OR content LIKE ? 
                OR content_text LIKE ?
            )",
            );

            // FTS 匹配模式：对用户输入进行清理。注意 FTS5 不支持前置 * 通配符，但支持末尾 *
            let fts_query = format!("\"{}\"*", q.replace("\"", " ").trim());
            let like_query = format!("%{}%", q);

            params.push(Box::new(fts_query));
            params.push(Box::new(like_query.clone()));
            params.push(Box::new(like_query));
        }
    }

    if let Some(ts) = last_timestamp {
        sql.push_str(" AND timestamp < ?");
        params.push(Box::new(ts));
    }

    sql.push_str(" ORDER BY timestamp DESC LIMIT ?");
    params.push(Box::new(limit as i64));

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(params), |row| {
        Ok(HistoryItem {
            id: row.get(0)?,
            content: row.get(1)?,
            r#type: row.get(2)?,
            content_text: row.get(3)?,
            timestamp: row.get(4)?,
            has_vec: row.get::<_, Option<i32>>(5)?.is_some(),
            mtime: row.get(6)?,
            source_app: row.get(7)?,
        })
    })?;

    rows.collect::<rusqlite::Result<Vec<_>>>()
}

pub fn get_history_semantic(
    conn: &Connection,
    embedding: &[f32],
    limit: usize,
) -> rusqlite::Result<Vec<HistoryItem>> {
    // 1. 将 f32 向量转换为字节，以便 sqlite-vec 处理
    let embedding_blob = unsafe {
        std::slice::from_raw_parts(
            embedding.as_ptr() as *const u8,
            std::mem::size_of_val(embedding),
        )
    };

    // 2. 向量检索：使用 KNN 查询
    // 我们将 clipboard_vec 和 clipboard JOIN 起来，获取完整信息
    // sqlite-vec 的 MATCH 返回结果默认按距离排序 (距离越小越相似)
    let sql = "SELECT 
            c.id, c.content, c.type, c.content_text, c.timestamp, 
            1 as has_vec, c.mtime, c.source_app, v.distance
         FROM clipboard c
         JOIN clipboard_vec v ON c.id = v.id
         WHERE v.embedding MATCH ?1 AND v.k = ?2
         ORDER BY v.distance ASC";

    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(rusqlite::params![embedding_blob, limit as i64], |row| {
        Ok(HistoryItem {
            id: row.get(0)?,
            content: row.get(1)?,
            r#type: row.get(2)?,
            content_text: row.get(3)?,
            timestamp: row.get(4)?,
            has_vec: row.get::<_, i32>(5)? == 1,
            mtime: row.get(6)?,
            source_app: row.get(7)?,
        })
    })?;

    rows.collect::<rusqlite::Result<Vec<_>>>()
}

pub fn get_item_by_id(conn: &Connection, id: i64) -> rusqlite::Result<Option<HistoryItem>> {
    get_item_by_id_with_conn(conn, id)
}

pub fn get_item_by_id_with_conn(
    conn: &Connection,
    id: i64,
) -> rusqlite::Result<Option<HistoryItem>> {
    conn.query_row(
        "SELECT id, content, type, content_text, timestamp,
         (SELECT 1 FROM clipboard_vec WHERE id = clipboard.id) as has_vec,
         mtime, source_app
         FROM clipboard WHERE id = ?1",
        [id],
        |row| {
            Ok(HistoryItem {
                id: row.get(0)?,
                content: row.get(1)?,
                r#type: row.get(2)?,
                content_text: row.get(3)?,
                timestamp: row.get(4)?,
                has_vec: row.get::<_, Option<i32>>(5)?.is_some(),
                mtime: row.get(6)?,
                source_app: row.get(7)?,
            })
        },
    )
    .optional()
}

pub fn get_images_dir(app_dir: &std::path::Path) -> std::path::PathBuf {
    let images_dir = app_dir.join("images");
    if !images_dir.exists() {
        std::fs::create_dir_all(&images_dir).expect("failed to create images dir");
    }
    images_dir
}

use rusqlite::OptionalExtension;
