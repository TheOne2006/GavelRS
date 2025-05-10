[English](README.md) | [中文简体](README.zh-CN.md)

# 🦀 GavelRS - GPU Task Scheduler by Ice\_Tea

**![Ferris Working](https://img.shields.io/badge/Rustacean-Approved-ff69b4?logo=rust)**  **![Peking University](https://img.shields.io/badge/%E5%8C%97%E4%BA%AC%E5%A4%A7%E5%AD%A6-PKU-red)**  **[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)**  **![Rust](https://img.shields.io/badge/Rust-2021-ff69b4?logo=rust)**

**The Judge of GPU Resources - Making Every GPU Fulfill Its Mission**
**Final Project for the *Rust Programming* Course**

**Only NVIDIA GPUs are supported**

---

## 🚀 Core Features

### 🎛️ Resource Arbitrator

* **Real-time GPU Health Monitoring** – Continuously tracks GPU temperature and memory usage
* **Intelligent Preemptive Scheduling** – Dynamically adjusts tasks based on priority and resource requirements

### ⚡ Development Progress

| Feature                       | Status         | Description                                  |
| ----------------------------- | -------------- | -------------------------------------------- |
| Basic Scheduling Framework    | 🚧 Implemented | Supports task queues and priority management |
| Resource Recycling Mechanism  | 🚧 Implemented | Graceful termination via SIGTERM signal      |
| Distributed Task Coordination | 🚧 Implemented | Multi-node GPU pooling supported             |

---

## 🛠️ Installation Guide

1. **Environment Setup**:

   * Ensure [Rust](https://www.rust-lang.org/tools/install) is installed (latest stable version recommended).
   * Ensure your system has NVIDIA drivers and CUDA Toolkit installed (if running CUDA apps). GavelRS itself does not require the CUDA toolkit for compilation but depends on NVIDIA's NVML library for GPU monitoring.

2. **Clone the Repository**:

   ```bash
   git clone https://github.com/TheOne2006/GavelRS.git
   cd GavelRS
   ```

3. **Build the Project**:

   * Build the daemon (`gavel-daemon`) and the client (`gavelrs`):

     ```bash
     cargo build --release
     ```
   * After building, executables will be located in `target/release/`:

     * `target/release/gavel-daemon` (the daemon)
     * `target/release/gavelrs` (the CLI client)

4. **(Optional) Installation**:
   You can copy the executables into a directory included in your `PATH`, such as `/usr/local/bin/`, or use the files directly from `target/release/`.

   ```bash
   sudo cp target/release/gavel-daemon /usr/local/bin/
   sudo cp target/release/gavelrs /usr/local/bin/
   ```

---

## 📖 Usage Guide

### 1. Daemon (`gavel-daemon`)

The daemon is the core of GavelRS, responsible for GPU monitoring, task queue management, and scheduling.

* **Initialization & Launch**:
  You'll need a config file to start the daemon. Use `configs/example.json` as a reference.

  ```bash
  gavel-daemon /path/to/your/config.json
  ```

  The daemon will run in the background. Logs will be written to the path specified in the config.

* **Check Status**:

  ```bash
  gavelrs daemon status
  ```

* **Stop the Daemon**:

  ```bash
  gavelrs daemon stop
  ```

---

### 2. CLI Client (`gavelrs`)

`gavelrs` is the CLI tool for interacting with the daemon: submitting tasks, managing queues, checking GPU status, etc.

* **Basic Command Format**:

  ```bash
  gavelrs [COMMAND] [SUBCOMMAND] [OPTIONS]
  ```

  Use `gavelrs --help` to view all commands, or `gavelrs <COMMAND> --help` for specific help.

* **Common Usage Examples**:

  * **Submit a Command-Line Task**:

    ```bash
    gavelrs submit command --cmd "echo 'Hello GavelRS on GPU' && sleep 10" --gpu_num 1
    ```

    This submits a task that requests 1 GPU to the default queue.

  * **List Tasks**:

    ```bash
    gavelrs task list
    gavelrs task list --all # Show all tasks, including completed ones
    ```

  * **View GPU Status**:

    ```bash
    gavelrs gpu list
    ```

  * **Manage Queues**:

    ```bash
    gavelrs queue list # List all queues
    gavelrs queue create my_custom_queue # Create a new queue named "my_custom_queue"
    ```

  * **Move a Task to Running Queue**:
    By default, tasks enter the waiting queue upon submission. You can manually move them to the running queue (or configure the target queue to auto-run):

    ```bash
    gavelrs task run <TASK_ID>
    ```

    You can get `<TASK_ID>` from the output of `gavelrs task list`.

For more detailed commands and parameters, refer to the `struct.md` file or use the `--help` option in the terminal.

---

## 📜 Academic Statement

**This project was developed as a final assignment for the *Rust Programming* course at Peking University**
**Academic Integrity Notice: Any form of code plagiarism or ghostwriting is strictly prohibited**

---

## ⭐ Star History

[![Star History Chart](https://api.star-history.com/svg?repos=TheOne2006/GavelRS.git\&type=Date)](https://www.star-history.com/#TheOne2006/GavelRS.git&Date)

---

## 📮 Contact Information

**Ice\_Tea**
[![GitHub](https://img.shields.io/badge/Follow%20Me-GitHub-black?logo=github)](https://github.com/TheOne2006)
[![Email](https://img.shields.io/badge/Any%20Questions-13574662023@163.com-blue?logo=mail.ru)](mailto:13574662023@163.com)