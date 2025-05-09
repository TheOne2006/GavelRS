use crate::daemon::state::DaemonState;
use anyhow::{Context, Result};
use gavel_core::rpc::message::{Message, TaskAction, TaskFilter};
use gavel_core::utils::models::TaskState;
use gavel_core::utils::DEFAULT_RUNNING_QUEUE_NAME; // Import the default running queue name
use gavel_core::utils::DEFAULT_WAITING_QUEUE_NAME;
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

/// Handles task commands
pub async fn handle_task_command(action: TaskAction, state: DaemonState) -> Result<Message> {
    match action {
        TaskAction::List { filter } => handle_task_list(filter, state).await,
        TaskAction::Info { task_id } => handle_task_info(task_id, state).await,
        TaskAction::Run { task_id } => handle_task_run(task_id, state).await,
        TaskAction::Kill { task_id } => handle_task_kill(task_id, state).await,
        TaskAction::Remove { task_id } => handle_task_remove(task_id, state).await, // Add Remove case
        TaskAction::Logs { task_id, tail } => handle_task_logs(task_id, tail, state).await,
    }
}

/// Handles the task list command
async fn handle_task_list(filter: TaskFilter, state: DaemonState) -> Result<Message> {
    log::info!("Handling task list command, filter: {:?}", filter);
    let all_tasks = state.get_all_tasks().await;
    let filtered_tasks: Vec<_> = all_tasks
        .into_iter()
        .filter(|task| match &filter {
            TaskFilter::All => true,
            TaskFilter::Running => task.state == TaskState::Running,
            TaskFilter::Finished => task.state == TaskState::Finished,
            TaskFilter::ByQueue(q_name) => &task.queue == q_name,
            TaskFilter::ByUser(_) => {
                log::warn!("Filtering by user is not yet implemented");
                true // Temporarily include all tasks
            }
        })
        .collect();

    if filtered_tasks.is_empty() {
        log::info!("No tasks found matching the criteria");
        return Ok(Message::Ack("No tasks found matching the criteria".to_string()));
    }

    log::debug!("Returning {} tasks", filtered_tasks.len());
    Ok(Message::TaskStatus(filtered_tasks))
}

/// Handles the task info command
async fn handle_task_info(task_id: u64, state: DaemonState) -> Result<Message> {
    log::info!("Handling task info command, task ID: {}", task_id);
    match state.get_task(task_id).await {
        Some(task) => {
            log::debug!("Found task {}: {:?}", task_id, task);
            Ok(Message::TaskStatus(vec![task])) // Return a list containing the single task
        }
        None => {
            log::warn!("Task with ID {} not found", task_id);
            Ok(Message::Error(format!("Task with ID {} not found", task_id)))
        }
    }
}

/// Handles the task run command
async fn handle_task_run(task_id: u64, state: DaemonState) -> Result<Message> {
    log::info!("Handling task run command, task ID: {}", task_id);

    // a. Get task metadata
    let task = match state.get_task(task_id).await {
        Some(t) => t,
        None => {
            log::warn!("Task with ID {} not found for run command", task_id);
            return Ok(Message::Error(format!("Task with ID {} not found", task_id)));
        }
    };

    // Confirm task is in Waiting state before attempting to run
    if task.queue != DEFAULT_WAITING_QUEUE_NAME {
        log::warn!(
            "Task {} is not in Waiting queue. Current state: {:?}, queue: '{}'",
            task_id,
            task.state,
            task.queue
        );
        return Ok(Message::Error(format!(
            "Task {} cannot be run. It must be in a Waiting queue. Current state: {:?}, queue: '{}'",
            task_id, task.state, task.queue
        )));
    }

    // b. Move task to the default running queue
    // This internally sets the task's queue field and moves it to the new queue's waiting_task_ids list
    // and sets its state to Waiting within that new queue.
    match state.update_task_queue(task_id, DEFAULT_RUNNING_QUEUE_NAME.to_string()).await {
        Ok(_) => {
            log::info!(
                "Task {} moved to queue '{}' and set to Waiting state there.",
                task_id,
                DEFAULT_RUNNING_QUEUE_NAME
            );
            Ok(Message::Ack(format!(
                "Task {} moved to queue '{}' and set to Waiting state, it will run soon.",
                task_id, DEFAULT_RUNNING_QUEUE_NAME
            )))
        }
        Err(e) => {
            log::error!(
                "Failed to move task {} to queue '{}': {}",
                task_id,
                DEFAULT_RUNNING_QUEUE_NAME,
                e
            );
            return Ok(Message::Error(format!(
                "Could not move task {} to queue '{}': {}",
                task_id, DEFAULT_RUNNING_QUEUE_NAME, e
            )));
        }
    }
}

