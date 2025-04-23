// src/daemon/state.rs

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use anyhow::Result;
use bincode::{self, Encode, Decode};
use log::{warn, info}; // Import log macros

// 从 core crate 引入共享的数据模型
use gavel_core::utils::models::{TaskMeta, QueueMeta, TaskState, ResourceLimit};
use gavel_core::gpu::monitor::GpuStats;

// 定义守护进程的共享状态
#[derive(Debug, Clone)]
pub struct DaemonState {
    // 使用 Arc<RwLock<...>> 允许多个线程安全地读写状态
    inner: Arc<RwLock<InnerDaemonState>>,
    // 持久化文件的路径
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
    pub fn new() -> Self {
        DaemonState {
            inner: Arc::new(RwLock::new(InnerDaemonState::default())),
        }
    }

    // --- Task related methods ---

    pub async fn add_task(&self, task: TaskMeta) -> Result<()> {
        let mut state = self.inner.write().await;
        let task_id = task.id;
        let queue_name = task.queue.clone();

        // Add task to the main task map
        if state.tasks.insert(task_id, task.clone()).is_some() {
            warn!("Task with ID {} already existed and was overwritten.", task_id);
        }

        // Add task ID to the corresponding queue's waiting list
        if let Some(queue) = state.queues.get_mut(&queue_name) {
            if !queue.waiting_task_ids.contains(&task_id) {
                queue.waiting_task_ids.push(task_id);
            }
        } else {
            // Queue doesn't exist, create a new one with default settings
            info!("Queue '{}' not found for task {}. Creating a new default queue.", queue_name, task_id);
            let new_queue = QueueMeta {
                name: queue_name.clone(),
                max_concurrent: 1, // Default max concurrent tasks
                priority: 10,      // Default priority (adjust as needed)
                waiting_task_ids: vec![task_id], // Add the current task
                running_task_ids: Vec::new(),
                allocated_gpus: Vec::new(), // Default: no allocated GPUs
                resource_limit: ResourceLimit::default(), // Corrected field name
            };
            state.queues.insert(queue_name, new_queue);
            // Task ID is already added to waiting_task_ids during creation above
        }

        Ok(())
    }

    pub async fn get_task(&self, task_id: u64) -> Option<TaskMeta> {
        self.inner.read().await.tasks.get(&task_id).cloned()
    }

    pub async fn get_all_tasks(&self) -> Vec<TaskMeta> {
        self.inner.read().await.tasks.values().cloned().collect()
    }

    pub async fn update_task_state(&self, task_id: u64, new_state_val: TaskState) -> Result<()> { // Renamed arg
        let mut state = self.inner.write().await;
        if let Some(task) = state.tasks.get_mut(&task_id) {
            let old_state = task.state.clone();
            let queue_name = task.queue.clone();
            task.state = new_state_val.clone(); // Clone the new state value here

            // Update queue task lists based on state transition
            if let Some(queue) = state.queues.get_mut(&queue_name) {
                match (old_state, new_state_val.clone()) { // Use the cloned value in the match, clone again for the inner check
                    (TaskState::Waiting, TaskState::Running) => {
                        // Move from waiting to running
                        queue.waiting_task_ids.retain(|&id| id != task_id);
                        if !queue.running_task_ids.contains(&task_id) {
                            queue.running_task_ids.push(task_id);
                        }
                    }
                    (TaskState::Running, TaskState::Finished) | (TaskState::Running, TaskState::Waiting) => {
                        // Remove from running (finished or stopped/reset)
                        queue.running_task_ids.retain(|&id| id != task_id);
                        // If reset to Waiting, add back to waiting list
                        if new_state_val == TaskState::Waiting && !queue.waiting_task_ids.contains(&task_id) { // Use new_state_val here
                             queue.waiting_task_ids.push(task_id);
                        }
                    }
                    (TaskState::Waiting, TaskState::Finished) => {
                         // Remove directly from waiting if somehow finished without running
                         queue.waiting_task_ids.retain(|&id| id != task_id);
                    }
                    // Other transitions (e.g., Finished -> Waiting) might need handling depending on logic
                    _ => {} // No change in queue lists needed for other transitions
                }
            } else {
                warn!("Queue '{}' not found while updating state for task {}.", queue_name, task_id);
            }

            Ok(())
        } else {
            Err(anyhow::anyhow!("Task with ID {} not found", task_id))
        }
    }

