// src/main.rs
use cli::commands::AppCommand;
use structopt::StructOpt;

mod cli;
mod daemon;
mod gpu;

fn main() -> anyhow::Result<()> {
    let cmd = AppCommand::from_args();
    cmd.execute()
}