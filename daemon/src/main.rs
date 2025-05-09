// src/main.rs
use anyhow::{Context, Ok, Result}; // Import Result
use log::LevelFilter;
use serde::Deserialize;
use std::{env, fs, path::Path};
use gavel_core::utils::logging;

mod daemon;

#[derive(Debug, Deserialize, Clone)] // Clone needed to pass to async block
struct Config {
    #[serde(rename = "bug-level")]
    bug_level: String,
    #[serde(rename = "log-path")]
    log_path: String,
    #[serde(rename = "sock-path")] // Added sock-path
    sock_path: String,
}

// Use tokio::main for the async runtime
#[tokio::main]
async fn main() -> Result<()> {
    // 获取命令行参数
    let args: Vec<String> = env::args().collect();

    // Expect config path as the first argument
    if args.len() < 2 {
        // Use eprintln for errors before logger is initialized
        eprintln!("Usage: gavel-daemon <config_path>");
        // Return an error instead of panicking with bail!
        return Err(anyhow::anyhow!("Usage: gavel-daemon <config_path>"));
    }
    let config_path = &args[1];

    // 读取并解析配置文件
    let config_content = fs::read_to_string(config_path)
        .with_context(|| format!("Failed to read config file: {}", config_path))?;

    let config: Config = serde_json::from_str(&config_content)
        .with_context(|| format!("Failed to parse config file: {}", config_path))?;

    // 转换日志级别
    let log_level = match config.bug_level.to_lowercase().as_str() {
        "trace" => LevelFilter::Trace,
        "debug" => LevelFilter::Debug,
        "info" => LevelFilter::Info,
        "warn" => LevelFilter::Warn,
        "error" => LevelFilter::Error,
        _ => LevelFilter::Info, // 默认 Info 级别
    };

    let log_path_str = config.log_path.clone(); // Clone for logger init
    let sock_path_str = config.sock_path.clone(); // Clone for daemon start

    // 验证日志路径
    let log_path = Path::new(&log_path_str);
    if let Some(parent) = log_path.parent() {
        if !parent.exists() {
             fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create log directory: {}", parent.display()))?;
        }
    }

    // 初始化日志系统
    // Ensure logger initialization happens before any potential logging in daemon::start
    logging::SimpleLogger::init(&log_path_str, log_level)
        .context("Failed to initialize logger")?;

    // Pass sock_path to daemon start function and await its completion
     daemon::start(&sock_path_str).await?; // Pass state to daemon::start
  
    Ok(())
}