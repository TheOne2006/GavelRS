use anyhow::Result;
use gavel_core::rpc::message::{Message, SubmitAction};
use gavel_core::utils::models::{TaskMeta, TaskState}; // Import TaskMeta and TaskState
use crate::daemon::state::DaemonState;
use std::time::{SystemTime, UNIX_EPOCH}; // For generating task IDs and timestamps
use std::path::PathBuf; // For log path

const DEFAULT_QUEUE_NAME: &str = "default"; // Define a default queue name
const DEFAULT_LOG_DIR: &str = "/tmp/gavel_logs"; // Define a default log directory

// Helper function to generate a unique task ID
fn generate_task_id() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos() as u64
}

// Helper function to generate log path
fn generate_log_path(task_id: u64) -> Result<String> {
    let log_dir = PathBuf::from(DEFAULT_LOG_DIR);
    std::fs::create_dir_all(&log_dir)?; // Ensure log directory exists
    Ok(log_dir.join(format!("{}.log", task_id)).to_string_lossy().to_string())
}

pub async fn handle_submit_command(action: SubmitAction, state: DaemonState) -> Result<Message> {
    match action {
        SubmitAction::Command { command, gpu_num_required, queue_name } => {
            log::info!("Handling SubmitCommand::Command: cmd={}, gpus={}, queue={:?}", command, gpu_num_required, queue_name);
            let task_id = generate_task_id();
            let log_path = generate_log_path(task_id)?;
            let queue = queue_name.unwrap_or_else(|| DEFAULT_QUEUE_NAME.to_string());
            let task = TaskMeta {
                pid: None,
                id: task_id,
                cmd: command.clone(), // Clone command string
                gpu_require: gpu_num_required,
                state: TaskState::Waiting,
                log_path,
                priority: 5, // Default priority
                queue: queue.clone(),
                create_time: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
                gpu_ids: Vec::new(),
            };
            state.add_task(task).await?;
            log::info!("Command task {} submitted to queue '{}'", task_id, queue);
            Ok(Message::Ack(format!("Command task {} submitted to queue '{}'", task_id, queue)))
        }
        SubmitAction::Script { script_path, gpu_num_required, queue_name } => {
            log::info!("Handling SubmitCommand::Script: path={}, gpus={}, queue={:?}", script_path, gpu_num_required, queue_name);
            let task_id = generate_task_id();
            let log_path = generate_log_path(task_id)?;
            let queue = queue_name.unwrap_or_else(|| DEFAULT_QUEUE_NAME.to_string());
            // Assuming the command to run the script is simply the path itself
            // Adjust if a specific interpreter (like bash, python) is needed
            let command = script_path.clone();
            let task = TaskMeta {
                pid: None,
                id: task_id,
                cmd: command, // Use script path as command for now
                gpu_require: gpu_num_required,
                state: TaskState::Waiting,
                log_path,
                priority: 5, // Default priority
                queue: queue.clone(),
                create_time: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
                gpu_ids: Vec::new(),
            };
            state.add_task(task).await?;
            log::info!("Script task {} submitted to queue '{}'", task_id, queue);
            Ok(Message::Ack(format!("Script task {} submitted from path '{}' to queue '{}'", task_id, script_path, queue)))
        }
        SubmitAction::BatchJson { mut tasks, default_queue_name } => {
            let num_tasks = tasks.len();
            log::info!("Handling SubmitCommand::BatchJson: num_tasks={}, default_queue={:?}", num_tasks, default_queue_name);
            let default_q = default_queue_name.unwrap_or_else(|| DEFAULT_QUEUE_NAME.to_string());
            let mut submitted_count = 0;
            let mut errors = Vec::new();

            for task_meta in tasks.iter_mut() {
                 // Assign default queue if task queue is empty
                if task_meta.queue.is_empty() {
                    task_meta.queue = default_q.clone();
                }
                // Ensure basic fields are set (ID, state, log path, create_time if not present)
                if task_meta.id == 0 { // Assuming 0 is not a valid ID from JSON
                   task_meta.id = generate_task_id();
                }
                 if task_meta.log_path.is_empty() {
                    match generate_log_path(task_meta.id) {
                        Ok(p) => task_meta.log_path = p,
                        Err(e) => {
                             errors.push(format!("Failed to generate log path for task {}: {}", task_meta.id, e));
                             continue; // Skip this task
                        }
                    }
                }
                if task_meta.create_time == 0 {
                     task_meta.create_time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
                }
                task_meta.state = TaskState::Waiting; // Ensure state starts as Waiting
                task_meta.pid = None; // Ensure pid is None initially
                task_meta.gpu_ids = Vec::new(); // Ensure gpu_ids is empty initially


                match state.add_task(task_meta.clone()).await {
                    Ok(_) => {
                        submitted_count += 1;
                        log::info!("Batch task {} submitted to queue '{}'", task_meta.id, task_meta.queue);
                    }
                    Err(e) => {
                        let error_msg = format!("Failed to submit batch task {}: {}", task_meta.id, e);
                        log::error!("{}", error_msg);
                        errors.push(error_msg);
                    }
                }
            }

            if errors.is_empty() {
                Ok(Message::Ack(format!("Batch JSON tasks submitted: {} tasks successfully", submitted_count)))
            } else {
                Err(anyhow::anyhow!("Errors submitting batch tasks: {}", errors.join("; ")))
            }
        }
    }
}
