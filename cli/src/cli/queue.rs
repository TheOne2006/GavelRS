use anyhow::{Result, anyhow, Context}; // Added anyhow imports
use structopt::StructOpt;
use gavel_core::rpc::message::{Message, QueueAction}; // Import RPC messages
use gavel_core::rpc::request_reply; // Import RPC function
use crate::cli::get_socket_path; // Import socket path helper

#[derive(StructOpt, Debug)]
pub enum QueueCommand {
    /// List all queue statuses
    #[structopt(name = "list")]
    List{
        /// Optional path to config file
        #[structopt(long)]
        config: Option<String>,
    },

    /// View queue status
    #[structopt(name = "status")]
    Status {
        /// Queue name
        queue_name: String,
        /// Optional path to config file
        #[structopt(long)]
        config: Option<String>,
    },

    /// Move all tasks from queue A to queue B
    #[structopt(name = "merge")]
    Merge {
        #[structopt(long = "from", name = "SOURCE_QUEUE")] // Use long name for clarity
        source: String,
        #[structopt(long = "to", name = "DEST_QUEUE")] // Use long name for clarity
        dest: String,
        /// Optional path to config file
        #[structopt(long)]
        config: Option<String>,
    },

    /// Create new queue
    #[structopt(name = "create")]
    Create {
        /// Queue name
        queue_name: String,
        /// Queue priority (0-9, higher is more important)
        #[structopt(long, default_value = "5")]
        priority: u8,
        /// Optional path to config file
        #[structopt(long)]
        config: Option<String>,
    },

    /// Move task to queue
    #[structopt(name = "move")]
    Move {
        /// Task ID
        task_id: String,
        /// Destination queue name
        #[structopt(name = "QUEUE_NAME")]
        dest_queue: String,
        /// Optional path to config file
        #[structopt(long)]
        config: Option<String>,
    },

    /// Set task priority
    #[structopt(name = "priority")]
    Priority {
        /// Task ID
        task_id: String,
        /// Priority level (0-9)
        level: u8, // Use u8 directly, structopt can parse it
        /// Optional path to config file
        #[structopt(long)]
        config: Option<String>,
    },
}

impl QueueCommand {
    pub fn execute(self) -> Result<()> {
        // Extract config path first
        let config_path: Option<String> = match &self {
            Self::List { config } => config.clone(),
            Self::Status { config, .. } => config.clone(),
            Self::Merge { config, .. } => config.clone(),
            Self::Create { config, .. } => config.clone(),
            Self::Move { config, .. } => config.clone(),
            Self::Priority { config, .. } => config.clone(),
        };
        let socket_path = get_socket_path(config_path.as_deref())?;

        match self {
            Self::List { .. } => Self::handle_list(&socket_path),
            Self::Status { queue_name, .. } => Self::handle_status(&socket_path, queue_name),
            Self::Merge { source, dest, .. } => Self::handle_merge(&socket_path, source, dest),
            Self::Create { queue_name, priority, .. } => Self::handle_create(&socket_path, queue_name, priority),
            Self::Move { task_id, dest_queue, .. } => Self::handle_move(&socket_path, task_id, dest_queue),
            Self::Priority { task_id, level, .. } => Self::handle_priority(&socket_path, task_id, level),
        }
    }

    fn handle_list(socket_path: &str) -> Result<()> {
        println!("Listing all queues via RPC...");
        let request = Message::QueueCommand(QueueAction::List);

        match request_reply(socket_path, &request) {
            Ok(Message::QueueStatus(queues)) => {
                if queues.is_empty() {
                    println!("No queues found.");
                } else {
                    // Pretty print queue list
                    println!("{:<15} {:<10} {:<10} {:<10} {:<15}", "Name", "Priority", "Waiting", "Running", "Allocated GPUs");
                    println!("{:-<65}", ""); // Separator line
                    for queue in queues {
                        println!(
                            "{:<15} {:<10} {:<10} {:<10} {:<15}",
                            queue.name,
                            queue.priority,
                            queue.waiting_tasks.len(),
                            queue.running_tasks.len(),
                            format!("{:?}", queue.allocated_gpus) // Display allocated GPUs
                        );
                    }
                }
                Ok(())
            }
            Ok(Message::Ack(msg)) => { // Handle case where daemon sends Ack (e.g., "No queues found")
                println!("Daemon reply: {}", msg);
                Ok(())
            }
            Ok(Message::Error(err_msg)) => Err(anyhow!("Daemon returned error: {}", err_msg)),
            Ok(other) => Err(anyhow!("Received unexpected reply: {:?}", other)),
            Err(e) => Err(anyhow!("Failed to send list command to daemon").context(e)),
        }
    }

