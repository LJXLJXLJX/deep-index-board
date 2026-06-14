import { useState, useCallback, useEffect } from "react";
import { Virtuoso } from "react-virtuoso";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

// 定义列表项的数据结构，需与 Rust 端 HistoryItem 匹配
export interface HistoryItem {
  id: number;
  content: string;
  type: "text" | "image" | "file" | "directory";
  content_text?: string | null;
  timestamp: string;
  has_vec: boolean;
  mtime: number;
  source_app?: string;
}

interface HistoryListProps {
  onHover: (item: HistoryItem | null) => void;
  onClear: () => void;
}

const getFileName = (path: string) => {
  return path.split(/[/\\]/).pop() || path;
};

const getAppName = (bundleId: string | undefined) => {
  if (!bundleId) return null;
  const parts = bundleId.split(".");
  // 处理 com.apple.Safari -> Safari
  const lastPart = parts[parts.length - 1];
  // 首字母大写
  return lastPart.charAt(0).toUpperCase() + lastPart.slice(1);
};

const isImageFile = (path: string) => {
  const ext = path.split(".").pop()?.toLowerCase();
  return (
    ext &&
    ["png", "jpg", "jpeg", "gif", "webp", "bmp", "heic", "heif"].includes(ext)
  );
};

const formatTime = (ts: string) => {
  try {
    // ts 格式通常为 "2025-12-29 14:50:48" (SQLite UTC)
    // 转换为本地 Date 对象
    const date = new Date(ts.replace(" ", "T") + "Z");
    const now = new Date();

    const diffInSeconds = Math.floor((now.getTime() - date.getTime()) / 1000);

    // 今天：显示具体时间
    if (now.toDateString() === date.toDateString()) {
      if (diffInSeconds < 60) return "刚刚";
      if (diffInSeconds < 3600)
        return `${Math.floor(diffInSeconds / 60)}分钟前`;
      const parts = ts.split(" ");
      return parts.length > 1 ? parts[1] : ts;
    }

    // 昨天
    const yesterday = new Date(now);
    yesterday.setDate(now.getDate() - 1);
    if (yesterday.toDateString() === date.toDateString()) {
      return "昨天";
    }

    // 一周内
    const diffInDays = Math.floor(diffInSeconds / 86400);
    if (diffInDays < 7) {
      return `${diffInDays}天前`;
    }

    // 更久以前
    return `${date.getMonth() + 1}-${date.getDate()}`;
  } catch {
    return ts;
  }
};

