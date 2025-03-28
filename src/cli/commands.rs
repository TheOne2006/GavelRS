// src/cli/commands.rs
use anyhow::{Context, Result};
use structopt::{clap::AppSettings, StructOpt};

use crate::daemon;

#[derive(StructOpt, Debug)]
#[structopt(
    name = "gavelrs",
    global_settings = &[AppSettings::DisableHelpSubcommand]
)]
pub enum AppCommand {
    /// 守护进程管理
    #[structopt(name = "daemon")]
    Daemon(DaemonCommand),

    /// 提交计算任务
    #[structopt(name = "submit")]
    Submit(SubmitCommand),
}

#[derive(StructOpt, Debug)]
pub enum DaemonCommand {
    /// 初始化守护进程
    #[structopt(name = "init")]
    Init,
}

#[derive(StructOpt, Debug)]
pub struct SubmitCommand {
    /// 任务配置文件路径
    #[structopt(short, long)]
    config: String,
    
    /// 需要的GPU数量
    #[structopt(short, long)]
    gpus: u32,
}

impl AppCommand {
    pub fn execute(self) -> Result<()> {
        match self {
            AppCommand::Daemon(cmd) => cmd.execute(),
            AppCommand::Submit(cmd) => cmd.execute(),
        }
    }
}

impl DaemonCommand {
    pub fn execute(self) -> Result<()> {
        match self {
            DaemonCommand::Init => daemon::start(),
        }
    }
}

impl SubmitCommand {
    pub fn execute(self) -> Result<()> {
        // 连接到守护进程的Unix socket
        use std::os::unix::net::UnixStream;
        use std::io::Write;
        
        let socket_path = "/tmp/gavelrs.sock";
        let mut stream = UnixStream::connect(socket_path)
            .with_context(|| format!("Failed to connect to daemon at {}", socket_path))?;

        // 序列化任务数据为JSON
        let task_data = serde_json::json!({
            "config": self.config,
            "gpus": self.gpus,
            "timestamp": chrono::Local::now().to_rfc3339(),
        });

        // 发送任务数据
        stream.write_all(task_data.to_string().as_bytes())
            .context("Failed to send task data")?;
        stream.write_all(b"\n")  // 添加换行符作为消息边界
            .context("Failed to send terminator")?;

        println!("Successfully submitted task: {}", task_data);
        Ok(())
    }
}