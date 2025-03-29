// src/cli/commands.rs
use anyhow::Result;
use structopt::{clap::AppSettings, StructOpt};
use super::daemon;
use super::submit;

#[derive(StructOpt, Debug)]
#[structopt(
    name = "gavelrs",
    global_settings = &[AppSettings::DisableHelpSubcommand]
)]
pub enum AppCommand {
    /// 守护进程管理
    #[structopt(name = "daemon")]
    Daemon(daemon::DaemonCommand),

    /// 提交计算任务
    #[structopt(name = "submit")]
    Submit(submit::SubmitCommand),
}

impl AppCommand {
    pub fn execute(self) -> Result<()> {
        match self {
            AppCommand::Daemon(cmd) => cmd.execute(),
            AppCommand::Submit(cmd) => cmd.execute(),
        }
    }
}