export function HistoryList({ onHover, onClear }: HistoryListProps) {
  const [items, setItems] = useState<HistoryItem[]>([]);
  const [hasMore, setHasMore] = useState(true);
  const [query, setQuery] = useState("");
  const [isSemantic, setIsSemantic] = useState(false);
  const [isSearching, setIsSearching] = useState(false);

  // 初始加载及监听更新
  useEffect(() => {
    // 加载第一页
    loadMore(null, query);

    // 监听 backend 发来的新剪贴板事件
    const unlistenClipboard = listen<HistoryItem>(
      "clipboard-updated",
      (event) => {
        const newItem = event.payload;

        if (
          query &&
          newItem.type === "text" &&
          !newItem.content.includes(query)
        ) {
          return;
        }

        setItems((prev) => {
          const filtered = prev.filter((i) => i.id !== newItem.id);
          return [newItem, ...filtered];
        });
      }
    );

    // 监听任务完成后的异步更新（如 OCR 结果）
    const unlistenUpdate = listen<HistoryItem>(
      "history-item-updated",
      (event) => {
        const updatedItem = event.payload;
        setItems((prev) =>
          prev.map((item) => (item.id === updatedItem.id ? updatedItem : item))
        );
      }
    );

    const unlistenClear = listen("history-cleared", () => {
      setItems([]);
      setHasMore(false);
      onHover(null);
      onClear();
    });

    return () => {
      unlistenClipboard.then((f) => f());
      unlistenUpdate.then((f) => f());
      unlistenClear.then((f) => f());
    };
  }, [query, isSemantic, onClear, onHover]); // 当 query 或 isSemantic 改变时重新加载

  const loadMore = useCallback(
    async (lastTimestamp: string | null, searchQuery: string) => {
      if (isSearching) return;

      try {
        if (isSemantic) {
          // 语义搜索目前不分页，直接取 Top 50
          if (lastTimestamp !== null) return;
          if (!searchQuery.trim()) {
            setItems([]);
            return;
          }

          setIsSearching(true);
          const results: HistoryItem[] = await invoke("get_history_semantic", {
            query: searchQuery,
            limit: 50,
          });
          setItems(results);
          setHasMore(false);
          setIsSearching(false);
        } else {
          const batch: HistoryItem[] = await invoke("get_history", {
            lastTimestamp,
            limit: 50,
            query: searchQuery || null,
          });

          if (batch.length < 50) {
            setHasMore(false);
          } else {
            setHasMore(true);
          }

          setItems((prev) => {
            if (lastTimestamp === null) return batch;
            const existingIds = new Set(prev.map((i) => i.id));
            const uniqueNew = batch.filter((i) => !existingIds.has(i.id));
            return [...prev, ...uniqueNew];
          });
        }
      } catch (error) {
        console.error("Failed to load history:", error);
        setIsSearching(false);
      }
    },
    [isSemantic, isSearching]
  );

  const handleEndReached = () => {
    if (!hasMore || items.length === 0) return;
    const lastItem = items[items.length - 1];
    loadMore(lastItem.timestamp, query);
  };

  const handleItemClick = async (item: HistoryItem) => {
    try {
      await invoke("paste_item", { id: item.id });
    } catch (error) {
      console.error("Failed to paste item:", error);
    }
  };

  const handleClearHistory = async () => {
    try {
      await invoke("clear_history");
      setItems([]);
      setHasMore(false);
      onHover(null);
      onClear();
    } catch (error) {
      console.error("Failed to clear history:", error);
    }
  };

  return (
    <div
      className="history-list-container"
      style={{ height: "100%", display: "flex", flexDirection: "column" }}
    >
      <div className="search-box-wrapper">
        <div className="search-input-container">
          <input
            type="text"
            className="search-input"
            placeholder={
              isSemantic ? "语义搜索 (如: 蓝色天空中的白云)" : "关键词搜索..."
            }
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
          <button
            className={`search-mode-toggle ${isSemantic ? "active" : ""}`}
            onClick={() => setIsSemantic(!isSemantic)}
            title={isSemantic ? "切换到精确搜索" : "切换到语义搜索 (文搜图)"}
          >
            {isSemantic ? "✨" : "🔍"}
          </button>
          <button
            className="clear-history-button"
            onClick={handleClearHistory}
            title="删除所有剪贴板记录和已保存的图片文件"
            disabled={items.length === 0}
          >
            清空
          </button>
        </div>
        {isSearching && (
          <div className="searching-indicator">AI 正在匹配中...</div>
        )}
      </div>
      <div className="history-list" style={{ flex: 1, overflow: "hidden" }}>
        <Virtuoso
          style={{ height: "100%" }}
          data={items}
          endReached={handleEndReached}
          increaseViewportBy={200}
          itemContent={(_index, item) => (
            <div
              className="history-item"
              onClick={() => handleItemClick(item)}
              onMouseEnter={() => onHover(item)}
              style={{ cursor: "pointer" }}
            >
              {item.type === "text" ? (
                <div
                  style={{
                    display: "flex",
                    alignItems: "center",
                    width: "100%",
                    gap: "8px",
                  }}
                >
                  <span className="item-content" style={{ flex: 1 }}>
                    {item.content}
                  </span>
                  <div className="item-meta">
                    {item.source_app && (
                      <span className="item-source-tag">
                        {getAppName(item.source_app)}
                      </span>
                    )}
                    <span className="item-time-hint">
                      {formatTime(item.timestamp)}
                    </span>
                  </div>
                </div>
              ) : item.type === "image" ||
                (item.type === "file" && isImageFile(item.content)) ? (
                <div className="item-image-wrapper">
                  {item.type === "file" && (
                    <span className="file-type-overlay-icon">📄</span>
                  )}
                  <img
                    src={convertFileSrc(item.content)}
                    alt="clipboard content"
                    className="item-image"
                    loading="lazy"
                  />
                  <div className="item-image-meta">
                    {item.source_app && (
                      <span className="item-source-tag">
                        {getAppName(item.source_app)}
                      </span>
                    )}
                    <span className="item-time-hint">
                      {formatTime(item.timestamp)}
                    </span>
                  </div>
                  {item.type === "file" && (
                    <span className="file-name-overlay">
                      {getFileName(item.content)}
                    </span>
                  )}
                  {item.has_vec && (
                    <div
                      className="item-vector-indicator"
                      title="已生成向量，支持语义搜索"
                    >
                      <span className="vector-icon">✨</span>
                    </div>
                  )}
                </div>
              ) : (
                <div className="item-file-wrapper">
                  <span className="file-icon">
                    {item.type === "directory" ? "📁" : "📄"}
                  </span>
                  <span className="file-name">{getFileName(item.content)}</span>
                  <div className="item-meta">
                    {item.source_app && (
                      <span className="item-source-tag">
                        {getAppName(item.source_app)}
                      </span>
                    )}
                    <span className="item-time-hint">
                      {formatTime(item.timestamp)}
                    </span>
                  </div>
                </div>
              )}
            </div>
          )}
        />
      </div>
    </div>
  );
}
