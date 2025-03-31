// src/daemon/mod.rs
pub mod scheduler;

use anyhow::Result;
use std::os::unix::net::UnixListener;
use std::path::Path;

const SOCKET_PATH: &str = "/tmp/gavelrs.sock";

pub fn start() -> Result<()> {
    // 清理旧套接字文件
    let path = Path::new(SOCKET_PATH);
    if path.exists() {
        std::fs::remove_file(path)?;
    }

    // 创建Unix套接字
    let listener = UnixListener::bind(SOCKET_PATH)?;
    println!("Daemon started listening at {}", SOCKET_PATH);

    // 启动调度器
    scheduler::start_scheduler(listener)
}

#[derive(Debug)]
pub enum DaemonStatus {
    Running,
    Stopped,
    Unknown
}

// 实现状态检查函数
pub fn status() -> Result<DaemonStatus> {
    // 实际实现检查逻辑，例如：
    // - 检查PID文件是否存在
    // - 检查端口是否监听
    // - 使用系统命令查询状态
    Ok(DaemonStatus::Unknown) // 示例实现
}