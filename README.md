<div align="center">

<img src="assets/icon.png" alt="RigMap Logo" width="128" height="128" />

# RigMap

**A fast, lightweight Windows 11 Task Manager & System Hardware Inspector written in 100% pure Rust.**  
Designed with a **WinUI 3 Fluent Design** interface and native **Windows 11 Mica** backdrop.

[![Release](https://img.shields.io/badge/Release-v1.0-f58220?style=for-the-badge)](https://github.com/susenayw/RigMap/releases/tag/v1.0)
[![Platform](https://img.shields.io/badge/Platform-Windows%2010%20%7C%2011-0078D4?style=for-the-badge&logo=windows)](https://github.com/susenayw/RigMap)
[![Language](https://img.shields.io/badge/Built%20With-Rust-dea584?style=for-the-badge&logo=rust)](https://www.rust-lang.org/)

[**⬇ Download RigMap v1.0 (`rigmap.exe`)**](https://github.com/susenayw/RigMap/releases/tag/v1.0)

</div>

---

## ✨ Key Features

### ⊞ Smart Process Manager
* **App Grouping & Parent-Child Attribution:** Automatically groups multi-process desktop and Microsoft Store / UWP applications (such as **Google Chrome**, **WhatsApp**, **Spotify**, **Discord**, **VS Code**) and attributes child `WebView2` / helper processes to their parent application.
* **Apps on Top:** Keeps user applications pinned above background system processes for instant access, with expandable process trees (`▶` / `▼`).
* **Windows 11 Resource Heat-Map:** Dynamic heat-tinted cells for CPU, Memory, and Disk I/O usage.
* **Hold `Ctrl` to Freeze List:** Just like native Windows Task Manager, holding down the **`Ctrl`** key temporarily freezes the process list so rows never jump while you select or inspect a task.
* **Administrator Controls:** Includes standard **End task**, elevated **🛡 End task (Admin)** (terminates stubborn process trees via UAC), **➕ Run new task** (with Administrator privilege option), and one-click **🛡 Restart as Admin**.

### 📈 Real-Time Performance Telemetry
* **Live 60-Second Sparklines & Charts:** Smooth utilization graphs for **CPU**, **Memory**, **Disk I/O**, and **Network**.
* **Per-Core CPU Grid:** Toggle between overall CPU utilization and individual live charts for every logical processor core.
* **Memory & Volume Composition:** Visual RAM composition bar and per-partition read/write throughput & capacity gauges.

### 💻 This PC — Universal Hardware & OS Inspector
* **Zero Hardcoding — Works on Any Windows PC:** Dynamically queries Windows Registry hardware classes (`BIOS`, `CentralProcessor`, Display Adapter GUID `{4d36e968-e325-11ce-bfc1-08002be10318}`, `Services\disk\Enum`, and `Windows NT\CurrentVersion`) alongside `sysinfo`:
  * **Laptop / PC Identity:** Device name, exact laptop/motherboard model, manufacturer, and BIOS version.
  * **Processor (CPU):** Full processor brand string, base clock speed, physical cores, and logical threads.
  * **Graphics Cards (GPU):** Detects both dedicated (NVIDIA / AMD) and integrated GPUs, including dedicated VRAM capacity and installed driver versions.
  * **Installed RAM & Storage Hardware:** Total and usable RAM, physical NVMe/SSD/HDD drive model names, and interactive partition cards (`C:\`, `D:\`, etc.) with one-click **📂 Open** in Explorer.
  * **Windows Specifications:** Edition (`Windows 11` / `Windows 10`), feature version (`24H2` / `26H2`), full OS build (`Build.UBR`), install date, and a one-click **📋 Copy all specs** button.

### 🚀 Startup Apps, 👤 Users & ☰ Details
* **Comprehensive Startup Inspector:** Lists startup entries across `HKCU\...\Run`, `HKLM\...\Run`, `WOW6432Node`, Startup folders, and **Microsoft Store / UWP `StartupTask` packages** with enabled/disabled state and startup impact estimates.
* **Active User Session & Technical Details:** View session resource totals or switch to the flat **Details** table with PIDs, Working Set, Virtual Memory, and full executable paths.

---

## ⌨️ Keyboard & Mouse Shortcuts

| Shortcut / Action | Description |
| :--- | :--- |
| **Hold `Ctrl`** | Temporarily freezes the process list and sorting order while held |
| **`Delete`** | Ends the currently selected app group or process |
| **`Shift + Delete`** | Force-ends the selected app group or process tree as **Administrator** |
| **Double-click App Group** | Expands or collapses child processes (`chrome.exe`, `WhatsApp`, etc.) |
| **Right-click Row** | Opens context menu (*End task*, *End task as Admin*, *Open file location*, *Copy PID*, *Go to details*) |

---

## 🛠️ Tech Stack & Architecture

* **GUI & Rendering:** [`eframe` / `egui` 0.29](https://github.com/emilk/egui) (`glow` backend), `egui_extras`, `egui_plot`
* **Windows 11 Backdrop:** [`window-vibrancy`](https://github.com/tauri-apps/window-vibrancy) (Native DWM Mica & Acrylic effects)
* **System Telemetry & Registry:** [`sysinfo`](https://github.com/GuillaumeGomez/sysinfo) + [`winreg`](https://github.com/gentoo90/winreg-rs)
* **Footprint:** ~4.8 MB single standalone `.exe`

```text
RigMap/
├── assets/
│   ├── icon.png            # Official RigMap icon
│   ├── icon.ico            # Windows executable icon resource
│   └── app.rc              # Win32 resource script
├── src/
│   ├── main.rs             # Viewport setup, icon & Windows 11 Mica initialization
│   ├── app.rs              # WinUI 3 NavigationView, Processes, Performance, This PC, Startup, Settings
│   ├── system_monitor.rs   # Process grouping, hardware/OS registry inspector, Admin taskkill
│   └── theme.rs            # WinUI 3 Fluent Dark/Light palettes & Segoe UI Variable typography
├── build.rs                # Compiles & embeds Windows .ico resource into rigmap.exe
└── Cargo.toml
```

---

## 🚀 Build from Source

### Prerequisites
* [Rust](https://rustup.rs/) (`stable` toolchain)

### Compile & Run
```powershell
git clone https://github.com/susenayw/RigMap.git
cd RigMap
cargo run --release
```

The optimized standalone executable will be generated at `target/release/rigmap.exe`.
