// 导出子模块
pub mod task_handler;
pub mod gpu_handler;
pub mod queue_handler;
pub mod submit_handler; // Add submit module

// 从各模块重新导出处理函数
pub use task_handler::handle_task_command;
pub use gpu_handler::handle_gpu_command;
pub use queue_handler::handle_queue_command;
pub use submit_handler::handle_submit_command; // Export submit handler