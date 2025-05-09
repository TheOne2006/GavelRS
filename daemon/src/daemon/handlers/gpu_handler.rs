use anyhow::Result;
use gavel_core::rpc::message::{Message, GPUAction};
use gavel_core::gpu::monitor::GpuStats;
use crate::daemon::state::DaemonState;

/// Handles GPU commands
pub async fn handle_gpu_command(action: GPUAction, state: DaemonState) -> Result<Message> {
    match action {
        GPUAction::List => handle_gpu_list(state).await,
        GPUAction::Info { gpu_id } => handle_gpu_info(gpu_id, state).await,
        GPUAction::Allocate { gpu_ids, queue } => handle_gpu_allocate(gpu_ids, queue, state).await,
        GPUAction::Release { gpu_id } => handle_gpu_release(gpu_id, state).await,
        GPUAction::Ignore { gpu_id } => handle_gpu_ignore(gpu_id, state).await,
        GPUAction::ResetIgnored => handle_gpu_reset_ignored(state).await,
    }
}

/// Handles the GPU list command
async fn handle_gpu_list(state: DaemonState) -> Result<Message> {
    log::info!("Handling GPU list command");
    let stats_map = state.get_all_gpu_stats().await;
    let stats_vec: Vec<GpuStats> = stats_map.values().cloned().collect();
    
    if stats_vec.is_empty() {
        log::info!("No GPUs detected");
        return Ok(Message::Ack("No GPUs detected".to_string()));
    }
    
    log::debug!("Returning status for {} GPUs", stats_vec.len());
    Ok(Message::GPUStatus(stats_vec))
}

/// Handles the GPU info command
async fn handle_gpu_info(gpu_id: Option<u8>, state: DaemonState) -> Result<Message> {
    log::info!("Handling GPU info command, GPU ID: {:?}", gpu_id);
    
    match gpu_id {
        Some(id) => {
            let id_u32 = id as u32; // Convert to u32
            match state.get_gpu_stats(id_u32).await {
                Some(stats) => {
                    log::debug!("Found GPU {}: {:?}", id, stats);
                    Ok(Message::GPUStatus(vec![stats]))
                }
                None => {
                    log::warn!("GPU with ID {} not found or status unavailable", id);
                    Ok(Message::Error(format!("GPU with ID {} not found or status unavailable", id)))
                }
            }
        }
        None => {
            // If no ID is specified, return status for all GPUs
            let stats_map = state.get_all_gpu_stats().await;
            let stats_vec: Vec<GpuStats> = stats_map.values().cloned().collect();
            
            if stats_vec.is_empty() {
                log::info!("No GPUs detected");
                return Ok(Message::Ack("No GPUs detected".to_string()));
            }
            
            log::debug!("Returning status for all {} GPUs", stats_vec.len());
            Ok(Message::GPUStatus(stats_vec))
        }
    }
}

