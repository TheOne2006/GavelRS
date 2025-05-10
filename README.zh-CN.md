[English](README.md) | [中文简体](README.zh-CN.md)
# 🦀 GavelRS - GPU Task Scheduler by Ice_Tea
**![Ferris Working](https://img.shields.io/badge/Rustacean-Approved-ff69b4?logo=rust)**  **![北京大学](https://img.shields.io/badge/%E5%8C%97%E4%BA%AC%E5%A4%A7%E5%AD%A6-PKU-red)**  **[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)**  **![Rust](https://img.shields.io/badge/Rust-2021-ff69b4?logo=rust)**

**The Judge of GPU Resources - 让每块GPU都找到自己的使命**  
**《Rust 编程》课程大作业**

**只支持NVIDIA显卡**

## 🚀 核心能力
### 🎛️ 资源仲裁者
- **实时GPU健康监测** - 持续监控GPU温度与显存使用
- **智能抢占式调度** - 基于优先级和资源需求的动态调整

### ⚡ 开发进展
| 功能                | 状态   | 说明                          |
|---------------------|--------|------------------------------|
| 基础调度框架        | 🚧 已实现 | 支持任务队列与优先级管理       |
| 资源回收机制        | 🚧 已实现 | 支持SIGTERM信号优雅终止        |
| 分布式任务协调      | 🚧 已实现 | 多节点GPU池化支持              |

## 🛠️ 安装指南

1.  **环境准备**:
    *   确保您已安装 [Rust](https://www.rust-lang.org/tools/install) 环境 (推荐最新稳定版)。
    *   确保您的系统已正确安装 NVIDIA 驱动程序和 CUDA 工具包 (如果需要运行 CUDA 应用)。GavelRS 本身不直接依赖 CUDA 工具包进行编译，但监控GPU信息依赖于NVIDIA驱动提供的NVML库。

2.  **克隆仓库**:
    ```bash
    git clone https://github.com/TheOne2006/GavelRS.git
    cd GavelRS
    ```

3.  **编译项目**:
    *   编译守护进程 (`gavel-daemon`) 和客户端 (`gavelrs`):
        ```bash
        cargo build --release
        ```
    *   编译完成后，可执行文件将位于 `target/release/` 目录下:
        *   `target/release/gavel-daemon` (守护进程)
        *   `target/release/gavelrs` (命令行客户端)

4.  **安装 (可选)**:
    您可以将编译好的可执行文件复制到您的 `PATH` 环境变量所包含的目录中，例如 `/usr/local/bin/`，或者直接使用 `target/release/` 下的路径。
    ```bash
    sudo cp target/release/gavel-daemon /usr/local/bin/
    sudo cp target/release/gavelrs /usr/local/bin/
    ```

## 📖 使用指南

### 1. 守护进程 (`gavel-daemon`)

守护进程是 GavelRS 的核心，负责监控GPU、管理任务队列和调度任务。

*   **初始化与启动**:
    您需要一个配置文件来启动守护进程。可以参考项目中的 `configs/example.json` 文件创建一个您自己的配置文件。
    ```bash
    # 假设您的配置文件位于 /path/to/your/config.json
    gavel-daemon /path/to/your/config.json
    ```
    守护进程将在后台运行。日志默认会输出到配置文件中指定的路径。

*   **查看状态**:
    使用 `gavelrs` 客户端查看守护进程状态：
    ```bash
    gavelrs daemon status
    ```

*   **停止守护进程**:
    ```bash
    gavelrs daemon stop
    ```

### 2. 命令行客户端 (`gavelrs`)

`gavelrs` 是与守护进程交互的命令行工具，用于提交任务、管理队列、查看GPU状态等。

*   **基本命令格式**:
    ```bash
    gavelrs [COMMAND] [SUBCOMMAND] [OPTIONS]
    ```
    您可以使用 `gavelrs --help` 查看所有可用命令，以及 `gavelrs <COMMAND> --help` 查看特定命令的帮助信息。

*   **常用操作示例**:

    *   **提交一个命令行任务**:
        ```bash
        gavelrs submit command --cmd "echo 'Hello GavelRS on GPU' && sleep 10" --gpu_num 1
        ```
        这会向默认队列提交一个需要1个GPU的任务。

    *   **列出任务**:
        ```bash
        gavelrs task list
        gavelrs task list --all # 显示所有任务，包括已完成的
        ```

    *   **查看GPU状态**:
        ```bash
        gavelrs gpu list
        ```

    *   **管理队列**:
        ```bash
        gavelrs queue list # 列出所有队列
        gavelrs queue create my_custom_queue # 创建一个名为 my_custom_queue 的新队列
        ```

    *   **将任务移至运行队列**:
        默认情况下，任务提交后进入等待队列，需要手动将其移至运行队列（或目标队列配置为自动运行）。
        ```bash
        gavelrs task run <TASK_ID>
        ```
        `<TASK_ID>` 可以从 `gavelrs task list` 的输出中获取。

详细的命令和参数请参考 `struct.md` 文档或使用命令行的 `--help` 选项。

## 📜 学术声明
**本项目为北京大学《Rust 编程》课程大作业开发**  
**学术诚信提示：禁止任何形式的代码抄袭或作业代写行为**

## ⭐ Star 历史

[![Star History Chart](https://api.star-history.com/svg?repos=TheOne2006/GavelRS.git&type=Date)](https://www.star-history.com/#TheOne2006/GavelRS.git&Date)

## 📮 联系信息
**Ice_Tea**  
[![GitHub](https://img.shields.io/badge/Follow%20Me-GitHub-black?logo=github)](https://github.com/TheOne2006)  
[![Email](https://img.shields.io/badge/Any%20Questions-13574662023@163.com-blue?logo=mail.ru)](mailto:13574662023@163.com)
