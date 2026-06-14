use tauri::{AppHandle, Manager, Runtime};

#[cfg(target_os = "windows")]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(target_os = "windows")]
static QUICK_WINDOW_DISMISS_MONITOR_RUNNING: AtomicBool = AtomicBool::new(false);

pub fn resolve_resource_path<R: Runtime>(
    app: &AppHandle<R>,
    resource_path: &str,
) -> Result<std::path::PathBuf, String> {
    let resolved = app
        .path()
        .resolve(resource_path, tauri::path::BaseDirectory::Resource)
        .map_err(|e| e.to_string())?;
    if resolved.exists() {
        return Ok(resolved);
    }

    let dev_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(resource_path);
    if dev_path.exists() {
        return Ok(dev_path);
    }

    Err(format!(
        "Resource not found. Tried {} and {}",
        resolved.display(),
        dev_path.display()
    ))
}

pub fn toggle_window_focused<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let is_visible = window.is_visible().unwrap_or(false);
        if is_visible {
            #[cfg(target_os = "windows")]
            let _ = set_window_topmost(&window, false);
            #[cfg(target_os = "windows")]
            let _ = restore_window_activation(&window);
            let _ = window.hide();
        } else {
            #[cfg(target_os = "windows")]
            let _ = restore_window_activation(&window);
            let _ = window.show();
            #[cfg(target_os = "windows")]
            let _ = set_window_topmost(&window, false);
            let _ = window.set_focus();
        }
    }
}

pub fn toggle_window_no_activate<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let is_visible = window.is_visible().unwrap_or(false);
        let is_focused = window.is_focused().unwrap_or(false);
        if is_visible && is_focused {
            #[cfg(target_os = "windows")]
            let _ = set_window_topmost(&window, false);
            let _ = window.hide();
        } else {
            #[cfg(target_os = "windows")]
            {
                if show_window_no_activate(&window).is_err() {
                    let _ = window.show();
                }
                start_quick_window_dismiss_monitor(app.clone());
            }

            #[cfg(not(target_os = "windows"))]
            {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }
    }
}

#[cfg(target_os = "windows")]
pub fn restore_window_activation<R: Runtime>(
    window: &tauri::WebviewWindow<R>,
) -> tauri::Result<()> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_NOACTIVATE,
    };

    let hwnd = window.hwnd()?.0 as windows_sys::Win32::Foundation::HWND;
    unsafe {
        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let active_style = ex_style & !(WS_EX_NOACTIVATE as isize);
        if active_style != ex_style {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, active_style);
        }
    }

    window.set_focusable(true)?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn show_window_no_activate<R: Runtime>(window: &tauri::WebviewWindow<R>) -> tauri::Result<()> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    };

    let hwnd = window.hwnd()?.0 as windows_sys::Win32::Foundation::HWND;

    unsafe {
        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let no_activate_style =
            ex_style | WS_EX_NOACTIVATE as isize | WS_EX_TOOLWINDOW as isize;

        if no_activate_style != ex_style {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, no_activate_style);
        }
    }

    window.set_focusable(false)?;
    window.show()?;
    set_window_topmost(window, true)?;

    Ok(())
}

#[cfg(target_os = "windows")]
fn set_window_topmost<R: Runtime>(
    window: &tauri::WebviewWindow<R>,
    topmost: bool,
) -> tauri::Result<()> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
        SWP_SHOWWINDOW,
    };

    let hwnd = window.hwnd()?.0 as windows_sys::Win32::Foundation::HWND;
    let insert_after = if topmost { HWND_TOPMOST } else { HWND_NOTOPMOST };

    unsafe {
        SetWindowPos(
            hwnd,
            insert_after,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
    }

    Ok(())
}

#[cfg(target_os = "windows")]
fn start_quick_window_dismiss_monitor<R: Runtime>(app: AppHandle<R>) {
    if QUICK_WINDOW_DISMISS_MONITOR_RUNNING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }

    std::thread::spawn(move || {
        let mut was_left_down = false;
        let mut was_escape_down = false;

        loop {
            let Some(window) = app.get_webview_window("main") else {
                break;
            };

            if !window.is_visible().unwrap_or(false) {
                break;
            }

            if should_dismiss_quick_window(&window, &mut was_left_down, &mut was_escape_down) {
                let _ = set_window_topmost(&window, false);
                let _ = window.hide();
                break;
            }

            std::thread::sleep(std::time::Duration::from_millis(35));
        }

        QUICK_WINDOW_DISMISS_MONITOR_RUNNING.store(false, Ordering::SeqCst);
    });
}

#[cfg(target_os = "windows")]
fn should_dismiss_quick_window<R: Runtime>(
    window: &tauri::WebviewWindow<R>,
    was_left_down: &mut bool,
    was_escape_down: &mut bool,
) -> bool {
    use windows_sys::Win32::Foundation::{POINT, RECT};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_ESCAPE, VK_LBUTTON,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetCursorPos, GetWindowRect};

    let left_down = unsafe { GetAsyncKeyState(VK_LBUTTON as i32) } < 0;
    let escape_down = unsafe { GetAsyncKeyState(VK_ESCAPE as i32) } < 0;

    let left_pressed = left_down && !*was_left_down;
    let escape_pressed = escape_down && !*was_escape_down;

    *was_left_down = left_down;
    *was_escape_down = escape_down;

    if escape_pressed {
        return true;
    }

    if !left_pressed {
        return false;
    }

    let Ok(hwnd) = window.hwnd() else {
        return false;
    };

    let hwnd = hwnd.0 as windows_sys::Win32::Foundation::HWND;
    let mut cursor = POINT { x: 0, y: 0 };
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };

    unsafe {
        if GetCursorPos(&mut cursor) == 0 || GetWindowRect(hwnd, &mut rect) == 0 {
            return false;
        }
    }

    cursor.x < rect.left || cursor.x >= rect.right || cursor.y < rect.top || cursor.y >= rect.bottom
}

pub fn get_mtime(path: &std::path::Path) -> i64 {
    path.metadata()
        .and_then(|m| m.modified())
        .map(|t| {
            t.duration_since(std::time::SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64
        })
        .unwrap_or(0)
}

#[cfg(target_os = "macos")]
pub fn get_frontmost_app() -> Option<String> {
    use objc2_app_kit::NSWorkspace;

    // sharedWorkspace 不需要 MainThreadMarker 参数
    let workspace = unsafe { NSWorkspace::sharedWorkspace() };
    let app = unsafe { workspace.frontmostApplication()? };

    // 优先获取 Bundle Identifier
    if let Some(bundle_id) = unsafe { app.bundleIdentifier() } {
        return Some(bundle_id.to_string());
    }

    // 如果没有 Bundle ID，则尝试获取应用名称
    if let Some(name) = unsafe { app.localizedName() } {
        return Some(name.to_string());
    }

    None
}

#[cfg(not(target_os = "macos"))]
pub fn get_frontmost_app() -> Option<String> {
    None
}
