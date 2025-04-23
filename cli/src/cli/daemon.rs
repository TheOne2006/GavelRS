use anyhow::{anyhow, Context, Result};
use std::{env, fs, path::Path, process::Command};
use structopt::StructOpt;
use gavel_core::rpc::{message::{Message, DaemonAction}, request_reply}; // Import RPC functions and messages
use crate::cli::get_socket_path;
use crate::cli::get_lock_file_path;
use colored::*; // Import colored

#[derive(StructOpt, Debug)]
pub enum DaemonCommand {
    /// Start the daemon process
    #[structopt(name = "start")] // Changed from Init to start for clarity
    Start {
        #[structopt(long)]
        config: Option<String>,
    },

    /// Stop the daemon process
    #[structopt(name = "stop")]
    Stop {
         #[structopt(long)] // Allow specifying config for stop/status as well
         config: Option<String>,
    },

    /// Check daemon status
    #[structopt(name = "status")]
    Status{
        #[structopt(long)]
        config: Option<String>,
    },
}

impl DaemonCommand {
    pub fn execute(self) -> Result<()> {
        match self {
            Self::Start { ref config } => Self::handle_start(config.as_deref()),
            Self::Stop { ref config } => Self::handle_stop(config.as_deref()), // Pass config
            Self::Status { ref config } => Self::handle_status(config.as_deref()), // Pass config
        }
    }

    fn handle_start(config: Option<&str>) -> Result<()> {
        let lock_file_path = get_lock_file_path()?;

        // --- Check if already running ---
        if lock_file_path.exists() {
            println!(
                "{} Daemon lock file found ({}). Checking status via RPC...",
                "[INFO]".blue(),
                lock_file_path.display()
            );
            match Self::handle_status(config) {
                 Ok(_) => {
                     println!("{} Daemon appears to be running (RPC status check successful).", "[INFO]".blue());
                     return Ok(());
                 }
                 Err(e) => {
                      println!("{} Daemon status check failed ({}). Assuming stale lock file or daemon unresponsive. Proceeding with start...", "[WARN]".yellow(), e);
                      if fs::remove_file(&lock_file_path).is_ok() {
                          println!("{} Removed potentially stale lock file.", "[INFO]".blue());
                      } else {
                           println!("{} Failed to remove potentially stale lock file.", "[WARN]".yellow());
                      }
                 }
            }
        }
        // --- End Check ---

        let current_dir = env::current_dir().context("Failed to get current directory")?;

        // Determine config path: use provided or default to 'default.json' in current dir
        let config_path = match config {
            Some(path) => Path::new(path).to_path_buf(),
            None => current_dir.join("default.json"),
        };
        if !config_path.exists() {
             return Err(anyhow!("{} Config file not found: {}", "[ERROR]".red(), config_path.display()));
        }


        // Find the gavel-daemon executable
        let exe_path = env::current_exe()?;
        let target_dir = exe_path
            .parent()
            .ok_or_else(|| anyhow!("Failed to get parent directory of executable"))?;
        let daemon_exe = target_dir.join("gavel-daemon");

        if !daemon_exe.exists() {
            return Err(anyhow!(
                "{} Daemon executable not found at expected location: {}",
                "[ERROR]".red(),
                daemon_exe.display()
            ));
        }

        println!(
            "{} Attempting to start daemon: {} with config: {}",
            "[INFO]".blue(),
            daemon_exe.display().to_string().cyan(), // Added color
            config_path.display().to_string().cyan() // Added color
        );

        // Start the daemon process in the background
        let mut command = Command::new(&daemon_exe);
        command.arg(config_path.to_str().context("Config path is not valid UTF-8")?);

        // Redirect stdout/stderr to /dev/null for a cleaner background process
        // For debugging, consider redirecting to files instead.
        command.stdout(std::process::Stdio::inherit());
        command.stderr(std::process::Stdio::inherit());


        let child = command.spawn()
            .with_context(|| format!("{} Failed to start daemon process: {}", "[ERROR]".red(), daemon_exe.display()))?;

        // --- Create lock file with PID ---
        let pid = child.id().to_string();
        fs::write(&lock_file_path, &pid)
            .with_context(|| format!("{} Failed to create or write lock file: {}", "[ERROR]".red(), lock_file_path.display()))?;
        println!(
            "{} Daemon started successfully (PID: {}). Lock file created: {}",
            "[SUCCESS]".green(),
            pid.bold(),
            lock_file_path.display().to_string().cyan() // Added color
        );
        // --- End Create lock file ---

        // Optional: Short delay and then check status via RPC to confirm startup
        std::thread::sleep(std::time::Duration::from_millis(500));
        println!("{} Verifying daemon status via RPC...", "[INFO]".blue());
        if let Err(e) = Self::handle_status(config) {
             println!("{} Daemon process started, but initial status check failed: {}", "[WARN]".yellow(), e);
        }

        Ok(())
    }

