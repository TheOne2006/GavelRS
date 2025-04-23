#![allow(dead_code)] // 允许未使用的代码（结构体和方法）

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use anyhow::{Context, Result};
use bincode::{self, Encode, Decode};

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
    persistence_path: Arc<Path>,
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

impl DaemonState {
    /// 创建一个新的 DaemonState 实例，并尝试从持久化文件加载状态。
    /// 如果文件不存在或加载失败，则创建一个新的默认状态。
    pub async fn new<P: AsRef<Path>>(persistence_path: P) -> Result<Self> {
        let path_ref = persistence_path.as_ref();
        let inner_state = if path_ref.exists() { // 移除括号
            match Self::load_from_disk(path_ref).await {
                Ok(state) => {
                    log::info!("Successfully loaded daemon state from {}", path_ref.display());
                    state
                },
                Err(e) => {
                    log::warn!("Failed to load state from {}: {}. Starting with default state.", path_ref.display(), e);
                    InnerDaemonState::default()
                }
            }
        } else {
            log::info!("Persistence file {} not found. Starting with default state.", path_ref.display());
            InnerDaemonState::default()
        };

        // 初始化时可以尝试获取一次 GPU 信息
        // let monitor = GpuMonitor::new()?;
        // let count = monitor.device_count()?;
        // for i in 0..count {
        //     if !inner_state.ignored_gpus.contains(&i) && !inner_state.gpu_allocations.contains_key(&i) {
        //          inner_state.gpu_allocations.insert(i, None); // 标记为可用但未分配
        //     }
        //     // 可以选择在这里获取一次初始 GpuStats，但这通常由监控线程负责更新
        // }


        Ok(Self {
            inner: Arc::new(RwLock::new(inner_state)),
            persistence_path: Arc::from(path_ref.to_path_buf()),
        })
    }

    /// 从磁盘加载状态
    async fn load_from_disk(path: &Path) -> Result<InnerDaemonState> {
        let data = tokio::fs::read(path).await.context("Failed to read persistence file")?;
        let (state, _): (InnerDaemonState, usize) = bincode::decode_from_slice(&data, bincode::config::standard())
                                        .context("Failed to decode state from file")?;
        Ok(state)
    }

    /// 将当前状态持久化到磁盘
    pub async fn save_to_disk(&self) -> Result<()> {
        let state = self.inner.read().await;
        let encoded = bincode::encode_to_vec(&*state, bincode::config::standard())
                        .context("Failed to encode state")?;
        tokio::fs::write(self.persistence_path.as_ref(), encoded)
            .await
            .context("Failed to write state to persistence file")?;
        log::debug!("Successfully saved daemon state to {}", self.persistence_path.display());
        Ok(())
    }

    // --- 任务相关操作 ---

    pub async fn add_task(&self, task: TaskMeta) -> Result<()> {
        let mut state = self.inner.write().await;
        let task_id = task.id;
        if state.tasks.insert(task_id, task).is_some() {
             log::warn!("Task with ID {} already exists, overwriting.", task_id);
        } else {
             log::info!("Added new task with ID {}.", task_id);
        }
        // 可以在这里触发一次保存状态
        // drop(state); // 释放写锁
        // self.save_to_disk().await?;
        Ok(())
    }

    pub async fn get_task(&self, task_id: u64) -> Option<TaskMeta> {
        self.inner.read().await.tasks.get(&task_id).cloned()
    }

    pub async fn update_task_state(&self, task_id: u64, new_state: TaskState) -> Result<()> {
        let mut state = self.inner.write().await;
        if let Some(task) = state.tasks.get_mut(&task_id) {
            task.state = new_state;
            Ok(())
        } else {
            Err(anyhow::anyhow!("Task with ID {} not found", task_id))
        }
    }

     pub async fn remove_task(&self, task_id: u64) -> Option<TaskMeta> {
        self.inner.write().await.tasks.remove(&task_id)
    }

    pub async fn get_all_tasks(&self) -> Vec<TaskMeta> {
        self.inner.read().await.tasks.values().cloned().collect::<Vec<_>>() // 显式 collect 类型
    }


    // --- 队列相关操作 ---

    pub async fn add_queue(&self, queue: QueueMeta) -> Result<()> {
        let mut state = self.inner.write().await;
        let queue_name = queue.name.clone();
        if state.queues.insert(queue_name.clone(), queue).is_some() {
            log::warn!("Queue with name '{}' already exists, overwriting.", queue_name);
        } else {
            log::info!("Added new queue '{}'.", queue_name);
        }
        Ok(())
    }

