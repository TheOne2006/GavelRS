// src/main.rs
use anyhow::{Context, Result};
use gavelrs::gpu;
use std::process;

fn main() -> Result<()> {
    // 初始化监控器
    let monitor = match gpu::GpuMonitor::new() {
        Ok(m) => m,
        Err(e) => {
            eprintln!("❌ 初始化失败: {e}");
            process::exit(1);
        }
    };

    // 获取设备数量
    let device_count = match monitor.device_count() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("❌ 获取设备数量失败: {e}");
            process::exit(1);
        }
    };

    println!("✅ 检测到 {} 个NVIDIA GPU设备", device_count);

    // 获取所有设备状态
    match monitor.get_all_stats() {
        Ok(stats) => {
            for (idx, stat) in stats.into_iter().enumerate() {
                match stat {
                    Ok(data) => print_gpu_info(idx, data),
                    Err(e) => eprintln!("🚨 GPU {} 数据获取失败: {e}", idx),
                }
            }
        }
        Err(e) => eprintln!("❌ 获取设备状态失败: {e}"),
    }

    Ok(())
}

/// 格式化输出GPU信息
fn print_gpu_info(index: usize, stats: gpu::GpuStats) {
    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("  GPU {} 实时监控数据", index);
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("🌡️  温度: {:3}°C", stats.temperature);
    println!("⚡ 核心使用率: {:3}%", stats.core_usage);
    println!("💾 显存使用: {:.2} GB / {:.2} GB ({:.1}%)", 
        bytes_to_gb(stats.memory_usage.used),
        bytes_to_gb(stats.memory_usage.total),
        (stats.memory_usage.used as f64 / stats.memory_usage.total as f64) * 100.0
    );
    println!("🔋 当前功耗: {:.1} W", stats.power_usage as f32 / 1000.0);
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
}

/// 字节转GB换算
fn bytes_to_gb(bytes: u64) -> f64 {
    bytes as f64 / 1024.0 / 1024.0 / 1024.0
}