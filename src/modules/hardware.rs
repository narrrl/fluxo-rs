use crate::state::SharedState;
use sysinfo::{Components, System};

pub struct HardwareDaemon {
    sys: System,
    components: Components,
}

impl HardwareDaemon {
    pub fn new() -> Self {
        let mut sys = System::new_all();
        sys.refresh_all();
        let components = Components::new_with_refreshed_list();
        Self { sys, components }
    }

    pub fn poll(&mut self, state: SharedState) {
        self.sys.refresh_cpu_usage();
        self.sys.refresh_memory();
        self.components.refresh(true);

        let cpu_usage = self.sys.global_cpu_usage();
        let cpu_model = self.sys.cpus().first().map(|c| c.brand().to_string()).unwrap_or_else(|| "Unknown".to_string());

        // Try to find a reasonable CPU temperature
        // Often 'coretemp' or 'k10temp' depending on AMD/Intel
        let mut cpu_temp = 0.0;
        for component in &self.components {
            let label = component.label().to_lowercase();
            if label.contains("tctl") || label.contains("cpu") || label.contains("package") || label.contains("temp1") {
                if let Some(temp) = component.temperature() {
                    cpu_temp = temp as f64;
                    if cpu_temp > 0.0 { break; }
                }
            }
        }

        let total_mem = self.sys.total_memory() as f64 / 1024.0 / 1024.0 / 1024.0;
        // Accurate used memory matching htop/free (Total - Available)
        let available_mem = self.sys.available_memory() as f64 / 1024.0 / 1024.0 / 1024.0;
        let used_mem = total_mem - available_mem;

        if let Ok(mut state_lock) = state.write() {
            state_lock.cpu.usage = cpu_usage as f64;
            state_lock.cpu.temp = cpu_temp as f64;
            state_lock.cpu.model = cpu_model;

            state_lock.memory.total_gb = total_mem;
            state_lock.memory.used_gb = used_mem;
        }
    }
}
