import { useState, useEffect, useCallback } from "react";
import { HistoryList, HistoryItem } from "./components/HistoryList";
import { PreviewArea } from "./components/PreviewArea";
import { StatusBar } from "./components/StatusBar";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

function App() {
  const [hoveredItem, setHoveredItem] = useState<HistoryItem | null>(null);
  const [leftWidth, setLeftWidth] = useState(300);
  const [isResizing, setIsResizing] = useState(false);
  const [previewIsDirty, setPreviewIsDirty] = useState(false);
  const clearHoveredItem = useCallback(() => setHoveredItem(null), []);
  const handleHoverItem = useCallback(
    (item: HistoryItem | null) => {
      setHoveredItem((prev) => {
        if (previewIsDirty && item?.id !== prev?.id) return prev;
        return item;
      });
    },
    [previewIsDirty]
  );

  useEffect(() => {
    const handleMouseMove = (e: MouseEvent) => {
      if (!isResizing) return;
      // 限制最小宽度和最大宽度
      const newWidth = Math.max(200, Math.min(600, e.clientX));
      setLeftWidth(newWidth);
    };

    const handleMouseUp = () => {
      setIsResizing(false);
      document.body.style.cursor = 'default';
    };

    if (isResizing) {
      window.addEventListener('mousemove', handleMouseMove);
      window.addEventListener('mouseup', handleMouseUp);
      document.body.style.cursor = 'col-resize';
    }

    return () => {
      window.removeEventListener('mousemove', handleMouseMove);
      window.removeEventListener('mouseup', handleMouseUp);
    };
  }, [isResizing]);

  useEffect(() => {
    // 监听全局更新，如果当前预览的项被更新了（如 OCR 完成），同步刷新预览
    const unlistenUpdate = listen<HistoryItem>("history-item-updated", (event) => {
      const updatedItem = event.payload;
      setHoveredItem((prev) => {
        if (prev && prev.id === updatedItem.id) {
          return updatedItem;
        }
        return prev;
      });
    });

    const unlistenDelete = listen<number>("history-item-deleted", (event) => {
      const deletedId = event.payload;
      setHoveredItem((prev) => (prev?.id === deletedId ? null : prev));
    });

    return () => {
      unlistenUpdate.then((f) => f());
      unlistenDelete.then((f) => f());
    };
  }, []);

  const handleWindowDragStart = async (event: React.MouseEvent<HTMLDivElement>) => {
    if (event.button !== 0) return;
    try {
      await invoke("start_window_dragging");
    } catch (error) {
      console.error("Failed to start window dragging:", error);
    }
  };

  const prepareWindowDragging = async () => {
    try {
      await invoke("prepare_window_dragging");
    } catch (error) {
      console.error("Failed to prepare window dragging:", error);
    }
  };

  return (
    <main className="app-container">
      <div
        className="window-drag-region"
        data-tauri-drag-region
        onMouseEnter={prepareWindowDragging}
        onMouseDown={handleWindowDragStart}
      />

      {/* 主要工作区：左右分栏 */}
      <div className="main-workspace">
        <div className="left-column" style={{ width: leftWidth }}>
          <HistoryList onHover={handleHoverItem} onClear={clearHoveredItem} />
        </div>

        {/* 拖拽手柄 */}
        <div 
          className="resizer" 
          onMouseDown={() => setIsResizing(true)}
        />

        <div className="right-column">
          <PreviewArea
            item={hoveredItem}
            onItemSaved={setHoveredItem}
            onDirtyChange={setPreviewIsDirty}
          />
        </div>
      </div>

      {/* 底部状态栏组件 */}
      <StatusBar />
    </main>
  );
}

export default App;