    fn handle_stop(config: Option<&str>) -> Result<()> {
        let lock_file_path = get_lock_file_path()?;
        let sock_path = get_socket_path(config)?; // Get socket path from config

        println!("{} Attempting to stop daemon via RPC (socket: {})...", "[INFO]".blue(), sock_path.cyan()); // Added color

        let request = Message::DaemonCommand(DaemonAction::Stop);

        match request_reply(&sock_path, &request) {
            Ok(Message::Ack(msg)) => {
                println!("{} Daemon acknowledged stop request: {}", "[SUCCESS]".green(), msg.italic()); // Added format
                if lock_file_path.exists() {
                    fs::remove_file(&lock_file_path)
                        .with_context(|| format!("{} Failed to remove lock file: {}", "[ERROR]".red(), lock_file_path.display()))?;
                    println!("{} Lock file removed: {}", "[INFO]".blue(), lock_file_path.display().to_string().cyan()); // Added color
                } else {
                    println!("{} Lock file was already removed.", "[INFO]".blue());
                }
                Ok(())
            }
            Ok(Message::Error(err_msg)) => {
                Err(anyhow!("{} Daemon reported error during stop: {}", "[ERROR]".red(), err_msg))
            }
            Ok(other) => {
                 Err(anyhow!("{} Received unexpected reply from daemon during stop: {:?}", "[ERROR]".red(), other))
            }
            Err(e) => {
                eprintln!("{} Failed to send stop command or receive reply: {}", "[ERROR]".red(), e);
                 if lock_file_path.exists() {
                     if fs::remove_file(&lock_file_path).is_ok() {
                         println!("{} Removed potentially stale lock file: {}", "[INFO]".blue(), lock_file_path.display().to_string().cyan()); // Added color
                     } else {
                          eprintln!("{} Failed to remove lock file. Manual cleanup might be needed.", "[WARN]".yellow());
                     }
                 }
                Err(anyhow!("{} Failed to communicate with daemon to stop it.", "[ERROR]".red()).context(e))
            }
        }
    }

    fn handle_status(config: Option<&str>) -> Result<()> {
        let sock_path = get_socket_path(config)?; // Get socket path from config

        println!("{} Checking daemon status via RPC (socket: {})...", "[INFO]".blue(), sock_path.cyan()); // Added color

        let request = Message::DaemonCommand(DaemonAction::Status);

        match request_reply(&sock_path, &request) {
            Ok(Message::Ack(status_msg)) => {
                println!("{} Daemon status: {}", "[INFO]".blue(), status_msg.green().bold()); // Added format
                Ok(())
            }
            Ok(Message::Error(err_msg)) => {
                 println!("{} Daemon reported an error status: {}", "[WARN]".yellow(), err_msg.italic()); // Added format
                 Ok(())
            }
             Ok(other) => {
                 Err(anyhow!("{} Received unexpected reply from daemon for status request: {:?}", "[ERROR]".red(), other))
            }
            Err(e) => {
                println!("{} Daemon is likely not running or unresponsive (RPC failed).", "[WARN]".yellow());
                Err(anyhow!("{} Failed to communicate with daemon for status.", "[ERROR]".red()).context(e))
            }
        }
    }
}