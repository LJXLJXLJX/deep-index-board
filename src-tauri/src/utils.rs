use tauri::{AppHandle, Manager, Runtime};

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
            crate::platform::hide_quick_window(&window);
        } else {
            let _ = crate::platform::restore_window_activation(&window);
            let _ = window.show();
            let _ = window.set_focus();
        }
    }
}

pub fn toggle_window_no_activate<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let is_visible = window.is_visible().unwrap_or(false);
        if is_visible {
            crate::platform::hide_quick_window(&window);
        } else if crate::platform::show_quick_window_no_activate(app, &window).is_err() {
            let _ = window.show();
        }
    }
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