    pub async fn get_queue(&self, name: &str) -> Option<QueueMeta> {
        self.inner.read().await.queues.get(name).cloned()
    }

     pub async fn remove_queue(&self, name: &str) -> Option<QueueMeta> {
        // 注意：删除队列前需要处理队列中的任务和分配的 GPU
        self.inner.write().await.queues.remove(name)
    }

    pub async fn get_all_queues(&self) -> Vec<QueueMeta> {
        self.inner.read().await.queues.values().cloned().collect::<Vec<_>>() // 显式 collect 类型
    }


    // --- GPU 相关操作 ---

    pub async fn update_gpu_stats(&self, gpu_id: u32, stats: GpuStats) {
        self.inner.write().await.gpu_stats.insert(gpu_id, stats);
    }

    pub async fn get_gpu_stats(&self, gpu_id: u32) -> Option<GpuStats> {
        self.inner.read().await.gpu_stats.get(&gpu_id).cloned()
    }

    pub async fn get_all_gpu_stats(&self) -> HashMap<u32, GpuStats> {
        self.inner.read().await.gpu_stats.clone()
    }

    pub async fn allocate_gpu(&self, gpu_id: u32, queue_name: String) -> Result<()> {
        let mut state = self.inner.write().await;
        if state.ignored_gpus.contains(&gpu_id) {
            return Err(anyhow::anyhow!("GPU {} is ignored and cannot be allocated.", gpu_id));
        }
        if !state.queues.contains_key(&queue_name) {
             return Err(anyhow::anyhow!("Queue '{}' does not exist.", queue_name));
        }
        // 检查 GPU 是否已被分配
        if let Some(Some(existing_queue)) = state.gpu_allocations.get(&gpu_id) {
             if existing_queue != &queue_name {
                 return Err(anyhow::anyhow!("GPU {} is already allocated to queue '{}'.", gpu_id, existing_queue));
             } else {
                 log::info!("GPU {} is already allocated to queue '{}'. No change needed.", gpu_id, queue_name);
                 return Ok(()); // 已经是目标状态
             }
        }

        state.gpu_allocations.insert(gpu_id, Some(queue_name.clone()));
        // 更新队列的 allocated_gpus 列表
        if let Some(queue) = state.queues.get_mut(&queue_name) {
            if !queue.allocated_gpus.contains(&(gpu_id as u8)) { // 注意类型转换
                 queue.allocated_gpus.push(gpu_id as u8);
            }
        }
        log::info!("Allocated GPU {} to queue '{}'.", gpu_id, queue_name);
        Ok(())
    }

    pub async fn release_gpu(&self, gpu_id: u32) -> Result<()> {
        let mut state = self.inner.write().await;
        if let Some(Some(queue_name)) = state.gpu_allocations.remove(&gpu_id) {
             // 从队列的 allocated_gpus 列表中移除
             if let Some(queue) = state.queues.get_mut(&queue_name) {
                 queue.allocated_gpus.retain(|&id| id != gpu_id as u8); // 注意类型转换
             }
             // 标记 GPU 为未分配状态
             state.gpu_allocations.insert(gpu_id, None);
             log::info!("Released GPU {} from queue '{}'.", gpu_id, queue_name);
             Ok(())
        } else if state.gpu_allocations.contains_key(&gpu_id) {
             log::info!("GPU {} was not allocated to any queue.", gpu_id);
             Ok(()) // GPU 存在但未分配，也算成功释放
        }
         else {
            Err(anyhow::anyhow!("GPU {} not found or not managed.", gpu_id))
        }
    }

     pub async fn ignore_gpu(&self, gpu_id: u32) -> Result<()> {
        // 忽略 GPU 前应先释放它
        self.release_gpu(gpu_id).await?; // 忽略错误，即使释放失败也要尝试忽略

        let mut state = self.inner.write().await;
        if state.ignored_gpus.insert(gpu_id) {
            // 从 allocations 中移除，确保不被视为可用
            state.gpu_allocations.remove(&gpu_id);
            log::info!("GPU {} is now ignored.", gpu_id);
        } else {
            log::info!("GPU {} was already ignored.", gpu_id);
        }
        Ok(())
    }

     pub async fn unignore_gpu(&self, gpu_id: u32) -> Result<()> {
        let mut state = self.inner.write().await;
        if state.ignored_gpus.remove(&gpu_id) {
             // 添加回 allocations，标记为未分配
             state.gpu_allocations.insert(gpu_id, None);
             log::info!("GPU {} is no longer ignored.", gpu_id);
             Ok(())
        } else {
             Err(anyhow::anyhow!("GPU {} was not ignored.", gpu_id))
        }
    }

