use tauri::{App, Manager, Runtime};

pub fn setup_app_ui<R: Runtime>(app: &mut App<R>) -> Result<(), Box<dyn std::error::Error>> {
    // 1. 各平台通用的窗口基础设置
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_skip_taskbar(true);
        crate::platform::setup_main_window(app, &window);

        let _ = window.hide();
    }

    Ok(())
}
