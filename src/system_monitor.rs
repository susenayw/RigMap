use std::collections::{HashMap, VecDeque};
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};
use sysinfo::{Disks, Networks, Pid, ProcessesToUpdate, System};
use winreg::enums::*;
use winreg::RegKey;

pub const HISTORY_LEN: usize = 60;
const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Clone, Debug)]
pub struct ProcessEntry {
    pub pid: u32,
    pub name: String,
    pub display_name: String,
    pub group_key: String,
    pub is_user_app: bool,
    pub status: String,
    pub cpu_pct: f32,
    pub memory_bytes: u64,
    pub memory_pct: f32,
    pub virtual_mem_bytes: u64,
    pub disk_total_bps: u64,
    pub run_time_secs: u64,
    pub exe_path: String,
}

#[derive(Clone, Debug)]
pub struct AppGroupEntry {
    pub group_key: String,
    pub display_name: String,
    pub is_user_app: bool,
    pub primary_pid: u32,
    pub cpu_pct: f32,
    pub memory_bytes: u64,
    pub memory_pct: f32,
    pub disk_total_bps: u64,
    pub max_run_time_secs: u64,
    pub exe_path: String,
    pub children: Vec<ProcessEntry>,
}

#[derive(Clone, Debug)]
pub struct DiskEntry {
    pub name: String,
    pub mount_point: String,
    pub file_system: String,
    pub kind: String,
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub used_bytes: u64,
    pub used_pct: f32,
}

#[derive(Clone, Debug)]
pub struct NetworkEntry {
    pub name: String,
    pub total_rx_bytes: u64,
    pub total_tx_bytes: u64,
}

#[derive(Clone, Debug)]
pub struct StartupEntry {
    pub name: String,
    pub command: String,
    pub location: String,
    pub enabled: bool,
    pub impact_estimate: &'static str,
}

#[derive(Clone, Debug)]
pub struct GpuSpec {
    pub name: String,
    pub provider: String,
    pub driver_version: String,
    pub vram_bytes: u64,
}

#[derive(Clone, Debug)]
pub struct PcSpecs {
    pub device_name: String,
    pub system_model: String,
    pub system_manufacturer: String,
    pub baseboard_model: String,
    pub bios_version: String,
    pub cpu_name: String,
    pub cpu_vendor: String,
    pub cpu_base_mhz: u32,
    pub physical_cores: usize,
    pub logical_cores: usize,
    pub system_type: String,
    pub installed_ram_bytes: u64,
    pub gpus: Vec<GpuSpec>,
    pub physical_disks: Vec<String>,
    pub windows_edition: String,
    pub windows_version: String,
    pub windows_build: String,
    pub windows_install_date: String,
    pub registered_owner: String,
}

pub struct SystemMonitor {
    sys: System,
    disks: Disks,
    networks: Networks,
    last_refresh: Instant,
    pub refresh_interval: Duration,
    pub paused: bool,
    pub is_elevated: bool,

    // Hardware metadata & full "This PC" specs
    pub cpu_brand: String,
    pub physical_cores: usize,
    pub logical_cores: usize,
    pub os_name: String,
    pub host_name: String,
    pub current_user: String,
    pub pc_specs: PcSpecs,

    // Live metrics
    pub cpu_usage_total: f32,
    pub cpu_freq_ghz: f32,
    pub per_core_usage: Vec<f32>,
    pub per_core_history: Vec<VecDeque<f32>>,

    pub mem_total_bytes: u64,
    pub mem_used_bytes: u64,
    pub mem_available_bytes: u64,
    pub mem_usage_pct: f32,
    pub swap_total_bytes: u64,
    pub swap_used_bytes: u64,

    pub disk_read_bps: u64,
    pub disk_write_bps: u64,
    pub net_rx_bps: u64,
    pub net_tx_bps: u64,
    pub uptime_secs: u64,

    // Rolling histories (60 samples)
    pub cpu_history: VecDeque<f32>,
    pub mem_history: VecDeque<f32>,
    pub disk_read_history: VecDeque<f32>,
    pub disk_write_history: VecDeque<f32>,
    pub net_rx_history: VecDeque<f32>,
    pub net_tx_history: VecDeque<f32>,

    // Snapshot lists
    pub processes: Vec<ProcessEntry>,
    pub app_groups: Vec<AppGroupEntry>,
    pub disk_list: Vec<DiskEntry>,
    pub network_list: Vec<NetworkEntry>,
    pub startup_apps: Vec<StartupEntry>,
}

