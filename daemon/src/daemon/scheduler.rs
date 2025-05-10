use crate::daemon::state::DaemonState;
use anyhow::Result;
use gavel_core::gpu::monitor::GpuStats; // Assuming GpuStats is here
use gavel_core::utils::models::{MemoryRequirementType, ResourceLimit, TaskMeta, TaskState}; // Import necessary models, ResourceLimit, MemoryRequirementType
use gavel_core::utils::DEFAULT_WAITING_QUEUE_NAME; // Import the constant
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
    let ignored_gpus = state.get_ignored_gpus().await; // Ignored GPUs
    let all_current_gpu_stats = state.get_all_gpu_stats().await;
    let gpu_allocations = state.get_gpu_allocations().await; // This variable is used below

    // Pre-fetch all tasks to avoid repeated lookups inside loops
    let all_tasks: HashMap<u64, TaskMeta> =
        state.get_all_tasks().await.into_iter().map(|t| (t.id, t)).collect();

    let mut started_tasks_this_cycle = 0;

    'queue_loop: for queue_meta in queues {
        // If this is the default waiting queue, skip it, as tasks here require explicit action to run.
        if queue_meta.name == DEFAULT_WAITING_QUEUE_NAME {
            continue 'queue_loop;
        }

        let mut available_gpus_for_queue: Vec<(u32, GpuStats)> = Vec::new();

        // Check GPUs allocated specifically to this queue first
        for (gpu_id, allocated_queue_name_opt) in &gpu_allocations {
            if ignored_gpus.contains(gpu_id) {
                continue;
            }
            if let Some(allocated_queue_name) = allocated_queue_name_opt {
                if *allocated_queue_name == queue_meta.name {
                    if let Some(gpu_stat) = all_current_gpu_stats.get(gpu_id) {
                        if is_gpu_qualifying_for_queue(*gpu_id, gpu_stat, &queue_meta.resource_limit)
                        {
                            available_gpus_for_queue.push((*gpu_id, gpu_stat.clone()));
                        }
                    }
                }
            }
        }

        // Then check unallocated (free) GPUs
        for (gpu_id, gpu_stat) in &all_current_gpu_stats {
            if ignored_gpus.contains(gpu_id) {
                continue;
            }
            if !gpu_allocations.contains_key(gpu_id) || gpu_allocations.get(gpu_id).map_or(true, |q_opt| q_opt.is_none()) {
                if !available_gpus_for_queue.iter().any(|(id, _)| id == gpu_id) {
                    if is_gpu_qualifying_for_queue(*gpu_id, gpu_stat, &queue_meta.resource_limit) {
                        available_gpus_for_queue.push((*gpu_id, gpu_stat.clone()));
                    }
                }
            }
        }
        
        let mut tasks_in_queue_to_process = Vec::new();
        for task_id in &queue_meta.waiting_task_ids {
            if let Some(task) = all_tasks.get(task_id) {
                if task.state == TaskState::Waiting {
                    tasks_in_queue_to_process.push(task.clone());
                }
            }
        }
        tasks_in_queue_to_process.sort_by(|a, b| {
            b.priority.cmp(&a.priority).then_with(|| a.create_time.cmp(&b.create_time))
        });

        let mut assigned_gpus_in_cycle: HashSet<u32> = HashSet::new();

        for task in tasks_in_queue_to_process {
            if queue_meta.running_task_ids.len() >= queue_meta.max_concurrent as usize {
                info!(
                    "Queue {} reached max concurrent tasks ({}). Task {} ({}) will wait.",
                    queue_meta.name, queue_meta.max_concurrent, task.name, task.id
                );
                continue 'queue_loop; 
            }

            if task.gpu_require == 0 {
                info!("Attempting to launch CPU-only task {} (ID: {}) from queue {}", task.name, task.id, queue_meta.name);
                match state.update_task_state(task.id, TaskState::Running, Some(Vec::new()), None).await {
                    Ok(_) => {
                        if let Some(updated_task_meta) = state.get_task(task.id).await {
                             match launch_task_process(state, updated_task_meta).await {
                                Ok(_) => {
                                    started_tasks_this_cycle += 1;
                                    info!("CPU-only task {} (ID: {}) launched successfully.", task.name, task.id);
                                }
                                Err(e) => {
                                    error!("Failed to launch process for CPU-only task {} (ID: {}): {}", task.name, task.id, e);
                                    if let Err(update_err) = state.update_task_state(task.id, TaskState::Failed, None, Some(e.to_string())).await {
                                        error!("Additionally, failed to update task {} state to Failed: {}", task.id, update_err);
                                    }
                                }
                            }
                        } else {
                            let reason = format!("State updated to Running for task {} (ID: {}), but failed to retrieve updated meta for launching!", task.name, task.id);
                            error!("{}", reason);
                            if let Err(update_err) = state.update_task_state(task.id, TaskState::Failed, None, Some(reason)).await {
                                error!("Additionally, failed to update task {} state to Failed: {}", task.id, update_err);
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to update state to Running for CPU-only task {} (ID: {}): {}", task.name, task.id, e);
                    }
                }
                continue; 
            }

            let mut selected_gpu_ids_for_task: Vec<u8> = Vec::new();
            let mut temp_available_gpus = available_gpus_for_queue.clone();
            temp_available_gpus.retain(|(gpu_id, _)| !assigned_gpus_in_cycle.contains(gpu_id));

            if temp_available_gpus.len() >= task.gpu_require as usize {
                for (gpu_id, _gpu_stat) in temp_available_gpus.iter().take(task.gpu_require as usize) {
                    selected_gpu_ids_for_task.push(*gpu_id as u8);
                }
            }

            if selected_gpu_ids_for_task.len() == task.gpu_require as usize {
                info!(
                    "Attempting to launch GPU task {} (ID: {}) from queue {} with GPUs {:?}",
                    task.name, task.id, queue_meta.name, selected_gpu_ids_for_task
                );
                for gpu_id in &selected_gpu_ids_for_task {
                    assigned_gpus_in_cycle.insert(*gpu_id as u32);
                }

                match state.update_task_state(task.id, TaskState::Running, Some(selected_gpu_ids_for_task.clone()), None).await {
                    Ok(_) => {
                        if let Some(updated_task_meta) = state.get_task(task.id).await {
                            match launch_task_process(state, updated_task_meta).await {
                                Ok(_) => {
                                    started_tasks_this_cycle += 1;
                                    info!("GPU Task {} (ID: {}) launched successfully with GPUs {:?}.", task.name, task.id, selected_gpu_ids_for_task);
                                }
                                Err(e) => {
                                    error!("Failed to launch process for GPU task {} (ID: {}): {}", task.name, task.id, e);
                                    if let Err(update_err) = state.update_task_state(task.id, TaskState::Failed, Some(selected_gpu_ids_for_task.clone()), Some(e.to_string())).await {
                                        error!("Additionally, failed to update task {} state to Failed: {}", task.id, update_err);
                                    }
                                }
                            }
                        } else {
                            let reason = format!("State updated to Running for task {} (ID: {}), but failed to retrieve updated meta for launching!", task.name, task.id);
                            error!("{}", reason);
                             if let Err(update_err) = state.update_task_state(task.id, TaskState::Failed, Some(selected_gpu_ids_for_task.clone()), Some(reason)).await {
                                error!("Additionally, failed to update task {} state to Failed: {}", task.id, update_err);
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to update state to Running for task {} (ID: {}): {}", task.name, task.id, e);
                        for gpu_id in &selected_gpu_ids_for_task {
                            assigned_gpus_in_cycle.remove(&(*gpu_id as u32));
                        }
                    }
                }
            } 
        } 
    } 

    if started_tasks_this_cycle > 0 {
        info!("Scheduler cycle finished. Started {} tasks.", started_tasks_this_cycle);
    }
    Ok(())
}

async fn launch_task_process(state: &DaemonState, task: TaskMeta) -> Result<()> {
    info!(
        "Launching process for task '{}' (ID: {}), CMD: '{}', LOG: '{}', GPUS: {:?}",
        task.name, task.id, task.cmd, task.log_path, task.gpu_ids
    );

    let log_file = match File::create(&task.log_path).await {
        Ok(f) => f,
        Err(e) => {
            return Err(anyhow::anyhow!("Failed to create log file {} for task {}: {}", task.log_path, task.id, e));
        }
    };

    let args = match shlex::split(&task.cmd) {
        Some(a) if !a.is_empty() => a,
        _ => {
            return Err(anyhow::anyhow!("Failed to parse command for task {}: '{}'", task.id, task.cmd));
        }
    };

    let mut command = Command::new(&args[0]);
    if args.len() > 1 {
        command.args(&args[1..]);
    }

    if !task.gpu_ids.is_empty() {
        let cuda_visible_devices = task.gpu_ids.iter().map(|id| id.to_string()).collect::<Vec<String>>().join(",");
        command.env("CUDA_VISIBLE_DEVICES", cuda_visible_devices);
    }

    let log_file_stdout = match log_file.try_clone().await {
        Ok(cloned_f) => Stdio::from(cloned_f.into_std().await),
        Err(e) => {
            return Err(anyhow::anyhow!("Failed to clone log file handle for stdout for task {}: {}", task.id, e));
        }
    };
    let log_file_stderr = Stdio::from(log_file.into_std().await);
    command.stdout(log_file_stdout);
    command.stderr(log_file_stderr);

    let mut child = match command.spawn() {
        Ok(c) => c,
        Err(e) => {
            return Err(anyhow::anyhow!("Failed to spawn command '{}' for task {}: {}", args[0], task.id, e));
        }
    };

    let pid = child.id().map(|u_pid| u_pid as i32);
    info!("Task {} (ID: {}) spawned with PID: {:?}", task.name, task.id, pid);
    if let Err(e) = state.set_task_pid(task.id, pid).await {
        error!(
            "CRITICAL: Task {} (ID: {}) spawned (PID: {:?}), but FAILED to set PID in state: {}. Manual intervention may be needed.",
            task.name, task.id, pid, e
        );
    }

    let state_clone_for_monitor = state.clone();
    let task_id_for_monitor = task.id;
    let task_name_for_monitor = task.name.clone();

    tokio::spawn(async move {
        info!("Monitoring process for task '{}' (ID: {}) PID: {:?}", task_name_for_monitor, task_id_for_monitor, pid);
        match child.wait().await {
            Ok(status) => {
                info!(
                    "Task '{}' (ID: {}) (PID: {:?}) exited with status: {}",
                    task_name_for_monitor, task_id_for_monitor, pid, status
                );
                let final_state = if status.success() {
                    TaskState::Finished
                } else {
                    TaskState::Failed
                };
                let reason = if status.success() {
                    None
                } else {
                    Some(format!("Process exited with status: {}", status))
                };

                if let Err(e) = state_clone_for_monitor.update_task_state(task_id_for_monitor, final_state.clone(), None, reason.clone()).await {
                    error!(
                        "Failed to update task '{}' (ID: {}) state to {:?} after process exit: {}",
                        task_name_for_monitor, task_id_for_monitor, final_state, e
                    );
                } else {
                    info!("Task '{}' (ID: {}) state updated to {:?} (Reason: {:?}) after process exit.", task_name_for_monitor, task_id_for_monitor, final_state, reason.as_deref().unwrap_or("None"));
                }
            }
            Err(e) => { // e is std::io::Error
                let io_error_string = e.to_string(); // Convert std::io::Error to String immediately.

                error!(
                    "Error waiting for task '{}' (ID: {}) (PID: {:?}) process. IO Error: {}",
                    task_name_for_monitor, task_id_for_monitor, pid, io_error_string
                );

                let reason_for_state = format!("Process monitoring failed: {}", io_error_string);

                if let Err(update_err) = state_clone_for_monitor.update_task_state(task_id_for_monitor, TaskState::Failed, None, Some(reason_for_state.clone())).await {
                    error!(
                        "Additionally, failed to update task '{}' (ID: {}) state to Failed: {}",
                        task_name_for_monitor, task_id_for_monitor, update_err
                    );
                } else {
                     info!("Task '{}' (ID: {}) state updated to Failed. Reason: {}", task_name_for_monitor, task_id_for_monitor, reason_for_state);
                }
            }
        }
    });

    info!("Task '{}' (ID: {}) process launched and monitoring started.", task.name, task.id);
    Ok(())
}

async fn update_tasks(state: &DaemonState) -> Result<()> {
    let tasks = state.get_all_tasks().await;
    let mut tasks_to_update: Vec<(u64, TaskState, Option<String>)> = Vec::new();

    for task in tasks {
        if task.state == TaskState::Running {
            if let Some(pid_val) = task.pid {
                let pid_exists = match tokio::process::Command::new("kill")
                    .arg("-0")
                    .arg(pid_val.to_string())
                    .status()
                    .await
                {
                    Ok(status) => status.success(),
                    Err(_) => false, 
                };

                if !pid_exists {
                    warn!(
                        "Running task {} (ID: {}) PID {} seems to have disappeared unexpectedly.",
                        task.name, task.id, pid_val
                    );
                    let reason = format!("Process PID {} disappeared unexpectedly.", pid_val);
                    tasks_to_update.push((task.id, TaskState::Failed, Some(reason)));
                }
            } else {
                warn!(
                    "Task {} (ID: {}) is in Running state but has no PID. Marking as Failed.",
                    task.name, task.id
                );
                let reason = "Task was in Running state without a PID.".to_string();
                tasks_to_update.push((task.id, TaskState::Failed, Some(reason)));
            }
        }
    }

    for (task_id, new_state, reason) in tasks_to_update {
        if let Err(e) = state.update_task_state(task_id, new_state.clone(), None, reason.clone()).await {
            error!("Failed to update task {} to state {:?} (Reason: {:?}): {}", task_id, new_state, reason.as_deref().unwrap_or("None"), e);
        } else {
            info!("Task {} state updated to {:?} (Reason: {:?}) by update_tasks.", task_id, new_state, reason.as_deref().unwrap_or("None"));
        }
    }

    Ok(())
}
