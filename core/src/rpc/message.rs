// core/src/rpc/message.rs
use crate::gpu::monitor::GpuStats;
use crate::utils::models::{TaskMeta, QueueMeta};
use bincode::{Decode, Encode};
/// 基础消息类型枚举
#[derive(Encode, Decode)]
pub enum Message {
    // 控制指令
    DaemonCommand(DaemonAction),
    TaskCommand(TaskAction),
    GPUCommand(GPUAction),
    QueueCommand(QueueAction),

    // 数据实体
    GPUStatus(Vec<GpuStats>),
    TaskStatus(Vec<TaskMeta>),
    QueueStatus(Vec<QueueMeta>),

    // 系统消息
    Ack(String),   // 操作确认
    Error(String), // 错误响应
}

// 守护进程操作指令
#[derive(Encode, Decode)]
pub enum DaemonAction {
    Init { config_path: String }, // 携带配置文件路径
    Stop,
    Status,
    ReloadConfig,
}

// 任务操作指令
#[derive(Encode, Decode)]
pub enum TaskAction {
    List { filter: TaskFilter }, // 增加过滤条件
    Info { task_id: u64 },
    Run { task_id: u64 },
    Kill { task_id: u64 },
    Logs { task_id: u64, tail: bool },
}

// GPU操作指令
#[derive(Encode, Decode)]
pub enum GPUAction {
    List,
    Info { gpu_id: Option<u8> },                  // 可选GPU ID
    Allocate { gpu_ids: Vec<u8>, queue: String }, // 绑定到指定队列
    Release { gpu_id: u8 },
    Ignore { gpu_id: u8 },
    ResetIgnored,
}

// 队列操作指令
#[derive(Encode, Decode)]
pub enum QueueAction {
    List,
    Status { queue_name: String },
    Merge { source: String, dest: String },
    Create { name: String, priority: u8 }, // 新建带优先级的队列
    Move { task_id: u64, dest_queue: String },
    SetPriority { queue_name: u64, level: u8 },
}

// 任务过滤条件
#[derive(Encode, Decode)]
pub enum TaskFilter {
    All,
    Running,
    Finished,
    ByQueue(String), // 按队列过滤
    ByUser(String),  // 预留用户字段
}
