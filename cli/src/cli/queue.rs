use anyhow::{Result, anyhow, Context}; // Added anyhow imports
use structopt::StructOpt;
use gavel_core::rpc::message::{Message, QueueAction}; // Import RPC messages
use gavel_core::rpc::request_reply; // Import RPC function
use crate::cli::get_socket_path; // Import socket path helper
use colored::*; // Import colored
use gavel_core::utils::models::{ResourceLimit, MemoryRequirementType}; // Import ResourceLimit and MemoryRequirementType

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

    #[structopt(name = "set-limit")]
    SetLimit {
        /// Name of the queue to modify
        queue_name: String,

        /// Memory requirement type: "ignore", "absolute", "percentage"
        #[structopt(long)]
        mem_type: String,

        /// Memory requirement value (MB for absolute, 0-100 for percentage)
        #[structopt(long, default_value = "0")]
        mem_value: u64,

        /// Maximum GPU utilization percentage (e.g., 75.0).
        /// Values outside 0.0-100.0 (e.g., -1.0) will ignore this limit.
        #[structopt(long, default_value = "-1.0")]
        max_util: f32,

        /// Optional path to config file
        #[structopt(long)]
        config: Option<String>,
    }
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
            Self::SetLimit { config, .. } => config.clone(),
        };
        let socket_path = get_socket_path(config_path.as_deref())?;

        match self {
            Self::List { .. } => Self::handle_list(&socket_path),
            Self::Status { queue_name, .. } => Self::handle_status(&socket_path, queue_name),
            Self::Merge { source, dest, .. } => Self::handle_merge(&socket_path, source, dest),
            Self::Create { queue_name, priority, .. } => Self::handle_create(&socket_path, queue_name, priority),
            Self::Move { task_id, dest_queue, .. } => Self::handle_move(&socket_path, task_id, dest_queue),
            Self::Priority { task_id, level, .. } => Self::handle_priority(&socket_path, task_id, level),
            Self::SetLimit { queue_name, mem_type, mem_value, max_util, .. } => {
                Self::handle_set_limit(&socket_path, queue_name, mem_type, mem_value, max_util)
            }
        }
    }

    fn handle_list(socket_path: &str) -> Result<()> {
        println!("{} Listing all queues via RPC...", "[INFO]".blue());
        let request = Message::QueueCommand(QueueAction::List);

        match request_reply(socket_path, &request) {
            Ok(Message::QueueStatus(queues)) => {
                if queues.is_empty() {
                    println!("{} No queues found.", "[INFO]".blue());
                } else {
                    // Pretty print queue list with colors
                    println!(
                        "{}",
                        format!(
                            "{:<15} {:<10} {:<10} {:<10} {:<15}",
                            "Name".bold().underline(),
                            "Priority".bold().underline(),
                            "Waiting".bold().underline(),
                            "Running".bold().underline(),
                            "Allocated GPUs".bold().underline()
                        )
                    );
                    println!("{}", "-".repeat(65)); // Separator line
                    for queue in queues {
                        println!(
                            "{:<15} {:<10} {:<10} {:<10} {:<15}",
                            queue.name.cyan(), // Color queue name
                            queue.priority.to_string().yellow(), // Color priority
                            queue.waiting_task_ids.len(),
                            queue.running_task_ids.len(),
                            format!("{:?}", queue.allocated_gpus).magenta() // Color GPU list
                        );
                    }
                }
                Ok(())
            }
            Ok(Message::Ack(msg)) => { // Handle case where daemon sends Ack (e.g., "No queues found")
                println!("{} Daemon reply: {}", "[INFO]".blue(), msg.italic()); // Format Ack
                Ok(())
            }
            Ok(Message::Error(err_msg)) => Err(anyhow!("{} Daemon returned error: {}", "[ERROR]".red(), err_msg)),
            Ok(other) => Err(anyhow!("{} Received unexpected reply: {:?}", "[ERROR]".red(), other)),
            Err(e) => Err(anyhow!("{} Failed to send list command to daemon", "[ERROR]".red()).context(e)),
        }
    }

    fn handle_status(socket_path: &str, queue_name: String) -> Result<()> {
        println!("{} Getting status for queue '{}' via RPC...", "[INFO]".blue(), queue_name.cyan()); // Color queue name
        let request = Message::QueueCommand(QueueAction::Status { queue_name: queue_name.clone() });

        match request_reply(socket_path, &request) {
            Ok(Message::QueueStatus(queues)) => {
                if let Some(queue) = queues.first() {
                    // Pretty print queue details with colors
                    println!("Queue Details: {}", queue.name.bold().cyan());
                    println!("  {:<15} {}", "Priority:".green(), queue.priority.to_string().yellow());
                    println!("  {:<15} {}", "Max Concurrent:".green(), queue.max_concurrent); // Assuming QueueMeta has this
                    println!("  {:<15} {} ({:?})", "Waiting Tasks:".green(), queue.waiting_task_ids.len(), queue.waiting_task_ids);
                    println!("  {:<15} {} ({:?})", "Running Tasks:".green(), queue.running_task_ids.len(), queue.running_task_ids);
                    // Convert ColoredString to String before joining
                    println!("  {:<15} {}", "Allocated GPUs:".green(), queue.allocated_gpus.iter().map(|id| id.to_string().magenta().to_string()).collect::<Vec<_>>().join(", "));
                    // Print resource limits if available
                    // println!("  Resource Limits: MaxMem={}, MinCompute={}", queue.resource_limit.max_mem, queue.resource_limit.min_compute);
                } else {
                    // Should not happen if daemon returns QueueStatus, but handle defensively
                    println!("{} No details returned for queue {}.", "[WARN]".yellow(), queue_name.cyan());
                }
                Ok(())
            }
            Ok(Message::Error(err_msg)) => Err(anyhow!("{} Daemon returned error: {}", "[ERROR]".red(), err_msg)),
            Ok(other) => Err(anyhow!("{} Received unexpected reply: {:?}", "[ERROR]".red(), other)),
            Err(e) => Err(anyhow!("{} Failed to send status command for queue {} to daemon", "[ERROR]".red(), queue_name).context(e)),
        }
    }

    fn handle_merge(socket_path: &str, source: String, dest: String) -> Result<()> {
        println!("{} Requesting to merge queue '{}' into '{}' via RPC...", "[INFO]".blue(), source.cyan(), dest.cyan()); // Color queue names
        let request = Message::QueueCommand(QueueAction::Merge { source: source.clone(), dest: dest.clone() });

        match request_reply(socket_path, &request) {
            Ok(Message::Ack(msg)) => {
                println!("{} Daemon reply: {}", "[SUCCESS]".green(), msg.italic()); // Format Ack
                Ok(())
            }
            Ok(Message::Error(err_msg)) => Err(anyhow!("{} Daemon returned error: {}", "[ERROR]".red(), err_msg)),
            Ok(other) => Err(anyhow!("{} Received unexpected reply: {:?}", "[ERROR]".red(), other)),
            Err(e) => Err(anyhow!("{} Failed to send merge command ({} -> {}) to daemon", "[ERROR]".red(), source, dest).context(e)),
        }
    }

    fn handle_create(socket_path: &str, queue_name: String, priority: u8) -> Result<()> {
        println!("{} Requesting to create queue '{}' with priority {} via RPC...", "[INFO]".blue(), queue_name.cyan(), priority.to_string().yellow()); // Color name and priority
        let request = Message::QueueCommand(QueueAction::Create { name: queue_name.clone(), priority });

        match request_reply(socket_path, &request) {
            Ok(Message::Ack(msg)) => {
                println!("{} Daemon reply: {}", "[SUCCESS]".green(), msg.italic()); // Format Ack
                Ok(())
            }
            Ok(Message::Error(err_msg)) => Err(anyhow!("{} Daemon returned error: {}", "[ERROR]".red(), err_msg)),
            Ok(other) => Err(anyhow!("{} Received unexpected reply: {:?}", "[ERROR]".red(), other)),
            Err(e) => Err(anyhow!("{} Failed to send create command for queue {} to daemon", "[ERROR]".red(), queue_name).context(e)),
        }
    }

    fn handle_move(socket_path: &str, task_id_str: String, dest_queue: String) -> Result<()> {
        let task_id = task_id_str.parse::<u64>().context("Invalid Task ID format, must be a number")?;
        println!("{} Requesting to move task {} to queue '{}' via RPC...", "[INFO]".blue(), task_id.to_string().yellow(), dest_queue.cyan()); // Color task ID and queue name
        let request = Message::QueueCommand(QueueAction::Move { task_id, dest_queue: dest_queue.clone() });

        match request_reply(socket_path, &request) {
            Ok(Message::Ack(msg)) => {
                println!("{} Daemon reply: {}", "[SUCCESS]".green(), msg.italic()); // Format Ack
                Ok(())
            }
            Ok(Message::Error(err_msg)) => Err(anyhow!("{} Daemon returned error: {}", "[ERROR]".red(), err_msg)),
            Ok(other) => Err(anyhow!("{} Received unexpected reply: {:?}", "[ERROR]".red(), other)),
            Err(e) => Err(anyhow!("{} Failed to send move command (task {} -> queue {}) to daemon", "[ERROR]".red(), task_id, dest_queue).context(e)),
        }
    }

    fn handle_priority(socket_path: &str, task_id_str: String, level: u8) -> Result<()> {
        let task_id = task_id_str.parse::<u64>().context("Invalid Task ID format, must be a number")?;
         if level > 9 { // Add validation consistent with handler
             return Err(anyhow!("{} Invalid priority level {}, must be 0-9", "[ERROR]".red(), level)); // Color error
         }
        println!("{} Requesting to set priority of task {} to {} via RPC...", "[INFO]".blue(), task_id.to_string().yellow(), level.to_string().yellow()); // Color task ID and level
        let request = Message::QueueCommand(QueueAction::SetPriority { task_id, level });

        match request_reply(socket_path, &request) {
            Ok(Message::Ack(msg)) => {
                println!("{} Daemon reply: {}", "[SUCCESS]".green(), msg.italic()); // Format Ack
                Ok(())
            }
            Ok(Message::Error(err_msg)) => Err(anyhow!("{} Daemon returned error: {}", "[ERROR]".red(), err_msg)),
            Ok(other) => Err(anyhow!("{} Received unexpected reply: {:?}", "[ERROR]".red(), other)),
            Err(e) => Err(anyhow!("{} Failed to send priority command (task {} -> level {}) to daemon", "[ERROR]".red(), task_id, level).context(e)),
        }
    }

    fn handle_set_limit(socket_path: &str, queue_name: String, mem_type_str: String, mem_value: u64, max_util: f32) -> Result<()> {
        println!(
            "{} Setting resource limit for queue '{}' via RPC...",
            "[INFO]".blue(),
            queue_name.cyan()
        );

        let mem_type = match mem_type_str.to_lowercase().as_str() {
            "ignore" => MemoryRequirementType::Ignore,
            "absolute" => MemoryRequirementType::AbsoluteMb,
            "percentage" => MemoryRequirementType::Percentage,
            _ => return Err(anyhow!(format!("{} Invalid memory requirement type: {}. Must be one of 'ignore', 'absolute', 'percentage'", "[ERROR]".red(), mem_type_str))),
        };

        match mem_type {
            MemoryRequirementType::Percentage => {
                if mem_value > 100 {
                    return Err(anyhow!(format!("{} Invalid memory value for percentage type: {}. Must be between 0 and 100", "[ERROR]".red(), mem_value)));
                }
            }
            MemoryRequirementType::AbsoluteMb => {
                if mem_value == 0 {
                    return Err(anyhow!(format!("{} Invalid memory value for absolute type: {}. Must be a positive number if type is 'absolute'", "[ERROR]".red(), mem_value)));
                }
            }
            MemoryRequirementType::Ignore => {
                // No specific validation for value if type is Ignore, though typically it would be 0
            }
        }

        let limit = ResourceLimit {
            memory_requirement_type: mem_type,
            memory_requirement_value: mem_value,
            max_gpu_utilization: max_util,
        };

        let request = Message::QueueCommand(QueueAction::SetResourceLimit {
            queue_name: queue_name.clone(),
            limit,
        });

        match request_reply(socket_path, &request) {
            Ok(Message::Ack(msg)) => {
                println!("{} Daemon reply: {}", "[SUCCESS]".green(), msg.italic());
                Ok(())
            }
            Ok(Message::Error(err_msg)) => Err(anyhow!(format!("{} Daemon returned error: {}", "[ERROR]".red(), err_msg))),
            Ok(other) => Err(anyhow!(format!("{} Received unexpected reply: {:?}", "[ERROR]".red(), other))),
            Err(e) => Err(anyhow!(
                format!("{} Failed to send set-limit command for queue {} to daemon", "[ERROR]".red(), queue_name)
            )
            .context(e)),
        }
    }
}