// 导出子模块
pub mod task_handler;
pub mod gpu_handler;
pub mod queue_handler;
pub mod submit_handler; // Add submit module

// 从各模块重新导出处理函数
pub use task_handler::handle_task_command;
pub use gpu_handler::handle_gpu_command;
pub use queue_handler::handle_queue_command;
pub use submit_handler::handle_submit_command; // Export submit handler

use crate::daemon::DaemonState;
use anyhow::Result;
use gavel_core::utils::models::{QueueMeta, ResourceLimit};
use gavel_core::utils::{DEFAULT_WAITING_QUEUE_NAME, DEFAULT_RUNNING_QUEUE_NAME};

// Function to ensure default queues exist
pub async fn ensure_default_queues_exist(state: &DaemonState) -> Result<()> {
    // Create default waiting queue if it doesn't exist
    if state.get_queue(DEFAULT_WAITING_QUEUE_NAME).await.is_none() {
        let waiting_queue = QueueMeta {
            name: DEFAULT_WAITING_QUEUE_NAME.to_string(),
            allocated_gpus: Vec::new(), // No GPUs allocated
            max_concurrent: 1, // Or some other sensible default
            priority: 10, // Default priority
            waiting_task_ids: Vec::new(),
            running_task_ids: Vec::new(),
            resource_limit: ResourceLimit::default(),
        };
        state.add_queue(waiting_queue).await?;
        log::info!("Created default waiting queue: {}", DEFAULT_WAITING_QUEUE_NAME);
    }

    // Create default running queue if it doesn't exist
    if state.get_queue(DEFAULT_RUNNING_QUEUE_NAME).await.is_none() {
        // Attempt to allocate all available (non-ignored) GPUs
        let all_gpus = state.get_all_gpu_stats().await;
        let ignored_gpus = state.get_ignored_gpus().await;
        let available_gpus: Vec<u32> = all_gpus.keys()
            .filter(|gpu_id| !ignored_gpus.contains(gpu_id))
            .cloned()
            .collect();

        let running_queue = QueueMeta {
            name: DEFAULT_RUNNING_QUEUE_NAME.to_string(),
            allocated_gpus: available_gpus.iter().map(|&id| id as u8).collect(), // Allocate all available non-ignored GPUs
            max_concurrent: available_gpus.len().max(1) as u8, // Allow concurrency based on GPU count
            priority: 1, // Highest priority for running queue
            waiting_task_ids: Vec::new(),
            running_task_ids: Vec::new(),
            resource_limit: ResourceLimit::default(),
        };
        state.add_queue(running_queue).await?;
        log::info!("Created default running queue: {} with GPUs: {:?}", DEFAULT_RUNNING_QUEUE_NAME, available_gpus);
    }
    Ok(())
}