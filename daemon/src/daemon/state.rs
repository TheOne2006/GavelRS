// src/daemon/state.rs

use std::collections::{HashMap, HashSet};
use std::path::PathBuf; // Import PathBuf, remove unused Path
use std::sync::Arc;
use tokio::sync::RwLock;
use anyhow::{Context, Result};
use bincode::{self, Encode, Decode};
use log::{error, info, warn}; // Import log macros
use std::fs; // Import fs for file operations
use std::io::{Read, Write}; // Import Read/Write traits

// 从 core crate 引入共享的数据模型
#[allow(unused_imports)] // 允许未使用的导入 (ResourceLimit 在测试中使用)
use gavel_core::utils::models::{TaskMeta, QueueMeta, TaskState, ResourceLimit};
use gavel_core::gpu::monitor::GpuStats;

// 定义守护进程的共享状态
#[derive(Debug, Clone)]
pub struct DaemonState {
    // 使用 Arc<RwLock<...>> 允许多个线程安全地读写状态
    inner: Arc<RwLock<InnerDaemonState>>,
    // 持久化文件的路径
    persistence_path: Arc<PathBuf>, // Change Path to PathBuf for ownership
}

// 内部状态结构，由 RwLock 保护
#[derive(Debug, Default, serde::Serialize, serde::Deserialize, Encode, Decode)] // 添加 Encode, Decode
struct InnerDaemonState {
    tasks: HashMap<u64, TaskMeta>, // 存储所有任务，通过任务 ID 索引
    queues: HashMap<String, QueueMeta>, // 存储所有队列，通过队列名称索引
    gpu_stats: HashMap<u32, GpuStats>, // 存储每个 GPU 的最新状态统计
    gpu_allocations: HashMap<u32, Option<String>>, // GPU ID -> 分配到的队列名称 (None 表示未分配或空闲)
    ignored_gpus: HashSet<u32>, // 被用户设置为忽略的 GPU ID 集合
}

// 为 DaemonState 实现方法
impl DaemonState {
    // 创建一个新的 DaemonState 实例
    pub fn new(persistence_path: PathBuf) -> Self {
        DaemonState {
            inner: Arc::new(RwLock::new(InnerDaemonState::default())),
            persistence_path: Arc::new(persistence_path),
        }
    }

    // 持久化状态到文件
    pub async fn persist(&self) -> Result<()> {
        let inner_state = self.inner.read().await;
        let encoded = bincode::encode_to_vec(&*inner_state, bincode::config::standard())
            .context("Failed to encode daemon state")?;

        // Ensure directory exists
        if let Some(parent_dir) = self.persistence_path.parent() {
            fs::create_dir_all(parent_dir)
                .context("Failed to create persistence directory")?;
        }

        let mut file = fs::File::create(&*self.persistence_path)
            .context("Failed to create persistence file")?;
        file.write_all(&encoded)
            .context("Failed to write daemon state to file")?;
        info!("Daemon state persisted to {:?}", self.persistence_path);
        Ok(())
    }

    // 从文件加载状态
    pub async fn load(&self) -> Result<()> {
        if !self.persistence_path.exists() {
            warn!("Persistence file {:?} not found, starting with default state.", self.persistence_path);
            return Ok(()); // No state to load
        }

        let mut file = fs::File::open(&*self.persistence_path)
            .context("Failed to open persistence file")?;
        let mut encoded = Vec::new();
        file.read_to_end(&mut encoded)
            .context("Failed to read daemon state from file")?;

        let (decoded, _len): (InnerDaemonState, usize) = bincode::decode_from_slice(&encoded, bincode::config::standard())
            .context("Failed to decode daemon state")?;

        let mut inner_state = self.inner.write().await;
        *inner_state = decoded;
        info!("Daemon state loaded from {:?}", self.persistence_path);
        Ok(())
    }

    // --- Task related methods ---

    pub async fn add_task(&self, task: TaskMeta) -> Result<()> {
        let mut state = self.inner.write().await;
        let task_id = task.id;
        if state.tasks.insert(task_id, task).is_some() {
            warn!("Task with ID {} already existed and was overwritten.", task_id);
        }
        self.persist().await.map_err(|e| {
            error!("Failed to persist state after adding task {}: {}", task_id, e);
            e
        })?; // Persist after modification
        Ok(())
    }

    pub async fn get_task(&self, task_id: u64) -> Option<TaskMeta> {
        self.inner.read().await.tasks.get(&task_id).cloned()
    }

    pub async fn get_all_tasks(&self) -> Vec<TaskMeta> {
        self.inner.read().await.tasks.values().cloned().collect()
    }

    pub async fn update_task_state(&self, task_id: u64, new_state: TaskState) -> Result<()> {
        let mut state = self.inner.write().await;
        if let Some(task) = state.tasks.get_mut(&task_id) {
            task.state = new_state;
            self.persist().await.map_err(|e| {
                error!("Failed to persist state after updating task {} state: {}", task_id, e);
                e
            })?; // Persist after modification
            Ok(())
        } else {
            Err(anyhow::anyhow!("Task with ID {} not found", task_id))
        }
    }

