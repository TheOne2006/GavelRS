// src/daemon/state.rs

use anyhow::Result;
use bincode::{self, Decode, Encode};
use gavel_core::gpu::monitor::GpuMonitor;
use log::{error, info, warn}; // Import log macros
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock; // Import GpuMonitor

// 从 core crate 引入共享的数据模型
use gavel_core::gpu::monitor::GpuStats;
use gavel_core::utils::models::{QueueMeta, ResourceLimit, TaskMeta, TaskState}; // TaskState will now include Failed

// 定义守护进程的共享状态
#[derive(Debug, Clone)]
pub struct DaemonState {
    // 使用 Arc<RwLock<...>> 允许多个线程安全地读写状态
    inner: Arc<RwLock<InnerDaemonState>>,
    // 持久化文件的路径
}

// 内部状态结构，由 RwLock 保护
#[derive(Debug, Default, serde::Serialize, serde::Deserialize, Encode, Decode)]
struct InnerDaemonState {
    tasks: HashMap<u64, TaskMeta>,      // 存储所有任务，通过任务 ID 索引
    queues: HashMap<String, QueueMeta>, // 存储所有队列，通过队列名称索引
    gpu_stats: HashMap<u32, GpuStats>,  // 存储每个 GPU 的最新状态统计
    gpu_allocations: HashMap<u32, Option<String>>, // GPU ID -> 分配到的队列名称 (None 表示未分配或空闲)
    ignored_gpus: HashSet<u32>,                    // 被用户设置为忽略的 GPU ID 集合
}

// 为 DaemonState 实现方法
impl DaemonState {
    // 创建一个新的 DaemonState 实例
    pub fn new() -> Self {
        DaemonState { inner: Arc::new(RwLock::new(InnerDaemonState::default())) }
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
            info!(
                "Queue '{}' not found for task {}. Creating a new default queue.",
                queue_name, task_id
            );
            let new_queue = QueueMeta {
                name: queue_name.clone(),
                max_concurrent: 1,               // Default max concurrent tasks
                priority: 10,                    // Default priority (adjust as needed)
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

    // New method to set task PID
    pub async fn set_task_pid(&self, task_id: u64, pid: Option<i32>) -> Result<()> {
        let mut state = self.inner.write().await;
        if let Some(task) = state.tasks.get_mut(&task_id) {
            task.pid = pid;
            Ok(())
        } else {
            Err(anyhow::anyhow!("Task with ID {} not found when trying to set PID", task_id))
        }
    }

    // Modified to accept optional assigned GPU IDs and failure_reason
    pub async fn update_task_state(
        &self,
        task_id: u64,
        new_state_val: TaskState,
        assigned_gpu_ids: Option<Vec<u8>>,
        failure_reason: Option<String>, // New parameter
    ) -> Result<()> {
        let mut state = self.inner.write().await;
        if let Some(task) = state.tasks.get_mut(&task_id) {
            let old_state = task.state.clone();
            let queue_name = task.queue.clone();

            task.state = new_state_val.clone();

            if new_state_val == TaskState::Failed {
                task.failure_reason = failure_reason;
                info!("Task {} ({}) set to Failed. Reason: {}", task_id, task.name, task.failure_reason.as_deref().unwrap_or("None"));
            } else {
                // Clear failure reason if task is moving to a non-failed state
                task.failure_reason = None;
            }

            if let Some(gpus) = assigned_gpu_ids {
                task.gpu_ids = gpus;
            }

            // If state changes, update queue lists
            if old_state != new_state_val {
                if let Some(queue) = state.queues.get_mut(&queue_name) {
                    // Remove from old state's list in the queue
                    match old_state {
                        TaskState::Waiting => {
                            queue.waiting_task_ids.retain(|&id| id != task_id);
                        }
                        TaskState::Running => {
                            queue.running_task_ids.retain(|&id| id != task_id);
                        }
                        _ => {} // Finished, Failed tasks are not expected to be in these active lists by the time they are old_state
                    }

                    // Add to new state's list in the queue if applicable
                    match new_state_val {
                        TaskState::Waiting => {
                            if !queue.waiting_task_ids.contains(&task_id) {
                                queue.waiting_task_ids.push(task_id);
                            }
                        }
                        TaskState::Running => {
                            if !queue.running_task_ids.contains(&task_id) {
                                queue.running_task_ids.push(task_id);
                            }
                        }
                        TaskState::Failed | TaskState::Finished => {
                            // For Failed or Finished, ensure it's removed from active lists
                            // (already done by removing from old_state list if it was Waiting/Running)
                            // Log the transition.
                            info!("Task {} in queue {} transitioned to state {:?}. It will no longer be in waiting/running lists of this queue.", task_id, queue_name, new_state_val);
                        }
                    }
                } else {
                    warn!("Task {}'s queue {} not found while updating task state from {:?} to {:?}.", task_id, queue_name, old_state, new_state_val);
                }
            }
            Ok(())
        } else {
            error!("Task {} not found when trying to update state to {:?}.", task_id, new_state_val);
            Err(anyhow::anyhow!("Task {} not found", task_id))
        }
    }

    pub async fn update_task_queue(&self, task_id: u64, new_queue_name: String) -> Result<()> {
        let mut state = self.inner.write().await;

        // 1. Check if the destination queue exists
        if !state.queues.contains_key(&new_queue_name) {
            error!("Destination queue {} not found for task {}.", new_queue_name, task_id);
            return Err(anyhow::anyhow!("Destination queue {} not found", new_queue_name));
        }

        // 2. Get task details and update task fields first
        let (old_queue_name, task_state_at_move_start) = {
            let task = state.tasks.get_mut(&task_id).ok_or_else(|| {
                error!("Task {} not found when trying to update its queue.", task_id);
                anyhow::anyhow!("Task {} not found", task_id)
            })?;

            if task.queue == new_queue_name {
                info!("Task {} is already in queue {}. No move needed.", task_id, new_queue_name);
                return Ok(());
            }

            let old_q_name = task.queue.clone();
            let original_state = task.state.clone(); 

            task.queue = new_queue_name.clone();
            info!("Task {} field `queue` updated from {} to {}. State at move start: {:?}", task_id, old_q_name, new_queue_name, original_state);
            (old_q_name, original_state)
        };

        // 3. Remove task ID from the old queue's lists
        if old_queue_name != new_queue_name {
            if let Some(old_queue) = state.queues.get_mut(&old_queue_name) {
                match task_state_at_move_start {
                    TaskState::Waiting => {
                        old_queue.waiting_task_ids.retain(|&id| id != task_id);
                        info!("Task {} removed from waiting list of old queue {}.", task_id, old_queue_name);
                    }
                    TaskState::Running => {
                        old_queue.running_task_ids.retain(|&id| id != task_id);
                        info!("Task {} removed from running list of old queue {}.", task_id, old_queue_name);
                    }
                    TaskState::Finished | TaskState::Failed => {
                        let mut found_in_waiting = false;
                        old_queue.waiting_task_ids.retain(|&id| if id == task_id { found_in_waiting = true; false } else { true });
                        let mut found_in_running = false;
                        old_queue.running_task_ids.retain(|&id| if id == task_id { found_in_running = true; false } else { true });

                        if found_in_waiting || found_in_running {
                            warn!("Task {} (state: {:?}) was unexpectedly found in active lists (waiting: {}, running: {}) of old queue {} during move and was removed.",
                                  task_id, task_state_at_move_start, found_in_waiting, found_in_running, old_queue_name);
                        } else {
                            info!("Task {} (state: {:?}) was not in active lists of old queue {} as expected during move.", task_id, task_state_at_move_start, old_queue_name);
                        }
                    }
                }
            } else {
                warn!("Old queue {} for task {} (state {:?}) not found during queue update. Task's queue field already updated to {}.", old_queue_name, task_id, task_state_at_move_start, new_queue_name);
            }
        }

        // 4. Add task ID to the new queue's waiting list
        // When a task is moved, it's generally added to the waiting list of the new queue.
        // The scheduler will then determine if it can run based on its actual state (TaskMeta.state).
        // If a Failed/Finished task is moved, it will sit in waiting_task_ids but won't be scheduled.
        if let Some(new_queue) = state.queues.get_mut(&new_queue_name) {
            // Ensure not to add if it's already there (e.g. if old_queue == new_queue or some race)
            if !new_queue.waiting_task_ids.contains(&task_id) && !new_queue.running_task_ids.contains(&task_id) {
                new_queue.waiting_task_ids.push(task_id);
                info!("Task {} added to waiting list of new queue {}.", task_id, new_queue_name);
            } else if old_queue_name != new_queue_name { // Log only if it was an actual move attempt to a different queue
                info!("Task {} already present in lists of new queue {} or was not added to waiting list (e.g. same queue move).", task_id, new_queue_name);
            }
        }
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
            let queue_name = task.queue.clone();
            let task_state_at_removal = task.state.clone(); // Get the state at removal

            if let Some(queue) = state.queues.get_mut(&queue_name) {
                // Remove task ID from the old queue's lists based on its state when removed
                match task_state_at_removal {
                    TaskState::Waiting => {
                        queue.waiting_task_ids.retain(|&id| id != task_id);
                        info!("Task {} (Waiting) removed from queue {} waiting list.", task_id, queue_name);
                    }
                    TaskState::Running => {
                        queue.running_task_ids.retain(|&id| id != task_id);
                        info!("Task {} (Running) removed from queue {} running list.", task_id, queue_name);
                    }
                    TaskState::Finished | TaskState::Failed => {
                        // Tasks in Finished or Failed state should ideally already be out of
                        // waiting_task_ids and running_task_ids due to state updates.
                        // This is a safeguard.
                        let mut was_in_waiting = false;
                        queue.waiting_task_ids.retain(|&id| if id == task_id { was_in_waiting = true; false } else { true });
                        let mut was_in_running = false;
                        queue.running_task_ids.retain(|&id| if id == task_id { was_in_running = true; false } else { true });

                        if was_in_waiting {
                            warn!("Task {} ({:?}) was unexpectedly in waiting list of queue {} during final removal.", task_id, task_state_at_removal, queue_name);
                        }
                        if was_in_running {
                            warn!("Task {} ({:?}) was unexpectedly in running list of queue {} during final removal.", task_id, task_state_at_removal, queue_name);
                        }
                        info!("Task {} ({:?}) removed from system. Was in queue {}. Active lists checked.", task_id, task_state_at_removal, queue_name);
                    }
                }
            } else {
                warn!(
                    "Queue {} for task {} (state: {:?}) not found during task removal.",
                    queue_name, task_id, task_state_at_removal
                );
            }
            info!("Task {} (ID: {}) metadata removed from state.", task.name, task_id);
        } else {
            warn!("Attempted to remove non-existent task with ID: {}", task_id);
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
        Ok(())
    }

    pub async fn get_queue(&self, queue_name: &str) -> Option<QueueMeta> {
        self.inner.read().await.queues.get(queue_name).cloned()
    }

    pub async fn get_all_queues(&self) -> Vec<QueueMeta> {
        self.inner.read().await.queues.values().cloned().collect()
    }

    pub async fn update_queue_resource_limit(
        &self,
        queue_name: String,
        new_limit: ResourceLimit,
    ) -> Result<()> {
        let mut state = self.inner.write().await;
        if let Some(queue) = state.queues.get_mut(&queue_name) {
            queue.resource_limit = new_limit;
            Ok(())
        } else {
            Err(anyhow::anyhow!("Queue '{}' not found", queue_name))
        }
    }

    // TODO: Add methods to update queue properties (priority, limits, tasks)

    // --- GPU related methods ---}

    // New method to update stats for ALL GPUs
    pub async fn update_all_gpu_stats(&self) -> Result<()> {
        // Note: Creating GpuMonitor here assumes NVML can be initialized repeatedly.
        // Consider creating it once if performance is critical and NVML allows it.
        let monitor = match GpuMonitor::new() {
            Ok(m) => m,
            Err(e) => {
                error!("Failed to initialize GpuMonitor: {}. Skipping GPU stats update.", e);
                // Decide if this should return an error or just log and continue
                return Err(e); // Propagate the error for now
            }
        };

        let stats_results = match monitor.get_all_stats() {
            Ok(sr) => sr,
            Err(e) => {
                error!("Failed to get all GPU stats: {}. Skipping GPU stats update.", e);
                return Err(e); // Propagate the error
            }
        };

        let mut state = self.inner.write().await;

        let current_gpu_ids: HashSet<u32> =
            stats_results.iter().enumerate().map(|(i, _)| i as u32).collect();

        // Update stats for detected GPUs
        for (i, stats_result) in stats_results.into_iter().enumerate() {
            let gpu_id = i as u32;
            if state.ignored_gpus.contains(&gpu_id) {
                // If ignored, ensure stats are removed if they exist
                if state.gpu_stats.remove(&gpu_id).is_some() {
                    info!("Removed stats for ignored GPU {}", gpu_id);
                }
                continue; // Skip update for ignored GPU
            }

            match stats_result {
                Ok(stats) => {
                    state.gpu_stats.insert(gpu_id, stats);
                }
                Err(e) => warn!("Failed to get stats for GPU {}: {}", gpu_id, e),
            }
        }

        // Remove stats for GPUs that are no longer detected or became ignored
        let existing_ids: Vec<u32> = state.gpu_stats.keys().cloned().collect();
        for gpu_id in existing_ids {
            if !current_gpu_ids.contains(&gpu_id) || state.ignored_gpus.contains(&gpu_id) {
                if state.gpu_stats.remove(&gpu_id).is_some() {
                    info!("Removed stale/ignored stats for GPU {}", gpu_id);
                }
            }
        }

        Ok(())
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
