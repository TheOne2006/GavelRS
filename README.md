[English](README.md) | [中文简体](README.zh-CN.md)
# 🦀 GavelRS - GPU Task Scheduler by Ice_Tea 
**![Ferris Working](https://img.shields.io/badge/Rustacean-Approved-ff69b4?logo=rust)** **![Peking University](https://img.shields.io/badge/Peking%20University-PKU-red)**  **[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)** **![Rust](https://img.shields.io/badge/Rust-2021-ff69b4?logo=rust)**

**The Judge of GPU Resources - Mission-Driven GPU Allocation**

**Only For the NVIDIA GPU**

**Work For Peking University Course "Rust promgramming"** 

## 🚀 Core Features
### 🎛️ Resource Arbiter
- Real-time GPU Monitoring (Temperature/Memory)
- Preemptive Scheduling (Process-group level ✅)
- Dynamic Priority Management

### ⚡ Development Roadmap
| Feature             | Status | Description                  |
|---------------------|--------|------------------------------|
| TUI Dashboard       | 🚧 WIP  | ratatui-based visualization  |
| Core Scheduler      | 🚧 WIP | Task queue implementation    |
| Resource Reclamation| 🚧 WIP | SIGTERM graceful termination |
| Distributed Support | 🔜 Planned | Multi-node coordination      |

## 🛠️ Installation

1.  **Prerequisites**:
    *   Ensure you have the [Rust](https://www.rust-lang.org/tools/install) environment installed (latest stable version recommended).
    *   Ensure your system has NVIDIA drivers and the CUDA toolkit correctly installed (if you need to run CUDA applications). GavelRS itself does not directly depend on the CUDA toolkit for compilation, but GPU monitoring relies on the NVML library provided by NVIDIA drivers.

2.  **Clone the Repository**:
    ```bash
    git clone https://github.com/TheOne2006/GavelRS.git
    cd GavelRS
    ```

3.  **Build the Project**:
    *   Compile the daemon (`gavel-daemon`) and the client (`gavelrs`):
        ```bash
        cargo build --release
        ```
    *   After compilation, the executables will be located in the `target/release/` directory:
        *   `target/release/gavel-daemon` (Daemon process)
        *   `target/release/gavelrs` (Command-line client)

4.  **Install (Optional)**: