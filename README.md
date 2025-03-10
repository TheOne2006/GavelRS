# 🦀 GavelRS - GPU Task Scheduler by Ice (International Edition)
**![Ferris Working](https://img.shields.io/badge/Rustacean-Approved-ff69b4?logo=rust)**  
**[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)**

**The Judge of GPU Resources - Mission-Driven GPU Allocation**

## 🚀 Core Features
### 🎛️ Resource Arbiter
- Real-time GPU Monitoring (Temperature/Memory)
- Preemptive Scheduling (Process-group level ✅)
- Dynamic Priority Management

### ⚡ Development Roadmap
| Feature             | Status | Description                  |
|---------------------|--------|------------------------------|
| TUI Dashboard       | 🚧 WIP  | ratatui-based visualization  |
| Core Scheduler      | ✅ Done | Task queue implementation    |
| Resource Reclamation| ✅ Done | SIGTERM graceful termination |
| Distributed Support | 🔜 Planned | Multi-node coordination      |

## 🛠️ Installation
```bash
# From Source
git clone https://github.com/Ice-Tech/GavelRS
cargo build --features "gpu/nvidia"
```

## 📜 Open Source Pledge
**Ice's Quality Gate:**
```bash
cargo fmt --check && cargo clippy -- -D warnings
cargo test --all-features -- --test-threads=1
```

[![Star History Chart](https://api.star-history.com/svg?repos=Ice-Tech/GavelRS&type=Date)](https://star-history.com/#Ice-Tech/GavelRS&Date)
