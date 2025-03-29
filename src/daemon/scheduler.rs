// src/daemon/scheduler.rs
use anyhow::{anyhow, Result};
use serde_json::Value;
use std::io::BufRead;
use std::os::unix::net::UnixListener;
use std::sync::{mpsc, Arc};
use std::thread;

use crate::gpu;

pub fn start_scheduler(listener: UnixListener) -> Result<()> {
    let listener = Arc::new(listener);

    // GPU监控线程
    let gpu_monitor = thread::spawn(move || {
        let monitor = match gpu::monitor::GpuMonitor::new() {
            Ok(m) => m,
            Err(e) => {
                eprintln!("Failed to initialize GPU monitor: {}", e);
                return;
            }
        };

        loop {
            match monitor.get_all_stats() {
                Ok(stats) => process_gpu_stats(stats),
                Err(e) => eprintln!("GPU monitoring error: {}", e),
            }
            std::thread::sleep(std::time::Duration::from_secs(10));
        }
    });

    // 任务接收线程
    let task_receiver = thread::spawn(move || {
        use std::io::BufReader;

        // 创建任务通道
        let (task_sender, task_receiver) = mpsc::channel();

        // 启动任务处理线程
        let processor = thread::spawn(move || {
            for task in task_receiver {
                match process_task(&task) {
                    Ok(_) => println!("Processed task: {:?}", task),
                    Err(e) => eprintln!("Task processing failed: {}", e),
                }
            }
        });

        let listener = Arc::clone(&listener);
        loop {
            match listener.accept() {
                Ok((stream, _addr)) => {
                    let sender = task_sender.clone();
                    thread::spawn(move || {
                        let mut reader = BufReader::new(stream);
                        let mut buffer = String::new();

                        while let Ok(bytes) = reader.read_line(&mut buffer) {
                            if bytes == 0 {
                                break;
                            }

                            // 解析JSON数据
                            match serde_json::from_str::<Value>(buffer.trim()) {
                                Ok(data) => {
                                    if let Err(e) = sender.send(data) {
                                        eprintln!("Failed to queue task: {}", e);
                                    }
                                }
                                Err(e) => eprintln!("Invalid task format: {}", e),
                            }
                            buffer.clear();
                        }
                    });
                }
                Err(e) => eprintln!("Connection error: {}", e),
            }
        }
    });

    gpu_monitor.join().unwrap();
    task_receiver.join().unwrap();
    Ok(())
}

fn process_gpu_stats(stats: Vec<Result<gpu::monitor::GpuStats>>) {
    for (i, stat) in stats.into_iter().enumerate() {
        match stat {
            Ok(s) => {
                let mem_usage = if s.memory_usage.total > 0 {
                    (s.memory_usage.used as f32 / s.memory_usage.total as f32) * 100.0
                } else {
                    0.0
                };
                println!(
                    "GPU {}: Temp {}°C, Usage {}%, Memory {:.1}%",
                    i, s.temperature, s.core_usage, mem_usage
                );
            }
            Err(e) => eprintln!("GPU {} error: {}", i, e),
        }
    }
}

// 任务处理函数
fn process_task(task: &Value) -> Result<()> {
    // 验证必要字段
    let config = task["config"]
        .as_str()
        .ok_or_else(|| anyhow!("Missing config path"))?;
    let gpus = task["gpus"]
        .as_u64()
        .ok_or_else(|| anyhow!("Invalid GPU count"))?;

    // 这里添加实际的任务调度逻辑
    println!("Scheduling task: config={}, GPUs={}", config, gpus);
    Ok(())
}
