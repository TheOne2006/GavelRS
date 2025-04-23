use anyhow::{Result, anyhow, Context}; // Added anyhow imports
use structopt::StructOpt;
use gavel_core::rpc::message::{Message, TaskAction, TaskFilter}; // Import RPC messages
use gavel_core::rpc::request_reply; // Import RPC function
use crate::cli::get_socket_path; // Import socket path helper

#[derive(StructOpt, Debug)]
pub enum TaskCommand {
    /// List tasks (default: pending tasks)
    #[structopt(name = "list")]
    List {
        /// Show all tasks
        #[structopt(long, conflicts_with = "running", conflicts_with = "finished")]
        all: bool,

        /// Show only running tasks
        #[structopt(long, conflicts_with = "all", conflicts_with = "finished")]
        running: bool,

        /// Show only finished tasks
        #[structopt(long, conflicts_with = "all", conflicts_with = "running")]
        finished: bool,

        /// Filter by queue name
        #[structopt(long, conflicts_with_all = &["all", "running", "finished"])]
        queue: Option<String>,

        /// Optional path to config file
        #[structopt(long)]
        config: Option<String>,
    },

    /// View task details
    #[structopt(name = "info")]
    Info {
        /// Task ID
        task_id: String,
        /// Optional path to config file
        #[structopt(long)]
        config: Option<String>,
    },

    /// Add task to running queue (mark as runnable)
    #[structopt(name = "run")]
    Run {
        /// Task ID
        task_id: String,
        /// Optional path to config file
        #[structopt(long)]
        config: Option<String>,
    },

    /// Terminate task
    #[structopt(name = "kill")]
    Kill {
        /// Task ID
        task_id: String,
        /// Optional path to config file
        #[structopt(long)]
        config: Option<String>,
    },

    /// Remove a finished or waiting task
    #[structopt(name = "remove")]
    Remove {
        /// Task ID
        task_id: String,
        /// Optional path to config file
        #[structopt(long)]
        config: Option<String>,
    },

    /// View task logs
    #[structopt(name = "logs")]
    Logs {
        /// Task ID
        task_id: String,

        /// Show only log tail
        #[structopt(long)]
        tail: bool,
        /// Optional path to config file
        #[structopt(long)]
        config: Option<String>,
    },
}

impl TaskCommand {
    pub fn execute(self) -> Result<()> {
        // Extract config path first
        let config_path: Option<String> = match &self {
            Self::List { config, .. } => config.clone(),
            Self::Info { config, .. } => config.clone(),
            Self::Run { config, .. } => config.clone(),
            Self::Kill { config, .. } => config.clone(),
            Self::Remove { config, .. } => config.clone(), // Added Remove
            Self::Logs { config, .. } => config.clone(),
        };
        let socket_path = get_socket_path(config_path.as_deref())?;

        match self {
            Self::List { all, running, finished, queue, .. } => Self::handle_list(&socket_path, all, running, finished, queue),
            Self::Info { task_id, .. } => Self::handle_info(&socket_path, task_id),
            Self::Run { task_id, .. } => Self::handle_run(&socket_path, task_id),
            Self::Kill { task_id, .. } => Self::handle_kill(&socket_path, task_id),
            Self::Remove { task_id, .. } => Self::handle_remove(&socket_path, task_id), // Added Remove
            Self::Logs { task_id, tail, .. } => Self::handle_logs(&socket_path, task_id, tail),
        }
    }

    fn handle_list(socket_path: &str, all: bool, running: bool, finished: bool, queue: Option<String>) -> Result<()> {
        let filter = if all {
            TaskFilter::All
        } else if running {
            TaskFilter::Running
        } else if finished {
            TaskFilter::Finished
        } else if let Some(q) = queue {
            TaskFilter::ByQueue(q)
        } else {
            // Default filter if none specified (e.g., Waiting or Running, adjust as needed)
            // For now, let's default to All if no specific flag is given
             TaskFilter::All // Or maybe TaskFilter::Waiting? Depends on desired default.
        };
        println!("Listing tasks with filter: {:?} via RPC...", filter);

        let request = Message::TaskCommand(TaskAction::List { filter });

        match request_reply(socket_path, &request) {
            Ok(Message::TaskStatus(tasks)) => {
                if tasks.is_empty() {
                    println!("No tasks found matching the criteria.");
                } else {
                    // Pretty print the tasks (example)
                    println!("{:<5} {:<10} {:<10} {:<15} {:<8} {:<10}", "ID", "State", "Queue", "Command", "GPUs", "PID");
                    println!("{:-<65}", ""); // Separator line
                    for task in tasks {
                        println!(
                            "{:<5} {:<10} {:<10} {:<15} {:<8} {:<10}",
                            task.id,
                            format!("{:?}", task.state), // Display enum variant name
                            task.queue,
                            task.cmd.chars().take(15).collect::<String>() + if task.cmd.len() > 15 { "..." } else { "" }, // Truncate command
                            task.gpu_require,
                            task.pid.map_or_else(|| "N/A".to_string(), |p| p.to_string())
                        );
                    }
                }
                Ok(())
            }
            Ok(Message::Ack(msg)) => { // Handle case where daemon sends Ack (e.g., "No tasks found")
                println!("Daemon reply: {}", msg);
                Ok(())
            }
            Ok(Message::Error(err_msg)) => Err(anyhow!("Daemon returned error: {}", err_msg)),
            Ok(other) => Err(anyhow!("Received unexpected reply: {:?}", other)),
            Err(e) => Err(anyhow!("Failed to send list command to daemon").context(e)),
        }
    }