/// Handles the task kill command
pub async fn handle_task_kill(task_id: u64, state: DaemonState) -> Result<Message> {
    log::info!("Handling task kill command, task ID: {}", task_id);

    // Get task info
    let task = match state.get_task(task_id).await {
        Some(t) => t,
        None => {
            log::warn!("Task with ID {} not found", task_id);
            return Ok(Message::Error(format!("Task with ID {} not found", task_id)));
        }
    };

    // Check if the task is running and has a PID
    if task.state != TaskState::Running {
        log::warn!("Task {} is not in Running state, no need to kill", task_id);
        return Ok(Message::Ack(format!(
            "Task {} is not in Running state, no need to kill",
            task_id
        )));
    }

    if let Some(pid_val) = task.pid {
        let pid = Pid::from_raw(pid_val as i32);
        // Send SIGTERM signal to the process using nix::sys::signal::kill
        match kill(pid, Signal::SIGTERM) {
            Ok(_) => {
                // Success
                log::info!("Successfully sent SIGTERM signal to process {}", pid_val);
                Ok(Message::Ack(format!("Sent kill signal to task {} (PID: {})", task_id, pid_val)))
            }
            Err(e) => {
                // Error sending signal
                log::error!("Failed to send SIGTERM signal to process {}: {}", pid_val, e);
                Ok(Message::Error(format!(
                    "Failed to kill task {} (PID: {}): {}",
                    task_id, pid_val, e
                )))
            }
        }
    } else {
        log::warn!("Task {} has no associated process ID, cannot kill", task_id);
        Ok(Message::Error(format!("Task {} has no associated process ID, cannot kill", task_id)))
    }
}

/// Handles the task remove command
async fn handle_task_remove(task_id: u64, state: DaemonState) -> Result<Message> {
    log::info!("Handling task remove command, task ID: {}", task_id);

    // Get task info to check its state
    let task = match state.get_task(task_id).await {
        Some(t) => t,
        None => {
            log::warn!("Task with ID {} not found for removal", task_id);
            return Ok(Message::Error(format!("Task with ID {} not found", task_id)));
        }
    };

    // Check if the task is in a removable state (Waiting or Finished)
    if task.state == TaskState::Running {
        log::warn!(
            "Attempted to remove running task {}. Task must be killed or finished first.",
            task_id
        );
        return Ok(Message::Error(format!(
            "Task {} is currently running (PID: {:?}). Please kill it before removing.",
            task_id,
            task.pid.unwrap_or(-1) // Show PID if available
        )));
    }

    // Attempt to remove the task from the state
    match state.remove_task(task_id).await {
        Ok(Some(_removed_task)) => {
            log::info!("Successfully removed task {}", task_id);
            Ok(Message::Ack(format!("Task {} removed successfully", task_id)))
        }
        Ok(None) => {
            // This case should ideally not happen if get_task succeeded earlier, but handle defensively
            log::warn!("Task {} was found but could not be removed (already gone?)", task_id);
            Ok(Message::Error(format!(
                "Task {} could not be removed (might have been removed already)",
                task_id
            )))
        }
        Err(e) => {
            log::error!("Failed to remove task {}: {}", task_id, e);
            Ok(Message::Error(format!("Failed to remove task {}: {}", task_id, e)))
        }
    }
}

/// Handles the task logs command
async fn handle_task_logs(task_id: u64, tail: bool, state: DaemonState) -> Result<Message> {
    log::info!("Handling task logs command, task ID: {}, tail mode: {}", task_id, tail);

    // Get task info
    let task = match state.get_task(task_id).await {
        Some(t) => t,
        None => {
            log::warn!("Task with ID {} not found", task_id);
            return Ok(Message::Error(format!("Task with ID {} not found", task_id)));
        }
    };

    // Check if log file exists
    let log_path = Path::new(&task.log_path);
    if !log_path.exists() {
        log::warn!("Log file for task {} does not exist: {}", task_id, task.log_path);
        return Ok(Message::Error(format!("Log file for task {} does not exist", task_id)));
    }

    // Read log file content
    let file =
        File::open(log_path).context(format!("Could not open log file: {}", task.log_path))?;
    let reader = BufReader::new(file);

    // If tail is true, read only the last few lines (simple implementation: read all, take last 10)
    let mut lines: Vec<String> = Vec::new();
    for line in reader.lines() {
        match line {
            Ok(l) => lines.push(l),
            Err(e) => {
                log::error!("Failed to read log line: {}", e);
                continue;
            }
        }
    }

    let log_content = if tail && lines.len() > 10 {
        let start_idx = lines.len() - 10;
        lines[start_idx..].join("\n")
    } else {
        lines.join("\n")
    };

    // Return log content
    log::debug!("Returning log content for task {}, {} lines total", task_id, lines.len());
    Ok(Message::Ack(log_content))
}