impl SystemMonitor {
    pub fn new() -> Self {
        let mut sys = System::new_all();
        sys.refresh_all();

        let disks = Disks::new_with_refreshed_list();
        let networks = Networks::new_with_refreshed_list();

        let logical_cores = sys.cpus().len().max(1);
        let physical_cores = sys.physical_core_count().unwrap_or(logical_cores);
        let cpu_brand = sys
            .cpus()
            .first()
            .map(|c| c.brand().trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "Windows Processor".to_string());

        let host_name = System::host_name().unwrap_or_else(|| "LOCALHOST".to_string());
        let current_user = std::env::var("USERNAME").unwrap_or_else(|_| "User".to_string());
        let is_elevated = Self::check_is_elevated();

        let pc_specs = Self::query_pc_specs(
            &host_name,
            &cpu_brand,
            physical_cores,
            logical_cores,
            sys.total_memory(),
        );
        let os_name = format!(
            "{} ({})",
            pc_specs.windows_edition, pc_specs.windows_version
        );

        let mut per_core_history = Vec::with_capacity(logical_cores);
        for _ in 0..logical_cores {
            let mut q = VecDeque::with_capacity(HISTORY_LEN);
            q.extend(std::iter::repeat(0.0).take(HISTORY_LEN));
            per_core_history.push(q);
        }

        let init_queue = || {
            let mut q = VecDeque::with_capacity(HISTORY_LEN);
            q.extend(std::iter::repeat(0.0).take(HISTORY_LEN));
            q
        };

        let mut monitor = Self {
            sys,
            disks,
            networks,
            last_refresh: Instant::now() - Duration::from_secs(2),
            refresh_interval: Duration::from_millis(1000),
            paused: false,
            is_elevated,
            cpu_brand,
            physical_cores,
            logical_cores,
            os_name,
            host_name,
            current_user,
            pc_specs,
            cpu_usage_total: 0.0,
            cpu_freq_ghz: 0.0,
            per_core_usage: vec![0.0; logical_cores],
            per_core_history,
            mem_total_bytes: 1,
            mem_used_bytes: 0,
            mem_available_bytes: 0,
            mem_usage_pct: 0.0,
            swap_total_bytes: 0,
            swap_used_bytes: 0,
            disk_read_bps: 0,
            disk_write_bps: 0,
            net_rx_bps: 0,
            net_tx_bps: 0,
            uptime_secs: 0,
            cpu_history: init_queue(),
            mem_history: init_queue(),
            disk_read_history: init_queue(),
            disk_write_history: init_queue(),
            net_rx_history: init_queue(),
            net_tx_history: init_queue(),
            processes: Vec::new(),
            app_groups: Vec::new(),
            disk_list: Vec::new(),
            network_list: Vec::new(),
            startup_apps: Vec::new(),
        };

        monitor.refresh_startup_apps();
        monitor.force_refresh();
        monitor
    }

    /// Dynamically reads hardware & Windows specifications from universal Windows Registry keys
    /// so it works accurately on any laptop or desktop PC.
    fn query_pc_specs(
        host_name: &str,
        fallback_cpu: &str,
        physical_cores: usize,
        logical_cores: usize,
        installed_ram_bytes: u64,
    ) -> PcSpecs {
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);

        // 1. Laptop / PC Model, Manufacturer, Motherboard & BIOS
        let mut system_manufacturer = String::new();
        let mut system_model = String::new();
        let mut baseboard_model = String::new();
        let mut bios_version = String::new();

        if let Ok(bios_key) = hklm.open_subkey(r"HARDWARE\DESCRIPTION\System\BIOS") {
            system_manufacturer = bios_key
                .get_value::<String, _>("SystemManufacturer")
                .unwrap_or_default()
                .trim()
                .to_string();
            system_model = bios_key
                .get_value::<String, _>("SystemProductName")
                .unwrap_or_default()
                .trim()
                .to_string();
            baseboard_model = bios_key
                .get_value::<String, _>("BaseBoardProduct")
                .unwrap_or_default()
                .trim()
                .to_string();
            bios_version = bios_key
                .get_value::<String, _>("BIOSVersion")
                .unwrap_or_default()
                .trim()
                .to_string();
        }

        if system_model.is_empty() || system_manufacturer.is_empty() {
            if let Ok(sys_info_key) =
                hklm.open_subkey(r"SYSTEM\CurrentControlSet\Control\SystemInformation")
            {
                if system_manufacturer.is_empty() {
                    system_manufacturer = sys_info_key
                        .get_value::<String, _>("SystemManufacturer")
                        .unwrap_or_default()
                        .trim()
                        .to_string();
                }
                if system_model.is_empty() {
                    system_model = sys_info_key
                        .get_value::<String, _>("SystemProductName")
                        .unwrap_or_default()
                        .trim()
                        .to_string();
                }
            }
        }

        if system_manufacturer.is_empty() {
            system_manufacturer = "Standard PC Manufacturer".to_string();
        }
        if system_model.is_empty() {
            system_model = host_name.to_string();
        }

        // 2. CPU Details
        let mut cpu_name = fallback_cpu.to_string();
        let mut cpu_vendor = String::new();
        let mut cpu_base_mhz = 0u32;

        if let Ok(cpu_key) = hklm.open_subkey(r"HARDWARE\DESCRIPTION\System\CentralProcessor\0") {
            if let Ok(name) = cpu_key.get_value::<String, _>("ProcessorNameString") {
                let trimmed = name.trim();
                if !trimmed.is_empty() {
                    cpu_name = trimmed.to_string();
                }
            }
            cpu_vendor = cpu_key
                .get_value::<String, _>("VendorIdentifier")
                .unwrap_or_default()
                .trim()
                .to_string();
            cpu_base_mhz = cpu_key.get_value::<u32, _>("~MHz").unwrap_or(0);
        }

