use anyhow::Result;
use gavel_core::rpc::message::{Message, QueueAction};
use gavel_core::utils::models::{QueueMeta, ResourceLimit};
use crate::daemon::state::DaemonState;

/// Handles queue commands
pub async fn handle_queue_command(action: QueueAction, state: DaemonState) -> Result<Message> {
    match action {
        QueueAction::List => handle_queue_list(state).await,
        QueueAction::Status { queue_name } => handle_queue_status(queue_name, state).await,
        QueueAction::Merge { source, dest } => handle_queue_merge(source, dest, state).await,
        QueueAction::Create { name, priority } => handle_queue_create(name, priority, state).await,
        QueueAction::Move { task_id, dest_queue } => handle_queue_move(task_id, dest_queue, state).await,
        QueueAction::SetPriority { task_id, level } => handle_task_priority(task_id, level, state).await,
    }
}

/// Handles the queue list command
async fn handle_queue_list(state: DaemonState) -> Result<Message> {
    log::info!("Handling queue list command");
    let queues = state.get_all_queues().await;
    
    if queues.is_empty() {
        log::info!("No queues found");
        return Ok(Message::Ack("No queues found".to_string()));
    }
    
    log::debug!("Returning {} queues", queues.len());
    Ok(Message::QueueStatus(queues))
}

/// Handles the queue status command
async fn handle_queue_status(queue_name: String, state: DaemonState) -> Result<Message> {
    log::info!("Handling queue status command, queue name: {}", queue_name);
    
    match state.get_queue(&queue_name).await {
        Some(queue) => {
            log::debug!("Found queue {}: {:?}", queue_name, queue);
            Ok(Message::QueueStatus(vec![queue]))
        }
        None => {
            log::warn!("Queue with name '{}' not found", queue_name);
            Ok(Message::Error(format!("Queue with name '{}' not found", queue_name)))
        }
    }
}

/// Handles the queue merge command
async fn handle_queue_merge(source: String, dest: String, state: DaemonState) -> Result<Message> {
    log::info!("Handling queue merge command, source: {}, destination: {}", source, dest);

    // Check if source and destination queues exist
    if state.get_queue(&source).await.is_none() {
        log::warn!("Source queue '{}' does not exist", source);
        return Ok(Message::Error(format!("Source queue '{}' does not exist", source)));
    }

    if state.get_queue(&dest).await.is_none() {
        log::warn!("Destination queue '{}' does not exist", dest);
        return Ok(Message::Error(format!("Destination queue '{}' does not exist", dest)));
    }

    if source == dest {
        log::warn!("Source and destination queues cannot be the same");
        return Ok(Message::Error("Source and destination queues cannot be the same".to_string()));
    }

    // Get all tasks and filter those in the source queue
    let all_tasks = state.get_all_tasks().await;
    let tasks_to_move: Vec<u64> = all_tasks
        .iter()
        .filter(|task| task.queue == source)
        .map(|task| task.id)
        .collect();

    // Move tasks one by one using the state method
    let mut moved_count = 0;
    let mut errors = Vec::new();
    for task_id in &tasks_to_move {
        match state.update_task_queue(*task_id, dest.clone()).await {
            Ok(_) => {
                moved_count += 1;
                log::debug!("Moved task ID: {}", task_id);
            }
            Err(e) => {
                log::error!("Failed to move task {}: {}", task_id, e);
                errors.push(format!("Task {}: {}", task_id, e));
            }
        }
    }

    // Log results
    if moved_count == 0 && errors.is_empty() {
        log::info!("No tasks needed to be moved from queue '{}'", source);
    } else if errors.is_empty() {
        log::info!("Successfully moved {} tasks from queue '{}' to queue '{}'", moved_count, source, dest);
    } else {
         log::warn!("Partially moved tasks from '{}' to '{}'. Moved: {}. Errors: {}", source, dest, moved_count, errors.join("; "));
    }

    // Return result to client
    if errors.is_empty() {
        if moved_count == 0 {
             Ok(Message::Ack(format!("No tasks needed to be moved from queue '{}'", source)))
        } else {
            Ok(Message::Ack(format!(
                "Successfully moved {} tasks from queue '{}' to queue '{}'",
                moved_count, source, dest
            )))
        }
    } else {
         Ok(Message::Error(format!(
             "Failed to move some tasks from '{}' to '{}'. Moved: {}. Errors: {}",
             source, dest, moved_count, errors.join("; ")
         )))
    }
}

