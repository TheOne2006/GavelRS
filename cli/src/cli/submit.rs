use anyhow::Result;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
pub enum SubmitCommand {
    /// Submit command-line task
    #[structopt(name = "command")]
    Command {
        /// Command to execute
        #[structopt(long)]
        cmd: String,
        
        /// Number of GPUs required
        #[structopt(long)]
        gpu_num: u32,
    },

    /// Submit script file task
    #[structopt(name = "script")]
    Script {
        /// Script file path
        #[structopt(long)]
        file: String,
        
        /// Number of GPUs required
        #[structopt(long)]
        gpu_num: u32,
    },

    /// Submit JSON-defined tasks (batch submission)
    #[structopt(name = "json")]
    Json {
        /// JSON file path
        #[structopt(long)]
        file: String,
        
        /// Queue name
        #[structopt(long)]
        queue: Option<String>,
    },
}

impl SubmitCommand {
    pub fn execute(self) -> Result<()> {
        match self {
            Self::Command { cmd, gpu_num } => Self::handle_command(cmd, gpu_num),
            Self::Script { file, gpu_num } => Self::handle_script(file, gpu_num),
            Self::Json { file, queue } => Self::handle_json(file, queue),
        }
    }

    fn handle_command(cmd: String, gpu_num: u32) -> Result<()> {
        println!("Submitting command task: '{}', GPU count: {}", cmd, gpu_num);
        Ok(())
    }

    fn handle_script(file: String, gpu_num: u32) -> Result<()> {
        println!("Submitting script task from file: '{}', GPU count: {}", file, gpu_num);
        Ok(())
    }

    fn handle_json(file: String, queue: Option<String>) -> Result<()> {
        match queue {
            Some(name) => {
                println!("Submitting JSON defined task from file: '{}', Queue: {}", file, name);
                Ok(())
            },
            None => {
                println!("Submitting JSON defined task from file: '{}' with default waiting queue", file);
                Ok(())
            }
        }
    }
}