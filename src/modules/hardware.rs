use crate::state::{DiskInfo, SharedState};
use sysinfo::{Components, Disks, System};

pub struct HardwareDaemon {
    sys: System,
    components: Components,
    gpu_vendor: Option<String>,
    gpu_poll_counter: u8,
    disk_poll_counter: u8,
}

impl HardwareDaemon {
    pub fn new() -> Self {
        let mut sys = System::new();
        sys.refresh_cpu_usage();
        sys.refresh_memory();
        let components = Components::new_with_refreshed_list();
        Self {
            sys,
            components,
            gpu_vendor: None,
            gpu_poll_counter: 0,
            disk_poll_counter: 9, // Start at 9 to poll on the first tick
        }
    }

    pub fn poll_fast(&mut self, state: SharedState) {
        self.sys.refresh_cpu_usage();
        self.sys.refresh_memory();
        self.components.refresh(true);

        let cpu_usage = self.sys.global_cpu_usage();
        let cpu_model = self
            .sys
            .cpus()
            .first()
            .map(|c| c.brand().to_string())
            .unwrap_or_else(|| "Unknown".to_string());

        let mut cpu_temp = 0.0;
        for component in &self.components {
            let label = component.label().to_lowercase();
            if (label.contains("tctl")
                || label.contains("cpu")
                || label.contains("package")
                || label.contains("temp1"))
                && let Some(temp) = component.temperature()
            {
                cpu_temp = temp as f64;
                if cpu_temp > 0.0 {
                    break;
                }
            }
        }

        let total_mem = self.sys.total_memory() as f64 / 1024.0 / 1024.0 / 1024.0;
        let available_mem = self.sys.available_memory() as f64 / 1024.0 / 1024.0 / 1024.0;
        let used_mem = total_mem - available_mem;

        let load_avg = System::load_average();
        let uptime = System::uptime();

        let mut process_count = 0;
        if let Ok(loadavg_str) = std::fs::read_to_string("/proc/loadavg") {
            let parts: Vec<&str> = loadavg_str.split_whitespace().collect();
            if parts.len() >= 4
                && let Some(total_procs) = parts[3].split('/').nth(1)
            {
                process_count = total_procs.parse().unwrap_or(0);
            }
        }

        if let Ok(mut state_lock) = state.write() {
            state_lock.cpu.usage = cpu_usage as f64;
            state_lock.cpu.temp = cpu_temp;
            state_lock.cpu.model = cpu_model;

            state_lock.memory.total_gb = total_mem;
            state_lock.memory.used_gb = used_mem;

            state_lock.sys.load_1 = load_avg.one;
            state_lock.sys.load_5 = load_avg.five;
            state_lock.sys.load_15 = load_avg.fifteen;
            state_lock.sys.uptime = uptime;
            state_lock.sys.process_count = process_count;
        }
    }

    pub fn poll_slow(&mut self, state: SharedState) {
        // 1. Gather GPU data outside of lock
        let mut gpu_state = crate::state::GpuState::default();
        self.gpu_poll_counter = (self.gpu_poll_counter + 1) % 5;
        let should_poll_gpu = self.gpu_poll_counter == 0;
        if should_poll_gpu {
            self.poll_gpu(&mut gpu_state);
        }

        // 2. Gather Disk data outside of lock
        let mut disks_data = None;
        self.disk_poll_counter = (self.disk_poll_counter + 1) % 10;
        if self.disk_poll_counter == 0 {
            disks_data = Some(
                Disks::new_with_refreshed_list()
                    .iter()
                    .map(|d| DiskInfo {
                        mount_point: d.mount_point().to_string_lossy().into_owned(),
                        filesystem: d.file_system().to_string_lossy().to_lowercase(),
                        total_bytes: d.total_space(),
                        available_bytes: d.available_space(),
                    })
                    .collect::<Vec<DiskInfo>>(),
            );
        }

        // 3. Apply to state
        if let Ok(mut state_lock) = state.write() {
            if should_poll_gpu {
                state_lock.gpu = gpu_state;
            }

            if let Some(d) = disks_data {
                state_lock.disks = d;
            }
        }
    }

