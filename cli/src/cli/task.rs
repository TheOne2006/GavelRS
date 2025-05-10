use crate::cli::get_socket_path; // Import socket path helper
use anyhow::{anyhow, Context, Result}; // Added anyhow imports
use colored::*; // Import colored
use gavel_core::rpc::message::{Message, TaskAction, TaskFilter}; // Import RPC messages
use gavel_core::rpc::request_reply; // Import RPC function
use gavel_core::utils::models::TaskState;
use chrono::DateTime; // Import chrono for time formatting
use structopt::StructOpt; // Import TaskState for coloring

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
            Self::List { all, running, finished, queue, .. } => {
                Self::handle_list(&socket_path, all, running, finished, queue)
            }
            Self::Info { task_id, .. } => Self::handle_info(&socket_path, task_id),
            Self::Run { task_id, .. } => Self::handle_run(&socket_path, task_id),
            Self::Kill { task_id, .. } => Self::handle_kill(&socket_path, task_id),
            Self::Remove { task_id, .. } => Self::handle_remove(&socket_path, task_id), // Added Remove
            Self::Logs { task_id, tail, .. } => Self::handle_logs(&socket_path, task_id, tail),
        }
    }

    fn handle_list(
        socket_path: &str,
        all: bool,
        running: bool,
        finished: bool,
        queue: Option<String>,
    ) -> Result<()> {
        let filter = if all {
            TaskFilter::All
        } else if running {
            TaskFilter::Running
        } else if finished {
            TaskFilter::Finished
        } else if let Some(q) = queue {
            TaskFilter::ByQueue(q.clone()) // Clone queue name
        } else {
            // Default to waiting tasks if no specific filter is given
            TaskFilter::ByQueue(gavel_core::utils::DEFAULT_WAITING_QUEUE_NAME.to_string())
        };
        println!("{} Listing tasks with filter: {:?} via RPC...", "[INFO]".blue(), filter);

        let request = Message::TaskCommand(TaskAction::List { filter });

        match request_reply(socket_path, &request) {
            Ok(Message::TaskStatus(tasks)) => {
                if tasks.is_empty() {
                    println!("{}", "No tasks found matching your criteria.".yellow());
                } else {
                    println!(
                        "{}",
                        format!(
                            "{:<5} | {:<20} | {:<10} | {:<10} | {:<8} | {:<15} | {:<6} | GPU IDs",
                            "ID", "Name", "State", "Queue", "Prio", "Submit Time", "PID"
                        )
                        .bold()
                    );
                    println!("{}", "-".repeat(100)); // Separator line
                    for task in tasks {
                        let state_str = match task.state {
                            TaskState::Waiting => "Waiting".cyan(),
                            TaskState::Running => "Running".green(),
                            TaskState::Finished => "Finished".blue(),
                            TaskState::Failed => "Failed".red(), // New: Red for Failed
                        };
                        let pid_str = task.pid.map_or("N/A".to_string(), |p| p.to_string());
                        let gpu_ids_str = if task.gpu_ids.is_empty() {
                            "CPU".to_string()
                        } else {
                            task.gpu_ids
                                .iter()
                                .map(|id| id.to_string())
                                .collect::<Vec<String>>()
                                .join(",")
                        };
                        // Format create_time from timestamp to human-readable string
                        let create_time_str = DateTime::from_timestamp(task.create_time as i64, 0)
                            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                            .unwrap_or_else(|| "Invalid Time".to_string());

                        println!(
                            "{:<5} | {:<20} | {:<10} | {:<10} | {:<8} | {:<19} | {:<6} | {}", // Adjusted width for time
                            task.id.to_string().yellow(), // Color ID
                            task.name,
                            state_str,
                            task.queue,
                            task.priority,
                            create_time_str, // Use formatted time string
                            pid_str,
                            gpu_ids_str
                        );
                    }
                }
                Ok(())
            }
            Ok(Message::Ack(msg)) => {
                println!("{} {}", "[INFO]".green(), msg);
                Ok(())
            }
            Ok(other) => Err(anyhow!("Unexpected response from daemon: {:?}", other)),
            Err(e) => Err(e.context("Failed to list tasks")),
        }
    }

    fn handle_info(socket_path: &str, task_id_str: String) -> Result<()> {
        let task_id =
            task_id_str.parse::<u64>().context("Invalid Task ID format, must be a number")?;
        println!(
            "{} Fetching info for task ID: {} via RPC...",
            "[INFO]".blue(),
            task_id.to_string().yellow()
        ); // Color task ID

        let request = Message::TaskCommand(TaskAction::Info { task_id });

        match request_reply(socket_path, &request) {
            Ok(Message::TaskStatus(tasks)) => {
                if let Some(task) = tasks.first() {
                    println!("{}", "---------------- Task Info ----------------".bold());
                    println!("{:<20}: {}", "ID", task.id.to_string().yellow());
                    println!("{:<20}: {}", "Name", task.name);
                    let state_str = match task.state {
                        TaskState::Waiting => "Waiting".cyan(),
                        TaskState::Running => "Running".green(),
                        TaskState::Finished => "Finished".blue(),
                        TaskState::Failed => "Failed".red(), // New: Red for Failed
                    };
                    println!("{:<20}: {}", "State", state_str);
                    if task.state == TaskState::Failed {
                        if let Some(reason) = &task.failure_reason {
                            println!("{:<20}: {}", "Failure Reason", reason.red()); // New: Display reason in red
                        }
                    }
                    println!("{:<20}: {}", "Queue", task.queue);
                    println!("{:<20}: {}", "Priority", task.priority);
                    println!("{:<20}: {}", "Command", task.cmd);
                    println!("{:<20}: {}", "Log Path", task.log_path);
                    println!("{:<20}: {}", "GPUs Required", task.gpu_require);
                    let gpu_ids_str = if task.gpu_ids.is_empty() {
                        "CPU (None assigned)".to_string()
                    } else {
                        task.gpu_ids
                            .iter()
                            .map(|id| id.to_string())
                            .collect::<Vec<String>>()
                            .join(", ")
                    };
                    println!("{:<20}: {}", "GPUs Assigned", gpu_ids_str);
                    println!(
                        "{:<20}: {}",
                        "PID",
                        task.pid.map_or("N/A".to_string(), |p| p.to_string())
                    );
                    // Format create_time from timestamp to human-readable string
                    let create_time_str = DateTime::from_timestamp(task.create_time as i64, 0)
                        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                        .unwrap_or_else(|| "Invalid Time".to_string());
                    println!("{:<20}: {}", "Create Time", create_time_str); // Use formatted time
                    println!("{}", "-----------------------------------------".bold());
                } else {
                    println!("{}", "Task not found.".red());
                }
                Ok(())
            }
            Ok(Message::Error(e)) => {
                println!("{} {}", "[ERROR]".red(), e);
                Err(anyhow!(e))
            }
            Ok(other) => Err(anyhow!("Unexpected response from daemon: {:?}", other)),
            Err(e) => Err(e.context(format!("Failed to get info for task {}", task_id))),
        }
    }

    fn handle_run(socket_path: &str, task_id_str: String) -> Result<()> {
        let task_id =
            task_id_str.parse::<u64>().context("Invalid Task ID format, must be a number")?;
        println!(
            "{} Requesting to run task {} via RPC...",
            "[INFO]".blue(),
            task_id.to_string().yellow()
        ); // Color task ID

        let request = Message::TaskCommand(TaskAction::Run { task_id });

        match request_reply(socket_path, &request) {
            Ok(Message::Ack(msg)) => {
                println!("{} Daemon reply: {}", "[SUCCESS]".green(), msg.italic()); // Format Ack
                Ok(())
            }
            Ok(Message::Error(err_msg)) => {
                Err(anyhow!("{} Daemon returned error: {}", "[ERROR]".red(), err_msg))
            }
            Ok(other) => Err(anyhow!("{} Received unexpected reply: {:?}", "[ERROR]".red(), other)),
            Err(e) => Err(anyhow!(
                "{} Failed to send run command for task {} to daemon",
                "[ERROR]".red(),
                task_id
            )
            .context(e)),
        }
    }

    fn handle_kill(socket_path: &str, task_id_str: String) -> Result<()> {
        let task_id =
            task_id_str.parse::<u64>().context("Invalid Task ID format, must be a number")?;
        println!(
            "{} Requesting to kill task {} via RPC...",
            "[INFO]".blue(),
            task_id.to_string().yellow()
        ); // Color task ID

        let request = Message::TaskCommand(TaskAction::Kill { task_id });

        match request_reply(socket_path, &request) {
            Ok(Message::Ack(msg)) => {
                println!("{} Daemon reply: {}", "[SUCCESS]".green(), msg.italic()); // Format Ack
                Ok(())
            }
            Ok(Message::Error(err_msg)) => {
                Err(anyhow!("{} Daemon returned error: {}", "[ERROR]".red(), err_msg))
            }
            Ok(other) => Err(anyhow!("{} Received unexpected reply: {:?}", "[ERROR]".red(), other)),
            Err(e) => Err(anyhow!(
                "{} Failed to send kill command for task {} to daemon",
                "[ERROR]".red(),
                task_id
            )
            .context(e)),
        }
    }

    // Added handle_remove
    fn handle_remove(socket_path: &str, task_id_str: String) -> Result<()> {
        let task_id =
            task_id_str.parse::<u64>().context("Invalid Task ID format, must be a number")?;
        println!(
            "{} Requesting to remove task {} via RPC...",
            "[INFO]".blue(),
            task_id.to_string().yellow()
        ); // Color task ID

        let request = Message::TaskCommand(TaskAction::Remove { task_id });

        match request_reply(socket_path, &request) {
            Ok(Message::Ack(msg)) => {
                println!("{} Daemon reply: {}", "[SUCCESS]".green(), msg.italic()); // Format Ack
                Ok(())
            }
            Ok(Message::Error(err_msg)) => {
                Err(anyhow!("{} Daemon returned error: {}", "[ERROR]".red(), err_msg))
            }
            Ok(other) => Err(anyhow!("{} Received unexpected reply: {:?}", "[ERROR]".red(), other)),
            Err(e) => Err(anyhow!(
                "{} Failed to send remove command for task {} to daemon",
                "[ERROR]".red(),
                task_id
            )
            .context(e)),
        }
    }

    fn handle_logs(socket_path: &str, task_id_str: String, tail: bool) -> Result<()> {
        let task_id =
            task_id_str.parse::<u64>().context("Invalid Task ID format, must be a number")?;
        println!(
            "{} Fetching {} logs for task {} via RPC...",
            "[INFO]".blue(),
            if tail { "tail of".italic() } else { "full".italic() }, // Removed unnecessary parentheses
            task_id.to_string().yellow()
        ); // Color task ID and format tail/full

        let request = Message::TaskCommand(TaskAction::Logs { task_id, tail });

        match request_reply(socket_path, &request) {
            Ok(Message::Ack(log_content)) => {
                println!("--- Logs for Task {} ---", task_id.to_string().bold());
                println!("{}", log_content); // Keep logs as is, maybe add syntax highlighting later if needed
                println!("--- End Logs ---");
                Ok(())
            }
            Ok(Message::Error(err_msg)) => {
                Err(anyhow!("{} Daemon returned error: {}", "[ERROR]".red(), err_msg))
            }
            Ok(other) => Err(anyhow!("{} Received unexpected reply: {:?}", "[ERROR]".red(), other)),
            Err(e) => Err(anyhow!(
                "{} Failed to send logs command for task {} to daemon",
                "[ERROR]".red(),
                task_id
            )
            .context(e)),
        }
    }
}
