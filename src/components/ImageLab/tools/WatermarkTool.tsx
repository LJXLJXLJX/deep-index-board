export function WatermarkTool() {
  return (
    <div className="watermark-tool-view">
      <div className="lab-placeholder-full">
        <div className="tool-icon">✂️</div>
        <h3>去水印实验室</h3>
        <p>基于 AI 自动识别并移除干扰水印，完美复原背景</p>
        <button className="lab-action-btn-primary" disabled>
          开始处理
        </button>
        <p className="lab-status-hint">内测功能，即将开放</p>
      </div>
    </div>
  );
}
