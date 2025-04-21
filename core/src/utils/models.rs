// core/src/gpu/model.rs
use bincode::{Decode, Encode};

#[derive(Encode, Decode)]
pub enum TaskState {
    Waiting,
    Running,
    Finished,
}

// 优化后的任务元数据
#[derive(Encode, Decode)]
pub struct TaskMeta {
    pub id: u64,
    pub cmd: String,
    pub gpu_require: u8,
    pub state: TaskState,
    pub log_path: String,
    pub priority: u8,     // 新增优先级字段 [0-9]
    pub queue: String,    // 所属队列名称
    pub create_time: u64, // SystemTime转为时间戳
    pub gpu_ids: Vec<u8>, // 实际分配的GPU ID列表
}

// 增强队列状态定义
#[derive(Encode, Decode)]
pub struct QueueMeta {
    pub name: String,
    pub max_concurrent: u8, // 最大并发任务数
    pub priority: u8,       // 队列优先级 [0-9]
    pub waiting_tasks: Vec<TaskMeta>,
    pub running_tasks: Vec<TaskMeta>,
    pub allocated_gpus: Vec<u8>,
    pub resource_limit: ResourceLimit, // 新增资源限制
}

// 新增资源限制结构
#[derive(Encode, Decode)]
pub struct ResourceLimit {
    pub max_mem: u64,     // 最大显存限制（MB）
    pub min_compute: f32, // 最低计算能力
}