// src/main.rs
use cli::AppCommand;
use structopt::StructOpt;

mod cli;

fn main() -> anyhow::Result<()> {
    let cmd = AppCommand::from_args();
    cmd.execute()
}
