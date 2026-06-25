#[cfg(target_os = "macos")]
pub mod coreml;
pub mod manager;
#[cfg(target_os = "windows")]
pub mod onnx_runtime;
#[cfg(feature = "openvino")]
pub mod openvino;
pub mod traits;
pub mod utils;

#[cfg(target_os = "macos")]
pub use coreml::CoreMLBackend;
pub use manager::SessionManager;

use std::sync::Arc;

pub struct InferenceState {
    pub manager: Arc<SessionManager>,
}
