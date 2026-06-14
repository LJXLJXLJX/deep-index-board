import { useState, useRef, useCallback } from "react";
import { HistoryItem } from "../HistoryList";
import { OcrTool } from "./tools/OcrTool";

export type LabToolType = "ocr";

interface ImageLabProps {
  src: string;
  item: HistoryItem;
}

export function ImageLab({ src, item }: ImageLabProps) {
  const [activeTool, setActiveTool] = useState<LabToolType>("ocr");
  const [panelHeight, setPanelHeight] = useState(220);
  const [isCollapsed, setIsCollapsed] = useState(false);
  const [isDragging, setIsDragging] = useState(false);

  const containerRef = useRef<HTMLDivElement>(null);
  const rafId = useRef<number | null>(null);

  const handleMouseMove = useCallback((e: MouseEvent) => {
    if (!containerRef.current) return;

    if (rafId.current) {
      cancelAnimationFrame(rafId.current);
    }

    rafId.current = requestAnimationFrame(() => {
      const containerRect = containerRef.current!.getBoundingClientRect();
      const newHeight = containerRect.bottom - e.clientY;

      if (newHeight > 100 && newHeight < containerRect.height - 100) {
        setPanelHeight(newHeight);
        setIsCollapsed(false);
      }
    });
  }, []);

  const stopResizing = useCallback(() => {
    setIsDragging(false);
    document.removeEventListener("mousemove", handleMouseMove);
    document.removeEventListener("mouseup", stopResizing);
    if (rafId.current) {
      cancelAnimationFrame(rafId.current);
    }
  }, [handleMouseMove]);

  const startResizing = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      setIsDragging(true);
      document.addEventListener("mousemove", handleMouseMove);
      document.addEventListener("mouseup", stopResizing);
    },
    [handleMouseMove, stopResizing],
  );

  return (
    <div
      className={`image-preview-container ${isDragging ? "resizing" : ""}`}
      ref={containerRef}
    >
      <div className="image-preview-main">
        <div className="image-container">
          <img src={src} alt="Preview" />
        </div>
      </div>

      <div
        className={`lab-panel ${isCollapsed ? "collapsed" : ""}`}
        style={{ height: isCollapsed ? "36px" : `${panelHeight}px` }}
      >
        <div className="lab-resizer" onMouseDown={startResizing} />

        <div className="lab-header">
          <div className="lab-tools-group">
            <button
              className={`lab-tool-btn ${activeTool === "ocr" ? "active" : ""}`}
              onClick={() => {
                setActiveTool("ocr");
                setIsCollapsed(false);
              }}
            >
              OCR 提取
            </button>
          </div>
          <div
            className="lab-header-actions"
            onClick={() => setIsCollapsed(!isCollapsed)}
          >
            <span className="lab-toggle-icon">
              {isCollapsed ? "展开" : "收起面板"}
            </span>
          </div>
        </div>

        <div className="lab-content">
          {activeTool === "ocr" && <OcrTool text={item.content_text} />}
        </div>
      </div>
    </div>
  );
}
