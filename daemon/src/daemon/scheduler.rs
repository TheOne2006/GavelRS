use crate::daemon::state::DaemonState;
use anyhow::Result;
use gavel_core::gpu::monitor::GpuStats; // Assuming GpuStats is here
use gavel_core::utils::models::{MemoryRequirementType, ResourceLimit, TaskMeta, TaskState}; // Import necessary models, ResourceLimit, MemoryRequirementType
use log::{error, info, warn};
use std::collections::{HashMap, HashSet}; // Add import for HashSet and HashMap
use std::process::Stdio;
use std::time::Duration;
use tokio::fs::File; // For file operations
use tokio::process::Command; // For launching processes
use tokio::time::sleep; // For redirecting output

// 调度器的主函数，在一个单独的 Tokio 任务中运行
pub async fn run_scheduler(state: DaemonState) {
    info!("Scheduler started.");
    loop {
        // 1. 更新 GPU 状态 (Now using the method from DaemonState)
        match state.update_all_gpu_stats().await {
            Ok(_) => { /* GPU stats updated successfully */ }
            Err(e) => error!("Failed to update GPU stats: {}", e),
        }

        // 2. 尝试调度任务
        match schedule_tasks(&state).await {
            Ok(_) => { /* Scheduling cycle completed */ }
            Err(e) => error!("Error during scheduling cycle: {}", e),
        }

        // 3. 尝试更新任务
        match update_tasks(&state).await {
            Ok(_) => { /* Updating cycle completed */ }
            Err(e) => error!("Error during updating cycle: {}", e),
        }

        // 等待一段时间再进行下一轮调度
        sleep(Duration::from_secs(3)).await; // Adjust interval as needed
    }
}

// 新增辅助函数：检查 GPU 是否满足队列的资源限制
fn is_gpu_qualifying_for_queue(gpu_id: u32, gpu_stat: &GpuStats, limit: &ResourceLimit) -> bool {
    // 1. 检查显存要求
    let free_memory_mb = gpu_stat.memory_usage.free / (1024 * 1024); // Convert bytes to MB
    let total_memory_mb = gpu_stat.memory_usage.total / (1024 * 1024); // Convert bytes to MB

    let memory_ok = match limit.memory_requirement_type {
        MemoryRequirementType::Ignore => true,
        MemoryRequirementType::AbsoluteMb => free_memory_mb >= limit.memory_requirement_value,
        MemoryRequirementType::Percentage => {
            if total_memory_mb == 0 {
                warn!(
                    "GPU {} has 0 total memory reported (after MB conversion), failing percentage requirement for a queue.",
                    gpu_id
                );
                false
            } else {
                let required_free_mb_float =
                    (limit.memory_requirement_value as f64 / 100.0) * total_memory_mb as f64;
                (free_memory_mb as f64) >= required_free_mb_float
            }
        }
    };

    if !memory_ok {
        // info!(
        //     "GPU {} failed memory requirement. Limit type: {:?}, value: {}, Stat: Free {}MB, Total {}MB",
        //     gpu_id,
        //     limit.memory_requirement_type,
        //     limit.memory_requirement_value,
        //     free_memory_mb,
        //     total_memory_mb
        // );
        return false;
    }

    // 2. 检查 GPU 利用率要求
    let utilization_ok = if limit.max_gpu_utilization < 0.0 || limit.max_gpu_utilization > 100.0 {
        true // 忽略限制
    } else {
        // Assuming GpuStats has utilization_gpu_percent directly, or it needs to be core_usage
        // Based on core/src/gpu/monitor.rs, it should be gpu_stat.core_usage
        (gpu_stat.core_usage as f32) <= limit.max_gpu_utilization
    };

    if !utilization_ok {
        // info!(
        //     "GPU {} failed utilization requirement. Limit: {}%, Stat: {}% (core_usage)",
        //     gpu_id,
        //     limit.max_gpu_utilization,
        //     gpu_stat.core_usage
        // );
        return false;
    }

    true // GPU 符合所有条件
}

