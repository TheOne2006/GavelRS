mod daemon;
mod submit;
mod task;
mod gpu;
mod queue;

use anyhow::Result;
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