    fn handle_status(socket_path: &str, queue_name: String) -> Result<()> {
        println!("Getting status for queue '{}' via RPC...", queue_name);
        let request = Message::QueueCommand(QueueAction::Status { queue_name: queue_name.clone() });

        match request_reply(socket_path, &request) {
            Ok(Message::QueueStatus(queues)) => {
                if let Some(queue) = queues.first() {
                    // Pretty print queue details
                    println!("Queue Details: {}", queue.name);
                    println!("  Priority:       {}", queue.priority);
                    println!("  Max Concurrent: {}", queue.max_concurrent); // Assuming QueueMeta has this
                    println!("  Waiting Tasks:  {} ({:?})", queue.waiting_tasks.len(), queue.waiting_tasks);
                    println!("  Running Tasks:  {} ({:?})", queue.running_tasks.len(), queue.running_tasks);
                    println!("  Allocated GPUs: {:?}", queue.allocated_gpus);
                    // Print resource limits if available
                    // println!("  Resource Limits: MaxMem={}, MinCompute={}", queue.resource_limit.max_mem, queue.resource_limit.min_compute);
                } else {
                    // Should not happen if daemon returns QueueStatus, but handle defensively
                    println!("No details returned for queue {}.", queue_name);
                }
                Ok(())
            }
            Ok(Message::Error(err_msg)) => Err(anyhow!("Daemon returned error: {}", err_msg)),
            Ok(other) => Err(anyhow!("Received unexpected reply: {:?}", other)),
            Err(e) => Err(anyhow!("Failed to send status command for queue {} to daemon", queue_name).context(e)),
        }
    }

    fn handle_merge(socket_path: &str, source: String, dest: String) -> Result<()> {
        println!("Requesting to merge queue '{}' into '{}' via RPC...", source, dest);
        let request = Message::QueueCommand(QueueAction::Merge { source: source.clone(), dest: dest.clone() });

        match request_reply(socket_path, &request) {
            Ok(Message::Ack(msg)) => {
                println!("Daemon reply: {}", msg);
                Ok(())
            }
            Ok(Message::Error(err_msg)) => Err(anyhow!("Daemon returned error: {}", err_msg)),
            Ok(other) => Err(anyhow!("Received unexpected reply: {:?}", other)),
            Err(e) => Err(anyhow!("Failed to send merge command ({} -> {}) to daemon", source, dest).context(e)),
        }
    }

    fn handle_create(socket_path: &str, queue_name: String, priority: u8) -> Result<()> {
        println!("Requesting to create queue '{}' with priority {} via RPC...", queue_name, priority);
        let request = Message::QueueCommand(QueueAction::Create { name: queue_name.clone(), priority });

        match request_reply(socket_path, &request) {
            Ok(Message::Ack(msg)) => {
                println!("Daemon reply: {}", msg);
                Ok(())
            }
            Ok(Message::Error(err_msg)) => Err(anyhow!("Daemon returned error: {}", err_msg)),
            Ok(other) => Err(anyhow!("Received unexpected reply: {:?}", other)),
            Err(e) => Err(anyhow!("Failed to send create command for queue {} to daemon", queue_name).context(e)),
        }
    }

    fn handle_move(socket_path: &str, task_id_str: String, dest_queue: String) -> Result<()> {
        let task_id = task_id_str.parse::<u64>().context("Invalid Task ID format, must be a number")?;
        println!("Requesting to move task {} to queue '{}' via RPC...", task_id, dest_queue);
        let request = Message::QueueCommand(QueueAction::Move { task_id, dest_queue: dest_queue.clone() });

        match request_reply(socket_path, &request) {
            Ok(Message::Ack(msg)) => {
                println!("Daemon reply: {}", msg);
                Ok(())
            }
            Ok(Message::Error(err_msg)) => Err(anyhow!("Daemon returned error: {}", err_msg)),
            Ok(other) => Err(anyhow!("Received unexpected reply: {:?}", other)),
            Err(e) => Err(anyhow!("Failed to send move command (task {} -> queue {}) to daemon", task_id, dest_queue).context(e)),
        }
    }

    fn handle_priority(socket_path: &str, task_id_str: String, level: u8) -> Result<()> {
        let task_id = task_id_str.parse::<u64>().context("Invalid Task ID format, must be a number")?;
         if level > 9 { // Add validation consistent with handler
             return Err(anyhow!("Invalid priority level {}, must be 0-9", level));
         }
        println!("Requesting to set priority of task {} to {} via RPC...", task_id, level);
        let request = Message::QueueCommand(QueueAction::SetPriority { task_id, level });

        match request_reply(socket_path, &request) {
            Ok(Message::Ack(msg)) => {
                println!("Daemon reply: {}", msg);
                Ok(())
            }
            Ok(Message::Error(err_msg)) => Err(anyhow!("Daemon returned error: {}", err_msg)),
            Ok(other) => Err(anyhow!("Received unexpected reply: {:?}", other)),
            Err(e) => Err(anyhow!("Failed to send priority command (task {} -> level {}) to daemon", task_id, level).context(e)),
        }
    }
}