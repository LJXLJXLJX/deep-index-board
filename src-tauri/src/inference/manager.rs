#[cfg(target_os = "macos")]
use crate::inference::traits::InferenceBackend;
use crate::inference::traits::InferenceSession;
use std::collections::HashMap;
#[cfg(target_os = "macos")]
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};

/// 内部槽位，用于管理单个模型的加载状态，实现“锁在 Value 上”
struct SessionSlot {
    // 这里的 Mutex 只保护这一个模型的加载过程
    inner: Mutex<Option<Arc<dyn InferenceSession>>>,
}

pub struct SessionManager {
    #[cfg(target_os = "macos")]
    backends: Vec<Box<dyn InferenceBackend>>,
    // 这里的 RwLock 允许并发读取不同或相同的 Session，而不会互相阻塞
    sessions: RwLock<HashMap<String, Arc<SessionSlot>>>,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            #[cfg(target_os = "macos")]
            backends: Vec::new(),
            sessions: RwLock::new(HashMap::new()),
        }
    }

    #[cfg(target_os = "macos")]
    pub fn register_backend(&mut self, backend: Box<dyn InferenceBackend>) {
        self.backends.push(backend);
    }

    /// 获取或加载 session
    #[cfg(target_os = "macos")]
    pub fn get_or_load_session(
        &self,
        model_id: &str,
        model_path: PathBuf,
        backend_name: Option<&str>,
    ) -> Result<Arc<dyn InferenceSession>, String> {
        // 1. 尝试获取现有的槽位 (使用读锁，支持高并发)
        let slot = {
            let sessions = self.sessions.read().unwrap();
            sessions.get(model_id).cloned()
        };

        // 2. 如果槽位不存在，创建新槽位 (使用写锁，仅在首次加载某模型时触发)
        let slot = match slot {
            Some(s) => s,
            None => {
                let mut sessions = self.sessions.write().unwrap();
                // 双重检查，防止在切换锁的间隙被其他线程创建
                sessions
                    .entry(model_id.to_string())
                    .or_insert_with(|| {
                        Arc::new(SessionSlot {
                            inner: Mutex::new(None),
                        })
                    })
                    .clone()
            }
        };

        // 3. 在槽位内部进行加载 (锁细化到具体模型)
        let mut inner = slot.inner.lock().unwrap();
        if let Some(ref session) = *inner {
            return Ok(Arc::clone(session));
        }

        // 查找合适的后端逻辑
        let backend = if let Some(name) = backend_name {
            self.backends
                .iter()
                .find(|b| b.name() == name)
                .ok_or_else(|| format!("Backend {} not found", name))?
        } else {
            self.backends
                .first()
                .ok_or_else(|| "No inference backends registered".to_string())?
        };

        // 实际加载模型：此时全局的 sessions Map 读/写锁已经释放
        // 其他线程可以并发获取已经加载好的其他模型
        let new_session: Arc<dyn InferenceSession> = backend
            .load_session(model_path, model_id.to_string())?
            .into();
        *inner = Some(Arc::clone(&new_session));

        Ok(new_session)
    }

    /// 获取当前所有 session 的总内存占用
    #[allow(dead_code)]
    pub fn get_total_memory_usage(&self) -> usize {
        let sessions = self.sessions.read().unwrap();
        sessions
            .values()
            .map(|slot| {
                // 如果模型正在加载，这里会短暂阻塞，或者可以使用 try_lock
                let inner = slot.inner.lock().unwrap();
                inner.as_ref().map(|s| s.memory_usage_bytes()).unwrap_or(0)
            })
            .sum()
    }

    /// 获取详细的内存报告
    pub fn get_memory_report(&self) -> HashMap<String, usize> {
        let sessions = self.sessions.read().unwrap();
        sessions
            .iter()
            .map(|(id, slot)| {
                let inner = slot.inner.lock().unwrap();
                (
                    id.clone(),
                    inner.as_ref().map(|s| s.memory_usage_bytes()).unwrap_or(0),
                )
            })
            .collect()
    }

    /// 释放指定的 session
    pub fn release_session(&self, model_id: &str) {
        let mut sessions = self.sessions.write().unwrap();
        sessions.remove(model_id);
    }

    /// 释放所有 session 以节省内存
    pub fn release_all(&self) {
        let mut sessions = self.sessions.write().unwrap();
        sessions.clear();
    }
}