/// Handles the GPU allocate command
async fn handle_gpu_allocate(gpu_ids: Vec<u8>, queue: String, state: DaemonState) -> Result<Message> {
    log::info!("Handling GPU allocate command, GPU IDs: {:?}, queue: {}", gpu_ids, queue);
    
    if gpu_ids.is_empty() {
        log::warn!("No GPU IDs provided for allocation");
        return Ok(Message::Error("No GPU IDs provided for allocation".to_string()));
    }

    // Check if queue exists first
    if state.get_queue(&queue).await.is_none() {
        log::warn!("Destination queue '{}' does not exist", queue);
        return Ok(Message::Error(format!("Destination queue '{}' does not exist", queue)));
    }
    
    let mut errors = Vec::new();
    let mut successes = Vec::new();
    
    for gpu_id_u8 in gpu_ids {
        let gpu_id = gpu_id_u8 as u32; // Convert to u32

        // Check if GPU is ignored
        if state.get_ignored_gpus().await.contains(&gpu_id) {
            let err_msg = format!("GPU {} is ignored and cannot be allocated.", gpu_id);
            log::warn!("{}", err_msg);
            errors.push(err_msg);
            continue;
        }

        // Check if GPU is already allocated to a different queue
        if let Some(Some(existing_queue)) = state.get_gpu_allocation(gpu_id).await {
            if existing_queue != queue {
                let err_msg = format!("GPU {} is already allocated to queue '{}'.", gpu_id, existing_queue);
                log::warn!("{}", err_msg);
                errors.push(err_msg);
                continue;
            } else {
                 // Already allocated to the target queue, treat as success
                 log::info!("GPU {} is already allocated to queue '{}'. No change needed.", gpu_id, queue);
                 successes.push(gpu_id.to_string());
                 continue;
            }
        }

        // Perform allocation using the new state method
        match state.set_gpu_allocation(gpu_id, Some(queue.clone())).await {
            Ok(_) => {
                log::info!("Successfully allocated GPU {} to queue '{}'", gpu_id, queue);
                successes.push(gpu_id.to_string());
            }
            Err(e) => {
                log::error!("Failed to allocate GPU {} to queue '{}': {}", gpu_id, queue, e);
                errors.push(format!("GPU {}: {}", gpu_id, e));
            }
        }
    }
    
    // Update the queue's allocated_gpus list (This logic might need refinement depending on QueueMeta structure)
    // This part requires careful handling of the QueueMeta update, potentially needing a dedicated state method.
    // For now, we assume set_gpu_allocation handles the persistence, but QueueMeta might need separate update.
    // Example (conceptual - needs proper implementation in DaemonState or here):
    /*
    if !successes.is_empty() {
        if let Some(mut queue_meta) = state.get_queue(&queue).await {
            let mut state_write = state.inner.write().await; // Need write access if modifying queue directly
            if let Some(q) = state_write.queues.get_mut(&queue) {
                for gpu_id_str in &successes {
                    if let Ok(gpu_id) = gpu_id_str.parse::<u32>() {
                        if !q.allocated_gpus.contains(&(gpu_id as u8)) {
                            q.allocated_gpus.push(gpu_id as u8);
                        }
                    }
                }
                // Persist state after modifying queue
                if let Err(e) = state.persist().await {
                     error!("Failed to persist state after updating queue '{}' allocations: {}", queue, e);
                     // Decide how to handle persistence error - maybe revert changes or just log
                }
            }
        }
    }
    */

    if errors.is_empty() {
        Ok(Message::Ack(format!("Successfully allocated GPU(s) {} to queue '{}'", successes.join(", "), queue)))
    } else {
        if successes.is_empty() {
            Ok(Message::Error(format!("Failed to allocate GPUs to queue '{}': {}", queue, errors.join("; "))))
        } else {
            Ok(Message::Error(format!(
                "Partially failed to allocate to queue '{}': {}. Successfully allocated: {}", 
                queue, errors.join("; "), successes.join(", ")
            )))
        }
    }
}

/// Handles the GPU release command
async fn handle_gpu_release(gpu_id: u8, state: DaemonState) -> Result<Message> {
    let gpu_id_u32 = gpu_id as u32; // Convert to u32
    log::info!("Handling GPU release command, GPU ID: {}", gpu_id_u32);

    // 新增：终止正在该 GPU 上运行的任务
    let all_tasks = state.get_all_tasks().await;
    for task in all_tasks.iter() {
        // 只处理 Running 状态且 gpu_ids 包含该 gpu_id 的任务
        if task.state == gavel_core::utils::models::TaskState::Running && task.gpu_ids.contains(&(gpu_id as u8)) {
            match super::task_handler::handle_task_kill(task.id, state.clone()).await {
                Ok(msg) => {
                    log::info!("Killed task {} on GPU {}: {:?}", task.id, gpu_id_u32, msg);
                }
                Err(e) => {
                    log::error!("Failed to kill task {} on GPU {}: {}", task.id, gpu_id_u32, e);
                }
            }
        }
    }
    // Perform release using the new state method (sets allocation to None)
    match state.set_gpu_allocation(gpu_id_u32, None).await {
        Ok(_) => {
            log::info!("Successfully released GPU {}", gpu_id_u32);
            
            // Update the queue's allocated_gpus list if it was allocated
            // Similar to allocate, this might need a dedicated state method for atomicity
            /*
            if let Some(queue_name) = queue_to_update {
                 if let Some(mut queue_meta) = state.get_queue(&queue_name).await {
                     let mut state_write = state.inner.write().await; // Need write access
                     if let Some(q) = state_write.queues.get_mut(&queue_name) {
                         q.allocated_gpus.retain(|&id| id != gpu_id);
                         // Persist state after modifying queue
                         if let Err(e) = state.persist().await {
                             error!("Failed to persist state after updating queue '{}' allocations: {}", queue_name, e);
                         }
                     }
                 }
            }
            */
            Ok(Message::Ack(format!("Successfully released GPU {}", gpu_id_u32)))
        }
        Err(e) => {
            log::error!("Failed to release GPU {}: {}", gpu_id_u32, e);
            Ok(Message::Error(format!("Failed to release GPU {}: {}", gpu_id_u32, e)))
        }
    }
}