    fn poll_gpu(&mut self, gpu: &mut crate::state::GpuState) {
        gpu.active = false;

        if (self.gpu_vendor.as_deref() == Some("NVIDIA") || self.gpu_vendor.is_none())
            && let Ok(output) = std::process::Command::new("nvidia-smi")
                .args([
                    "--query-gpu=utilization.gpu,memory.used,memory.total,temperature.gpu,name",
                    "--format=csv,noheader,nounits",
                ])
                .output()
            && output.status.success()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Some(line) = stdout.lines().next() {
                let parts: Vec<&str> = line.split(',').collect();
                if parts.len() >= 5 {
                    gpu.active = true;
                    gpu.vendor = "NVIDIA".to_string();
                    gpu.usage = parts[0].trim().parse().unwrap_or(0.0);
                    gpu.vram_used = parts[1].trim().parse::<f64>().unwrap_or(0.0) / 1024.0;
                    gpu.vram_total = parts[2].trim().parse::<f64>().unwrap_or(0.0) / 1024.0;
                    gpu.temp = parts[3].trim().parse().unwrap_or(0.0);
                    gpu.model = parts[4].trim().to_string();
                    self.gpu_vendor = Some("NVIDIA".to_string());
                    return;
                }
            }
        }

        if self.gpu_vendor.as_deref() == Some("AMD")
            || self.gpu_vendor.as_deref() == Some("Intel")
            || self.gpu_vendor.is_none()
        {
            for i in 0..=3 {
                let base = format!("/sys/class/drm/card{}/device", i);

                if (self.gpu_vendor.as_deref() == Some("AMD") || self.gpu_vendor.is_none())
                    && let Ok(usage_str) =
                        std::fs::read_to_string(format!("{}/gpu_busy_percent", base))
                {
                    gpu.active = true;
                    gpu.vendor = "AMD".to_string();
                    gpu.usage = usage_str.trim().parse().unwrap_or(0.0);

                    if let Ok(mem_used) =
                        std::fs::read_to_string(format!("{}/mem_info_vram_used", base))
                    {
                        gpu.vram_used = mem_used.trim().parse::<f64>().unwrap_or(0.0)
                            / 1024.0
                            / 1024.0
                            / 1024.0;
                    }
                    if let Ok(mem_total) =
                        std::fs::read_to_string(format!("{}/mem_info_vram_total", base))
                    {
                        gpu.vram_total = mem_total.trim().parse::<f64>().unwrap_or(0.0)
                            / 1024.0
                            / 1024.0
                            / 1024.0;
                    }

                    if let Ok(entries) = std::fs::read_dir(format!("{}/hwmon", base)) {
                        for entry in entries.flatten() {
                            let temp_path = entry.path().join("temp1_input");
                            if let Ok(temp_str) = std::fs::read_to_string(temp_path) {
                                gpu.temp = temp_str.trim().parse::<f64>().unwrap_or(0.0) / 1000.0;
                                break;
                            }
                        }
                    }
                    gpu.model = "AMD GPU".to_string();
                    self.gpu_vendor = Some("AMD".to_string());
                    return;
                }

                if self.gpu_vendor.as_deref() == Some("Intel") || self.gpu_vendor.is_none() {
                    let freq_path =
                        if std::path::Path::new(&format!("{}/gt_cur_freq_mhz", base)).exists() {
                            Some(format!("{}/gt_cur_freq_mhz", base))
                        } else if std::path::Path::new(&format!(
                            "/sys/class/drm/card{}/gt_cur_freq_mhz",
                            i
                        ))
                        .exists()
                        {
                            Some(format!("/sys/class/drm/card{}/gt_cur_freq_mhz", i))
                        } else {
                            None
                        };

                    if let Some(path) = freq_path
                        && let Ok(freq_str) = std::fs::read_to_string(&path)
                    {
                        gpu.active = true;
                        gpu.vendor = "Intel".to_string();

                        let cur_freq = freq_str.trim().parse::<f64>().unwrap_or(0.0);
                        let mut max_freq = 0.0;

                        let max_path = path.replace("gt_cur_freq_mhz", "gt_max_freq_mhz");
                        if let Ok(max_str) = std::fs::read_to_string(max_path) {
                            max_freq = max_str.trim().parse::<f64>().unwrap_or(0.0);
                        }

                        gpu.usage = if max_freq > 0.0 {
                            (cur_freq / max_freq) * 100.0
                        } else {
                            0.0
                        };
                        gpu.temp = 0.0;
                        gpu.vram_used = 0.0;
                        gpu.vram_total = 0.0;
                        gpu.model = format!("Intel iGPU ({}MHz)", cur_freq);
                        self.gpu_vendor = Some("Intel".to_string());
                        return;
                    }
                }
            }
        }
    }
}
