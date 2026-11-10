//! CPU, memory, GPU runtime-PM and backlight readings from procfs/sysfs.

use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Default)]
pub struct CpuTimes {
    pub idle: u64,
    pub total: u64,
}

/// Parse the aggregate "cpu" line of /proc/stat.
pub fn parse_cpu_times(stat: &str) -> Option<CpuTimes> {
    let line = stat.lines().find(|l| l.starts_with("cpu "))?;
    let nums: Vec<u64> = line.split_whitespace().skip(1).filter_map(|n| n.parse().ok()).collect();
    if nums.len() < 4 {
        return None;
    }
    // user nice system idle iowait irq softirq steal (guest is already in user)
    let idle = nums[3] + nums.get(4).copied().unwrap_or(0);
    let total: u64 = nums.iter().take(8).sum();
    Some(CpuTimes { idle, total })
}

pub fn read_cpu_times() -> Option<CpuTimes> {
    parse_cpu_times(&fs::read_to_string("/proc/stat").ok()?)
}

pub fn cpu_percent(prev: CpuTimes, now: CpuTimes) -> Option<f64> {
    let total = now.total.checked_sub(prev.total)?;
    let idle = now.idle.checked_sub(prev.idle)?;
    if total == 0 {
        return None;
    }
    Some(((total - idle.min(total)) as f64 / total as f64 * 100.0).clamp(0.0, 100.0))
}

#[derive(Debug, Clone, Copy, Serialize, Default)]
pub struct Memory {
    pub total_bytes: u64,
    pub used_bytes: u64,
}

pub fn parse_meminfo(s: &str) -> Option<Memory> {
    let field = |name: &str| -> Option<u64> {
        s.lines()
            .find(|l| l.starts_with(name) && l[name.len()..].starts_with(':'))?
            .split_whitespace()
            .nth(1)?
            .parse::<u64>()
            .ok()
            .map(|kb| kb * 1024)
    };
    let total = field("MemTotal")?;
    let avail = field("MemAvailable")?;
    Some(Memory { total_bytes: total, used_bytes: total.saturating_sub(avail) })
}

pub fn read_memory() -> Option<Memory> {
    parse_meminfo(&fs::read_to_string("/proc/meminfo").ok()?)
}

#[derive(Debug, Clone, Serialize)]
pub struct GpuState {
    pub pci_address: String,
    pub name: String,
    pub vendor: String,
    pub driver: Option<String>,
    /// Kernel runtime PM state: active, suspended, suspending, resuming, unsupported.
    pub runtime_status: String,
    /// "auto" lets the kernel suspend the device; "on" keeps it awake.
    pub runtime_control: Option<String>,
    /// True only when the kernel itself reports the device suspended.
    pub suspended: bool,
    /// Milliseconds the device has spent suspended / active since boot.
    pub suspended_ms: Option<u64>,
    pub active_ms: Option<u64>,
    /// NVIDIA driver's own view of video memory, when exposed.
    pub nvidia_video_memory: Option<String>,
    pub nvidia_runtime_d3: Option<String>,
}

fn read_trim(p: &Path) -> Option<String> {
    fs::read_to_string(p).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// Discrete GPUs: display-class PCI devices (class 0x03xxxx) that are not the boot VGA.
pub fn discover_dgpus(pci_root: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = fs::read_dir(pci_root)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| read_trim(&p.join("class")).is_some_and(|c| c.starts_with("0x03")))
        .filter(|p| read_trim(&p.join("boot_vga")).as_deref() != Some("1"))
        .collect();
    out.sort();
    out
}

fn vendor_name(id: &str) -> &'static str {
    match id {
        "0x10de" => "NVIDIA",
        "0x1002" => "AMD",
        "0x8086" => "Intel",
        _ => "Unknown",
    }
}

/// Parse /proc/driver/nvidia/gpus/<addr>/power.
pub fn parse_nvidia_power(s: &str) -> (Option<String>, Option<String>) {
    let get = |key: &str| {
        s.lines()
            .find(|l| l.trim_start().starts_with(key))
            .and_then(|l| l.split_once(':'))
            .map(|(_, v)| v.trim().to_string())
    };
    (get("Video Memory"), get("Runtime D3 status"))
}