// 辅助函数：执行调度逻辑 (Refactored to use DaemonState public methods)
async fn schedule_tasks(state: &DaemonState) -> Result<()> {
    // 1. 获取所有队列并按优先级排序
    let mut queues = state.get_all_queues().await;
    queues.sort_by(|a, b| b.priority.cmp(&a.priority)); // Descending priority

    // 2. 获取 GPU 分配和状态信息
    let gpu_allocations = state.get_gpu_allocations().await; // GPU ID -> Option<QueueName>
    let ignored_gpus = state.get_ignored_gpus().await; // Ignored GPUs

    // 获取当前所有 GPU 的统计信息 (假设 DaemonState 提供了此方法)
    // update_all_gpu_stats 已经在 run_scheduler 循环开始时调用，这里获取缓存/最新的状态
    let all_current_gpu_stats = state.get_all_gpu_stats().await; // Changed from get_all_current_gpu_stats

    // Pre-fetch all tasks to avoid repeated lookups inside loops
    let all_tasks =
        state.get_all_tasks().await.into_iter().map(|t| (t.id, t)).collect::<HashMap<_, _>>();

    let mut started_tasks_this_cycle = 0;

    'queue_loop: for queue_meta in queues {
        // 3. 获取当前队列的最新状态 (refetch in case it changed)
        let current_queue = match state.get_queue(&queue_meta.name).await {
            Some(q) => q,
            None => {
                warn!("Queue {} disappeared during scheduling cycle.", queue_meta.name);
                continue 'queue_loop; // Queue no longer exists
            }
        };

        // 4. 确定此队列拥有的 GPU (原始列表)
        let owned_gpu_ids_raw: HashSet<u32> = gpu_allocations
            .iter()
            .filter(|(_, allocation)| allocation.as_ref() == Some(&current_queue.name))
            .map(|(&gpu_id, _)| gpu_id)
            .filter(|gpu_id| !ignored_gpus.contains(gpu_id)) // Exclude ignored GPUs
            .collect();

        // 新增：根据资源限制筛选符合条件的 GPU
        let mut qualifying_owned_gpus: HashSet<u32> = HashSet::new();
        for gpu_id_ref in &owned_gpu_ids_raw {
            let gpu_id = *gpu_id_ref;
            if let Some(gpu_stat) = all_current_gpu_stats.get(&gpu_id) {
                if is_gpu_qualifying_for_queue(gpu_id, gpu_stat, &current_queue.resource_limit) {
                    qualifying_owned_gpus.insert(gpu_id);
                } else {
                    // info!("GPU {} does not qualify for queue '{}' due to resource limits.", gpu_id, current_queue.name);
                }
            } else {
                warn!("Stats not found for owned GPU {}. It will be considered not qualifying for queue '{}'.", gpu_id, current_queue.name);
            }
        }

        // 5. 确定队列当前使用的 GPU
        let mut used_gpu_ids: HashSet<u32> = HashSet::new();
        for task_id in &current_queue.running_task_ids {
            if let Some(task) = all_tasks.get(task_id) {
                // Assuming task.gpu_ids stores the u8 representation
                for gpu_id_u8 in &task.gpu_ids {
                    used_gpu_ids.insert(*gpu_id_u8 as u32);
                }
            } else {
                warn!(
                    "Running task ID {} in queue {} not found in global map. Inconsistency?",
                    task_id, current_queue.name
                );
            }
        }

        // 6. 计算队列可用的 GPU (从符合条件的、拥有的 GPU 中排除已使用的)
        let mut available_gpus_for_queue: Vec<u32> = qualifying_owned_gpus // 使用筛选后的 qualifying_owned_gpus
            .difference(&used_gpu_ids)
            .cloned()
            .collect();
        // Sort for deterministic assignment (optional but good practice)
        available_gpus_for_queue.sort();

        // 7. 获取此队列的等待任务并按优先级排序
        let mut waiting_tasks_meta: Vec<TaskMeta> = Vec::new();
        for task_id in &current_queue.waiting_task_ids {
            if let Some(task) = all_tasks.get(task_id) {
                // Double check state just in case all_tasks is slightly stale
                if task.state == TaskState::Waiting {
                    waiting_tasks_meta.push(task.clone()); // Clone task meta
                } else {
                    // This might happen if a task was manually set back to waiting but not properly handled elsewhere
                    // Or if the all_tasks map is slightly out of sync with the queue list update.
                    warn!("Task {} in queue {} waiting list but state is {:?} in fetched map. Inconsistency?", task_id, current_queue.name, task.state);
                }
            } else {
                warn!("Waiting task ID {} in queue {} not found in global map. Attempting to remove from queue's waiting list.", task_id, current_queue.name);
                // TODO: Add cleanup logic in state management? -> Implemented below
                // Assumes a method in DaemonState to remove the task ID from the specific queue's waiting list.
            }
        }
        waiting_tasks_meta.sort_by(|a, b| b.priority.cmp(&a.priority)); // Descending priority

        // 8. 尝试启动等待任务
        let mut current_running_count = current_queue.running_task_ids.len(); // Track running count for concurrency check

        for task in waiting_tasks_meta {
            // Check overall queue concurrency limit first
            if current_running_count >= current_queue.max_concurrent as usize {
                // info!("Queue '{}' reached max concurrency ({}), cannot start more tasks in this cycle.", current_queue.name, current_queue.max_concurrent);
                break; // Stop trying to schedule for this queue in this cycle
            }

            let required_gpus = task.gpu_require as usize;
            let mut gpus_to_assign: Vec<u8> = Vec::new();
            let can_run: bool;

            if required_gpus > 0 {
                // GPU Task
                if available_gpus_for_queue.len() >= required_gpus {
                    // Assign GPUs from the available list
                    // Take the first 'required_gpus' IDs after sorting
                    gpus_to_assign = available_gpus_for_queue
                        .iter() // Iterate without draining yet
                        .take(required_gpus)
                        .map(|&id| id as u8) // Convert to u8 for TaskMeta
                        .collect();
                    can_run = true;
                    // info!("Found {} available GPUs ({:?}) for task {} (ID: {}) requiring {}", gpus_to_assign.len(), gpus_to_assign, task.name, task.id, required_gpus);
                } else {
                    can_run = false;
                    // info!("Not enough available GPUs in queue '{}' for task {} (ID: {}). Required: {}, Available: {}", current_queue.name, task.name, task.id, required_gpus, available_gpus_for_queue.len());
                }
            } else {
                // CPU Task (requires 0 GPUs)
                can_run = true; // Can always run if concurrency limit not hit
                                // info!("Task {} (ID: {}) is a CPU task.", task.name, task.id);
            }

            if can_run {
                info!(
                    "Attempting to start task {} (ID: {}) from queue {}",
                    task.name, task.id, current_queue.name
                );

                // 9. 更新任务状态并分配 GPU (atomically)
                // Pass the GPUs intended for assignment
                match state
                    .update_task_state(task.id, TaskState::Running, Some(gpus_to_assign.clone()))
                    .await
                {
                    Ok(_) => {
                        info!("Successfully updated state for task {} (ID: {}). Assigned GPUs: {:?}. Starting execution...", task.name, task.id, gpus_to_assign);

                        // Remove assigned GPUs from the available list *after* successful state update
                        if !gpus_to_assign.is_empty() {
                            let assigned_set: HashSet<u32> =
                                gpus_to_assign.iter().map(|&id| id as u32).collect();
                            available_gpus_for_queue.retain(|id| !assigned_set.contains(id));
                        }

                        // Fetch the updated task meta to pass to launch function
                        if let Some(updated_task) = state.get_task(task.id).await {
                            // Launch the task process asynchronously
                            // Pass &state for the updated launch_task_process signature
                            if let Err(e) = launch_task_process(&state, updated_task).await {
                                error!(
                                    "Failed to launch task process for {} (ID: {}): {}",
                                    task.name, task.id, e
                                );
                                // Attempt to revert state or handle error more robustly
                                if let Err(revert_err) =
                                    state.update_task_state(task.id, TaskState::Waiting, None).await
                                {
                                    // Revert to Waiting, clear GPUs
                                    error!(
                                        "Failed to revert task state for {} (ID: {}): {}",
                                        task.name, task.id, revert_err
                                    );
                                }
                            } else {
                                started_tasks_this_cycle += 1;
                                current_running_count += 1; // Increment running count for concurrency check
                            }
                        } else {
                            error!("Task {} (ID: {}) state updated to Running, but failed to retrieve updated meta for launching!", task.name, task.id);
                            // Attempt to revert state? Or log and potentially leave inconsistent?
                            if let Err(revert_err) =
                                state.update_task_state(task.id, TaskState::Waiting, None).await
                            {
                                error!(
                                    "Failed to revert task state for {} (ID: {}): {}",
                                    task.name, task.id, revert_err
                                );
                            }
                        }

                        // Continue to the next *task* in the same queue to see if more can be scheduled
                    }
                    Err(e) => {
                        warn!("Failed to update state for task {} (ID: {}) before starting: {}. Maybe it was removed or changed? GPUs were not removed from available list.", task.name, task.id, e);
                        // Do not modify available_gpus_for_queue as the state update failed.
                        // Continue to the next task in this queue
                    }
                }
            } else {
                // Cannot run due to resource constraints (GPU)
                // info!("Task {} (ID: {}) cannot run yet due to insufficient GPUs in queue '{}'.", task.name, task.id, current_queue.name);
                // Continue checking other tasks in the *same* queue
            }
        } // End loop through waiting tasks for this queue
    } // End loop through queues

    if started_tasks_this_cycle > 0 {
        info!("Scheduler cycle finished. Started {} tasks.", started_tasks_this_cycle);
    }

    Ok(())
}

