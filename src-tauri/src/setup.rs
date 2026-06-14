use tauri::{App, Manager, Runtime};

#[cfg(target_os = "macos")]
use objc2_app_kit::{NSWindow, NSWindowButton};
#[cfg(target_os = "macos")]
use tauri::ActivationPolicy;

pub fn setup_app_ui<R: Runtime>(app: &mut App<R>) -> Result<(), Box<dyn std::error::Error>> {
    // 1. 各平台通用的窗口基础设置
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_skip_taskbar(true);

        // 2. 平台特定逻辑
        #[cfg(target_os = "macos")]
        {
            // 隐藏 Dock 图标
            app.set_activation_policy(ActivationPolicy::Accessory);

            // 处理原生按钮隐藏
            let ns_window = window.ns_window().unwrap() as *mut NSWindow;
            unsafe {
                let ns_window = &*ns_window;
                if let Some(close_button) =
                    ns_window.standardWindowButton(NSWindowButton::NSWindowCloseButton)
                {
                    close_button.setHidden(true);
                }
                if let Some(minimize_button) =
                    ns_window.standardWindowButton(NSWindowButton::NSWindowMiniaturizeButton)
                {
                    minimize_button.setHidden(true);
                }
                if let Some(zoom_button) =
                    ns_window.standardWindowButton(NSWindowButton::NSWindowZoomButton)
                {
                    zoom_button.setHidden(true);
                }
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            // 非 macOS 平台使用无装饰窗口，彻底隐藏系统标题栏按钮。
            let _ = window.set_decorations(false);
        }

        let _ = window.hide();
    }

    Ok(())
}
