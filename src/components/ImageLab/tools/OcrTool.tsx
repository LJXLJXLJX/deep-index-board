export function OcrTool({ text }: { text?: string | null }) {
  if (text == null) {
    return (
      <div className="lab-empty-placeholder">
        <span>OCR 正在识别或等待结果</span>
      </div>
    );
  }

  if (text.trim() === "") {
    return (
      <div className="lab-empty-placeholder">
        <span>未识别到文字内容</span>
      </div>
    );
  }

  return (
    <div className="ocr-tool-view">
      <pre className="ocr-text">{text}</pre>
    </div>
  );
}
