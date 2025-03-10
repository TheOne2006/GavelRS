# 🦀 GavelRS - GPU Task Scheduler by Ice (PKU Edition)
**![Ferris Working](https://img.shields.io/badge/Rustacean-Approved-ff69b4?logo=rust)**  **![北京大学](https://img.shields.io/badge/%E5%8C%97%E4%BA%AC%E5%A4%A7%E5%AD%A6-PKU-red)**  **[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)**  **![Rust](https://img.shields.io/badge/Rust-2021-ff69b4?logo=rust)**

**The Judge of GPU Resources - 让每块GPU都找到自己的使命**  
**《Rust 编程》课程大作业 - 让深度学习任务调度更优雅**

```text
                    ⌠╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬⌡
               ⌠╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬⌡
          ⌠╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬⌡
     ⌠╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬⌡
     ╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬
     ╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬╬
```

## 🚀 核心能力
### 🎛️ 资源仲裁者
- **实时GPU健康监测** - 持续监控GPU温度与显存使用
- **智能抢占式调度** - 基于优先级和资源需求的动态调整（✅ 已实现进程组级资源回收）
- **PKU集群适配** - 针对北大计算中心环境优化（支持Slurm基础集成）

### ⚡ 开发进展
| 功能                | 状态   | 说明                          |
|---------------------|--------|------------------------------|
| TUI监控面板         | 🚧 开发中 | 基于ratatui的实时监控界面      |
| 基础调度框架        | ✅ 已实现 | 支持任务队列与优先级管理       |
| 资源回收机制        | ✅ 已实现 | 支持SIGTERM信号优雅终止        |
| 分布式任务协调      | 🔜 规划中 | 多节点GPU池化支持              |

## 🛠️ 安装指南
### 开发者模式
```bash
git clone https://github.com/PKU-ICE-TEA/GavelRS
cargo build --features "pku-cluster"
```

### 学术环境部署
```bash
# 无root权限安装（需预装Rust工具链）
cargo install --path . --features "gpu/nvidia"
```

## 🧑💻 开发者文档
### 架构概览
```text
[CLI] ←gRPC→ [Daemon Core] ↔ [GPU Inspector]
                 ↓
          [Scheduler Brain]
                 ↓
       [Process Warden] ↔ [Logger]
```

### 贡献规范
```bash
# 提交消息格式
git commit -m "feat: [🦀CRAB-42] 增加螃蟹式调度算法"
```

## 📜 学术声明
**本项目为北京大学《Rust 语言编程》课程大作业开发**  
**学术诚信提示：禁止任何形式的代码抄袭或作业代写行为**

## 📮 联系信息
**Ice_Tea（北京大学 2024 级）**  
[![GitHub](https://img.shields.io/badge/Follow%20Me-GitHub-black?logo=github)](https://github.com/Ice-Tech)  
[![Email](https://img.shields.io/badge/课程咨询-13574662023@163.com-blue?logo=mail.ru)](mailto:13574662023@163.com)
