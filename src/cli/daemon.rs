use anyhow::Result;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
pub enum DaemonCommand {
    /// Initialize daemon (specify config file)
    #[structopt(name = "init")]
    Init {
        #[structopt(long)]
        config: Option<String>,
    },

    /// Stop daemon process
    #[structopt(name = "stop")]
    Stop,

    /// Check daemon status
    #[structopt(name = "status")]
    Status,
}

impl DaemonCommand {
    pub fn execute(self) -> Result<()> {
        match self {
            Self::Init { ref config } => Self::handle_init(config.as_deref()),
            Self::Stop => Self::handle_stop(),
            Self::Status => Self::handle_status(),
        }
    }

    fn handle_init(config: Option<&str>) -> Result<()> {
        println!("Initializing daemon with config: {:?}", config);
        Ok(())
    }

    fn handle_stop() -> Result<()> {
        println!("Stopping daemon...");
        Ok(())
    }

    fn handle_status() -> Result<()> {
        println!("Checking daemon status...");
        Ok(())
    }
}