    pub async fn get_gpu_allocations(&self) -> HashMap<u32, Option<String>> {
        self.inner.read().await.gpu_allocations.clone()
    }

     pub async fn get_ignored_gpus(&self) -> HashSet<u32> {
        self.inner.read().await.ignored_gpus.clone()
    }
}

// 可以在这里添加单元测试
#[cfg(test)]
mod tests {
    use super::*;
    // use gavel_core::utils::models::TaskState; // TaskState 已在上面导入
    // use gavel_core::utils::models::ResourceLimit; // ResourceLimit 已在上面导入
    use tempfile::NamedTempFile; // 确保 dev-dependencies 有 tempfile
    use anyhow::Context; // 导入 Context trait

    #[tokio::test]
    async fn test_state_persistence() -> Result<()> {
        let temp_file = NamedTempFile::new().context("Failed to create temp file")?; // 添加 context
        let file_path = temp_file.path().to_path_buf();

        // 1. 创建新状态并添加数据
        let state1 = DaemonState::new(&file_path).await?;
        state1.add_task(TaskMeta {
            id: 1,
            cmd: "echo hello".to_string(),
            gpu_require: 1,
            state: TaskState::Waiting,
            log_path: "/tmp/task1.log".to_string(),
            priority: 5,
            queue: "default".to_string(),
            create_time: 0, // 实际应用中应使用 SystemTime::now() 转换
            gpu_ids: vec![],
        }).await?;
        state1.save_to_disk().await?;

        // 2. 从文件加载状态并验证
        let state2 = DaemonState::new(&file_path).await?;
        let task = state2.get_task(1).await;
        assert!(task.is_some());
        assert_eq!(task.unwrap().cmd, "echo hello");

        Ok(())
    }

     #[tokio::test]
    async fn test_gpu_allocation_and_ignore() -> Result<()> {
        let temp_file = NamedTempFile::new().context("Failed to create temp file")?; // 添加 context
        let state = DaemonState::new(temp_file.path()).await?;

        // 假设存在 GPU 0 和队列 'q1'
        state.add_queue(QueueMeta {
            name: "q1".to_string(),
            max_concurrent: 1,
            priority: 5,
            waiting_tasks: vec![],
            running_tasks: vec![],
            allocated_gpus: vec![],
            resource_limit: ResourceLimit { max_mem: 0, min_compute: 0.0 }, // 使用导入的 ResourceLimit
        }).await?;
        // 初始化时假设 GPU 0 是存在的且未分配 (模拟监控发现)
        state.inner.write().await.gpu_allocations.insert(0, None);


        // 分配 GPU 0 到 q1
        state.allocate_gpu(0, "q1".to_string()).await?;
        let allocs = state.get_gpu_allocations().await;
        assert_eq!(allocs.get(&0), Some(&Some("q1".to_string())));
        let queue = state.get_queue("q1").await.unwrap();
        assert!(queue.allocated_gpus.contains(&0));


        // 忽略 GPU 0
        // 先释放才能忽略
        state.release_gpu(0).await?; // 确保先释放
        state.ignore_gpu(0).await?;
        let ignored = state.get_ignored_gpus().await;
        assert!(ignored.contains(&0));
        let allocs_after_ignore = state.get_gpu_allocations().await;
        assert!(!allocs_after_ignore.contains_key(&0)); // 忽略后不应出现在 allocations 中


        // 尝试分配被忽略的 GPU
        let result = state.allocate_gpu(0, "q1".to_string()).await;
        assert!(result.is_err());
        assert!(format!("{}", result.unwrap_err()).contains("is ignored")); // 检查错误信息


        // 取消忽略 GPU 0
        state.unignore_gpu(0).await?;
        let ignored_after_unignore = state.get_ignored_gpus().await;
        assert!(!ignored_after_unignore.contains(&0));
        let allocs_after_unignore = state.get_gpu_allocations().await;
         assert_eq!(allocs_after_unignore.get(&0), Some(&None)); // 取消忽略后应为未分配状态


        // 再次分配
         state.allocate_gpu(0, "q1".to_string()).await?;
         let allocs_final = state.get_gpu_allocations().await;
         assert_eq!(allocs_final.get(&0), Some(&Some("q1".to_string())));


        Ok(())
    }
}