/// Handles the GPU ignore command
async fn handle_gpu_ignore(gpu_id: u8, state: DaemonState) -> Result<Message> {
    let gpu_id_u32 = gpu_id as u32; // Convert to u32
    log::info!("Handling GPU ignore command, GPU ID: {}", gpu_id_u32);

    // Check if GPU is currently allocated
    if let Some(Some(queue_name)) = state.get_gpu_allocation(gpu_id_u32).await {
        let err_msg = format!("GPU {} is allocated to queue '{}' and must be released before ignoring.", gpu_id_u32, queue_name);
        log::warn!("{}", err_msg);
        return Ok(Message::Error(err_msg));
    }
    
    // Perform ignore using the new state method
    match state.set_gpu_ignore(gpu_id_u32).await {
        Ok(_) => {
            log::info!("Successfully set GPU {} to ignored state", gpu_id_u32);
            // Ensure it's removed from allocations if it existed as None
            let _ = state.remove_gpu_allocation(gpu_id_u32).await; 
            Ok(Message::Ack(format!("GPU {} has been set to ignored state", gpu_id_u32)))
        }
        Err(e) => {
            log::error!("Failed to set GPU {} to ignored state: {}", gpu_id_u32, e);
            Ok(Message::Error(format!("Failed to set GPU {} to ignored state: {}", gpu_id_u32, e)))
        }
    }
}

/// Handles the command to reset all ignored GPUs
async fn handle_gpu_reset_ignored(state: DaemonState) -> Result<Message> {
    log::info!("Handling command to reset all ignored GPUs");
    
    let ignored_gpus = state.get_ignored_gpus().await;
    
    if ignored_gpus.is_empty() {
        log::info!("No GPUs are currently ignored");
        return Ok(Message::Ack("No GPUs are currently ignored".to_string()));
    }
    
    let mut errors = Vec::new();
    let mut successes = Vec::new();
    
    for gpu_id in ignored_gpus {
        // Perform unignore using the new state method
        match state.unset_gpu_ignore(gpu_id).await {
            Ok(_) => {
                log::info!("Successfully unignored GPU {}", gpu_id);
                // Add back to allocations as None (available)
                let _ = state.set_gpu_allocation(gpu_id, None).await;
                successes.push(gpu_id.to_string());
            }
            Err(e) => {
                log::error!("Failed to unignore GPU {}: {}", gpu_id, e);
                errors.push(format!("GPU {}: {}", gpu_id, e));
            }
        }
    }
    
    if errors.is_empty() {
        Ok(Message::Ack(format!("Successfully unignored GPUs: {}", successes.join(", "))))
    } else {
        if successes.is_empty() {
            Ok(Message::Error(format!("Failed to unignore GPUs: {}", errors.join("; "))))
        } else {
            Ok(Message::Error(format!(
                "Partially failed to unignore GPUs: {}. Successfully unignored: {}", 
                errors.join("; "), successes.join(", ")
            )))
        }
    }
}