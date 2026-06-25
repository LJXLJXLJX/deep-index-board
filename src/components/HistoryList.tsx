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
  is_favorite: boolean;
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
  const [historyItems, setHistoryItems] = useState<HistoryItem[]>([]);
  const [favoriteItems, setFavoriteItems] = useState<HistoryItem[]>([]);
  const [hasMoreHistory, setHasMoreHistory] = useState(true);
  const [hasMoreFavorites, setHasMoreFavorites] = useState(true);
  const [query, setQuery] = useState("");
  const [isSemantic, setIsSemantic] = useState(false);
  const [isSearching, setIsSearching] = useState(false);
  const [favoritesExpanded, setFavoritesExpanded] = useState(false);
  const [favoritesHeight, setFavoritesHeight] = useState(160);

  const itemMatchesQuery = useCallback((item: HistoryItem, searchQuery: string) => {
    const trimmed = searchQuery.trim();
    if (!trimmed || isSemantic) return true;
    return (
      item.content.includes(trimmed) ||
      (item.content_text?.includes(trimmed) ?? false)
    );
  }, [isSemantic]);

  const fetchItems = useCallback(
    async (
      favoritesOnly: boolean,
      lastTimestamp: string | null,
      searchQuery: string
    ) => {
      if (isSemantic && searchQuery.trim()) {
        if (lastTimestamp !== null) return [];
        return invoke<HistoryItem[]>("get_history_semantic", {
          query: searchQuery,
          limit: 50,
          favoritesOnly,
        });
      }

      return invoke<HistoryItem[]>("get_history", {
        lastTimestamp,
        limit: 50,
        query: searchQuery || null,
        favoritesOnly,
      });
    },
    [isSemantic]
  );

  const loadInitial = useCallback(
    async (searchQuery: string) => {
      const semanticSearching = isSemantic && searchQuery.trim();
      try {
        if (semanticSearching) setIsSearching(true);
        const [favorites, history] = await Promise.all([
          fetchItems(true, null, searchQuery),
          fetchItems(false, null, searchQuery),
        ]);

        setFavoriteItems(favorites);
        setHistoryItems(history);
        setHasMoreFavorites(!semanticSearching && favorites.length >= 50);
        setHasMoreHistory(!semanticSearching && history.length >= 50);
      } catch (error) {
        console.error("Failed to load history:", error);
      } finally {
        setIsSearching(false);
      }
    },
    [fetchItems, isSemantic]
  );

  // 初始加载及监听更新
  useEffect(() => {
    loadInitial(query);

    const applyItemUpdate = (updatedItem: HistoryItem, expandFavorite: boolean) => {
      if (!itemMatchesQuery(updatedItem, query)) {
        setFavoriteItems((prev) =>
          prev.filter((item) => item.id !== updatedItem.id)
        );
        setHistoryItems((prev) =>
          prev.filter((item) => item.id !== updatedItem.id)
        );
        return;
      }

      if (updatedItem.is_favorite) {
        if (expandFavorite) setFavoritesExpanded(true);
        setFavoriteItems((prev) => {
          const filtered = prev.filter((item) => item.id !== updatedItem.id);
          return [updatedItem, ...filtered];
        });
        setHistoryItems((prev) =>
          prev.filter((item) => item.id !== updatedItem.id)
        );
        return;
      }

      setFavoriteItems((prev) =>
        prev.filter((item) => item.id !== updatedItem.id)
      );
      setHistoryItems((prev) => {
        const filtered = prev.filter((item) => item.id !== updatedItem.id);
        return [updatedItem, ...filtered];
      });
    };

    const unlistenClipboard = listen<HistoryItem>(
      "clipboard-updated",
      (event) => {
        applyItemUpdate(event.payload, event.payload.is_favorite);
      }
    );

    const unlistenUpdate = listen<HistoryItem>(
      "history-item-updated",
      (event) => {
        applyItemUpdate(event.payload, event.payload.is_favorite);
      }
    );

    const unlistenClear = listen("history-cleared", () => {
      loadInitial(query);
      onHover(null);
      onClear();
    });

    const unlistenDeleted = listen<number>("history-item-deleted", (event) => {
      const deletedId = event.payload;
      setFavoriteItems((prev) => prev.filter((item) => item.id !== deletedId));
      setHistoryItems((prev) => prev.filter((item) => item.id !== deletedId));
      onHover(null);
    });

    return () => {
      unlistenClipboard.then((f) => f());
      unlistenUpdate.then((f) => f());
      unlistenClear.then((f) => f());
      unlistenDeleted.then((f) => f());
    };
  }, [query, itemMatchesQuery, loadInitial, onClear, onHover]);

  const loadMoreHistory = useCallback(async () => {
    if (!hasMoreHistory || historyItems.length === 0 || isSearching) return;
    const lastItem = historyItems[historyItems.length - 1];

    try {
      const batch = await fetchItems(false, lastItem.timestamp, query);
      setHasMoreHistory(batch.length >= 50);
      setHistoryItems((prev) => {
        const existingIds = new Set(prev.map((item) => item.id));
        const uniqueNew = batch.filter((item) => !existingIds.has(item.id));
        return [...prev, ...uniqueNew];
      });
    } catch (error) {
      console.error("Failed to load more history:", error);
    }
  }, [fetchItems, hasMoreHistory, historyItems, isSearching, query]);

  const loadMoreFavorites = useCallback(async () => {
    if (!hasMoreFavorites || favoriteItems.length === 0 || isSearching) return;
    const lastItem = favoriteItems[favoriteItems.length - 1];

    try {
      const batch = await fetchItems(true, lastItem.timestamp, query);
      setHasMoreFavorites(batch.length >= 50);
      setFavoriteItems((prev) => {
        const existingIds = new Set(prev.map((item) => item.id));
        const uniqueNew = batch.filter((item) => !existingIds.has(item.id));
        return [...prev, ...uniqueNew];
      });
    } catch (error) {
      console.error("Failed to load more favorites:", error);
    }
  }, [favoriteItems, fetchItems, hasMoreFavorites, isSearching, query]);

  const handleItemClick = async (item: HistoryItem) => {
    try {
      await invoke("paste_item", { id: item.id });
    } catch (error) {
      console.error("Failed to paste item:", error);
    }
  };

  const handleClearHistory = async () => {
    try {
      await invoke("clear_history", { favoritesOnly: false });
    } catch (error) {
      console.error("Failed to clear history:", error);
    }
  };

  const handleFavoriteToggle = async (
    event: React.MouseEvent<HTMLButtonElement>,
    item: HistoryItem
  ) => {
    event.stopPropagation();

    try {
      const updatedItem: HistoryItem = await invoke("set_favorite", {
        id: item.id,
        isFavorite: !item.is_favorite,
      });

      if (updatedItem.is_favorite) {
        setFavoritesExpanded(true);
        setFavoriteItems((prev) => {
          const filtered = prev.filter((current) => current.id !== updatedItem.id);
          return [updatedItem, ...filtered];
        });
        setHistoryItems((prev) =>
          prev.filter((current) => current.id !== updatedItem.id)
        );
      } else {
        setFavoriteItems((prev) =>
          prev.filter((current) => current.id !== updatedItem.id)
        );
        setHistoryItems((prev) => [updatedItem, ...prev]);
      }
    } catch (error) {
      console.error("Failed to toggle favorite:", error);
    }
  };

  const handleUnfavoriteAll = async (
    event: React.MouseEvent<HTMLButtonElement>
  ) => {
    event.stopPropagation();
    if (favoriteItems.length === 0) return;

    try {
      await invoke("unfavorite_all");
      await loadInitial(query);
      setFavoritesExpanded(false);
    } catch (error) {
      console.error("Failed to unfavorite all:", error);
    }
  };

  const handleDeleteItem = async (
    event: React.MouseEvent<HTMLButtonElement>,
    item: HistoryItem
  ) => {
    event.stopPropagation();

    try {
      await invoke("delete_item", { id: item.id });
    } catch (error) {
      console.error("Failed to delete item:", error);
    }
  };

  const handleFavoritesResizeStart = (
    event: React.MouseEvent<HTMLDivElement>
  ) => {
    event.preventDefault();
    const startY = event.clientY;
    const startHeight = favoritesHeight;

    const handleMouseMove = (moveEvent: MouseEvent) => {
      const nextHeight = Math.min(
        360,
        Math.max(90, startHeight + moveEvent.clientY - startY)
      );
      setFavoritesHeight(nextHeight);
    };

    const handleMouseUp = () => {
      window.removeEventListener("mousemove", handleMouseMove);
      window.removeEventListener("mouseup", handleMouseUp);
    };

    window.addEventListener("mousemove", handleMouseMove);
    window.addEventListener("mouseup", handleMouseUp);
  };

  const renderFavoriteButton = (item: HistoryItem) => (
    <button
      className={`favorite-button ${item.is_favorite ? "active" : ""}`}
      onClick={(event) => handleFavoriteToggle(event, item)}
      title={item.is_favorite ? "取消收藏" : "加入收藏"}
      aria-label={item.is_favorite ? "取消收藏" : "加入收藏"}
    >
      {item.is_favorite ? "★" : "☆"}
    </button>
  );

  const renderDeleteButton = (item: HistoryItem) => (
    <button
      className="delete-item-button"
      onClick={(event) => handleDeleteItem(event, item)}
      title="删除此条目"
      aria-label="删除此条目"
    >
      ×
    </button>
  );

  const renderItemActions = (item: HistoryItem) => (
    <div className="item-actions">
      {renderFavoriteButton(item)}
      {renderDeleteButton(item)}
    </div>
  );

  const renderItemMeta = (item: HistoryItem) => (
    <div className="item-meta">
      {item.source_app && (
        <span className="item-source-tag">{getAppName(item.source_app)}</span>
      )}
      <span className="item-time-hint">{formatTime(item.timestamp)}</span>
    </div>
  );

  const renderItem = (_index: number, item: HistoryItem) => (
    <div
      className="history-item"
      onClick={() => handleItemClick(item)}
      onMouseEnter={() => onHover(item)}
      style={{ cursor: "pointer" }}
    >
      <div className="history-item-row">
        <div className="history-item-main">
          {item.type === "text" ? (
            <span className="item-content">{item.content}</span>
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
            </div>
          )}
        </div>
        <div className="history-item-side">
          {renderItemMeta(item)}
          {renderItemActions(item)}
        </div>
      </div>
    </div>
  );

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
            title="删除未收藏的剪贴板记录和对应图片文件"
            disabled={historyItems.length === 0}
          >
            清空
          </button>
        </div>
        {isSearching && (
          <div className="searching-indicator">AI 正在匹配中...</div>
        )}
      </div>
      <div className="history-content">
        <section
          className={`favorites-section ${
            favoritesExpanded ? "expanded" : "collapsed"
          }`}
          style={favoritesExpanded ? { height: `${favoritesHeight}px` } : undefined}
        >
          <div className="favorites-header">
            <button
              className="favorites-toggle-button"
              onClick={() => setFavoritesExpanded((expanded) => !expanded)}
              aria-expanded={favoritesExpanded}
            >
              <span className="favorites-chevron">
                {favoritesExpanded ? "⌄" : "›"}
              </span>
              <span>收藏</span>
            </button>
            <button
              className="favorites-unfavorite-all-button"
              onClick={handleUnfavoriteAll}
              disabled={favoriteItems.length === 0}
              title="全部取消收藏"
              aria-label="全部取消收藏"
            >
              ☆
            </button>
            <span className="favorites-count">{favoriteItems.length}</span>
          </div>
          {favoritesExpanded && (
            <div className="favorites-list">
              {favoriteItems.length > 0 ? (
                <Virtuoso
                  style={{ height: "100%" }}
                  data={favoriteItems}
                  endReached={loadMoreFavorites}
                  increaseViewportBy={120}
                  itemContent={renderItem}
                />
              ) : (
                <div className="empty-section-text">暂无收藏</div>
              )}
            </div>
          )}
        </section>
        {favoritesExpanded && (
          <div
            className="history-section-resizer"
            onMouseDown={handleFavoritesResizeStart}
            role="separator"
            aria-orientation="horizontal"
          />
        )}
        <div className="history-list">
          <Virtuoso
            style={{ height: "100%" }}
            data={historyItems}
            endReached={loadMoreHistory}
            increaseViewportBy={200}
            itemContent={renderItem}
          />
        </div>
      </div>
    </div>
  );
}