pub fn read_gpu(dev: &Path, nvidia_proc: &Path) -> GpuState {
    let addr = dev.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let vendor = read_trim(&dev.join("vendor")).unwrap_or_default();
    let status = read_trim(&dev.join("power/runtime_status")).unwrap_or_else(|| "unsupported".into());
    let driver = fs::read_link(dev.join("driver"))
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()));
    let (vm, d3) = fs::read_to_string(nvidia_proc.join(&addr).join("power"))
        .map(|s| parse_nvidia_power(&s))
        .unwrap_or((None, None));
    let label = read_trim(&dev.join("label"));
    GpuState {
        name: label.unwrap_or_else(|| format!("{} GPU", vendor_name(&vendor))),
        vendor: vendor_name(&vendor).into(),
        suspended: status == "suspended",
        runtime_status: status,
        runtime_control: read_trim(&dev.join("power/control")),
        suspended_ms: read_trim(&dev.join("power/runtime_suspended_time")).and_then(|v| v.parse().ok()),
        active_ms: read_trim(&dev.join("power/runtime_active_time")).and_then(|v| v.parse().ok()),
        nvidia_video_memory: vm,
        nvidia_runtime_d3: d3,
        driver,
        pci_address: addr,
    }
}

pub fn read_gpus() -> Vec<GpuState> {
    let proc = Path::new("/proc/driver/nvidia/gpus");
    discover_dgpus(Path::new("/sys/bus/pci/devices"))
        .iter()
        .map(|d| read_gpu(d, proc))
        .collect()
}

#[derive(Debug, Clone, Serialize)]
pub struct Backlight {
    pub device: String,
    pub brightness: u64,
    pub max_brightness: u64,
    pub percent: f64,
}

/// Pick the first backlight, preferring firmware/platform over raw.
pub fn read_backlight(root: &Path) -> Option<Backlight> {
    let mut devs: Vec<PathBuf> = fs::read_dir(root).ok()?.flatten().map(|e| e.path()).collect();
    let rank = |p: &PathBuf| match read_trim(&p.join("type")).as_deref() {
        Some("firmware") => 0,
        Some("platform") => 1,
        _ => 2,
    };
    devs.sort_by_key(|p| (rank(p), p.clone()));
    let d = devs.first()?;
    let b: u64 = read_trim(&d.join("brightness"))?.parse().ok()?;
    let m: u64 = read_trim(&d.join("max_brightness"))?.parse().ok()?;
    Some(Backlight {
        device: d.file_name()?.to_string_lossy().into_owned(),
        brightness: b,
        max_brightness: m,
        percent: if m > 0 { b as f64 / m as f64 * 100.0 } else { 0.0 },
    })
}

/// Keyboard backlight LEDs exposed through the leds class.
pub fn keyboard_leds(root: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains("kbd_backlight"))
        .collect();
    v.sort();
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_and_memory_parsing() {
        let a = parse_cpu_times("cpu  100 0 100 800 0 0 0 0 0 0\ncpu0 1 2 3 4\n").unwrap();
        let b = parse_cpu_times("cpu  150 0 150 900 0 0 0 0 0 0\n").unwrap();
        assert!((cpu_percent(a, b).unwrap() - 50.0).abs() < 1e-9);
        let m = parse_meminfo("MemTotal:       16000000 kB\nMemFree: 1 kB\nMemAvailable:   12000000 kB\n").unwrap();
        assert_eq!(m.used_bytes, 4_000_000 * 1024);
    }

    #[test]
    fn nvidia_proc_power() {
        let s = " Runtime D3 status:          Enabled (fine-grained)\n Video Memory:               Off\n";
        let (vm, d3) = parse_nvidia_power(s);
        assert_eq!(vm.as_deref(), Some("Off"));
        assert_eq!(d3.as_deref(), Some("Enabled (fine-grained)"));
    }

    #[test]
    fn gpu_suspended_only_when_kernel_says_so() {
        let d = tempfile::tempdir().unwrap();
        let dev = d.path().join("0000:01:00.0");
        std::fs::create_dir_all(dev.join("power")).unwrap();
        std::fs::write(dev.join("class"), "0x030000\n").unwrap();
        std::fs::write(dev.join("vendor"), "0x10de\n").unwrap();
        std::fs::write(dev.join("boot_vga"), "0\n").unwrap();
        std::fs::write(dev.join("power/control"), "auto\n").unwrap();
        std::fs::write(dev.join("power/runtime_status"), "active\n").unwrap();
        let igpu = d.path().join("0000:00:02.0");
        std::fs::create_dir_all(&igpu).unwrap();
        std::fs::write(igpu.join("class"), "0x030000\n").unwrap();
        std::fs::write(igpu.join("boot_vga"), "1\n").unwrap();

        let found = discover_dgpus(d.path());
        assert_eq!(found.len(), 1, "boot VGA (iGPU) is excluded");
        let g = read_gpu(&found[0], d.path());
        assert!(!g.suspended, "control=auto alone is not suspension");
        std::fs::write(dev.join("power/runtime_status"), "suspended\n").unwrap();
        assert!(read_gpu(&found[0], d.path()).suspended);
    }
}
