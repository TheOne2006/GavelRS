use anyhow::{anyhow, Result}; // Removed Context
use gavel_core::rpc::message::{GPUAction, Message};
use structopt::StructOpt;
// Use the actual GpuStats struct from monitor
use crate::cli::get_socket_path;
use colored::*;
use gavel_core::rpc::request_reply; // Import colored

#[derive(StructOpt, Debug)]
pub enum GpuCommand {
    /// List all GPU statuses
    #[structopt(name = "list")]
    List {
        /// Optional path to config file
        #[structopt(long)]
        config: Option<String>,
    },

    /// View detailed GPU information
    #[structopt(name = "info")]
    Info {
        /// Specify GPU ID
        gpu_id: u8, // Assuming GPU IDs are numeric
        /// Optional path to config file
        #[structopt(long)]
        config: Option<String>,
    },

    /// Allocate GPU resources to a queue
    #[structopt(name = "allocate")]
    Allocate {
        /// GPU IDs (comma-separated or space-separated, handle parsing)
        #[structopt(name = "GPU_IDS")]
        gpu_ids: Vec<u8>, // Assuming numeric IDs

        /// Specify queue name
        #[structopt(name = "QUEUE_NAME")]
        queue_name: String,
        /// Optional path to config file
        #[structopt(long)]
        config: Option<String>,
    },

    /// Release GPU allocation from its queue
    #[structopt(name = "release")]
    Release {
        /// Specify GPU ID
        gpu_id: u8, // Assuming numeric ID
        /// Optional path to config file
        #[structopt(long)]
        config: Option<String>,
    },

    /// Ignore specified GPU (remove from scheduling)
    #[structopt(name = "ignore")]
    Ignore {
        /// Specify GPU ID
        gpu_id: u8, // Assuming numeric ID
        /// Optional path to config file
        #[structopt(long)]
        config: Option<String>,
    },

    /// Reset all ignored GPUs (add them back to the scheduling pool)
    #[structopt(name = "unignore")] // Keep the command name for user convenience
    Unignore {
        // No specific GPU ID needed for ResetIgnored action
        /// Optional path to config file
        #[structopt(long)]
        config: Option<String>,
    },
}

impl GpuCommand {
    pub fn execute(self) -> Result<()> {
        // Extract config path first
        let config_path: Option<String> = match &self {
            Self::List { config } => config.clone(),
            Self::Info { config, .. } => config.clone(),
            Self::Allocate { config, .. } => config.clone(),
            Self::Release { config, .. } => config.clone(),
            Self::Ignore { config, .. } => config.clone(),
            Self::Unignore { config } => config.clone(), // Corrected Unignore
        };
        let socket_path = get_socket_path(config_path.as_deref())?;

        match self {
            Self::List { .. } => Self::handle_list(&socket_path),
            Self::Info { gpu_id, .. } => Self::handle_info(&socket_path, gpu_id),
            Self::Allocate { gpu_ids, queue_name, .. } => {
                Self::handle_allocate(&socket_path, gpu_ids, queue_name)
            }
            Self::Release { gpu_id, .. } => Self::handle_release(&socket_path, gpu_id),
            Self::Ignore { gpu_id, .. } => Self::handle_ignore(&socket_path, gpu_id),
            Self::Unignore { .. } => Self::handle_unignore(&socket_path), // Corrected call
        }
    }

