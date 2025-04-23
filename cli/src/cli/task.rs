use anyhow::{Result, anyhow, Context}; // Added anyhow imports
use structopt::StructOpt;
use gavel_core::rpc::message::{Message, TaskAction, TaskFilter}; // Import RPC messages
use gavel_core::rpc::request_reply; // Import RPC function
use crate::cli::get_socket_path; // Import socket path helper
use colored::*; // Import colored
use gavel_core::utils::models::TaskState; // Import TaskState for coloring

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
            TaskFilter::ByQueue(q.clone()) // Clone queue name
        } else {
            // Default filter if none specified (e.g., Waiting or Running, adjust as needed)
            // For now, let's default to All if no specific flag is given
             TaskFilter::All // Or maybe TaskFilter::Waiting? Depends on desired default.
        };
        println!("{} Listing tasks with filter: {:?} via RPC...", "[INFO]".blue(), filter);

        let request = Message::TaskCommand(TaskAction::List { filter });

        match request_reply(socket_path, &request) {
            Ok(Message::TaskStatus(tasks)) => {
                if tasks.is_empty() {
                    println!("{} No tasks found matching the criteria.", "[INFO]".blue());
                } else {
                    // Pretty print the tasks with colors
                    println!(
                        "{}",
                        format!(
                            // Adjusted widths for better alignment
                            "{:<10} {:<20} {:<12} {:<15} {:<40} {:<5} {:<10}",
                            "ID".bold().underline(),
                            "Name".bold().underline(),
                            "State".bold().underline(),
                            "Queue".bold().underline(),
                            "Command".bold().underline(),
                            "GPUs".bold().underline(),
                            "PID".bold().underline()
                        )
                    );
                    // Adjusted separator line length
                    println!("{}", "-".repeat(118)); // Separator line
                    for task in tasks {
                        let state_str = format!("{:?}", task.state);
                        let state_colored = match task.state {
                            TaskState::Waiting => state_str.yellow(),
                            TaskState::Running => state_str.green(),
                            TaskState::Finished => state_str.blue(),
                            // Removed unreachable _ arm
                        };
                        let pid_str = task.pid.map_or("N/A".to_string(), |p| p.to_string());
                        // Truncate command if it's too long to avoid breaking the table too much
                        let cmd_display = if task.cmd.len() > 38 {
                             format!("{}...", &task.cmd[..35])
                        } else {
                            task.cmd.clone()
                        };

                        println!(
                            // Adjusted widths to match header
                            "{:<10} {:<20} {:<12} {:<15} {:<40} {:<5} {:<10}",
                            task.id.to_string().bold(),
                            task.name.cyan(),
                            state_colored,
                            task.queue.magenta(),
                            cmd_display, // Use potentially truncated command
                            task.gpu_require.to_string().yellow(),
                            pid_str.dimmed() // Color PID
                        );
                    }
                }
                Ok(())
            }
            Ok(Message::Ack(msg)) => { // Handle case where daemon sends Ack (e.g., "No tasks found")
                println!("{} Daemon reply: {}", "[INFO]".blue(), msg.italic()); // Format Ack
                Ok(())
            }
            Ok(Message::Error(err_msg)) => Err(anyhow!("{} Daemon returned error: {}", "[ERROR]".red(), err_msg)),
            Ok(other) => Err(anyhow!("{} Received unexpected reply: {:?}", "[ERROR]".red(), other)),
            Err(e) => Err(anyhow!("{} Failed to send list command to daemon", "[ERROR]".red()).context(e)),
        }
    }

    fn handle_info(socket_path: &str, task_id_str: String) -> Result<()> {
        let task_id = task_id_str.parse::<u64>().context("Invalid Task ID format, must be a number")?;
        println!("{} Getting info for task {} via RPC...", "[INFO]".blue(), task_id.to_string().yellow()); // Color task ID

        let request = Message::TaskCommand(TaskAction::Info { task_id });

        match request_reply(socket_path, &request) {
             Ok(Message::TaskStatus(tasks)) => {
                 if let Some(task) = tasks.first() {
                     // Pretty print task details with colors
                     let state_str = format!("{:?}", task.state);
                     let state_colored = match task.state {
                         TaskState::Waiting => state_str.yellow(),
                         TaskState::Running => state_str.green(),
                         TaskState::Finished => state_str.blue(),
                         // Removed unreachable _ arm
                     };
                     println!("Task Details (ID: {})", task.id.to_string().bold());
                     println!("  {:<12} {}", "Name:".green(), task.name.cyan());
                     println!("  {:<12} {}", "State:".green(), state_colored);
                     println!("  {:<12} {}", "Queue:".green(), task.queue.magenta());
                     println!("  {:<12} {}", "Priority:".green(), task.priority.to_string().yellow());
                     println!("  {:<12} {}", "Command:".green(), task.cmd);
                     println!("  {:<12} {}", "GPUs Req:".green(), task.gpu_require.to_string().yellow());
                     // Convert ColoredString to String before joining
                     println!("  {:<12} {}", "GPUs Alloc:".green(), task.gpu_ids.iter().map(|id| id.to_string().magenta().to_string()).collect::<Vec<_>>().join(", "));
                     println!("  {:<12} {}", "PID:".green(), task.pid.map_or("N/A".dimmed().to_string(), |p| p.to_string()));
                     println!("  {:<12} {}", "Log Path:".green(), task.log_path.underline()); // Underline log path
                     // Convert create_time (timestamp) to human-readable format if needed
                     // Example using chrono (add chrono = "0.4" to Cargo.toml)
                     // use chrono::{DateTime, Utc, TimeZone};
                     // let dt = Utc.timestamp_opt(task.create_time as i64, 0).single();
                     // let created_str = dt.map_or("Invalid time".to_string(), |t| t.to_rfc2822());
                     let created_str = task.create_time.to_string(); // Placeholder
                     println!("  {:<12} {}", "Created:".green(), created_str);
                 } else {
                     // Should not happen if daemon returns TaskStatus, but handle defensively
                     println!("{} No details returned for task {}.", "[WARN]".yellow(), task_id.to_string().yellow());
                 }
                 Ok(())
             }
             Ok(Message::Error(err_msg)) => Err(anyhow!("{} Daemon returned error: {}", "[ERROR]".red(), err_msg)),
             Ok(other) => Err(anyhow!("{} Received unexpected reply: {:?}", "[ERROR]".red(), other)),
             Err(e) => Err(anyhow!("{} Failed to send info command for task {} to daemon", "[ERROR]".red(), task_id).context(e)),
        }
    }

    fn handle_run(socket_path: &str, task_id_str: String) -> Result<()> {
        let task_id = task_id_str.parse::<u64>().context("Invalid Task ID format, must be a number")?;
        println!("{} Requesting to run task {} via RPC...", "[INFO]".blue(), task_id.to_string().yellow()); // Color task ID

        let request = Message::TaskCommand(TaskAction::Run { task_id });

        match request_reply(socket_path, &request) {
            Ok(Message::Ack(msg)) => {
                println!("{} Daemon reply: {}", "[SUCCESS]".green(), msg.italic()); // Format Ack
                Ok(())
            }
            Ok(Message::Error(err_msg)) => Err(anyhow!("{} Daemon returned error: {}", "[ERROR]".red(), err_msg)),
            Ok(other) => Err(anyhow!("{} Received unexpected reply: {:?}", "[ERROR]".red(), other)),
            Err(e) => Err(anyhow!("{} Failed to send run command for task {} to daemon", "[ERROR]".red(), task_id).context(e)),
        }
    }

    fn handle_kill(socket_path: &str, task_id_str: String) -> Result<()> {
        let task_id = task_id_str.parse::<u64>().context("Invalid Task ID format, must be a number")?;
        println!("{} Requesting to kill task {} via RPC...", "[INFO]".blue(), task_id.to_string().yellow()); // Color task ID

        let request = Message::TaskCommand(TaskAction::Kill { task_id });

        match request_reply(socket_path, &request) {
            Ok(Message::Ack(msg)) => {
                println!("{} Daemon reply: {}", "[SUCCESS]".green(), msg.italic()); // Format Ack
                Ok(())
            }
            Ok(Message::Error(err_msg)) => Err(anyhow!("{} Daemon returned error: {}", "[ERROR]".red(), err_msg)),
            Ok(other) => Err(anyhow!("{} Received unexpected reply: {:?}", "[ERROR]".red(), other)),
            Err(e) => Err(anyhow!("{} Failed to send kill command for task {} to daemon", "[ERROR]".red(), task_id).context(e)),
        }
    }

    // Added handle_remove
    fn handle_remove(socket_path: &str, task_id_str: String) -> Result<()> {
        let task_id = task_id_str.parse::<u64>().context("Invalid Task ID format, must be a number")?;
        println!("{} Requesting to remove task {} via RPC...", "[INFO]".blue(), task_id.to_string().yellow()); // Color task ID

        let request = Message::TaskCommand(TaskAction::Remove { task_id });

        match request_reply(socket_path, &request) {
            Ok(Message::Ack(msg)) => {
                println!("{} Daemon reply: {}", "[SUCCESS]".green(), msg.italic()); // Format Ack
                Ok(())
            }
            Ok(Message::Error(err_msg)) => Err(anyhow!("{} Daemon returned error: {}", "[ERROR]".red(), err_msg)),
            Ok(other) => Err(anyhow!("{} Received unexpected reply: {:?}", "[ERROR]".red(), other)),
            Err(e) => Err(anyhow!("{} Failed to send remove command for task {} to daemon", "[ERROR]".red(), task_id).context(e)),
        }
    }


    fn handle_logs(socket_path: &str, task_id_str: String, tail: bool) -> Result<()> {
        let task_id = task_id_str.parse::<u64>().context("Invalid Task ID format, must be a number")?;
        println!("{} Fetching {} logs for task {} via RPC...", "[INFO]".blue(), if tail { "tail of".italic() } else { "full".italic() }, task_id.to_string().yellow()); // Color task ID and format tail/full

        let request = Message::TaskCommand(TaskAction::Logs { task_id, tail });

        match request_reply(socket_path, &request) {
            Ok(Message::Ack(log_content)) => {
                println!("--- Logs for Task {} ---", task_id.to_string().bold());
                println!("{}", log_content); // Keep logs as is, maybe add syntax highlighting later if needed
                println!("--- End Logs ---");
                Ok(())
            }
            Ok(Message::Error(err_msg)) => Err(anyhow!("{} Daemon returned error: {}", "[ERROR]".red(), err_msg)),
            Ok(other) => Err(anyhow!("{} Received unexpected reply: {:?}", "[ERROR]".red(), other)),
            Err(e) => Err(anyhow!("{} Failed to send logs command for task {} to daemon", "[ERROR]".red(), task_id).context(e)),
        }
    }
}
