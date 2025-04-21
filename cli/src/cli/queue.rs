use anyhow::Result;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
pub enum QueueCommand {
    /// List all queue statuses
    #[structopt(name = "list")]
    List,

    /// View queue status
    #[structopt(name = "status")]
    Status {
        /// Queue name
        queue_name: String,
    },

    /// Move all tasks from queue A to queue B
    #[structopt(name = "merge")]
    Merge {
        #[structopt(long, name = "SOURCE_QUEUE")]
        from: String,
        #[structopt(long, name = "DEST_QUEUE")]
        to: String,
    },

    /// Create new queue
    #[structopt(name = "create")]
    Create {
        /// Queue name
        queue_name: String,
    },

    /// Move task to queue
    #[structopt(name = "move")]
    Move {
        /// Task ID
        task_id: String,
        /// Queue name
        queue_name: String,
    },

    /// Set task priority
    #[structopt(name = "priority")]
    Priority {
        /// Task ID
        task_id: String,
        /// Priority level
        level: i32,
    },
}

impl QueueCommand {
    pub fn execute(self) -> Result<()> {
        match self {
            Self::List => Self::handle_list(),
            Self::Status { queue_name } => Self::handle_status(queue_name),
            Self::Merge { from, to } => Self::handle_merge(from, to),
            Self::Create { queue_name } => Self::handle_create(queue_name),
            Self::Move { task_id, queue_name } => Self::handle_move(task_id, queue_name),
            Self::Priority { task_id, level } => Self::handle_priority(task_id, level),
        }
    }

    fn handle_list() -> Result<()> {
        println!("Listing all queues...");
        Ok(())
    }

    fn handle_status(queue_name: String) -> Result<()> {
        println!("Showing status for queue: {}", queue_name);
        Ok(())
    }

    fn handle_merge(from: String, to: String) -> Result<()> {
        println!("Merging queue '{}' into queue '{}'", from, to);
        Ok(())
    }

    fn handle_create(queue_name: String) -> Result<()> {
        println!("Creating queue: {}", queue_name);
        Ok(())
    }

    fn handle_move(task_id: String, queue_name: String) -> Result<()> {
        println!("Moving task '{}' to queue '{}'", task_id, queue_name);
        Ok(())
    }

    fn handle_priority(task_id: String, level: i32) -> Result<()> {
        println!("Setting priority of task '{}' to level {}", task_id, level);
        Ok(())
    }
}