    fn handle_list(socket_path: &str) -> Result<()> {
        println!("{} Listing all GPU statuses via RPC...", "[INFO]".blue());
        let request = Message::GPUCommand(GPUAction::List);

        match request_reply(socket_path, &request) {
            // Correct match arm for the Message enum variant
            Ok(Message::GPUStatus(gpus)) => {
                if gpus.is_empty() {
                    // Use the Ack message if daemon returns that for no GPUs
                    println!("{} No GPUs detected or reported by daemon.", "[INFO]".blue());
                } else {
                    // Adjust output based on actual fields in GpuStats, add colors
                    println!(
                        "{}",
                        format!(
                            "{:<5} {:<10} {:<10} {:<20} {:<15}",
                            "ID".bold().underline(),
                            "Temp".bold().underline(),
                            "Core(%)".bold().underline(),
                            "Memory(Used/Total MB)".bold().underline(),
                            "Power(mW)".bold().underline()
                        )
                    );
                    println!("{}", "-".repeat(70)); // Separator line
                                                    // We need the index to display a GPU ID, as GpuStats doesn't contain it.
                    for (id, gpu) in gpus.iter().enumerate() {
                        // Convert bytes to MB for memory
                        let mem_total_mb = gpu.memory_usage.total / (1024 * 1024);
                        let mem_used_mb = gpu.memory_usage.used / (1024 * 1024);
                        let temp_colored = if gpu.temperature > 80 {
                            format!("{}C", gpu.temperature).red()
                        } else if gpu.temperature > 60 {
                            format!("{}C", gpu.temperature).yellow()
                        } else {
                            format!("{}C", gpu.temperature).green()
                        };
                        let core_usage_colored = if gpu.core_usage > 90 {
                            format!("{}%", gpu.core_usage).red()
                        } else if gpu.core_usage > 50 {
                            format!("{}%", gpu.core_usage).yellow()
                        } else {
                            format!("{}%", gpu.core_usage).green()
                        };
                        let mem_usage_str = format!("{}/{}MB", mem_used_mb, mem_total_mb);
                        let mem_usage_colored = if mem_used_mb as f64 / mem_total_mb as f64 > 0.8 {
                            mem_usage_str.red()
                        } else if mem_used_mb as f64 / mem_total_mb as f64 > 0.5 {
                            mem_usage_str.yellow()
                        } else {
                            mem_usage_str.green()
                        };

                        println!(
                            "{:<5} {:<18} {:<18} {:<28} {:<15}", // Adjusted widths for colors
                            id.to_string().bold(),               // Bold ID
                            temp_colored,
                            core_usage_colored,
                            mem_usage_colored,
                            gpu.power_usage.to_string().cyan() // Color power usage
                        );
                    }
                }
                Ok(())
            }
            Ok(Message::Ack(msg)) => {
                // Handle Ack message specifically (e.g., when no GPUs are found)
                println!("{} Daemon reply: {}", "[INFO]".blue(), msg.italic()); // Format Ack
                Ok(())
            }
            Ok(Message::Error(err_msg)) => {
                Err(anyhow!("{} Daemon returned error: {}", "[ERROR]".red(), err_msg))
            }
            Ok(other) => {
                Err(anyhow!("{} Received unexpected reply type: {:?}", "[ERROR]".red(), other))
            }
            Err(e) => {
                Err(anyhow!("{} Failed to send GPU list command to daemon", "[ERROR]".red())
                    .context(e))
            }
        }
    }

    fn handle_info(socket_path: &str, gpu_id: u8) -> Result<()> {
        println!(
            "{} Getting info for GPU ID {} via RPC...",
            "[INFO]".blue(),
            gpu_id.to_string().yellow()
        ); // Color GPU ID
           // Correctly wrap gpu_id in Some for the message
        let request = Message::GPUCommand(GPUAction::Info { gpu_id: Some(gpu_id) });

        match request_reply(socket_path, &request) {
            // Correct match arm and use actual GpuStats fields
            Ok(Message::GPUStatus(gpus)) => {
                // The daemon returns a Vec<GpuStats>, even for a single ID request.
                if let Some(gpu) = gpus.first() {
                    // Print the requested ID and the available stats with colors
                    println!("GPU Details (ID: {})", gpu_id.to_string().bold());
                    let temp_colored = if gpu.temperature > 80 {
                        format!("{}C", gpu.temperature).red()
                    } else if gpu.temperature > 60 {
                        format!("{}C", gpu.temperature).yellow()
                    } else {
                        format!("{}C", gpu.temperature).green()
                    };
                    let core_usage_colored = if gpu.core_usage > 90 {
                        format!("{}%", gpu.core_usage).red()
                    } else if gpu.core_usage > 50 {
                        format!("{}%", gpu.core_usage).yellow()
                    } else {
                        format!("{}%", gpu.core_usage).green()
                    };
                    let mem_total_mb = gpu.memory_usage.total / (1024 * 1024);
                    let mem_used_mb = gpu.memory_usage.used / (1024 * 1024);
                    let mem_usage_str = format!("{}/{} MB", mem_used_mb, mem_total_mb);
                    let mem_usage_colored = if mem_used_mb as f64 / mem_total_mb as f64 > 0.8 {
                        mem_usage_str.red()
                    } else if mem_used_mb as f64 / mem_total_mb as f64 > 0.5 {
                        mem_usage_str.yellow()
                    } else {
                        mem_usage_str.green()
                    };

                    println!("  {:<15} {}", "Temperature:".green(), temp_colored);
                    println!("  {:<15} {}", "Core Usage:".green(), core_usage_colored);
                    println!("  {:<15} {}", "Memory Usage:".green(), mem_usage_colored);
                    println!(
                        "  {:<15} {}",
                        "Power Usage:".green(),
                        format!("{} mW", gpu.power_usage).cyan()
                    );
                } else {
                    // This case might happen if the daemon returns an empty list for an invalid ID
                    // instead of an Error message.
                    println!(
                        "{} No details returned for GPU {}. It might not exist or is ignored.",
                        "[WARN]".yellow(),
                        gpu_id.to_string().yellow()
                    );
                }
                Ok(())
            }
            Ok(Message::Error(err_msg)) => {
                // Handle specific error message from daemon (e.g., GPU not found)
                Err(anyhow!("{} Daemon error for GPU {}: {}", "[ERROR]".red(), gpu_id, err_msg))
            }
            Ok(other) => Err(anyhow!(
                "{} Received unexpected reply type for GPU {}: {:?}",
                "[ERROR]".red(),
                gpu_id,
                other
            )),
            Err(e) => Err(anyhow!(
                "{} Failed to send info command for GPU {} to daemon",
                "[ERROR]".red(),
                gpu_id
            )
            .context(e)),
        }
    }

