// core/src/gpu/model.rs
use bincode::{Decode, Encode};
use serde::{Serialize, Deserialize}; // 添加 serde 导入

#[derive(Encode, Decode, Clone, Debug, Serialize, Deserialize, PartialEq)] // 添加 derive
pub enum TaskState {
    Waiting,
    Running,
    Finished,
}

// 优化后的任务元数据
#[derive(Encode, Decode, Clone, Debug, Serialize, Deserialize, PartialEq)] // 添加 derive
pub struct TaskMeta {
    pub pid: Option<i32>,
    pub id: u64,
    pub name: String, // 新增任务名称字段
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
#[derive(Encode, Decode, Clone, Debug, Serialize, Deserialize, PartialEq)] // 添加 derive
pub struct QueueMeta {
    pub name: String,
    pub max_concurrent: u8, // 最大并发任务数
    pub priority: u8,       // 队列优先级 [0-9]
    pub waiting_task_ids: Vec<u64>, // 存储等待任务的 ID
    pub running_task_ids: Vec<u64>, // 存储运行中任务的 ID
    pub allocated_gpus: Vec<u8>,
    pub resource_limit: ResourceLimit, // 新增资源限制
}

// 新增资源限制结构
#[derive(Encode, Decode, Clone, Debug, Serialize, Deserialize, PartialEq)] // 添加 derive
pub struct ResourceLimit {
    pub max_mem: u64,     // 最大显存限制（MB）
    pub min_compute: f32, // 最低计算能力
}

// 为 ResourceLimit 实现 Default trait
impl Default for ResourceLimit {
    fn default() -> Self {
        ResourceLimit {
            max_mem: u64::MAX, // 默认无显存限制
            min_compute: 0.0,  // 默认无最低计算能力要求
        }
    }
}