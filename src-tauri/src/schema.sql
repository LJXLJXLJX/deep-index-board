CREATE TABLE IF NOT EXISTS clipboard (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    content TEXT NOT NULL,
    type TEXT DEFAULT 'text',
    content_hash TEXT UNIQUE, -- 唯一哈希，用于去重
    content_text TEXT,       -- 提取的文本内容（用于搜索：OCR文字、文件提取的正文等）
    mtime INTEGER DEFAULT 0, -- 文件修改时间戳
    source_app TEXT,         -- 来源应用（如 com.apple.Safari）
    is_favorite INTEGER NOT NULL DEFAULT 0, -- 是否收藏
    timestamp DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_clipboard_hash ON clipboard(content_hash);
CREATE INDEX IF NOT EXISTS idx_clipboard_timestamp ON clipboard(timestamp);
CREATE INDEX IF NOT EXISTS idx_clipboard_favorite_timestamp ON clipboard(is_favorite, timestamp);

-- FTS5 Virtual Table for optimized searching
CREATE VIRTUAL TABLE IF NOT EXISTS clipboard_fts USING fts5(
    content,
    content_text,
    content='clipboard',
    content_rowid='id'
);

-- Triggers to keep FTS in sync
CREATE TRIGGER IF NOT EXISTS clipboard_ai AFTER INSERT ON clipboard BEGIN
  INSERT INTO clipboard_fts(rowid, content, content_text) VALUES (new.id, new.content, new.content_text);
END;

CREATE TRIGGER IF NOT EXISTS clipboard_ad AFTER DELETE ON clipboard BEGIN
  INSERT INTO clipboard_fts(clipboard_fts, rowid, content, content_text) VALUES('delete', old.id, old.content, old.content_text);
END;

CREATE TRIGGER IF NOT EXISTS clipboard_au AFTER UPDATE ON clipboard BEGIN
  INSERT INTO clipboard_fts(clipboard_fts, rowid, content, content_text) VALUES('delete', old.id, old.content, old.content_text);
  INSERT INTO clipboard_fts(rowid, content, content_text) VALUES (new.id, new.content, new.content_text);
END;

-- Vector table for CLIP embeddings
-- dimension depends on the model, 512 is common for CLIP ViT-B/16
CREATE VIRTUAL TABLE IF NOT EXISTS clipboard_vec USING vec0(
  id INTEGER PRIMARY KEY,
  embedding FLOAT[512]
);