    fn handle_allocate(socket_path: &str, gpu_ids: Vec<u8>, queue_name: String) -> Result<()> {
        // Convert ColoredString to String before joining
        let gpu_ids_str = gpu_ids
            .iter()
            .map(|id| id.to_string().yellow().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        println!(
            "{} Requesting to allocate GPUs [{}] to queue '{}' via RPC...",
            "[INFO]".blue(),
            gpu_ids_str,
            queue_name.cyan()
        ); // Color IDs and queue name
        let request = Message::GPUCommand(GPUAction::Allocate {
            gpu_ids: gpu_ids.clone(),
            queue: queue_name.clone(),
        });

        match request_reply(socket_path, &request) {
            Ok(Message::Ack(msg)) => {
                println!("{} Daemon reply: {}", "[SUCCESS]".green(), msg.italic()); // Format Ack
                Ok(())
            }
            Ok(Message::Error(err_msg)) => {
                Err(anyhow!("{} Daemon returned error: {}", "[ERROR]".red(), err_msg))
            }
            Ok(other) => {
                Err(anyhow!("{} Received unexpected reply type: {:?}", "[ERROR]".red(), other))
            }
            Err(e) => Err(anyhow!(
                "{} Failed to send allocate command (GPUs {:?} -> Queue {}) to daemon",
                "[ERROR]".red(),
                gpu_ids,
                queue_name
            )
            .context(e)),
        }
    }

    fn handle_release(socket_path: &str, gpu_id: u8) -> Result<()> {
        println!(
            "{} Requesting to release GPU {} via RPC...",
            "[INFO]".blue(),
            gpu_id.to_string().yellow()
        ); // Color GPU ID
        let request = Message::GPUCommand(GPUAction::Release { gpu_id });

        match request_reply(socket_path, &request) {
            Ok(Message::Ack(msg)) => {
                println!("{} Daemon reply: {}", "[SUCCESS]".green(), msg.italic()); // Format Ack
                Ok(())
            }
            Ok(Message::Error(err_msg)) => {
                Err(anyhow!("{} Daemon returned error: {}", "[ERROR]".red(), err_msg))
            }
            Ok(other) => {
                Err(anyhow!("{} Received unexpected reply type: {:?}", "[ERROR]".red(), other))
            }
            Err(e) => Err(anyhow!(
                "{} Failed to send release command for GPU {} to daemon",
                "[ERROR]".red(),
                gpu_id
            )
            .context(e)),
        }
    }

    fn handle_ignore(socket_path: &str, gpu_id: u8) -> Result<()> {
        println!(
            "{} Requesting to ignore GPU {} via RPC...",
            "[INFO]".blue(),
            gpu_id.to_string().yellow()
        ); // Color GPU ID
        let request = Message::GPUCommand(GPUAction::Ignore { gpu_id });

        match request_reply(socket_path, &request) {
            Ok(Message::Ack(msg)) => {
                println!("{} Daemon reply: {}", "[SUCCESS]".green(), msg.italic()); // Format Ack
                Ok(())
            }
            Ok(Message::Error(err_msg)) => {
                Err(anyhow!("{} Daemon returned error: {}", "[ERROR]".red(), err_msg))
            }
            Ok(other) => {
                Err(anyhow!("{} Received unexpected reply type: {:?}", "[ERROR]".red(), other))
            }
            Err(e) => Err(anyhow!(
                "{} Failed to send ignore command for GPU {} to daemon",
                "[ERROR]".red(),
                gpu_id
            )
            .context(e)),
        }
    }

    // Correct signature: no gpu_id needed for ResetIgnored
    fn handle_unignore(socket_path: &str) -> Result<()> {
        println!("{} Requesting to reset all ignored GPUs via RPC...", "[INFO]".blue());
        // Use ResetIgnored action
        let request = Message::GPUCommand(GPUAction::ResetIgnored);

        match request_reply(socket_path, &request) {
            Ok(Message::Ack(msg)) => {
                println!("{} Daemon reply: {}", "[SUCCESS]".green(), msg.italic()); // Format Ack
                Ok(())
            }
            Ok(Message::Error(err_msg)) => {
                Err(anyhow!("{} Daemon returned error: {}", "[ERROR]".red(), err_msg))
            }
            Ok(other) => {
                Err(anyhow!("{} Received unexpected reply type: {:?}", "[ERROR]".red(), other))
            }
            // Correct error message: no specific gpu_id involved
            Err(e) => Err(anyhow!(
                "{} Failed to send reset ignored GPUs command to daemon",
                "[ERROR]".red()
            )
            .context(e)),
        }
    }
}