    pub async fn update_task_queue(&self, task_id: u64, new_queue: String) -> Result<()> {
        let mut state = self.inner.write().await;
        if let Some(task) = state.tasks.get_mut(&task_id) {
            // TODO: Update queue's task lists as well
            task.queue = new_queue;
            self.persist().await.map_err(|e| {
                error!("Failed to persist state after moving task {}: {}", task_id, e);
                e
            })?; // Persist after modification
            Ok(())
        } else {
            Err(anyhow::anyhow!("Task with ID {} not found", task_id))
        }
    }

    pub async fn update_task_priority(&self, task_id: u64, new_priority: u8) -> Result<()> {
        let mut state = self.inner.write().await;
        if let Some(task) = state.tasks.get_mut(&task_id) {
            task.priority = new_priority;
            self.persist().await.map_err(|e| {
                error!("Failed to persist state after updating task {} priority: {}", task_id, e);
                e
            })?; // Persist after modification
            Ok(())
        } else {
            Err(anyhow::anyhow!("Task with ID {} not found", task_id))
        }
    }

    pub async fn remove_task(&self, task_id: u64) -> Result<Option<TaskMeta>> {
        let mut state = self.inner.write().await;
        let removed_task = state.tasks.remove(&task_id);
        if removed_task.is_some() {
            self.persist().await.map_err(|e| {
                error!("Failed to persist state after removing task {}: {}", task_id, e);
                e
            })?; // Persist after modification
        }
        Ok(removed_task)
    }

    // --- Queue related methods ---

    pub async fn add_queue(&self, queue: QueueMeta) -> Result<()> {
        let mut state = self.inner.write().await;
        let queue_name = queue.name.clone();
        if state.queues.insert(queue_name.clone(), queue).is_some() {
            warn!("Queue with name '{}' already existed and was overwritten.", queue_name);
        }
        self.persist().await.map_err(|e| {
            error!("Failed to persist state after adding queue '{}': {}", queue_name, e);
            e
        })?; // Persist after modification
        Ok(())
    }

    pub async fn get_queue(&self, queue_name: &str) -> Option<QueueMeta> {
        self.inner.read().await.queues.get(queue_name).cloned()
    }

    pub async fn get_all_queues(&self) -> Vec<QueueMeta> {
        self.inner.read().await.queues.values().cloned().collect()
    }

    // TODO: Add methods to update queue properties (priority, limits, tasks)

    // --- GPU related methods ---

    pub async fn update_gpu_stats(&self, gpu_id: u32, stats: GpuStats) {
        let mut state = self.inner.write().await;
        state.gpu_stats.insert(gpu_id, stats);
        // No persistence needed for transient stats
    }

    pub async fn get_gpu_stats(&self, gpu_id: u32) -> Option<GpuStats> {
        self.inner.read().await.gpu_stats.get(&gpu_id).cloned()
    }

    pub async fn get_all_gpu_stats(&self) -> HashMap<u32, GpuStats> {
        self.inner.read().await.gpu_stats.clone()
    }

    pub async fn set_gpu_ignore(&self, gpu_id: u32) -> Result<()> {
        let mut state = self.inner.write().await;
        state.ignored_gpus.insert(gpu_id);
        self.persist().await.map_err(|e| {
            error!("Failed to persist state after ignoring GPU {}: {}", gpu_id, e);
            e
        })?; // Persist after modification
        Ok(())
    }

    pub async fn unset_gpu_ignore(&self, gpu_id: u32) -> Result<()> {
        let mut state = self.inner.write().await;
        state.ignored_gpus.remove(&gpu_id);
        self.persist().await.map_err(|e| {
            error!("Failed to persist state after un-ignoring GPU {}: {}", gpu_id, e);
            e
        })?; // Persist after modification
        Ok(())
    }

    pub async fn get_ignored_gpus(&self) -> HashSet<u32> {
        self.inner.read().await.ignored_gpus.clone()
    }

    pub async fn set_gpu_allocation(&self, gpu_id: u32, queue_name: Option<String>) -> Result<()> {
        let mut state = self.inner.write().await;
        state.gpu_allocations.insert(gpu_id, queue_name.clone());
        self.persist().await.map_err(|e| {
            error!("Failed to persist state after setting GPU {} allocation: {}", gpu_id, e);
            e
        })?; // Persist after modification
        Ok(())
    }

    pub async fn remove_gpu_allocation(&self, gpu_id: u32) -> Result<()> {
        let mut state = self.inner.write().await;
        state.gpu_allocations.remove(&gpu_id);
        self.persist().await.map_err(|e| {
            error!("Failed to persist state after removing GPU {} allocation: {}", gpu_id, e);
            e
        })?; // Persist after modification
        Ok(())
    }

    pub async fn get_gpu_allocation(&self, gpu_id: u32) -> Option<Option<String>> {
        self.inner.read().await.gpu_allocations.get(&gpu_id).cloned()
    }

    // 新增方法：获取所有 GPU 的分配情况
    pub async fn get_gpu_allocations(&self) -> HashMap<u32, Option<String>> {
        self.inner.read().await.gpu_allocations.clone()
    }

    // TODO: Add methods for scheduler interactions (e.g., find available GPU)
}