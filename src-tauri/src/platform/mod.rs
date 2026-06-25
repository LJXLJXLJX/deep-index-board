#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod other;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "macos")]
pub use macos::*;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub use other::*;
#[cfg(target_os = "windows")]
pub use windows::*;