        // 3. GPU Adapters (via universal Windows Display Class GUID)
        let mut gpus = Vec::new();
        if let Ok(display_class) = hklm.open_subkey(
            r"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}",
        ) {
            for subkey_name in display_class.enum_keys().flatten() {
                // Only inspect 4-digit adapter subkeys ("0000", "0001", etc.)
                if subkey_name.len() != 4 || !subkey_name.chars().all(|c| c.is_ascii_digit()) {
                    continue;
                }
                if let Ok(adapter_key) = display_class.open_subkey(&subkey_name) {
                    let desc = adapter_key
                        .get_value::<String, _>("DriverDesc")
                        .or_else(|_| {
                            adapter_key.get_value::<String, _>("HardwareInformation.AdapterString")
                        })
                        .unwrap_or_default()
                        .trim()
                        .to_string();

                    if desc.is_empty()
                        || desc.eq_ignore_ascii_case("Microsoft Basic Render Driver")
                    {
                        continue;
                    }

                    let provider = adapter_key
                        .get_value::<String, _>("ProviderName")
                        .unwrap_or_else(|_| "Display Adapter".to_string())
                        .trim()
                        .to_string();
                    let driver_version = adapter_key
                        .get_value::<String, _>("DriverVersion")
                        .unwrap_or_else(|_| "Unknown".to_string())
                        .trim()
                        .to_string();

                    // Read 64-bit qwMemorySize or 32-bit MemorySize regardless of REG_QWORD / REG_DWORD / REG_BINARY type
                    let vram_bytes =
                        Self::read_reg_u64(&adapter_key, "HardwareInformation.qwMemorySize")
                            .or_else(|| {
                                Self::read_reg_u64(&adapter_key, "HardwareInformation.MemorySize")
                            })
                            .unwrap_or(0);

                    // Avoid duplicate entries if same GPU name is already listed
                    if !gpus.iter().any(|g: &GpuSpec| g.name == desc) {
                        gpus.push(GpuSpec {
                            name: desc,
                            provider,
                            driver_version,
                            vram_bytes,
                        });
                    }
                }
            }
        }
        // Sort GPUs so dedicated GPUs (highest VRAM) appear first
        gpus.sort_by(|a, b| b.vram_bytes.cmp(&a.vram_bytes));

        // 4. Physical Storage Disk Models (SSD / NVMe / HDD hardware names)
        let mut physical_disks = Vec::new();
        if let Ok(disk_enum) = hklm.open_subkey(r"SYSTEM\CurrentControlSet\Services\disk\Enum") {
            let count = disk_enum.get_value::<u32, _>("Count").unwrap_or(0);
            for idx in 0..count {
                if let Ok(pnp_path) = disk_enum.get_value::<String, _>(&idx.to_string()) {
                    let full_enum_path = format!(r"SYSTEM\CurrentControlSet\Enum\{}", pnp_path);
                    if let Ok(dev_key) = hklm.open_subkey(&full_enum_path) {
                        if let Ok(friendly) = dev_key.get_value::<String, _>("FriendlyName") {
                            let clean = friendly.trim().to_string();
                            if !clean.is_empty() {
                                physical_disks.push(clean);
                            }
                        }
                    }
                }
            }
        }

        // 5. Windows Version, Edition, Build & Install Date
        let mut windows_edition = "Windows".to_string();
        let mut windows_version = String::new();
        let mut windows_build = String::new();
        let mut windows_install_date = "Unknown".to_string();
        let mut registered_owner = String::new();

        if let Ok(nt_key) = hklm.open_subkey(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion") {
            let raw_product = nt_key
                .get_value::<String, _>("ProductName")
                .unwrap_or_else(|_| "Windows".to_string());
            let build_str = nt_key
                .get_value::<String, _>("CurrentBuildNumber")
                .or_else(|_| nt_key.get_value::<String, _>("CurrentBuild"))
                .unwrap_or_default();
            let ubr = nt_key.get_value::<u32, _>("UBR").unwrap_or(0);
            let build_num: u32 = build_str.trim().parse().unwrap_or(0);

            // Windows 11 starts at build 22000, even though Registry ProductName still says "Windows 10 ..."
            windows_edition = if build_num >= 22000 && raw_product.contains("Windows 10") {
                raw_product.replace("Windows 10", "Windows 11")
            } else {
                raw_product
            };

            windows_version = nt_key
                .get_value::<String, _>("DisplayVersion")
                .or_else(|_| nt_key.get_value::<String, _>("ReleaseId"))
                .unwrap_or_else(|_| "23H2".to_string());

            windows_build = if ubr > 0 && !build_str.is_empty() {
                format!("{}.{}", build_str, ubr)
            } else {
                build_str
            };

            if let Ok(install_ts) = nt_key.get_value::<u32, _>("InstallDate") {
                if install_ts > 0 {
                    windows_install_date = Self::format_unix_date(install_ts as u64);
                }
            }

            registered_owner = nt_key
                .get_value::<String, _>("RegisteredOwner")
                .unwrap_or_default()
                .trim()
                .to_string();
        }

        let arch_desc = match std::env::consts::ARCH {
            "x86_64" => "64-bit operating system, x64-based processor",
            "aarch64" => "64-bit operating system, ARM64-based processor",
            "x86" => "32-bit operating system, x86-based processor",
            other => other,
        }
        .to_string();

