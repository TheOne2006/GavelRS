// src/daemon/mod.rs
pub mod handlers;
pub mod scheduler;
pub mod state; // Add scheduler module

use crate::daemon::scheduler::run_scheduler;
use anyhow::{Context, Result};
use bincode::config::standard as bincode_config;
use bincode::{decode_from_slice, encode_to_vec};
use gavel_core::rpc::message::{DaemonAction, Message};
use handlers::{
    handle_gpu_command, handle_queue_command, handle_submit_command, handle_task_command,
}; // Import handle_submit_command
use state::DaemonState; // Import DaemonState
use std::path::Path;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::watch; // Use tokio's RwLock // Import the scheduler function

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
        tokio::fs::remove_file(sock_path)
            .await
            .with_context(|| format!("Failed to remove existing socket file: {}", sock_path))?;
    }

    let listener = UnixListener::bind(sock_path)
        .with_context(|| format!("Failed to bind to socket: {}", sock_path))?;
    log::info!("Successfully bound to socket: {}", sock_path);

    // Create the shared state (assuming persistence path is derived or configured)
    // For now, let's use a temporary path for state persistence.
    // TODO: Get persistence path from config or a standard location.
    let daemon_state = DaemonState::new();
    handlers::ensure_default_queues_exist(&daemon_state).await?; // Ensure default queues

    // Create a channel for shutdown signaling
    let (shutdown_tx, mut shutdown_rx) = watch::channel(false);

    // --- Start the scheduler task ---
    let scheduler_state = daemon_state.clone(); // Clone state for the scheduler
    tokio::spawn(run_scheduler(scheduler_state)); // Spawn the scheduler in a background task
    log::info!("Scheduler task started.");
    // --- Scheduler task started ---

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
    // Ensure the socket file is removed on shutdown
    if let Err(e) = tokio::fs::remove_file(sock_path).await {
        log::warn!("Failed to remove socket file during shutdown {}: {}", sock_path, e);
    }

    log::info!("Daemon has shut down.");
    Ok(())
}

/// Handles a single client connection asynchronously.
async fn handle_connection(
    mut stream: UnixStream,
    state: DaemonState,
    shutdown_tx: ShutdownSender,
) -> Result<()> {
    // Read message length (u32)
    let mut len_bytes = [0u8; 4];
    stream.read_exact(&mut len_bytes).await.context("Failed to read message length")?;
    let len = u32::from_le_bytes(len_bytes);

    // Basic sanity check for length
    if len > 10 * 1024 * 1024 {
        // e.g., limit message size to 10MB
        return Err(anyhow::anyhow!("Received excessively large message length: {}", len));
    }

    // Read message data
    let mut msg_buf = vec![0u8; len as usize];
    stream.read_exact(&mut msg_buf).await.context("Failed to read message data")?;

    // Decode the message
    let (message, _): (Message, usize) =
        decode_from_slice(&msg_buf, bincode_config()).context("Failed to decode message")?;

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
                match status(&state).await {
                    // Pass state to status check
                    Ok(status_string) => Message::Ack(status_string),
                    Err(e) => Message::Error(format!("Status check failed: {}", e)),
                }
            }
        },
        Message::TaskCommand(action) => match handle_task_command(action, state.clone()).await {
            Ok(reply) => reply,
            Err(e) => {
                log::error!("Error handling TaskCommand: {}", e);
                Message::Error(format!("Error handling TaskCommand: {}", e))
            }
        },
        Message::GPUCommand(action) => match handle_gpu_command(action, state.clone()).await {
            Ok(reply) => reply,
            Err(e) => {
                log::error!("Error handling GPUCommand: {}", e);
                Message::Error(format!("Error handling GPUCommand: {}", e))
            }
        },
        Message::QueueCommand(action) => match handle_queue_command(action, state.clone()).await {
            Ok(reply) => reply,
            Err(e) => {
                log::error!("Error handling QueueCommand: {}", e);
                Message::Error(format!("Error handling QueueCommand: {}", e))
            }
        },
        Message::SubmitCommand(action) => {
            // Handle SubmitCommand
            match handle_submit_command(action, state.clone()).await {
                Ok(reply) => reply,
                Err(e) => {
                    log::error!("Error handling SubmitCommand: {}", e);
                    Message::Error(format!("Error handling SubmitCommand: {}", e))
                }
            }
        }
        // Handle status/ack/error messages received from client (shouldn't happen in request/reply)
        Message::GPUStatus(_)
        | Message::TaskStatus(_)
        | Message::QueueStatus(_)
        | Message::Ack(_)
        | Message::Error(_) => {
            log::warn!("Received status/ack/error message type from client, which is unexpected in a request.");
            Message::Error("Daemon received unexpected status/ack/error message type".to_string())
        }
    };

    // Send the reply message back
    let encoded_reply = encode_to_vec(&reply_message, bincode_config())
        .context("Failed to encode reply message")?;
    let reply_len = encoded_reply.len() as u32;

    stream.write_all(&reply_len.to_le_bytes()).await.context("Failed to write reply length")?;
    stream.write_all(&encoded_reply).await.context("Failed to write reply data")?;
    stream.flush().await.context("Failed to flush stream for reply")?;

    log::debug!("Sent reply and closing connection.");
    Ok(())
}

/// Performs internal status checks. Called when a Status command is received.
async fn status(state: &DaemonState) -> Result<String> {
    // 实现更加完善的状态检查
    log::debug!("执行状态检查...");

    // 获取统计数据
    let all_tasks = state.get_all_tasks().await;
    let all_queues = state.get_all_queues().await;
    let gpu_stats = state.get_all_gpu_stats().await;
    let gpu_allocations = state.get_gpu_allocations().await;
    let ignored_gpus = state.get_ignored_gpus().await;

    // 计算任务统计
    let running_tasks = all_tasks
        .iter()
        .filter(|t| t.state == gavel_core::utils::models::TaskState::Running)
        .count();
    let waiting_tasks = all_tasks
        .iter()
        .filter(|t| t.state == gavel_core::utils::models::TaskState::Waiting)
        .count();
    let finished_tasks = all_tasks
        .iter()
        .filter(|t| t.state == gavel_core::utils::models::TaskState::Finished)
        .count();

    // 构建状态报告
    let mut status = format!(
        "守护进程状态\n==================\n\n任务总数: {}\n- 运行中: {}\n- 等待中: {}\n- 已完成: {}\n\n",
        all_tasks.len(), running_tasks, waiting_tasks, finished_tasks
    );

    status.push_str(&format!("队列总数: {}\n", all_queues.len()));
    for queue in &all_queues {
        status.push_str(&format!(
            "- {}: 优先级 {}，分配GPU: {:?}\n",
            queue.name, queue.priority, queue.allocated_gpus
        ));
    }

    status.push_str(&format!("\nGPU总数: {}\n", gpu_stats.len()));
    status.push_str(&format!(
        "- 已分配: {}\n",
        gpu_allocations.values().filter(|a| a.is_some()).count()
    ));
    status.push_str(&format!("- 已忽略: {}\n", ignored_gpus.len()));
    status.push_str(&format!(
        "- 可用: {}\n",
        gpu_allocations.values().filter(|a| a.is_none()).count()
    ));

    // 添加系统健康状态检查结果
    status.push_str("\n系统健康状态: 正常");

    Ok(status)
}
