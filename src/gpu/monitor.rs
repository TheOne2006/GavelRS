use nvml_wrapper::Nvml;
use nvml_wrapper::device::Device;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化 NVML
    let nvml = Nvml::init()?;

    // 获取第一个 GPU 设备（索引 0）
    let device: Device = nvml.device_by_index(0)?;

    // 读取基础信息
    let name = device.name()?;
    let driver_version = nvml.driver_version()?;
    let temperature = device.temperature()?; // 温度（摄氏度）
    let mem_info = device.memory_info()?;     // 显存信息
    let utilization = device.utilization_rates()?; // GPU 使用率

    println!("GPU 名称: {}", name);
    println!("驱动版本: {}", driver_version);
    println!("温度: {}°C", temperature);
    println!("显存使用: {}/{} MB", mem_info.used / 1024 / 1024, mem_info.total / 1024 / 1024);
    println!("GPU 使用率: {}%", utilization.gpu);

    Ok(())
}