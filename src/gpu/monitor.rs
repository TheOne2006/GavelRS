// src/gpu/monitor.rs
use nvml_wrapper::Nvml;
use anyhow::{Result, Context};

#[derive(Debug, Clone)]
pub struct MemoryInfo {
    pub total: u64,
    pub used: u64,
    pub free: u64,
}

#[derive(Debug, Clone)]
pub struct GpuStats {
    pub temperature: u32,
    pub core_usage: u32,
    pub memory_usage: MemoryInfo,
    pub power_usage: u32,
}

#[derive(Debug)]
pub struct GpuMonitor {
    // 持有 NVML 的唯一所有权
    nvml: Nvml,
}

impl GpuMonitor {
    pub fn new() -> Result<Self> {
        let nvml = Nvml::init().context("NVML 初始化失败")?;
        Ok(Self { nvml })
    }

    // 动态获取设备数量
    pub fn device_count(&self) -> Result<u32> {
        self.nvml.device_count().context("获取设备数量失败")
    }

    // 按需获取单个设备信息
    pub fn get_stats(&self, index: u32) -> Result<GpuStats> {
        let device = self.nvml.device_by_index(index)
            .with_context(|| format!("无法获取 GPU 设备 {}", index))?;

        Ok(GpuStats {
            temperature: device.temperature(nvml_wrapper::enum_wrappers::device::TemperatureSensor::Gpu)
                .context("获取温度失败")? as u32,
            core_usage: device.utilization_rates()
                .context("获取核心使用率失败")?.gpu,
            memory_usage: {
                let mem = device.memory_info().context("获取显存信息失败")?;
                MemoryInfo {
                    total: mem.total,
                    used: mem.used,
                    free: mem.free,
                }
            },
            power_usage: device.power_usage()
                .context("获取功耗失败")? as u32,
        })
    }

    // 批量获取所有设备信息
    pub fn get_all_stats(&self) -> Result<Vec<Result<GpuStats>>> {
        let count = self.device_count()?;
        let mut stats = Vec::with_capacity(count as usize);
        
        for i in 0..count {
            stats.push(self.get_stats(i));
        }
        
        Ok(stats)
    }
}