    fn handle_info(socket_path: &str, task_id_str: String) -> Result<()> {
        let task_id = task_id_str.parse::<u64>().context("Invalid Task ID format, must be a number")?;
        println!("Getting info for task {} via RPC...", task_id);

        let request = Message::TaskCommand(TaskAction::Info { task_id });

        match request_reply(socket_path, &request) {
             Ok(Message::TaskStatus(tasks)) => {
                 if let Some(task) = tasks.first() {
                     // Pretty print task details
                     println!("Task Details (ID: {})", task.id);
                     println!("  State:    {:?}", task.state);
                     println!("  Queue:    {}", task.queue);
                     println!("  Priority: {}", task.priority);
                     println!("  Command:  {}", task.cmd);
                     println!("  GPUs Req: {}", task.gpu_require);
                     println!("  GPUs Alloc: {:?}", task.gpu_ids);
                     println!("  PID:      {}", task.pid.map_or("N/A".to_string(), |p| p.to_string()));
                     println!("  Log Path: {}", task.log_path);
                     // Convert create_time (timestamp) to human-readable format if needed
                     println!("  Created:  {}", task.create_time); // Placeholder, needs time formatting
                 } else {
                     // Should not happen if daemon returns TaskStatus, but handle defensively
                     println!("No details returned for task {}.", task_id);
                 }
                 Ok(())
             }
             Ok(Message::Error(err_msg)) => Err(anyhow!("Daemon returned error: {}", err_msg)),
             Ok(other) => Err(anyhow!("Received unexpected reply: {:?}", other)),
             Err(e) => Err(anyhow!("Failed to send info command for task {} to daemon", task_id).context(e)),
        }
    }

    fn handle_run(socket_path: &str, task_id_str: String) -> Result<()> {
        let task_id = task_id_str.parse::<u64>().context("Invalid Task ID format, must be a number")?;
        println!("Requesting to run task {} via RPC...", task_id);

        let request = Message::TaskCommand(TaskAction::Run { task_id });

        match request_reply(socket_path, &request) {
            Ok(Message::Ack(msg)) => {
                println!("Daemon reply: {}", msg);
                Ok(())
            }
            Ok(Message::Error(err_msg)) => Err(anyhow!("Daemon returned error: {}", err_msg)),
            Ok(other) => Err(anyhow!("Received unexpected reply: {:?}", other)),
            Err(e) => Err(anyhow!("Failed to send run command for task {} to daemon", task_id).context(e)),
        }
    }

    fn handle_kill(socket_path: &str, task_id_str: String) -> Result<()> {
        let task_id = task_id_str.parse::<u64>().context("Invalid Task ID format, must be a number")?;
        println!("Requesting to kill task {} via RPC...", task_id);

        let request = Message::TaskCommand(TaskAction::Kill { task_id });

        match request_reply(socket_path, &request) {
            Ok(Message::Ack(msg)) => {
                println!("Daemon reply: {}", msg);
                Ok(())
            }
            Ok(Message::Error(err_msg)) => Err(anyhow!("Daemon returned error: {}", err_msg)),
            Ok(other) => Err(anyhow!("Received unexpected reply: {:?}", other)),
            Err(e) => Err(anyhow!("Failed to send kill command for task {} to daemon", task_id).context(e)),
        }
    }

    // Added handle_remove
    fn handle_remove(socket_path: &str, task_id_str: String) -> Result<()> {
        let task_id = task_id_str.parse::<u64>().context("Invalid Task ID format, must be a number")?;
        println!("Requesting to remove task {} via RPC...", task_id);

        let request = Message::TaskCommand(TaskAction::Remove { task_id });

        match request_reply(socket_path, &request) {
            Ok(Message::Ack(msg)) => {
                println!("Daemon reply: {}", msg);
                Ok(())
            }
            Ok(Message::Error(err_msg)) => Err(anyhow!("Daemon returned error: {}", err_msg)),
            Ok(other) => Err(anyhow!("Received unexpected reply: {:?}", other)),
            Err(e) => Err(anyhow!("Failed to send remove command for task {} to daemon", task_id).context(e)),
        }
    }


    fn handle_logs(socket_path: &str, task_id_str: String, tail: bool) -> Result<()> {
        let task_id = task_id_str.parse::<u64>().context("Invalid Task ID format, must be a number")?;
        println!("Fetching {} logs for task {} via RPC...", if tail { "tail of" } else { "full" }, task_id);

        let request = Message::TaskCommand(TaskAction::Logs { task_id, tail });

        match request_reply(socket_path, &request) {
            Ok(Message::Ack(log_content)) => {
                println!("--- Logs for Task {} ---", task_id);
                println!("{}", log_content);
                println!("--- End Logs ---");
                Ok(())
            }
            Ok(Message::Error(err_msg)) => Err(anyhow!("Daemon returned error: {}", err_msg)),
            Ok(other) => Err(anyhow!("Received unexpected reply: {:?}", other)),
            Err(e) => Err(anyhow!("Failed to send logs command for task {} to daemon", task_id).context(e)),
        }
    }
}