    pub async fn update_task_queue(&self, task_id: u64, new_queue_name: String) -> Result<()> {
        let mut state = self.inner.write().await;

        // 1. Check if the destination queue exists
        if !state.queues.contains_key(&new_queue_name) {
             return Err(anyhow::anyhow!("Destination queue '{}' not found", new_queue_name));
        }

        // 2. Get task details and update task fields first
        let (old_queue_name, task_state_at_move_start) = { // Use a block to limit the scope of task borrow
            if let Some(task) = state.tasks.get_mut(&task_id) {
                let old_name = task.queue.clone();
                if old_name == new_queue_name { return Ok(()); } // No change needed

                let current_state = task.state.clone();
                task.queue = new_queue_name.clone(); // Update queue name

                // If task was running, reset its state to Waiting
                if current_state == TaskState::Running {
                    task.state = TaskState::Waiting;
                    warn!("Task {} moved while running. State reset to Waiting in new queue '{}'.", task_id, new_queue_name);
                }
                (old_name, current_state) // Return old name and original state
            } else {
                return Err(anyhow::anyhow!("Task with ID {} not found", task_id));
            }
        }; // Mutable borrow of task ends here

        // 3. Remove task ID from the old queue's lists
        if let Some(old_queue) = state.queues.get_mut(&old_queue_name) {
            match task_state_at_move_start {
                TaskState::Waiting => old_queue.waiting_task_ids.retain(|&id| id != task_id),
                TaskState::Running => old_queue.running_task_ids.retain(|&id| id != task_id),
                TaskState::Finished => {} // Or handle finished if necessary
            }
        } else {
             warn!("Old queue '{}' not found while moving task {}.", old_queue_name, task_id);
        }

        // 4. Add task ID to the new queue's waiting list
        if let Some(new_queue) = state.queues.get_mut(&new_queue_name) {
             if !new_queue.waiting_task_ids.contains(&task_id) {
                 new_queue.waiting_task_ids.push(task_id);
             }
             // Ensure it's not in the running list if it was moved while running (state was reset)
             new_queue.running_task_ids.retain(|&id| id != task_id);
        }
        // No else needed, checked existence earlier

        Ok(())
    }

    pub async fn update_task_priority(&self, task_id: u64, new_priority: u8) -> Result<()> {
        let mut state = self.inner.write().await;
        if let Some(task) = state.tasks.get_mut(&task_id) {
            task.priority = new_priority;
            Ok(())
        } else {
            Err(anyhow::anyhow!("Task with ID {} not found", task_id))
        }
    }

    pub async fn remove_task(&self, task_id: u64) -> Result<Option<TaskMeta>> {
        let mut state = self.inner.write().await;
        let removed_task = state.tasks.remove(&task_id);

        if let Some(ref task) = removed_task {
            let queue_name = &task.queue;
             // Remove task ID from the corresponding queue's lists
            if let Some(queue) = state.queues.get_mut(queue_name) {
                queue.waiting_task_ids.retain(|&id| id != task_id);
                queue.running_task_ids.retain(|&id| id != task_id);
            } else {
                 warn!("Queue '{}' not found while removing task {}. Task removed but queue lists might be inconsistent.", queue_name, task_id);
            }
        }
        // No warning if task wasn't found initially

        Ok(removed_task)
    }

    // --- Queue related methods ---

    pub async fn add_queue(&self, queue: QueueMeta) -> Result<()> {
        let mut state = self.inner.write().await;
        let queue_name = queue.name.clone();
        if state.queues.insert(queue_name.clone(), queue).is_some() {
            warn!("Queue with name '{}' already existed and was overwritten.", queue_name);
        }
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
        Ok(())
    }

    pub async fn unset_gpu_ignore(&self, gpu_id: u32) -> Result<()> {
        let mut state = self.inner.write().await;
        state.ignored_gpus.remove(&gpu_id);
        Ok(())
    }

    pub async fn get_ignored_gpus(&self) -> HashSet<u32> {
        self.inner.read().await.ignored_gpus.clone()
    }

    pub async fn set_gpu_allocation(&self, gpu_id: u32, queue_name: Option<String>) -> Result<()> {
        let mut state = self.inner.write().await;
        state.gpu_allocations.insert(gpu_id, queue_name.clone());
        Ok(())
    }

    pub async fn remove_gpu_allocation(&self, gpu_id: u32) -> Result<()> {
        let mut state = self.inner.write().await;
        state.gpu_allocations.remove(&gpu_id);
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