use crate::inference::SessionManager;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;
use std::sync::OnceLock;
use std::thread;
use std::time::Duration;
use threadpool::ThreadPool;

/// 全局单例容器
static GLOBAL_TASK_MANAGER: OnceLock<HeavyWorkManager> = OnceLock::new();

/// 任务优先级定义
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TaskPriority {
    High,   // 立即交互类任务 (如用户发起的翻译)
    Normal, // 标准任务
    Low,    // 背景任务 (如延迟 OCR)
}

/// 重度任务动作定义
#[allow(dead_code)]
pub enum HeavyTask {
    Ocr {
        image_path: PathBuf,
        model_root: Option<PathBuf>,
        callback: Box<dyn FnOnce(Result<String, String>) + Send>,
    },
    Translate {
        text: String,
        target_lang: String,
        callback: Box<dyn FnOnce(Result<String, String>) + Send>,
    },
    FileTextExtract {
        file_path: PathBuf,
        callback: Box<dyn FnOnce(Result<String, String>) + Send>,
    },
    ClipImageEmbedding {
        image_path: PathBuf,
        model_path: PathBuf,
        callback: Box<dyn FnOnce(Result<Vec<f32>, String>) + Send>,
    },
    ClipTextEmbedding {
        text: String,
        model_path: PathBuf,
        vocab_path: PathBuf,
        callback: Box<dyn FnOnce(Result<Vec<f32>, String>) + Send>,
    },
}

/// 内部消息类型
enum InternalMsg {
    NewTask(HeavyTask, TaskPriority),
}

/// 异步重度任务管理器
pub struct HeavyWorkManager {
    task_sender: Sender<InternalMsg>,
}

#[allow(dead_code)]
impl HeavyWorkManager {
    pub fn init(inference_manager: Arc<SessionManager>) -> &'static Self {
        GLOBAL_TASK_MANAGER.get_or_init(|| Self::init_internal(inference_manager))
    }

    pub fn global() -> &'static Self {
        GLOBAL_TASK_MANAGER
            .get()
            .expect("HeavyWorkManager not initialized")
    }

    fn init_internal(inference_manager: Arc<SessionManager>) -> Self {
        let (tx, rx) = mpsc::channel::<InternalMsg>();

        thread::spawn(move || {
            let pool = ThreadPool::new(4);
            let session_manager = inference_manager;

            // 按优先级划分队列
            let mut high_queue = VecDeque::new();
            let mut normal_queue = VecDeque::new();
            let mut low_queue = VecDeque::new();

            loop {
                // 接收新任务
                while let Ok(msg) = rx.try_recv() {
                    match msg {
                        InternalMsg::NewTask(task, priority) => match priority {
                            TaskPriority::High => high_queue.push_back(task),
                            TaskPriority::Normal => normal_queue.push_back(task),
                            TaskPriority::Low => low_queue.push_back(task),
                        },
                    }
                }

                // 调度逻辑：只要线程池未满，就按优先级分发任务
                while pool.active_count() < pool.max_count() {
                    let task = high_queue
                        .pop_front()
                        .or_else(|| normal_queue.pop_front())
                        .or_else(|| low_queue.pop_front());

                    if let Some(task) = task {
                        let sm = session_manager.clone();
                        pool.execute(move || match task {
                            HeavyTask::Ocr {
                                image_path,
                                model_root,
                                callback,
                            } => {
                                #[cfg(not(target_os = "windows"))]
                                let _ = model_root;

                                #[cfg(target_os = "macos")]
                                let res = crate::tasks::ocr_macos::run_native_ocr(&image_path);

                                #[cfg(target_os = "windows")]
                                let res = crate::tasks::ocr_windows::run_onnx_ocr(
                                    &image_path,
                                    model_root.as_deref(),
                                );

                                #[cfg(not(any(target_os = "macos", target_os = "windows")))]
                                let res: Result<String, String> =
                                    Err("OCR is only supported on macOS and Windows".to_string());

                                callback(res);
                            }
                            HeavyTask::Translate {
                                text,
                                target_lang,
                                callback,
                            } => {
                                let res = crate::tasks::translate::run_translate(text, target_lang);
                                callback(res);
                            }
                            HeavyTask::FileTextExtract {
                                file_path,
                                callback,
                            } => {
                                let res = crate::tasks::file_extract::run_file_extract(file_path);
                                callback(res);
                            }
                            HeavyTask::ClipImageEmbedding {
                                image_path,
                                model_path,
                                callback,
                            } => {
                                let res = crate::tasks::clip::run_clip_image_embedding(
                                    &sm,
                                    model_path,
                                    &image_path,
                                );
                                callback(res);
                            }
                            HeavyTask::ClipTextEmbedding {
                                text,
                                model_path,
                                vocab_path,
                                callback,
                            } => {
                                let res = crate::tasks::clip::run_clip_text_embedding(
                                    &sm,
                                    model_path,
                                    &vocab_path,
                                    &text,
                                );
                                callback(res);
                            }
                        });
                    } else {
                        break; // 所有队列都为空
                    }
                }

                thread::sleep(Duration::from_millis(50));
            }
        });

        Self { task_sender: tx }
    }

    pub fn execute(&self, task: HeavyTask, priority: TaskPriority) -> Result<(), String> {
        self.task_sender
            .send(InternalMsg::NewTask(task, priority))
            .map_err(|e| e.to_string())
    }
}

#[allow(dead_code)]
pub struct HeavyWorkState {
    pub manager: &'static HeavyWorkManager,
}