/// Handles the queue create command
async fn handle_queue_create(name: String, priority: u8, state: DaemonState) -> Result<Message> {
    log::info!("Handling queue create command, name: {}, priority: {}", name, priority);

    // Check if queue already exists
    if state.get_queue(&name).await.is_some() {
        log::warn!("Queue '{}' already exists", name);
        return Ok(Message::Error(format!("Queue '{}' already exists", name)));
    }

    // 创建队列元数据
    let new_queue = QueueMeta {
        name: name.clone(),
        max_concurrent: 1, // 默认值，后续可调整
        priority,
        waiting_tasks: Vec::new(),
        running_tasks: Vec::new(),
        allocated_gpus: Vec::new(),
        resource_limit: ResourceLimit { // 默认资源限制
            max_mem: 0, // 0表示无限制
            min_compute: 0.0,
        },
    };
    
    // 添加新队列
    match state.add_queue(new_queue).await {
        Ok(_) => {
            log::info!("Successfully created queue '{}'", name);
            Ok(Message::Ack(format!("Successfully created queue '{}' with priority {}", name, priority)))
        }
        Err(e) => {
            log::error!("Failed to create queue '{}': {}", name, e);
            Ok(Message::Error(format!("Failed to create queue '{}': {}", name, e)))
        }
    }
}

/// Handles the task move command
async fn handle_queue_move(task_id: u64, dest_queue: String, state: DaemonState) -> Result<Message> {
    log::info!("Handling task move command, task ID: {}, destination queue: {}", task_id, dest_queue);

    // Get task data
    let task = match state.get_task(task_id).await {
        Some(t) => t,
        None => {
            log::warn!("Task with ID {} not found", task_id);
            return Ok(Message::Error(format!("Task with ID {} not found", task_id)));
        }
    };

    // Check if destination queue exists
    if state.get_queue(&dest_queue).await.is_none() {
        log::warn!("Destination queue '{}' does not exist", dest_queue);
        return Ok(Message::Error(format!("Destination queue '{}' does not exist", dest_queue)));
    }

    let source_queue = task.queue.clone();

    // If already in the destination queue, return success
    if source_queue == dest_queue {
        log::info!("Task {} is already in queue '{}'", task_id, dest_queue);
        return Ok(Message::Ack(format!("Task {} is already in queue '{}'", task_id, dest_queue)));
    }

    // Update the task's queue using the state method
    match state.update_task_queue(task_id, dest_queue.clone()).await {
        Ok(_) => {
            log::info!("Successfully moved task {} from queue '{}' to queue '{}'", task_id, source_queue, dest_queue);
            Ok(Message::Ack(format!("Successfully moved task {} from queue '{}' to queue '{}'", task_id, source_queue, dest_queue)))
        }
        Err(e) => {
            log::error!("Failed to move task {}: {}", task_id, e);
            Ok(Message::Error(format!("Failed to move task {}: {}", task_id, e)))
        }
    }
}

/// Handles the task priority command
async fn handle_task_priority(task_id: u64, level: u8, state: DaemonState) -> Result<Message> {
    log::info!("Handling task priority command, task ID: {}, priority level: {}", task_id, level);

    // Check if priority level is valid (0-9)
    if level > 9 {
        log::warn!("Invalid priority level {}, must be 0-9", level);
        return Ok(Message::Error(format!("Invalid priority level {}, must be 0-9", level)));
    }

    // Get the current priority before updating
    let old_priority = match state.get_task(task_id).await {
         Some(task) => task.priority,
         None => {
             log::warn!("Task with ID {} not found", task_id);
             return Ok(Message::Error(format!("Task with ID {} not found", task_id)));
         }
     };

    // Update task priority using the state method
    match state.update_task_priority(task_id, level).await {
        Ok(_) => {
            log::info!("Successfully updated priority for task {} from {} to {}", task_id, old_priority, level);
            Ok(Message::Ack(format!("Successfully updated priority for task {} from {} to {}", task_id, old_priority, level)))
        }
        Err(e) => {
            log::error!("Failed to update priority for task {}: {}", task_id, e);
            Ok(Message::Error(format!("Failed to update priority for task {}: {}", task_id, e)))
        }
    }
}