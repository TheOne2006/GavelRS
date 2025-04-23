use anyhow::{Context, Result, anyhow};
use structopt::StructOpt;
use std::{fs, path::Path}; // Added fs, env, Path
use serde::Deserialize; // Added for JSON parsing and config reading
use gavel_core::rpc::message::{Message, SubmitAction}; // Import RPC messages
use gavel_core::rpc::request_reply; // Import RPC function
use gavel_core::utils::models::TaskMeta; // Import TaskMeta for BatchJson
use crate::cli::get_socket_path;

// Define the structure expected in the JSON file for batch submission
#[derive(Deserialize, Debug, Clone)] // Added Clone
struct JsonTaskInput {
    command: String,
    #[serde(rename = "gpus_required")] // Match the user's example {command:, gpus_required:}
    gpu_require: u32,
    queue: Option<String>, // Allow specifying queue per task
    priority: Option<u8>, // Allow specifying priority per task
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
        gpu_num: u32,

        /// Optional queue name
        #[structopt(long)]
        queue: Option<String>,

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
        gpu_num: u32,

        /// Optional queue name
        #[structopt(long)]
        queue: Option<String>,

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
            Self::Command { cmd, gpu_num, queue, .. } => Self::handle_command(&socket_path, cmd, gpu_num, queue),
            Self::Script { file, gpu_num, queue, .. } => Self::handle_script(&socket_path, file, gpu_num, queue),
            Self::Json { file, queue, .. } => Self::handle_json(&socket_path, file, queue),
        }
    }

    fn handle_command(socket_path: &str, cmd: String, gpu_num: u32, queue: Option<String>) -> Result<()> {
        println!("Submitting command task via RPC: '{}', GPU count: {}, Queue: {:?}", cmd, gpu_num, queue);
        let action = SubmitAction::Command {
            command: cmd,
            gpu_num_required: gpu_num as u8, // Cast u32 to u8
            queue_name: queue,
        };
        let request = Message::SubmitCommand(action);

        match request_reply(socket_path, &request) {
            Ok(Message::Ack(msg)) => {
                println!("Daemon reply: {}", msg);
                Ok(())
            }
            Ok(Message::Error(err_msg)) => {
                Err(anyhow!("Daemon returned error: {}", err_msg))
            }
             Ok(other) => {
                 Err(anyhow!("Received unexpected reply from daemon: {:?}", other))
            }
            Err(e) => {
                Err(anyhow!("Failed to send command task to daemon").context(e))
            }
        }
    }

    fn handle_script(socket_path: &str, file: String, gpu_num: u32, queue: Option<String>) -> Result<()> {
        println!("Submitting script task via RPC from file: '{}', GPU count: {}, Queue: {:?}", file, gpu_num, queue);
        // Ensure the script file exists before sending? Optional, daemon could also check.
        if !Path::new(&file).exists() {
             return Err(anyhow!("Script file not found: {}", file));
        }

        let action = SubmitAction::Script {
            script_path: file, // Send the path
            gpu_num_required: gpu_num as u8, // Cast u32 to u8
            queue_name: queue,
        };
        let request = Message::SubmitCommand(action);

         match request_reply(socket_path, &request) {
            Ok(Message::Ack(msg)) => {
                println!("Daemon reply: {}", msg);
                Ok(())
            }
            Ok(Message::Error(err_msg)) => {
                Err(anyhow!("Daemon returned error: {}", err_msg))
            }
             Ok(other) => {
                 Err(anyhow!("Received unexpected reply from daemon: {:?}", other))
            }
            Err(e) => {
                Err(anyhow!("Failed to send script task to daemon").context(e))
            }
        }
    }

    fn handle_json(socket_path: &str, file: String, default_queue: Option<String>) -> Result<()> {
        println!("Submitting JSON defined tasks via RPC from file: '{}', Default Queue: {:?}", file, default_queue);

        let json_content = fs::read_to_string(&file)
            .with_context(|| format!("Failed to read JSON file: {}", file))?;

        let inputs: Vec<JsonTaskInput> = serde_json::from_str(&json_content)
            // Fix format string escaping for braces
            .with_context(|| format!("Failed to parse JSON file: {}. Expected format: [{{{{\"command\": \"...\", \"gpus_required\": N, \"queue\": \"...\"?, \"priority\": N?}}}}, ...] ", file))?;

        if inputs.is_empty() {
            println!("JSON file contains no tasks to submit.");
            return Ok(());
        }

        // Map JsonTaskInput to TaskMeta.
        // Note: The daemon handler also sets defaults, but we map the known fields here.
        // Fields like id, state, log_path, create_time will be set by the daemon.
        let tasks: Vec<TaskMeta> = inputs.into_iter().map(|input| {
            TaskMeta {
                cmd: input.command,
                gpu_require: input.gpu_require as u8, // Cast u32 to u8
                queue: input.queue.unwrap_or_else(|| default_queue.clone().unwrap_or_default()), // Use task queue, then default, then empty string (daemon uses its default)
                priority: input.priority.unwrap_or(5), // Default priority 5 if not specified
                // Fields below will be filled by the daemon
                id: 0,
                pid: None,
                state: gavel_core::utils::models::TaskState::Waiting, // Set initial state
                log_path: String::new(),
                create_time: 0,
                gpu_ids: Vec::new(),
            }
        }).collect();


        let action = SubmitAction::BatchJson {
            tasks,
            default_queue_name: default_queue, // Pass the CLI default queue name
        };
        let request = Message::SubmitCommand(action);

        match request_reply(socket_path, &request) {
            Ok(Message::Ack(msg)) => {
                println!("Daemon reply: {}", msg);
                Ok(())
            }
            Ok(Message::Error(err_msg)) => {
                Err(anyhow!("Daemon returned error during batch submission: {}", err_msg))
            }
             Ok(other) => {
                 Err(anyhow!("Received unexpected reply from daemon: {:?}", other))
            }
            Err(e) => {
                Err(anyhow!("Failed to send batch JSON task to daemon").context(e))
            }
        }
    }
}