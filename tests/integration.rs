use gavelrs::gpu::monitor::GpuMonitor;

#[cfg(test)]
mod gpu_tests {
    use super::*;
    use anyhow::Result;

    #[test]
    fn test_gpu_monitor_initialization() -> Result<()> {
        let monitor = GpuMonitor::new()?;
        // Verify NVML initialization succeeded
        assert!(monitor.device_count().is_ok());
        Ok(())
    }

    #[test]
    fn test_get_stats_valid_index() -> Result<()> {
        let monitor = GpuMonitor::new()?;
        let count = monitor.device_count()?;
        if count == 0 {
            // Skip test if no GPUs detected
            return Ok(());
        }
        let stats = monitor.get_stats(0)?;
        // Validate data ranges
        assert!(stats.temperature <= 150, "Temperature exceeds 150°C");
        assert!(stats.core_usage <= 100, "Core utilization over 100%");
        assert!(
            stats.memory_usage.used <= stats.memory_usage.total,
            "Used memory exceeds total capacity"
        );
        assert!(stats.power_usage > 0, "Power usage not positive");
        Ok(())
    }

    #[test]
    fn test_get_stats_invalid_index() -> Result<()> {
        let monitor = GpuMonitor::new()?;
        let count = monitor.device_count()?;
        let invalid_index = count;  // Use count as first invalid index
        let result = monitor.get_stats(invalid_index);
        // Should return error for invalid index
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn test_get_all_stats() -> Result<()> {
        let monitor = GpuMonitor::new()?;
        let count = monitor.device_count()?;
        let stats = monitor.get_all_stats()?;
        // Result count should match device count
        assert_eq!(stats.len(), count as usize);
        // Verify all results are OK variants
        for (idx, stat) in stats.into_iter().enumerate() {
            assert!(
                stat.is_ok(),
                "GPU {} returned error: {:?}",
                idx,
                stat.err().unwrap()
            );
        }
        Ok(())
    }
}