async fn launch_task_process(state: &DaemonState, task: TaskMeta) -> Result<()> {
    info!(
        "Launching task process for \'{}\' (ID: {}). CMD: \'{}\'. Log: \'{}\'",
        task.name, task.id, task.cmd, task.log_path
    );

    // 1. Create/overwrite log file
    let log_file = match File::create(&task.log_path).await {
        Ok(file) => file,
        Err(e) => {
            error!("Failed to create log file \'{}\' for task {}: {}", task.log_path, task.id, e);
            return Err(e.into());
        }
    };

    // 2. Parse command
    let args = match shlex::split(&task.cmd) {
        Some(args) if !args.is_empty() => args,
        _ => {
            error!("Failed to parse command for task {}: \'{}\'", task.id, task.cmd);
            return Err(anyhow::anyhow!("Failed to parse command: {}", task.cmd));
        }
    };

    // 3. Build tokio::process::Command
    let mut command = Command::new(&args[0]);
    if args.len() > 1 {
        command.args(&args[1..]);
    }

    // Set environment variables
    if !task.gpu_ids.is_empty() {
        let cuda_visible_devices =
            task.gpu_ids.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(",");
        info!(
            "Task {} (ID: {}): Setting CUDA_VISIBLE_DEVICES={}",
            task.name, task.id, cuda_visible_devices
        );
        command.env("CUDA_VISIBLE_DEVICES", cuda_visible_devices);
    } else {
        info!(
            "Task {} (ID: {}): No GPUs assigned, not setting CUDA_VISIBLE_DEVICES.",
            task.name, task.id
        );
    }

    // Set working directory if specified (assuming TaskMeta has a `cwd: Option<String>` field)
    // If TaskMeta does not have `cwd`, this part should be removed or adapted.
    // For now, let's assume it might be added later and keep it commented or conditional.
    /*
    if let Some(cwd_path) = &task.cwd { // Assuming task.cwd is Option<String>
        if !cwd_path.is_empty() {
            command.current_dir(cwd_path);
            info!("Task {} (ID: {}): Set working directory to {}", task.name, task.id, cwd_path);
        }
    }
    */

    // Redirect stdout and stderr
    let log_file_stdout = match log_file.try_clone().await {
        Ok(cloned_file) => Stdio::from(cloned_file.into_std().await),
        Err(e) => {
            error!("Failed to clone log file handle for stdout (task {}): {}", task.id, e);
            return Err(e.into());
        }
    };
    let log_file_stderr = Stdio::from(log_file.into_std().await);
    command.stdout(log_file_stdout);
    command.stderr(log_file_stderr);

    // 4. Asynchronously spawn the child process
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(e) => {
            error!("Failed to spawn process for task {} (ID: {}): {}", task.name, task.id, e);
            // Attempt to set task state to Failed if spawning fails
            if let Err(update_err) =
                state.update_task_state(task.id, TaskState::Finished, None).await
            {
                error!(
                    "Additionally failed to update task {} state to Failed: {}",
                    task.id, update_err
                );
            }
            return Err(e.into());
        }
    };

    // 5. Get child PID and update state
    let pid = child.id().map(|u_pid| u_pid as i32);
    info!("Task {} (ID: {}) spawned with PID: {:?}", task.name, task.id, pid);
    if let Err(e) = state.set_task_pid(task.id, pid).await {
        error!("Failed to set PID for task {} (ID: {}): {}", task.name, task.id, e);
        // Not returning error here, as process is already spawned. Log and continue.
    }

    // 6. Asynchronously monitor the child process
    let state_clone_for_monitor = state.clone();
    let task_id_for_monitor = task.id;
    let task_name_for_monitor = task.name.clone(); // Clone name for logging

    tokio::spawn(async move {
        info!(
            "Monitoring task \'{}\' (ID: {}) with PID {:?} for completion.",
            task_name_for_monitor, task_id_for_monitor, pid
        );
        let exit_status_result = child.wait().await;

        let final_state = match exit_status_result {
            Ok(status) => {
                if status.success() {
                    info!(
                        "Task \'{}\' (ID: {}) with PID {:?} finished successfully. Exit status: {}",
                        task_name_for_monitor, task_id_for_monitor, pid, status
                    );
                    TaskState::Finished
                } else {
                    warn!(
                        "Task \'{}\' (ID: {}) with PID {:?} failed. Exit status: {}",
                        task_name_for_monitor, task_id_for_monitor, pid, status
                    );
                    TaskState::Finished
                }
            }
            Err(e) => {
                error!(
                    "Error waiting for task \'{}\' (ID: {}) with PID {:?} to complete: {}",
                    task_name_for_monitor, task_id_for_monitor, pid, e
                );
                TaskState::Finished // Assume failed if waiting errored
            }
        };

        // Update task state in DaemonState
        if let Err(e) =
            state_clone_for_monitor.update_task_state(task_id_for_monitor, final_state, None).await
        {
            error!(
                "Failed to update final state for task \'{}\' (ID: {}): {}",
                task_name_for_monitor, task_id_for_monitor, e
            );
        }

        // Clear PID in DaemonState
        if let Err(e) = state_clone_for_monitor.set_task_pid(task_id_for_monitor, None).await {
            error!(
                "Failed to clear PID for task \'{}\' (ID: {}): {}",
                task_name_for_monitor, task_id_for_monitor, e
            );
        }
        info!(
            "Monitoring finished for task \'{}\' (ID: {}).",
            task_name_for_monitor, task_id_for_monitor
        );
    });

    info!("Task \'{}\' (ID: {}) process launched and monitoring started.", task.name, task.id);
    Ok(())
}

