// src/daemon/mod.rs
pub mod scheduler;
pub mod state;

use anyhow::{Context, Result};
use std::path::Path;
use tokio::net::{UnixListener, UnixStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::watch; // Use tokio's RwLock
use bincode::{decode_from_slice, encode_to_vec};
use bincode::config::standard as bincode_config;
use gavel_core::rpc::message::{Message, DaemonAction};
use state::DaemonState; // Import DaemonState

// Define a type for the shutdown signal sender
type ShutdownSender = watch::Sender<bool>;
// Define a type for the shutdown signal receiver

/// Starts the daemon, listens for connections, and handles messages.
/// Runs until a Stop command is received or an error occurs.
pub async fn start(sock_path: &str) -> Result<()> {
    log::info!("Daemon starting, attempting to listen on socket: {}", sock_path);

    // Ensure the socket file doesn't exist before binding
    if Path::new(sock_path).exists() {
        log::warn!("Socket file {} already exists, attempting to remove.", sock_path);
        tokio::fs::remove_file(sock_path).await
            .with_context(|| format!("Failed to remove existing socket file: {}", sock_path))?;
    }

    let listener = UnixListener::bind(sock_path)
        .with_context(|| format!("Failed to bind to socket: {}", sock_path))?;
    log::info!("Successfully bound to socket: {}", sock_path);

    // Create the shared state (assuming persistence path is derived or configured)
    // For now, let's use a temporary path for state persistence.
    // TODO: Get persistence path from config or a standard location.
    let state_path = format!("{}.state", sock_path);
    let daemon_state = DaemonState::new(&state_path).await
        .context("Failed to initialize daemon state")?;

    // Create a channel for shutdown signaling
    let (shutdown_tx, mut shutdown_rx) = watch::channel(false);

    log::info!("Daemon ready and listening for connections.");

    loop {
        tokio::select! {
            // Wait for incoming connections
            result = listener.accept() => {
                match result {
                    Ok((stream, _addr)) => {
                        log::debug!("Accepted new connection");
                        // Clone Arc<RwLock<DaemonState>> and shutdown sender for the handler task
                        let state_clone = daemon_state.clone();
                        let shutdown_tx_clone = shutdown_tx.clone();
                        // Spawn a task to handle the connection
                        tokio::spawn(async move {
                            if let Err(e) = handle_connection(stream, state_clone, shutdown_tx_clone).await {
                                log::error!("Error handling connection: {}", e);
                            }
                        });
                    }
                    Err(e) => {
                        log::error!("Failed to accept connection: {}", e);
                        // Consider if we should break the loop on accept errors
                    }
                }
            }
            // Wait for the shutdown signal
            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() {
                    log::info!("Shutdown signal received, stopping listener.");
                    break; // Exit the main loop
                }
            }
        }
    }

    log::info!("Daemon shutting down...");
    // Perform cleanup, e.g., save state one last time
    if let Err(e) = daemon_state.save_to_disk().await {
        log::error!("Failed to save state during shutdown: {}", e);
    }
    // Ensure the socket file is removed on shutdown
    if let Err(e) = tokio::fs::remove_file(sock_path).await {
         log::warn!("Failed to remove socket file during shutdown {}: {}", sock_path, e);
    }

    log::info!("Daemon has shut down.");
    Ok(())
}

/// Handles a single client connection asynchronously.
async fn handle_connection(mut stream: UnixStream, state: DaemonState, shutdown_tx: ShutdownSender) -> Result<()> {
    // Read message length (u32)
    let mut len_bytes = [0u8; 4];
    stream.read_exact(&mut len_bytes).await
        .context("Failed to read message length")?;
    let len = u32::from_le_bytes(len_bytes);

    // Basic sanity check for length
    if len > 10 * 1024 * 1024 { // e.g., limit message size to 10MB
        return Err(anyhow::anyhow!("Received excessively large message length: {}", len));
    }

    // Read message data
    let mut msg_buf = vec![0u8; len as usize];
    stream.read_exact(&mut msg_buf).await
        .context("Failed to read message data")?;

    // Decode the message
    let (message, _): (Message, usize) = decode_from_slice(&msg_buf, bincode_config())
        .context("Failed to decode message")?;

    // Process the message and potentially create a reply
    let reply_message = match message {
        Message::DaemonCommand(action) => match action {
            DaemonAction::Stop => {
                log::info!("Received Stop command.");
                // Send shutdown signal
                if shutdown_tx.send(true).is_err() {
                    log::error!("Failed to send shutdown signal: receiver dropped?");
                    // Still try to send an error back if possible
                    Message::Error("Failed to initiate shutdown".to_string())
                } else {
                    Message::Ack("Shutdown initiated".to_string())
                }
            }
            DaemonAction::Status => {
                log::info!("Received Status command.");
                match status(&state).await { // Pass state to status check
                    Ok(status_string) => Message::Ack(status_string),
                    Err(e) => Message::Error(format!("Status check failed: {}", e)),
                }
            }
            // Add other DaemonAction variants here if needed
        },
        Message::TaskCommand(_) => {
            // TODO: Handle Task commands
            log::warn!("Received unhandled TaskCommand");
            Message::Error("Task commands not yet implemented".to_string())
        }
        Message::GPUCommand(_) => {
            // TODO: Handle GPU commands
            log::warn!("Received unhandled GPUCommand");
            Message::Error("GPU commands not yet implemented".to_string())
        }
        Message::QueueCommand(_) => {
            // TODO: Handle Queue commands
            log::warn!("Received unhandled QueueCommand");
            Message::Error("Queue commands not yet implemented".to_string())
        }
        // Ignore status/ack/error messages received from client (shouldn't happen in request/reply)
        _ => {
            log::warn!("Received unexpected message type from client.");
            // Don't send a reply for unexpected types? Or send an error?
            // Let's send an error for now.
             Message::Error("Received unexpected message type".to_string())
        }
    };

    // Send the reply message back
    let encoded_reply = encode_to_vec(&reply_message, bincode_config())
        .context("Failed to encode reply message")?;
    let reply_len = encoded_reply.len() as u32;

    stream.write_all(&reply_len.to_le_bytes()).await
        .context("Failed to write reply length")?;
    stream.write_all(&encoded_reply).await
        .context("Failed to write reply data")?;
    stream.flush().await.context("Failed to flush stream for reply")?;

    log::debug!("Sent reply and closing connection.");
    Ok(())
}


// Internal function called by the CLI via RPC, no longer directly callable externally
// This function is now effectively replaced by the message handling logic.
// pub fn stop() -> Result<()> {
//     log::info!("Internal stop function called (likely via RPC)");
//     // The actual shutdown is triggered by sending true on the shutdown_tx channel
//     // in handle_connection.
//     Ok(())
// }

/// Performs internal status checks. Called when a Status command is received.
async fn status(_state: &DaemonState) -> Result<String> {
    // TODO: Implement actual status checks
    // - Check if scheduler thread is alive (if applicable)
    // - Check if GPU monitor thread is alive (if applicable)
    // - Check state consistency (e.g., number of tasks matches queue contents)
    // - Check last successful GPU poll time
    log::debug!("Performing status check...");
    // For now, just return a simple "OK"
    Ok("Daemon is running OK.".to_string())
}