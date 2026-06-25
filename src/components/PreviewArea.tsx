import { useEffect, useState } from "react";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { HistoryItem } from "./HistoryList";
import { ImageLab } from "./ImageLab/ImageLab";

interface PreviewAreaProps {
  item: HistoryItem | null;
  onItemSaved: (item: HistoryItem) => void;
  onDirtyChange: (isDirty: boolean) => void;
}

const isImageFile = (path: string) => {
  const ext = path.split(".").pop()?.toLowerCase();
  return (
    ext &&
    ["png", "jpg", "jpeg", "gif", "webp", "bmp", "heic", "heif"].includes(ext)
  );
};

export function PreviewArea({
  item,
  onItemSaved,
  onDirtyChange,
}: PreviewAreaProps) {
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
        <TextPreview
          item={item}
          onItemSaved={onItemSaved}
          onDirtyChange={onDirtyChange}
        />
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

function TextPreview({
  item,
  onItemSaved,
  onDirtyChange,
}: {
  item: HistoryItem;
  onItemSaved: (item: HistoryItem) => void;
  onDirtyChange: (isDirty: boolean) => void;
}) {
  const [isEditing, setIsEditing] = useState(false);
  const [draft, setDraft] = useState(item.content);
  const [isSaving, setIsSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);

  useEffect(() => {
    setDraft(item.content);
    setIsEditing(false);
    setSaveError(null);
    onDirtyChange(false);
  }, [item.id, item.content, onDirtyChange]);

  useEffect(() => {
    onDirtyChange(isEditing && draft !== item.content);
    return () => onDirtyChange(false);
  }, [draft, isEditing, item.content, onDirtyChange]);

  const isDirty = draft !== item.content;
  const canSave = isDirty && draft.length > 0 && !isSaving;

  const saveCopy = async () => {
    if (!canSave) return;
    try {
      setIsSaving(true);
      setSaveError(null);
      const savedItem = await invoke<HistoryItem>("save_text_item_copy", {
        content: draft,
      });
      onItemSaved(savedItem);
      setIsEditing(false);
      onDirtyChange(false);
    } catch (error) {
      console.error("Failed to save text copy:", error);
      setSaveError(String(error));
    } finally {
      setIsSaving(false);
    }
  };

  const overwrite = async () => {
    if (!canSave) return;
    try {
      setIsSaving(true);
      setSaveError(null);
      const savedItem = await invoke<HistoryItem>("overwrite_text_item", {
        id: item.id,
        content: draft,
      });
      onItemSaved(savedItem);
      setIsEditing(false);
      onDirtyChange(false);
    } catch (error) {
      console.error("Failed to overwrite text item:", error);
      setSaveError(String(error));
    } finally {
      setIsSaving(false);
    }
  };

  const cancelEdit = () => {
    setDraft(item.content);
    setIsEditing(false);
    setSaveError(null);
    onDirtyChange(false);
  };

  return (
    <div className="text-preview">
      <div className="text-preview-toolbar">
        {isEditing ? (
          <>
            <button
              className="text-preview-action secondary"
              onClick={cancelEdit}
              disabled={isSaving}
            >
              取消
            </button>
            <button
              className="text-preview-action"
              onClick={saveCopy}
              disabled={!canSave}
            >
              另存新条目
            </button>
            <button
              className="text-preview-action"
              onClick={overwrite}
              disabled={!canSave}
            >
              覆盖保存
            </button>
          </>
        ) : (
          <button
            className="text-preview-action"
            onClick={() => setIsEditing(true)}
          >
            编辑
          </button>
        )}
      </div>
      {isEditing ? (
        <textarea
          className="text-preview-editor"
          value={draft}
          onChange={(event) => {
            setDraft(event.target.value);
            setSaveError(null);
          }}
          spellCheck={false}
        />
      ) : (
        <pre>{item.content}</pre>
      )}
      {saveError && <div className="text-preview-error">{saveError}</div>}
    </div>
  );
}