async fn update_tasks(state: &DaemonState) -> Result<()> {
    let tasks = state.get_all_tasks().await;
    let mut tasks_to_update = Vec::new();

    for task in tasks {
        if task.state == TaskState::Running && task.pid.is_some() {
            let pid_val = task.pid.unwrap();
            // Check if process exists (Linux specific)
            match tokio::fs::metadata(format!("/proc/{}/status", pid_val)).await {
                Ok(_) => {
                    // Process still exists, do nothing
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    // Process does not exist
                    warn!(
                        "Running task \'{}\' (ID: {}, PID: {}) no longer found. Marking as Failed.",
                        task.name, task.id, pid_val
                    );
                    tasks_to_update.push((task.id, TaskState::Finished, None)); // Store task ID, new state, and None for PID
                }
                Err(e) => {
                    error!(
                        "Error checking status for task \'{}\' (ID: {}, PID: {}): {}",
                        task.name, task.id, pid_val, e
                    );
                }
            }
        }
    }

    for (task_id, new_state, new_pid) in tasks_to_update {
        if let Err(e) = state.update_task_state(task_id, new_state, None).await {
            // Pass None for assigned_gpus
            error!("Failed to update state for unexpectedly terminated task {}: {}", task_id, e);
        }
        if let Err(e) = state.set_task_pid(task_id, new_pid).await {
            // Clear PID
            error!("Failed to clear PID for unexpectedly terminated task {}: {}", task_id, e);
        }
    }

    Ok(())
}
