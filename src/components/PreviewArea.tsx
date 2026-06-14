import { convertFileSrc } from "@tauri-apps/api/core";
import { HistoryItem } from "./HistoryList";
import { ImageLab } from "./ImageLab/ImageLab";

interface PreviewAreaProps {
  item: HistoryItem | null;
}

const isImageFile = (path: string) => {
  const ext = path.split(".").pop()?.toLowerCase();
  return (
    ext &&
    ["png", "jpg", "jpeg", "gif", "webp", "bmp", "heic", "heif"].includes(ext)
  );
};

export function PreviewArea({ item }: PreviewAreaProps) {
  if (!item) {
    return (
      <div className="preview-area empty">
        <div className="placeholder-text">移动鼠标进行预览</div>
      </div>
    );
  }

  return (
    <div className="preview-area">
      {item.type === "text" ? (
        <TextPreview content={item.content} />
      ) : item.type === "image" ||
        (item.type === "file" && isImageFile(item.content)) ? (
        <ImageLab src={convertFileSrc(item.content)} item={item} />
      ) : (
        <FilePreview item={item} />
      )}
    </div>
  );
}

function FilePreview({ item }: { item: HistoryItem }) {
  const fileName = item.content.split(/[/\\]/).pop() || item.content;

  // 格式化预览区的快照时间（显示完整年月日时分秒）
  const formatTime = (ts: string) => {
    try {
      // ts 为 "2025-12-29 14:49:17", 转换为本地时间
      const date = new Date(ts.replace(" ", "T") + "Z");
      const Y = date.getFullYear();
      const M = String(date.getMonth() + 1).padStart(2, "0");
      const D = String(date.getDate()).padStart(2, "0");
      const timeStr = ts.split(" ")[1] || "";
      return `${Y}-${M}-${D} ${timeStr}`;
    } catch (error) {
      return ts;
    }
  };

  return (
    <div className="file-preview">
      <div className="file-preview-header">
        <div className="file-preview-icon">
          {item.type === "directory" ? "📁" : "📄"}
        </div>
        <div className="file-preview-name">{fileName}</div>
        <div className="file-preview-type-badge">
          {item.type === "directory" ? "文件夹" : "文件"}
        </div>
      </div>
      <div className="file-preview-body">
        <div className="info-label">完整路径</div>
        <div className="info-value path-text">{item.content}</div>

        {item.content_text && (
          <>
            <div className="content-section-header">
              <div className="info-label">文件内容快照</div>
              <div className="snapshot-tip">
                <span className="snapshot-badge">SNAPSHOT</span>
                <span className="snapshot-time">
                  获取于 {formatTime(item.timestamp)}
                </span>
              </div>
            </div>
            <div className="snapshot-disclaimer">
              * 此处仅展示文件开头 4KB
              的历史快照。磁盘原文件若被修改，此处不会同步更新。
            </div>
            <div className="file-content-preview">
              <pre>{item.content_text}</pre>
            </div>
          </>
        )}
      </div>
    </div>
  );
}

function TextPreview({ content }: { content: string }) {
  return (
    <div className="text-preview">
      <pre>{content}</pre>
    </div>
  );
}