        PcSpecs {
            device_name: host_name.to_string(),
            system_model,
            system_manufacturer,
            baseboard_model,
            bios_version,
            cpu_name,
            cpu_vendor,
            cpu_base_mhz,
            physical_cores,
            logical_cores,
            system_type: arch_desc,
            installed_ram_bytes,
            gpus,
            physical_disks,
            windows_edition,
            windows_version,
            windows_build,
            windows_install_date,
            registered_owner,
        }
    }

    fn read_reg_u64(key: &RegKey, name: &str) -> Option<u64> {
        if let Ok(val) = key.get_value::<u64, _>(name) {
            return Some(val);
        }
        if let Ok(val) = key.get_value::<u32, _>(name) {
            return Some(val as u64);
        }
        if let Ok(raw) = key.get_raw_value(name) {
            if raw.bytes.len() >= 8 {
                let mut arr = [0u8; 8];
                arr.copy_from_slice(&raw.bytes[..8]);
                return Some(u64::from_le_bytes(arr));
            } else if raw.bytes.len() >= 4 {
                let mut arr = [0u8; 4];
                arr.copy_from_slice(&raw.bytes[..4]);
                return Some(u32::from_le_bytes(arr) as u64);
            }
        }
        None
    }

    fn format_unix_date(secs: u64) -> String {
        // Howard Hinnant civil-from-days algorithm for accurate YYYY-MM-DD
        let z = (secs / 86400) as i64 + 719468;
        let era = if z >= 0 { z } else { z - 146096 } / 146097;
        let doe = (z - era * 146097) as u32;
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
        let y = yoe as i64 + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let year = if m <= 2 { y + 1 } else { y };
        format!("{:04}-{:02}-{:02}", year, m, d)
    }

    fn check_is_elevated() -> bool {
        Command::new("net")
            .arg("session")
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    pub fn tick(&mut self, freeze_for_ctrl: bool) -> bool {
        if self.paused || freeze_for_ctrl {
            return false;
        }
        if self.last_refresh.elapsed() >= self.refresh_interval {
            self.force_refresh();
            true
        } else {
            false
        }
    }

    pub fn force_refresh(&mut self) {
        let elapsed_secs = self.last_refresh.elapsed().as_secs_f32().max(0.2);
        self.last_refresh = Instant::now();

        self.sys.refresh_cpu_all();
        self.sys.refresh_memory();
        self.sys.refresh_processes(ProcessesToUpdate::All, true);
        self.disks.refresh();
        self.networks.refresh();

        self.uptime_secs = System::uptime();

        // CPU metrics
        self.cpu_usage_total = self.sys.global_cpu_usage().clamp(0.0, 100.0);
        let cpus = self.sys.cpus();
        if !cpus.is_empty() {
            let avg_mhz: f32 =
                cpus.iter().map(|c| c.frequency() as f32).sum::<f32>() / (cpus.len() as f32);
            self.cpu_freq_ghz = avg_mhz / 1000.0;
        }

        self.per_core_usage.clear();
        for (idx, core) in cpus.iter().enumerate() {
            let usage = core.cpu_usage().clamp(0.0, 100.0);
            self.per_core_usage.push(usage);
            if let Some(hist) = self.per_core_history.get_mut(idx) {
                if hist.len() >= HISTORY_LEN {
                    hist.pop_front();
                }
                hist.push_back(usage);
            }
        }

        // Memory metrics
        self.mem_total_bytes = self.sys.total_memory().max(1);
        self.mem_used_bytes = self.sys.used_memory();
        self.mem_available_bytes = self.sys.available_memory();
        self.mem_usage_pct =
            ((self.mem_used_bytes as f64 / self.mem_total_bytes as f64) * 100.0) as f32;
        self.swap_total_bytes = self.sys.total_swap();
        self.swap_used_bytes = self.sys.used_swap();

        let mut pid_to_raw: HashMap<u32, (String, String, Option<u32>)> =
            HashMap::with_capacity(self.sys.processes().len());
        for (pid, proc_info) in self.sys.processes() {
            let pid_u32 = pid.as_u32();
            if pid_u32 == 0 {
                continue;
            }
            let raw_name = proc_info.name().to_string_lossy().to_string();
            let exe_path = proc_info
                .exe()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            let parent_pid = proc_info.parent().map(|p| p.as_u32());
            pid_to_raw.insert(pid_u32, (raw_name, exe_path, parent_pid));
        }

        let num_cpus_f32 = (self.logical_cores as f32).max(1.0);
        let mut total_read_delta: u64 = 0;
        let mut total_write_delta: u64 = 0;

        let mut procs = Vec::with_capacity(self.sys.processes().len());
        for (pid, proc_info) in self.sys.processes() {
            let pid_u32 = pid.as_u32();
            if pid_u32 == 0 {
                continue;
            }

            let disk = proc_info.disk_usage();
            let read_bps = (disk.read_bytes as f32 / elapsed_secs) as u64;
            let write_bps = (disk.written_bytes as f32 / elapsed_secs) as u64;
            total_read_delta = total_read_delta.saturating_add(read_bps);
            total_write_delta = total_write_delta.saturating_add(write_bps);

            let cpu_normalized = (proc_info.cpu_usage() / num_cpus_f32).clamp(0.0, 100.0);
            let mem_bytes = proc_info.memory();
            let mem_pct = ((mem_bytes as f64 / self.mem_total_bytes as f64) * 100.0) as f32;

            let raw_name = proc_info.name().to_string_lossy().to_string();
            let exe_path = proc_info
                .exe()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            let parent_pid = proc_info.parent().map(|p| p.as_u32());

            let (group_key, display_name, is_user_app) =
                Self::classify_process(pid_u32, &raw_name, &exe_path, parent_pid, &pid_to_raw);

            procs.push(ProcessEntry {
                pid: pid_u32,
                name: raw_name,
                display_name,
                group_key,
                is_user_app,
                status: format!("{:?}", proc_info.status()),
                cpu_pct: cpu_normalized,
                memory_bytes: mem_bytes,
                memory_pct: mem_pct,
                virtual_mem_bytes: proc_info.virtual_memory(),
                disk_total_bps: read_bps.saturating_add(write_bps),
                run_time_secs: proc_info.run_time(),
                exe_path,
            });
        }

        let mut group_map: HashMap<String, AppGroupEntry> = HashMap::new();
        for p in &procs {
            let entry = group_map
                .entry(p.group_key.clone())
                .or_insert_with(|| AppGroupEntry {
                    group_key: p.group_key.clone(),
                    display_name: p.display_name.clone(),
                    is_user_app: p.is_user_app,
                    primary_pid: p.pid,
                    cpu_pct: 0.0,
                    memory_bytes: 0,
                    memory_pct: 0.0,
                    disk_total_bps: 0,
                    max_run_time_secs: 0,
                    exe_path: p.exe_path.clone(),
                    children: Vec::new(),
                });

            entry.is_user_app |= p.is_user_app;
            entry.cpu_pct = (entry.cpu_pct + p.cpu_pct).clamp(0.0, 100.0);
            entry.memory_bytes = entry.memory_bytes.saturating_add(p.memory_bytes);
            entry.memory_pct =
                ((entry.memory_bytes as f64 / self.mem_total_bytes as f64) * 100.0) as f32;
            entry.disk_total_bps = entry.disk_total_bps.saturating_add(p.disk_total_bps);
            if p.run_time_secs > entry.max_run_time_secs {
                entry.max_run_time_secs = p.run_time_secs;
            }
            if entry.exe_path.is_empty() && !p.exe_path.is_empty() {
                entry.exe_path = p.exe_path.clone();
            }
            entry.children.push(p.clone());
        }

        for grp in group_map.values_mut() {
            grp.children
                .sort_by(|a, b| b.memory_bytes.cmp(&a.memory_bytes));
            if let Some(first) = grp.children.first() {
                grp.primary_pid = first.pid;
            }
        }

        self.app_groups = group_map.into_values().collect();
        self.processes = procs;
        self.disk_read_bps = total_read_delta;
        self.disk_write_bps = total_write_delta;

        // Disk volumes / partitions
        let mut disk_entries = Vec::new();
        for d in self.disks.list() {
            let total = d.total_space();
            let avail = d.available_space();
            let used = total.saturating_sub(avail);
            let used_pct = if total > 0 {
                ((used as f64 / total as f64) * 100.0) as f32
            } else {
                0.0
            };
            disk_entries.push(DiskEntry {
                name: d.name().to_string_lossy().to_string(),
                mount_point: d.mount_point().to_string_lossy().to_string(),
                file_system: d.file_system().to_string_lossy().to_string(),
                kind: format!("{:?}", d.kind()),
                total_bytes: total,
                available_bytes: avail,
                used_bytes: used,
                used_pct,
            });
        }
        self.disk_list = disk_entries;

        // Network interfaces
        let mut net_rx_sum: u64 = 0;
        let mut net_tx_sum: u64 = 0;
        let mut net_entries = Vec::new();
        for (name, data) in &self.networks {
            let rx_bps = (data.received() as f32 / elapsed_secs) as u64;
            let tx_bps = (data.transmitted() as f32 / elapsed_secs) as u64;
            net_rx_sum = net_rx_sum.saturating_add(rx_bps);
            net_tx_sum = net_tx_sum.saturating_add(tx_bps);

            net_entries.push(NetworkEntry {
                name: name.clone(),
                total_rx_bytes: data.total_received(),
                total_tx_bytes: data.total_transmitted(),
            });
        }
        net_entries.sort_by(|a, b| {
            (b.total_rx_bytes + b.total_tx_bytes).cmp(&(a.total_rx_bytes + a.total_tx_bytes))
        });
        self.network_list = net_entries;
        self.net_rx_bps = net_rx_sum;
        self.net_tx_bps = net_tx_sum;

        // Push to rolling histories
        Self::push_history(&mut self.cpu_history, self.cpu_usage_total);
        Self::push_history(&mut self.mem_history, self.mem_usage_pct);
        Self::push_history(
            &mut self.disk_read_history,
            self.disk_read_bps as f32 / (1024.0 * 1024.0),
        );
        Self::push_history(
            &mut self.disk_write_history,
            self.disk_write_bps as f32 / (1024.0 * 1024.0),
        );
        Self::push_history(
            &mut self.net_rx_history,
            (self.net_rx_bps as f32 * 8.0) / 1000.0,
        );
        Self::push_history(
            &mut self.net_tx_history,
            (self.net_tx_bps as f32 * 8.0) / 1000.0,
        );
    }

    fn classify_process(
        _pid: u32,
        raw_name: &str,
        exe_path: &str,
        parent_pid: Option<u32>,
        pid_map: &HashMap<u32, (String, String, Option<u32>)>,
    ) -> (String, String, bool) {
        let lower_name = raw_name.to_lowercase();
        let lower_path = exe_path.to_lowercase();

        if lower_name.starts_with("msedgewebview2")
            || lower_name.contains("crashpad")
            || lower_name.contains("runtimebroker")
        {
            let mut curr_parent = parent_pid;
            for _ in 0..4 {
                if let Some(ppid) = curr_parent {
                    if let Some((pname, ppath, next_parent)) = pid_map.get(&ppid) {
                        let plower = pname.to_lowercase();
                        let ppath_lower = ppath.to_lowercase();
                        if plower.contains("whatsapp") || ppath_lower.contains("whatsapp") {
                            return ("whatsapp".to_string(), "WhatsApp".to_string(), true);
                        }
                        if plower.contains("spotify") || ppath_lower.contains("spotify") {
                            return ("spotify".to_string(), "Spotify".to_string(), true);
                        }
                        if plower.contains("teams") || ppath_lower.contains("msteams") {
                            return (
                                "msteams".to_string(),
                                "Microsoft Teams".to_string(),
                                true,
                            );
                        }
                        if plower.contains("notion") || ppath_lower.contains("notion") {
                            return ("notion".to_string(), "Notion".to_string(), true);
                        }
                        curr_parent = *next_parent;
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
        }

        if lower_name == "chrome.exe" || lower_name == "chrome" {
            return ("chrome".to_string(), "Google Chrome".to_string(), true);
        }
        if lower_name.contains("whatsapp") || lower_path.contains("whatsapp") {
            return ("whatsapp".to_string(), "WhatsApp".to_string(), true);
        }
        if lower_name.contains("spotify") || lower_path.contains("spotify") {
            return ("spotify".to_string(), "Spotify".to_string(), true);
        }
        if lower_name == "msedge.exe" || lower_name == "msedge" {
            return ("msedge".to_string(), "Microsoft Edge".to_string(), true);
        }
        if lower_name == "firefox.exe" || lower_name == "firefox" {
            return ("firefox".to_string(), "Mozilla Firefox".to_string(), true);
        }
        if lower_name == "brave.exe" || lower_name == "brave" {
            return ("brave".to_string(), "Brave Browser".to_string(), true);
        }
        if lower_name == "discord.exe" || lower_name == "discord" {
            return ("discord".to_string(), "Discord".to_string(), true);
        }
        if lower_name == "code.exe" || lower_name == "code" {
            return ("vscode".to_string(), "Visual Studio Code".to_string(), true);
        }
        if lower_name.contains("antigravity") || lower_path.contains("antigravity") {
            return ("antigravity".to_string(), "Antigravity".to_string(), true);
        }
        if lower_name.contains("rigmap") || lower_name.contains("taskmanager-polo") {
            return ("rigmap".to_string(), "RigMap".to_string(), true);
        }
        if lower_name == "explorer.exe" || lower_name == "explorer" {
            return ("explorer".to_string(), "Windows Explorer".to_string(), true);
        }
        if lower_name.contains("windowsterminal") || lower_name == "wt.exe" {
            return (
                "windowsterminal".to_string(),
                "Windows Terminal".to_string(),
                true,
            );
        }
        if lower_name.contains("notion") || lower_path.contains("notion") {
            return ("notion".to_string(), "Notion".to_string(), true);
        }
        if lower_name == "steam.exe" || lower_name == "steamwebhelper.exe" {
            return ("steam".to_string(), "Steam".to_string(), true);
        }
        if lower_name == "notepad.exe" || lower_name == "notepad" {
            return ("notepad".to_string(), "Notepad".to_string(), true);
        }
        if lower_name.contains("powershell") || lower_name == "pwsh.exe" {
            return (
                "powershell".to_string(),
                "Windows PowerShell".to_string(),
                true,
            );
        }

        let is_system_binary = lower_path.starts_with(r"c:\windows\system32")
            || lower_path.starts_with(r"c:\windows\syswow64")
            || lower_path.is_empty()
            || lower_name == "svchost.exe"
            || lower_name == "conhost.exe"
            || lower_name == "dllhost.exe"
            || lower_name == "wmiprvse.exe"
            || lower_name == "runtimebroker.exe"
            || lower_name == "backgroundtaskhost.exe"
            || lower_name == "sihost.exe"
            || lower_name == "taskhostw.exe"
            || lower_name == "csrss.exe"
            || lower_name == "lsass.exe"
            || lower_name == "services.exe"
            || lower_name == "smss.exe"
            || lower_name == "wininit.exe"
            || lower_name == "winlogon.exe"
            || lower_name == "dwm.exe"
            || lower_name == "fontdrvhost.exe"
            || lower_name == "audiodg.exe"
            || lower_name == "spoolsv.exe"
            || lower_name.starts_with("msedgewebview2");

        let is_user_app = !is_system_binary
            && (lower_path.contains(r"\windowsapps\")
                || lower_path.contains(r"\program files")
                || lower_path.contains(r"\appdata\local\programs\"));

        let clean_title = raw_name
            .strip_suffix(".exe")
            .or_else(|| raw_name.strip_suffix(".EXE"))
            .unwrap_or(raw_name)
            .to_string();

        (lower_name, clean_title, is_user_app)
    }

    fn push_history(queue: &mut VecDeque<f32>, val: f32) {
        if queue.len() >= HISTORY_LEN {
            queue.pop_front();
        }
        queue.push_back(val);
    }

    pub fn kill_process(
        &mut self,
        pid: u32,
        kill_tree: bool,
        as_admin: bool,
    ) -> Result<String, String> {
        if as_admin {
            let args = if kill_tree {
                format!("/F /T /PID {}", pid)
            } else {
                format!("/F /PID {}", pid)
            };
            let ps_cmd = format!(
                "Start-Process -FilePath 'taskkill.exe' -ArgumentList '{}' -Verb RunAs -WindowStyle Hidden -Wait",
                args
            );
            return match Command::new("powershell")
                .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &ps_cmd])
                .creation_flags(CREATE_NO_WINDOW)
                .status()
            {
                Ok(st) if st.success() => {
                    self.force_refresh();
                    Ok(format!("Terminated PID {} (Admin)", pid))
                }
                Ok(_) => Err(format!("Admin elevation canceled or failed for PID {}", pid)),
                Err(e) => Err(format!("Failed to invoke Admin taskkill: {}", e)),
            };
        }

        if !kill_tree {
            let sys_pid = Pid::from_u32(pid);
            if let Some(proc_info) = self.sys.process(sys_pid) {
                let name = proc_info.name().to_string_lossy().to_string();
                if proc_info.kill() {
                    self.force_refresh();
                    return Ok(format!("Terminated {} (PID {})", name, pid));
                }
            }
        }

        let mut cmd = Command::new("taskkill");
        if kill_tree {
            cmd.args(["/F", "/T", "/PID", &pid.to_string()]);
        } else {
            cmd.args(["/F", "/PID", &pid.to_string()]);
        }
        cmd.creation_flags(CREATE_NO_WINDOW);

        match cmd.output() {
            Ok(out) if out.status.success() => {
                self.force_refresh();
                Ok(format!("Terminated PID {}", pid))
            }
            Ok(out) => {
                let err_msg = String::from_utf8_lossy(&out.stderr).trim().to_string();
                Err(if err_msg.is_empty() {
                    format!(
                        "Access denied terminating PID {} (try Admin End Task)",
                        pid
                    )
                } else {
                    err_msg
                })
            }
            Err(e) => Err(format!("Failed to terminate PID {}: {}", pid, e)),
        }
    }

    pub fn kill_group(&mut self, group_key: &str, as_admin: bool) -> Result<String, String> {
        let pids: Vec<u32> = self
            .processes
            .iter()
            .filter(|p| p.group_key == group_key)
            .map(|p| p.pid)
            .collect();

        if pids.is_empty() {
            return Err("No active processes found in group".to_string());
        }

        if as_admin {
            let pid_args: String = pids
                .iter()
                .map(|id| format!("/PID {}", id))
                .collect::<Vec<_>>()
                .join(" ");
            let ps_cmd = format!(
                "Start-Process -FilePath 'taskkill.exe' -ArgumentList '/F /T {}' -Verb RunAs -WindowStyle Hidden -Wait",
                pid_args
            );
            return match Command::new("powershell")
                .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &ps_cmd])
                .creation_flags(CREATE_NO_WINDOW)
                .status()
            {
                Ok(st) if st.success() => {
                    self.force_refresh();
                    Ok(format!(
                        "Terminated {} process(es) as Administrator",
                        pids.len()
                    ))
                }
                Ok(_) => Err("Administrator elevation was canceled or failed".to_string()),
                Err(e) => Err(format!("Failed to run Admin taskkill: {}", e)),
            };
        }

        let mut killed = 0usize;
        for pid in &pids {
            if self.kill_process(*pid, true, false).is_ok() {
                killed += 1;
            }
        }
        self.force_refresh();
        if killed > 0 {
            Ok(format!("Terminated {} process(es)", killed))
        } else {
            Err("Could not terminate group (try End Task as Admin)".to_string())
        }
    }

    pub fn restart_self_as_admin() -> Result<(), String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let exe_str = exe.to_string_lossy();
        let ps_cmd = format!(
            "Start-Process -FilePath '{}' -Verb RunAs",
            exe_str.replace('\'', "''")
        );
        Command::new("powershell")
            .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &ps_cmd])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map(|_| std::process::exit(0))
            .map_err(|e| format!("Failed to elevate: {}", e))
    }

    pub fn open_file_location(exe_path: &str) {
        if exe_path.is_empty() {
            return;
        }
        let _ = Command::new("explorer.exe")
            .arg(format!("/select,{}", exe_path))
            .spawn();
    }

    pub fn open_drive_in_explorer(mount_point: &str) {
        let _ = Command::new("explorer.exe").arg(mount_point).spawn();
    }

    pub fn run_new_task(cmd: &str, as_admin: bool) -> Result<(), String> {
        let trimmed = cmd.trim();
        if trimmed.is_empty() {
            return Err("Command cannot be empty".to_string());
        }
        if as_admin {
            let ps_cmd = format!(
                "Start-Process -FilePath '{}' -Verb RunAs",
                trimmed.replace('\'', "''")
            );
            Command::new("powershell")
                .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &ps_cmd])
                .creation_flags(CREATE_NO_WINDOW)
                .spawn()
                .map(|_| ())
                .map_err(|e| format!("Failed to start '{}' as Admin: {}", trimmed, e))
        } else {
            Command::new("cmd")
                .args(["/C", "start", "", trimmed])
                .creation_flags(CREATE_NO_WINDOW)
                .spawn()
                .map(|_| ())
                .map_err(|e| format!("Failed to start '{}': {}", trimmed, e))
        }
    }

    pub fn refresh_startup_apps(&mut self) {
        let mut entries = Vec::new();

        self.collect_reg_run(
            HKEY_CURRENT_USER,
            r"Software\Microsoft\Windows\CurrentVersion\Run",
            r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run",
            "Registry (HKCU\\...\\Run)",
            &mut entries,
        );

        self.collect_reg_run(
            HKEY_LOCAL_MACHINE,
            r"Software\Microsoft\Windows\CurrentVersion\Run",
            r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run",
            "Registry (HKLM\\...\\Run)",
            &mut entries,
        );

        self.collect_reg_run(
            HKEY_LOCAL_MACHINE,
            r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Run",
            r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run32",
            "Registry (HKLM\\...\\Run32)",
            &mut entries,
        );

        if let Ok(sys_app_data) = RegKey::predef(HKEY_CURRENT_USER).open_subkey(
            r"Software\Classes\Local Settings\Software\Microsoft\Windows\CurrentVersion\AppModel\SystemAppData",
        ) {
            for pkg_name in sys_app_data.enum_keys().flatten() {
                if let Ok(pkg_key) = sys_app_data.open_subkey(&pkg_name) {
                    for task_id in pkg_key.enum_keys().flatten() {
                        if let Ok(task_key) = pkg_key.open_subkey(&task_id) {
                            if let Ok(state) = task_key.get_value::<u32, _>("State") {
                                let enabled = state == 2;
                                let friendly = Self::friendly_uwp_package_name(&pkg_name, &task_id);
                                entries.push(StartupEntry {
                                    name: friendly,
                                    impact_estimate: Self::estimate_impact(&pkg_name),
                                    command: format!("UWP Package: {} ({})", pkg_name, task_id),
                                    location: "Microsoft Store StartupTask".to_string(),
                                    enabled,
                                });
                            }
                        }
                    }
                }
            }
        }

        let mut startup_dirs = Vec::new();
        if let Ok(appdata) = std::env::var("APPDATA") {
            startup_dirs.push((
                PathBuf::from(appdata).join(r"Microsoft\Windows\Start Menu\Programs\Startup"),
                "Startup Folder (User)",
            ));
        }
        if let Ok(progdata) = std::env::var("PROGRAMDATA") {
            startup_dirs.push((
                PathBuf::from(progdata).join(r"Microsoft\Windows\Start Menu\Programs\StartUp"),
                "Startup Folder (All Users)",
            ));
        }

        for (dir, label) in startup_dirs {
            if let Ok(read_dir) = std::fs::read_dir(&dir) {
                for entry in read_dir.flatten() {
                    let file_name = entry.file_name().to_string_lossy().to_string();
                    if file_name.eq_ignore_ascii_case("desktop.ini") {
                        continue;
                    }
                    let full_path = entry.path().to_string_lossy().to_string();
                    entries.push(StartupEntry {
                        name: file_name
                            .strip_suffix(".lnk")
                            .unwrap_or(&file_name)
                            .to_string(),
                        impact_estimate: "Medium",
                        command: full_path,
                        location: label.to_string(),
                        enabled: true,
                    });
                }
            }
        }

        entries.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        self.startup_apps = entries;
    }

    fn collect_reg_run(
        &self,
        hive: winreg::HKEY,
        run_path: &str,
        approved_path: &str,
        location_label: &str,
        entries: &mut Vec<StartupEntry>,
    ) {
        if let Ok(run_key) = RegKey::predef(hive).open_subkey(run_path) {
            let approved_key = RegKey::predef(hive).open_subkey(approved_path).ok();
            for item in run_key.enum_values().flatten() {
                let (name, val) = item;
                let command = val.to_string();
                let enabled = Self::is_startup_approved(&approved_key, &name);
                entries.push(StartupEntry {
                    name,
                    impact_estimate: Self::estimate_impact(&command),
                    command,
                    location: location_label.to_string(),
                    enabled,
                });
            }
        }
    }

    fn friendly_uwp_package_name(pkg_name: &str, task_id: &str) -> String {
        let lower = pkg_name.to_lowercase();
        if lower.contains("whatsapp") {
            return "WhatsApp Desktop".to_string();
        }
        if lower.contains("spotify") {
            return "Spotify Music".to_string();
        }
        if lower.contains("notion") {
            return "Notion".to_string();
        }
        if lower.contains("msteams") {
            return "Microsoft Teams".to_string();
        }
        if lower.contains("windowsterminal") {
            return "Windows Terminal".to_string();
        }
        if lower.contains("gamingapp") {
            return "Xbox App".to_string();
        }
        if lower.contains("yourphone") {
            return "Phone Link".to_string();
        }
        if lower.contains("commandpalette") {
            return "PowerToys Command Palette".to_string();
        }
        if lower.contains("amdradeon") {
            return "AMD Radeon Software".to_string();
        }
        let base = pkg_name.split('_').next().unwrap_or(pkg_name);
        format!("{} ({})", base, task_id)
    }

    fn is_startup_approved(approved_key: &Option<RegKey>, name: &str) -> bool {
        if let Some(key) = approved_key {
            if let Ok(raw) = key.get_raw_value(name) {
                if let Some(&first_byte) = raw.bytes.first() {
                    return first_byte == 2 || first_byte == 6;
                }
            }
        }
        true
    }

    fn estimate_impact(command: &str) -> &'static str {
        let lower = command.to_lowercase();
        if lower.contains("electron")
            || lower.contains("discord")
            || lower.contains("steam")
            || lower.contains("teams")
            || lower.contains("spotify")
            || lower.contains("whatsapp")
            || lower.contains("onedrive")
            || lower.contains("chrome")
        {
            "High"
        } else if lower.contains("update") || lower.contains("helper") || lower.contains("tray") {
            "Low"
        } else {
            "Medium"
        }
    }
}

pub fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;

    let b = bytes as f64;
    if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.0} KB", b / KB)
    } else {
        format!("{} B", bytes)
    }
}

pub fn format_rate_bps(bps: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;

    let b = bps as f64;
    if b >= MB {
        format!("{:.1} MB/s", b / MB)
    } else if b >= KB {
        format!("{:.1} KB/s", b / KB)
    } else if bps > 0 {
        format!("{} B/s", bps)
    } else {
        "0 MB/s".to_string()
    }
}

pub fn format_duration(secs: u64) -> String {
    let days = secs / 86400;
    let hours = (secs % 86400) / 3600;
    let mins = (secs % 3600) / 60;
    let s = secs % 60;
    if days > 0 {
        format!("{}:{:02}:{:02}:{:02}", days, hours, mins, s)
    } else {
        format!("{:02}:{:02}:{:02}", hours, mins, s)
    }
}
