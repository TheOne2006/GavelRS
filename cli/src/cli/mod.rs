mod daemon;
mod submit;
mod task;
mod gpu;
mod queue;
use anyhow::{anyhow, Context, Result};
use std::{env, fs, path::{Path, PathBuf}};
const LOCK_FILE_NAME: &str = "gavelrs.lock";
use serde::Deserialize; // For reading config

use structopt::{clap::AppSettings, StructOpt};
// Aggregate all subcommand types
use self::{
    daemon::DaemonCommand,
    gpu::GpuCommand,
    queue::QueueCommand,
    submit::SubmitCommand,
    task::TaskCommand,
};

#[derive(StructOpt, Debug)]
#[structopt(
    name = "gavelrs",
    global_settings = &[AppSettings::DisableHelpSubcommand]
)]
pub enum AppCommand {
    /// Daemon process management
    #[structopt(name = "daemon")]
    Daemon(DaemonCommand),

    /// Task submission
    #[structopt(name = "submit")]
    Submit(SubmitCommand),

    /// Task management
    #[structopt(name = "task")]
    Task(TaskCommand),

    /// GPU resource management
    #[structopt(name = "gpu")]
    Gpu(GpuCommand),

    /// Queue scheduling management
    #[structopt(name = "queue")]
    Queue(QueueCommand),
}

impl AppCommand {
    pub fn execute(self) -> Result<()> {
        match self {
            AppCommand::Daemon(cmd) => cmd.execute(),
            AppCommand::Submit(cmd) => cmd.execute(),
            AppCommand::Task(cmd) => cmd.execute(),
            AppCommand::Gpu(cmd) => cmd.execute(),
            AppCommand::Queue(cmd) => cmd.execute(),
        }
    }
}

// Minimal config structure to read sock-path
#[derive(Debug, Deserialize)]
struct CliConfig {
    #[serde(rename = "sock-path")]
    sock_path: String,
}

// Helper function to find and read the socket path from config
fn get_socket_path(config_override: Option<&str>) -> Result<String> {
    let current_dir = env::current_dir().context("Failed to get current directory")?;
    let config_path = match config_override {
        Some(path) => Path::new(path).to_path_buf(),
        None => current_dir.join("default.json"), // Default config file
    };

    if !config_path.exists() {
        return Err(anyhow!("Config file not found at {}", config_path.display()));
    }

    let config_content = fs::read_to_string(&config_path)
        .with_context(|| format!("Failed to read config file: {}", config_path.display()))?;

    let config: CliConfig = serde_json::from_str(&config_content)
        .with_context(|| format!("Failed to parse sock-path from config file: {}", config_path.display()))?;

    Ok(config.sock_path)
}

fn get_lock_file_path() -> Result<PathBuf> {
    // Place lock file in a standard user-specific runtime directory if possible,
    // fallback to current directory.
    // Example using dirs crate (add `dirs = "..."` to Cargo.toml):
    /*
    if let Some(runtime_dir) = dirs::runtime_dir() {
        Ok(runtime_dir.join(LOCK_FILE_NAME))
    } else {
        env::current_dir()
            .map(|p| p.join(LOCK_FILE_NAME))
            .context("Failed to get current directory for lock file")
    }
    */
    // Simple version: lock file in current directory
    env::current_dir()
        .map(|p| p.join(LOCK_FILE_NAME))
        .context("Failed to get current directory for lock file")
}