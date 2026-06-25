use arboard::Clipboard;
use tauri::{App, AppHandle, Runtime, WebviewWindow};

pub fn setup_main_window<R: Runtime>(_app: &mut App<R>, window: &WebviewWindow<R>) {
    let _ = window.set_decorations(false);
}

pub fn show_quick_window_no_activate<R: Runtime>(
    _app: &AppHandle<R>,
    window: &WebviewWindow<R>,
) -> tauri::Result<()> {
    window.show()?;
    window.set_focus()
}

pub fn hide_quick_window<R: Runtime>(window: &WebviewWindow<R>) {
    let _ = window.hide();
}

pub fn restore_window_activation<R: Runtime>(window: &WebviewWindow<R>) -> tauri::Result<()> {
    window.set_focusable(true)
}

pub fn paste_clipboard_item<R: Runtime>(
    _app_handle: &AppHandle<R>,
    _item_type: &str,
    _text: Option<&str>,
) {
}

pub fn write_file_path_to_clipboard(clipboard: &mut Clipboard, path: &str) -> Result<(), String> {
    clipboard.set_text(path).map_err(|e| e.to_string())
}

pub fn clipboard_change_count() -> isize {
    0
}

pub fn read_clipboard_file_paths() -> Vec<String> {
    Vec::new()
}

pub fn get_frontmost_app() -> Option<String> {
    None
}
