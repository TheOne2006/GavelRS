// core/src/gpu/model.rs
use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize}; // 添加 serde 导入

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
    pub max_concurrent: u8,         // 最大并发任务数
    pub priority: u8,               // 队列优先级 [0-9]
    pub waiting_task_ids: Vec<u64>, // 存储等待任务的 ID
    pub running_task_ids: Vec<u64>, // 存储运行中任务的 ID
    pub allocated_gpus: Vec<u8>,
    pub resource_limit: ResourceLimit, // 新增资源限制
}

// 新增显存要求类型枚举
#[derive(Encode, Decode, Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum MemoryRequirementType {
    Ignore,
    AbsoluteMb,
    Percentage,
}

// 新增资源限制结构
#[derive(Encode, Decode, Clone, Debug, Serialize, Deserialize, PartialEq)] // 添加 derive
pub struct ResourceLimit {
    pub memory_requirement_type: MemoryRequirementType,
    pub memory_requirement_value: u64, // MB for AbsoluteMb, Percentage (0-100) for Percentage
    pub max_gpu_utilization: f32,      // < 0.0 or > 100.0 means ignore
}

// 为 ResourceLimit 实现 Default trait
impl Default for ResourceLimit {
    fn default() -> Self {
        ResourceLimit {
            memory_requirement_type: MemoryRequirementType::Ignore,
            memory_requirement_value: 0,
            max_gpu_utilization: -1.0, // 默认忽略 GPU 利用率限制
        }
    }
}
