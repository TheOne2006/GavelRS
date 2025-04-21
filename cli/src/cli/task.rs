use anyhow::Result;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
pub enum TaskCommand {
    /// List tasks (default: pending tasks)
    #[structopt(name = "list")]
    List {
        /// Show all tasks
        #[structopt(long, conflicts_with = "running")]
        all: bool,
        
        /// Show only running tasks
        #[structopt(long, conflicts_with = "all")]
        running: bool,
    },

    /// View task details
    #[structopt(name = "info")]
    Info {
        /// Task ID
        task_id: String,
    },

    /// Add task to running queue
    #[structopt(name = "run")]
    Run {
        /// Task ID
        task_id: String,
    },

    /// Terminate task
    #[structopt(name = "kill")]
    Kill {
        /// Task ID
        task_id: String,
    },

    /// View task logs
    #[structopt(name = "logs")]
    Logs {
        /// Task ID
        task_id: String,
        
        /// Show only log tail
        #[structopt(long)]
        tail: bool,
    },
}

impl TaskCommand {
    pub fn execute(self) -> Result<()> {
        match self {
            Self::List { all, running } => Self::handle_list(all, running),
            Self::Info { task_id } => Self::handle_info(task_id),
            Self::Run { task_id } => Self::handle_run(task_id),
            Self::Kill { task_id } => Self::handle_kill(task_id),
            Self::Logs { task_id, tail } => Self::handle_logs(task_id, tail),
        }
    }

    fn handle_list(all: bool, running: bool) -> Result<()> {
        if all {
            println!("Listing all tasks...");
        } else if running {
            println!("Listing running tasks...");
        } else {
            println!("Listing pending tasks...");
        }
        Ok(())
    }

    fn handle_info(task_id: String) -> Result<()> {
        println!("Showing information for task: {}", task_id);
        Ok(())
    }

    fn handle_run(task_id: String) -> Result<()> {
        println!("Running task: {}", task_id);
        Ok(())
    }

    fn handle_kill(task_id: String) -> Result<()> {
        println!("Killing task: {}", task_id);
        Ok(())
    }

    fn handle_logs(task_id: String, tail: bool) -> Result<()> {
        if tail {
            println!("Showing tail of logs for task: {}", task_id);
        } else {
            println!("Showing full logs for task: {}", task_id);
        }
        Ok(())
    }
}