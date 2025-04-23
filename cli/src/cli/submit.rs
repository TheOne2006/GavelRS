use anyhow::{Context, Result, anyhow};
use structopt::StructOpt;
use std::fs; // Added fs, env, Path
use serde::Deserialize; // Added for JSON parsing and config reading
use gavel_core::rpc::message::{Message, SubmitAction}; // Import RPC messages
use gavel_core::rpc::request_reply; // Import RPC function
use gavel_core::utils::models::TaskMeta; // Import TaskMeta for BatchJson
use crate::cli::get_socket_path;
use colored::*; // Import colored

// Define the structure expected in the JSON file for batch submission
#[derive(Deserialize, Debug, Clone)] // Added Clone
struct JsonTaskInput {
    command: String,
    #[serde(rename = "gpus_required")] // Match the user's example {command:, gpus_required:}
    gpu_require: u8, // Use u8 to match TaskMeta
    queue: Option<String>, // Allow specifying queue per task
    priority: Option<u8>, // Allow specifying priority per task
    name: Option<String>, // Allow specifying name per task
}


#[derive(StructOpt, Debug)]
pub enum SubmitCommand {
    /// Submit command-line task
    #[structopt(name = "command")]
    Command {
        /// Command to execute
        #[structopt(long)]
        cmd: String,

        /// Number of GPUs required
        #[structopt(long)]
        gpu_num: u8,

        /// Optional queue name
        #[structopt(long)]
        queue: Option<String>,

        /// Optional task name
        #[structopt(long)]
        name: Option<String>,

        /// Optional path to config file
        #[structopt(long)]
        config: Option<String>,
    },

    /// Submit script file task
    #[structopt(name = "script")]
    Script {
        /// Script file path
        #[structopt(long)]
        file: String,

        /// Number of GPUs required
        #[structopt(long)]
        gpu_num: u8,

        /// Optional queue name
        #[structopt(long)]
        queue: Option<String>,

        /// Optional task name
        #[structopt(long)]
        name: Option<String>,

        /// Optional path to config file
        #[structopt(long)]
        config: Option<String>,
    },

    /// Submit JSON-defined tasks (batch submission)
    #[structopt(name = "json")]
    Json {
        /// JSON file path
        #[structopt(long)]
        file: String,

        /// Default queue name if not specified in JSON
        #[structopt(long)]
        queue: Option<String>,

        /// Optional path to config file
        #[structopt(long)]
        config: Option<String>,
    },
}

impl SubmitCommand {
    pub fn execute(self) -> Result<()> {
        // Extract config path first, common to all subcommands
        let config_path: Option<String> = match &self {
             Self::Command { config, .. } => config.clone(),
             Self::Script { config, .. } => config.clone(),
             Self::Json { config, .. } => config.clone(),
        };
        let socket_path = get_socket_path(config_path.as_deref())?; // Get socket path once

        match self {
            Self::Command { cmd, gpu_num, queue, name, .. } => {
                println!(
                    "{} Submitting command task '{}' (Name: {}, GPUs: {}, Queue: {}) via RPC...",
                    "[INFO]".blue(),
                    cmd.bold(),
                    name.as_deref().unwrap_or("Default").cyan(),
                    gpu_num.to_string().yellow(),
                    queue.as_deref().unwrap_or("Default").magenta()
                );
                let request = Message::SubmitCommand(SubmitAction::Command {
                    command: cmd,
                    gpu_num_required: gpu_num,
                    queue_name: queue,
                    name, // Pass name
                });
                match request_reply(&socket_path, &request) {
                    Ok(Message::Ack(msg)) => {
                        println!("{} {}", "[SUCCESS]".green(), msg);
                        Ok(())
                    }
                    Ok(Message::Error(err_msg)) => Err(anyhow!("{} Daemon returned error: {}", "[ERROR]".red(), err_msg)),
                    Ok(other) => Err(anyhow!("{} Received unexpected reply: {:?}", "[ERROR]".red(), other)),
                    Err(e) => Err(anyhow!("{} Failed to send command task to daemon", "[ERROR]".red()).context(e)),
                }
            }
            Self::Script { file, gpu_num, queue, name, .. } => {
                 println!(
                    "{} Submitting script task '{}' (Name: {}, GPUs: {}, Queue: {}) via RPC...",
                    "[INFO]".blue(),
                    file.bold(),
                    name.as_deref().unwrap_or("Default").cyan(),
                    gpu_num.to_string().yellow(),
                    queue.as_deref().unwrap_or("Default").magenta()
                );
                let request = Message::SubmitCommand(SubmitAction::Script {
                    script_path: file,
                    gpu_num_required: gpu_num,
                    queue_name: queue,
                    name, // Pass name
                });
                match request_reply(&socket_path, &request) {
                    Ok(Message::Ack(msg)) => {
                        println!("{} {}", "[SUCCESS]".green(), msg);
                        Ok(())
                    }
                    Ok(Message::Error(err_msg)) => Err(anyhow!("{} Daemon returned error: {}", "[ERROR]".red(), err_msg)),
                    Ok(other) => Err(anyhow!("{} Received unexpected reply: {:?}", "[ERROR]".red(), other)),
                    Err(e) => Err(anyhow!("{} Failed to send script task to daemon", "[ERROR]".red()).context(e)),
                }
            }
            Self::Json { file, queue, .. } => {
                println!(
                    "{} Submitting tasks from JSON file '{}' (Default Queue: {}) via RPC...",
                    "[INFO]".blue(),
                    file.bold(),
                    queue.as_deref().unwrap_or("Default").magenta()
                );
                // Read and parse the JSON file
                let json_content = fs::read_to_string(&file)
                    .with_context(|| format!("Failed to read JSON file: {}", file))?;
                let inputs: Vec<JsonTaskInput> = serde_json::from_str(&json_content)
                    .with_context(|| format!("Failed to parse JSON file: {}", file))?;

                // Convert JsonTaskInput to TaskMeta
                let tasks: Vec<TaskMeta> = inputs.into_iter().map(|input| {
                    // Create a TaskMeta with default values for fields not in JsonTaskInput
                    TaskMeta {
                        pid: None,
                        id: 0, // Daemon will generate ID
                        name: input.name.unwrap_or_default(), // Use provided name or empty string (daemon will default)
                        cmd: input.command,
                        gpu_require: input.gpu_require,
                        state: gavel_core::utils::models::TaskState::Waiting, // Default state
                        log_path: String::new(), // Daemon will generate log path
                        priority: input.priority.unwrap_or(5), // Default priority 5
                        queue: input.queue.unwrap_or_default(), // Use provided or empty (daemon will default)
                        create_time: 0, // Daemon will set time
                        gpu_ids: Vec::new(),
                    }
                }).collect();


                let request = Message::SubmitCommand(SubmitAction::BatchJson {
                    tasks,
                    default_queue_name: queue,
                });

                match request_reply(&socket_path, &request) {
                    Ok(Message::Ack(msg)) => {
                        println!("{} {}", "[SUCCESS]".green(), msg);
                        Ok(())
                    }
                    Ok(Message::Error(err_msg)) => Err(anyhow!("{} Daemon returned error: {}", "[ERROR]".red(), err_msg)),
                    Ok(other) => Err(anyhow!("{} Received unexpected reply: {:?}", "[ERROR]".red(), other)),
                    Err(e) => Err(anyhow!("{} Failed to send batch JSON task to daemon", "[ERROR]".red()).context(e)),
                }
            }
        }
    }
}