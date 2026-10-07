# RigMap

**RigMap** is a lightweight Windows 11 Task Manager & System Specifications Inspector built in **pure Rust** (`eframe`/`egui`, `sysinfo`, `window-vibrancy`, `winreg`) with a **WinUI 3 Fluent Design** interface and native **Windows 11 Mica** backdrop.

## Features

- **⊞ Processes**:
  - Smart grouping of multi-process desktop and Microsoft Store / UWP apps (such as Google Chrome, WhatsApp, Spotify, VS Code, Discord) with expandable child process trees.
  - Windows 11 style resource heat-map shading for CPU, Memory, and Disk columns.
  - Hold **`Ctrl`** at any time to temporarily freeze the process list from moving while selecting tasks.
  - Standard **End task** and elevated **🛡 End task (Admin)** / **Restart as Admin** support.
- **📈 Performance**:
  - Live 60-second sparklines and detailed charts for **CPU** (with overall & per-logical-core grid toggle), **Memory**, **Disk I/O**, and **Network** throughput.
- **💻 This PC (System Specifications)**:
  - Dynamically reads hardware and OS specifications on any Windows PC (Device name, Laptop/PC model & manufacturer, Motherboard/BIOS, CPU, all integrated/dedicated GPUs with VRAM & driver versions, RAM, physical SSD/NVMe drives, partitions, and Windows 11/10 edition & build).
- **🚀 Startup Apps**:
  - Inspects startup entries across Registry (`HKCU`/`HKLM` `Run` & `WOW6432Node`), Startup folders, and Microsoft Store / UWP `StartupTask` entries.
- **⚙ Settings**:
  - Switch between **WinUI 3 Dark & Light themes**, adjust real-time polling intervals, toggle resource heatmaps, and pin the window **Always on top**.

## Build & Run

```powershell
cargo run --release
```
