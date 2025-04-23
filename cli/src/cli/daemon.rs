use anyhow::{anyhow, Context, Result};
use std::{env, fs, path::Path, process::Command};
use structopt::StructOpt;
use gavel_core::rpc::{message::{Message, DaemonAction}, request_reply}; // Import RPC functions and messages
use crate::cli::get_socket_path;
use crate::cli::get_lock_file_path;



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
            // Optionally read PID from lock file and check if process exists
            // We can also try sending a status request here to be more certain.
            println!(
                "Daemon lock file found ({}). Checking status via RPC...",
                lock_file_path.display()
            );
            // Try to get status, if it succeeds, daemon is likely running.
            match Self::handle_status(config) {
                 Ok(_) => {
                     println!("Daemon appears to be running (RPC status check successful).");
                     return Ok(());
                 }
                 Err(e) => {
                      println!("Daemon status check failed ({}). Assuming stale lock file or daemon unresponsive. Proceeding with start...", e);
                      // Attempt to remove stale lock file before starting
                      if fs::remove_file(&lock_file_path).is_ok() {
                          println!("Removed potentially stale lock file.");
                      } else {
                           println!("Warning: Failed to remove potentially stale lock file.");
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
             return Err(anyhow!("Config file not found: {}", config_path.display()));
        }


        // Find the gavel-daemon executable
        let exe_path = env::current_exe()?;
        let target_dir = exe_path
            .parent()
            .ok_or_else(|| anyhow!("Failed to get parent directory of executable"))?;
        let daemon_exe = target_dir.join("gavel-daemon");

        if !daemon_exe.exists() {
            return Err(anyhow!(
                "Daemon executable not found at expected location: {}",
                daemon_exe.display()
            ));
        }

        println!(
            "Attempting to start daemon: {} with config: {}",
            daemon_exe.display(),
            config_path.display()
        );

        // Start the daemon process in the background
        let mut command = Command::new(&daemon_exe);
        command.arg(config_path.to_str().context("Config path is not valid UTF-8")?);

        // Redirect stdout/stderr to /dev/null for a cleaner background process
        // For debugging, consider redirecting to files instead.
        command.stdout(std::process::Stdio::inherit());
        command.stderr(std::process::Stdio::inherit());


        let child = command.spawn()
            .with_context(|| format!("Failed to start daemon process: {}", daemon_exe.display()))?;

        // --- Create lock file with PID ---
        let pid = child.id().to_string();
        fs::write(&lock_file_path, &pid)
            .with_context(|| format!("Failed to create or write lock file: {}", lock_file_path.display()))?;
        println!(
            "Daemon started successfully (PID: {}). Lock file created: {}",
            pid,
            lock_file_path.display()
        );
        // --- End Create lock file ---

        // Optional: Short delay and then check status via RPC to confirm startup
        std::thread::sleep(std::time::Duration::from_millis(500));
        println!("Verifying daemon status via RPC...");
        if let Err(e) = Self::handle_status(config) {
             println!("Warning: Daemon process started, but initial status check failed: {}", e);
        }

        Ok(())
    }

    fn handle_stop(config: Option<&str>) -> Result<()> {
        let lock_file_path = get_lock_file_path()?;
        let sock_path = get_socket_path(config)?; // Get socket path from config

        println!("Attempting to stop daemon via RPC (socket: {})...", sock_path);

        let request = Message::DaemonCommand(DaemonAction::Stop);

        match request_reply(&sock_path, &request) {
            Ok(Message::Ack(msg)) => {
                println!("Daemon acknowledged stop request: {}", msg);
                // Remove lock file after successful acknowledgment
                if lock_file_path.exists() {
                    fs::remove_file(&lock_file_path)
                        .with_context(|| format!("Failed to remove lock file: {}", lock_file_path.display()))?;
                    println!("Lock file removed: {}", lock_file_path.display());
                } else {
                    println!("Lock file was already removed.");
                }
                Ok(())
            }
            Ok(Message::Error(err_msg)) => {
                Err(anyhow!("Daemon reported error during stop: {}", err_msg))
            }
            Ok(other) => {
                 Err(anyhow!("Received unexpected reply from daemon during stop: {:?}", other)) // Use debug print for unexpected types
            }
            Err(e) => {
                eprintln!("Failed to send stop command or receive reply: {}", e);
                // Attempt to remove lock file anyway, might be stale
                 if lock_file_path.exists() {
                     if fs::remove_file(&lock_file_path).is_ok() {
                         println!("Removed potentially stale lock file: {}", lock_file_path.display());
                     } else {
                          eprintln!("Warning: Failed to remove lock file. Manual cleanup might be needed.");
                     }
                 }
                Err(anyhow!("Failed to communicate with daemon to stop it.").context(e))
            }
        }
    }

    fn handle_status(config: Option<&str>) -> Result<()> {
        let sock_path = get_socket_path(config)?; // Get socket path from config

        println!("Checking daemon status via RPC (socket: {})...", sock_path);

        let request = Message::DaemonCommand(DaemonAction::Status);

        match request_reply(&sock_path, &request) {
            Ok(Message::Ack(status_msg)) => {
                println!("Daemon status: {}", status_msg);
                Ok(())
            }
            Ok(Message::Error(err_msg)) => {
                 // Still Ok from CLI perspective, but print the error status
                 println!("Daemon reported an error status: {}", err_msg);
                 Ok(())
                 // Or return Err if an error status means failure for the CLI command
                 // Err(anyhow!("Daemon reported error status: {}", err_msg))
            }
             Ok(other) => {
                 Err(anyhow!("Received unexpected reply from daemon for status request: {:?}", other)) // Use debug print
            }
            Err(e) => {
                println!("Daemon is likely not running or unresponsive (RPC failed).");
                Err(anyhow!("Failed to communicate with daemon for status.").context(e))
            }
        }
    }
}