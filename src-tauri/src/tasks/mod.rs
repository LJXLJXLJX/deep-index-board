#[cfg(any(target_os = "macos", target_os = "windows"))]
pub mod background_worker;
pub mod clip;
pub mod file_extract;
#[cfg(target_os = "macos")]
pub mod ocr_macos;
#[cfg(target_os = "windows")]
pub mod ocr_windows;
pub mod translate;
