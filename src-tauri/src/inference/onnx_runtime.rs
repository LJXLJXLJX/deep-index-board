#[cfg(target_os = "windows")]
use std::path::PathBuf;
#[cfg(target_os = "windows")]
use std::sync::OnceLock;

#[cfg(target_os = "windows")]
static ORT_INIT: OnceLock<Result<(), String>> = OnceLock::new();

#[cfg(target_os = "windows")]
pub fn ensure_ort_initialized() -> Result<(), String> {
    ORT_INIT
        .get_or_init(|| {
            let dll_path = find_onnxruntime_dll().ok_or_else(|| {
                "onnxruntime.dll not found next to executable, target directory, or resources/native/onnxruntime".to_string()
            })?;
            ort::init_from(&dll_path)
                .map_err(|e| format!("Failed to load {}: {e}", dll_path.display()))?
                .commit();
            Ok(())
        })
        .clone()
}

#[cfg(target_os = "windows")]
fn find_onnxruntime_dll() -> Option<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("onnxruntime.dll"));
            candidates.push(dir.join("deps").join("onnxruntime.dll"));
            candidates.push(
                dir.join("resources")
                    .join("native")
                    .join("onnxruntime")
                    .join("onnxruntime.dll"),
            );
        }
    }

    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("onnxruntime.dll"));
        candidates.push(cwd.join("target").join("debug").join("onnxruntime.dll"));
        candidates.push(
            cwd.join("target")
                .join("debug")
                .join("deps")
                .join("onnxruntime.dll"),
        );
        candidates.push(
            cwd.join("src-tauri")
                .join("target")
                .join("debug")
                .join("onnxruntime.dll"),
        );
        candidates.push(
            cwd.join("src-tauri")
                .join("target")
                .join("debug")
                .join("deps")
                .join("onnxruntime.dll"),
        );
        candidates.push(
            cwd.join("resources")
                .join("native")
                .join("onnxruntime")
                .join("onnxruntime.dll"),
        );
        candidates.push(
            cwd.join("src-tauri")
                .join("resources")
                .join("native")
                .join("onnxruntime")
                .join("onnxruntime.dll"),
        );
    }

    candidates.into_iter().find(|path| path.exists())
}
