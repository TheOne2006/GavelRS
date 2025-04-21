// core/src/gpu/monitor.rs
use anyhow::{Context, Result};
use bincode::{Decode, Encode};
use nvml_wrapper::Nvml;

#[derive(Debug, Clone, Encode, Decode)]
pub struct MemoryInfo {
    pub total: u64, // Total memory
    pub used: u64,  // Used memory
    pub free: u64,  // Free memory
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct GpuStats {
    pub temperature: u32, // Temperature in °C
    pub core_usage: u32,  // GPU core utilization percentage
    pub memory_usage: MemoryInfo,
    pub power_usage: u32, // Power usage in milliwatts
}

#[derive(Debug)]
pub struct GpuMonitor {
    // Holds exclusive ownership of NVML
    nvml: Nvml,
}

impl GpuMonitor {
    pub fn new() -> Result<Self> {
        let nvml = Nvml::init().context("NVML initialization failed")?;
        Ok(Self { nvml })
    }

    // Dynamically get device count
    pub fn device_count(&self) -> Result<u32> {
        self.nvml
            .device_count()
            .context("Failed to get device count")
    }

    // Get stats for a single device on demand
    pub fn get_stats(&self, index: u32) -> Result<GpuStats> {
        let device = self
            .nvml
            .device_by_index(index)
            .with_context(|| format!("Failed to access GPU device {}", index))?;

        Ok(GpuStats {
            temperature: device
                .temperature(nvml_wrapper::enum_wrappers::device::TemperatureSensor::Gpu)
                .context("Failed to get temperature")? as u32,
            core_usage: device
                .utilization_rates()
                .context("Failed to get core utilization")?
                .gpu,
            memory_usage: {
                let mem = device.memory_info().context("Failed to get memory info")?;
                MemoryInfo {
                    total: mem.total,
                    used: mem.used,
                    free: mem.free,
                }
            },
            power_usage: device.power_usage().context("Failed to get power usage")? as u32,
        })
    }

    // Batch get all devices' stats
    pub fn get_all_stats(&self) -> Result<Vec<Result<GpuStats>>> {
        let count = self.device_count()?;
        let mut stats = Vec::with_capacity(count as usize);
        for i in 0..count {
            stats.push(self.get_stats(i));
        }
        Ok(stats)
    }
}
