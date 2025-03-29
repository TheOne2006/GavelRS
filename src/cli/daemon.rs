// src/cli/daemon.rs

use anyhow::Result;
use structopt::StructOpt;
use crate::daemon;

// 守护进程子命令
#[derive(StructOpt, Debug)]
pub enum DaemonCommand {
    Init,
}

impl DaemonCommand {
    pub fn execute(self) -> Result<()> {
        
        match self {
            DaemonCommand::Init => daemon::start(),
        }
    }
} 