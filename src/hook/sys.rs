use std::thread;
use std::time::Duration;
use sysinfo::{Networks, System};

pub struct SystemUsage {
    pub cpu_percent: f64,
    pub ram_used_gb: f64,
    pub ram_total_gb: f64,
    pub ram_percent: f64,
    pub net_download: f64,
    pub net_upload: f64,
}

pub fn format_speed(kbps: f64) -> String {
    if kbps >= 1024.0 {
        format!("{:.2} MB/s", kbps / 1024.0)
    } else {
        format!("{:.1} KB/s", kbps)
    }
}
pub fn get_system_usage(sys: &mut System, networks: &mut Networks) -> SystemUsage {
    sys.refresh_cpu_usage();
    sys.refresh_memory();
    networks.refresh(true);

    let ram_total = sys.total_memory() as f64;
    let ram_used = sys.used_memory() as f64;

    // Sum across all network interfaces
    let (total_rx, total_tx) = networks.iter().fold((0u64, 0u64), |(rx, tx), (_, data)| {
        (rx + data.received(), tx + data.transmitted())
    });

    // received()/transmitted() returns bytes since last refresh (1 second interval)
    let download_kbps = total_rx as f64 / 1024.0;
    let upload_kbps = total_tx as f64 / 1024.0;

    SystemUsage {
        cpu_percent: sys.global_cpu_usage() as f64,
        ram_used_gb: ram_used / 1_073_741_824.0,
        ram_total_gb: ram_total / 1_073_741_824.0,
        ram_percent: (ram_used / ram_total) * 100.0,
        net_download: download_kbps,
        net_upload: upload_kbps,
    }
}
pub struct SystemInfo {
    pub sys: sysinfo::System,
    pub networks: sysinfo::Networks,
}

impl SystemInfo {
    pub fn new() -> Self {
        let mut sys = sysinfo::System::new_all();
        let networks = sysinfo::Networks::new_with_refreshed_list();
        // Init CPU baseline
        sys.refresh_cpu_usage();
        std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
        Self { sys, networks }
    }

    pub fn update(&mut self) -> SystemUsage {
        get_system_usage(&mut self.sys, &mut self.networks)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_sysinfo() {
        let mut info = SystemInfo::new(); // call once

        loop {
            let usage = info.update();
            println!(
                "CPU: {:.1}% | RAM: {:.2}/{:.2} GB ({:.1}%) | ↓ {} ↑ {}",
                usage.cpu_percent,
                usage.ram_used_gb,
                usage.ram_total_gb,
                usage.ram_percent,
                format_speed(usage.net_download),
                format_speed(usage.net_upload),
            );
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    }
}
