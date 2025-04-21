use anyhow::Ok;
use gavel_core::utils::logging;

// src/main.rs
mod daemon;

fn main() -> anyhow::Result<()> {
    
    daemon::start()?;
    Ok(())
}