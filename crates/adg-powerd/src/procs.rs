//! "What is using power?" — top processes by CPU and what keeps the dGPU awake.

use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::Duration;

#[derive(Debug, Clone, Serialize)]
pub struct Proc {
    pub pid: u32,
    pub name: String,
    pub cpu_percent: f64,
    pub rss_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct GpuUser {
    pub pid: u32,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PowerHogs {
    pub top: Vec<Proc>,
    pub gpu_users: Vec<GpuUser>,
    /// Active system services known to hold the NVIDIA GPU open.
    pub gpu_services: Vec<String>,
}

/// (comm, utime+stime ticks, rss pages) from /proc/<pid>/stat.
pub fn parse_stat(s: &str) -> Option<(String, u64, u64)> {
    let open = s.find('(')?;
    let close = s.rfind(')')?;
    let name = s[open + 1..close].to_string();
    let f: Vec<&str> = s[close + 2..].split_whitespace().collect();
    // fields after ")": state(0) ppid(1) ... utime(11) stime(12) ... rss(21)
    let ut: u64 = f.get(11)?.parse().ok()?;
    let st: u64 = f.get(12)?.parse().ok()?;
    let rss: u64 = f.get(21)?.parse().ok()?;
    Some((name, ut + st, rss))
}

fn snapshot() -> HashMap<u32, (String, u64, u64)> {
    let mut m = HashMap::new();
    for e in fs::read_dir("/proc").into_iter().flatten().flatten() {
        let Ok(pid) = e.file_name().to_string_lossy().parse::<u32>() else { continue };
        if let Some(v) = fs::read_to_string(e.path().join("stat")).ok().and_then(|s| parse_stat(&s)) {
            m.insert(pid, v);
        }
    }
    m
}

const CLK_TCK: f64 = 100.0;
const PAGE: u64 = 4096;

pub fn top(n: usize, window: Duration) -> Vec<Proc> {
    let a = snapshot();
    std::thread::sleep(window);
    let b = snapshot();
    let secs = window.as_secs_f64();
    let mut v: Vec<Proc> = b
        .iter()
        .filter_map(|(pid, (name, t1, rss))| {
            let t0 = a.get(pid).map(|x| x.1)?;
            let pct = (t1.saturating_sub(t0)) as f64 / CLK_TCK / secs * 100.0;
            Some(Proc { pid: *pid, name: name.clone(), cpu_percent: pct, rss_bytes: rss * PAGE })
        })
        // rss 0 = kernel thread; not something the user can close.
        .filter(|p| p.cpu_percent > 0.0 && p.rss_bytes > 0)
        .collect();
    // Merge by name: Chrome's 20 renderers read better as one line.
    let mut merged: HashMap<String, Proc> = HashMap::new();
    for p in v.drain(..) {
        merged
            .entry(p.name.clone())
            .and_modify(|m| {
                m.cpu_percent += p.cpu_percent;
                m.rss_bytes += p.rss_bytes;
            })
            .or_insert(p);
    }
    let mut v: Vec<Proc> = merged.into_values().collect();
    v.sort_by(|a, b| b.cpu_percent.total_cmp(&a.cpu_percent));
    v.truncate(n);
    v
}

/// Our own processes with an NVIDIA device node open (others' fds are unreadable).
pub fn gpu_users(proc_root: &Path) -> Vec<GpuUser> {
    let mut out = Vec::new();
    for e in fs::read_dir(proc_root).into_iter().flatten().flatten() {
        let Ok(pid) = e.file_name().to_string_lossy().parse::<u32>() else { continue };
        let Ok(fds) = fs::read_dir(e.path().join("fd")) else { continue };
        let holds = fds.flatten().any(|fd| {
            fs::read_link(fd.path()).is_ok_and(|t| t.to_string_lossy().starts_with("/dev/nvidia"))
        });
        if holds {
            let name = fs::read_to_string(e.path().join("comm")).unwrap_or_default().trim().to_string();
            out.push(GpuUser { pid, name });
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

pub const GPU_SERVICES: &[&str] = &["nvidia-persistenced.service", "ollama.service", "nvidia-powerd.service"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stat_with_spaces_in_name() {
        let s = "1234 (Web Content (x)) S 1 1234 1234 0 -1 4194560 100 0 0 0 250 50 0 0 20 0 30 0 5000 123456789 2048 18446744073709551615";
        let (name, ticks, rss) = parse_stat(s).unwrap();
        assert_eq!(name, "Web Content (x)");
        assert_eq!(ticks, 300);
        assert_eq!(rss, 2048);
    }
}
