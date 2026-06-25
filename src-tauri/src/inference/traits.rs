use std::path::PathBuf;

/// 预测任务的输入数据
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub enum InferenceInput {
    Image(PathBuf),               // 路径
    Tensor(Vec<f32>, Vec<usize>), // 原始张量数据 (默认名称)
    #[cfg(test)]
    NamedTensor(String, Vec<f32>, Vec<usize>), // 带名称的张量
}

/// 预测任务的输出数据
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub enum InferenceOutput {
    Tensors(Vec<(Vec<f32>, Vec<usize>)>),
}

pub trait InferenceSession: Send + Sync {
    /// 执行预测
    #[allow(dead_code)]
    fn predict(&self, input: InferenceInput) -> Result<InferenceOutput, String>;

    /// 获取当前 session 的内存占用提示（字节）
    fn memory_usage_bytes(&self) -> usize;
}

pub trait InferenceBackend: Send + Sync {
    /// 加载模型并创建 session
    #[allow(dead_code)]
    fn load_session(&self, model_path: PathBuf) -> Result<Box<dyn InferenceSession>, String>;

    /// 后端名称 (e.g., "CoreML", "OpenVINO")
    #[allow(dead_code)]
    fn name(&self) -> &str;
}
