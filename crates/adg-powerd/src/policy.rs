//! Eco service policy, safety checks, the eco profile with rollback, and the
//! audit log.

use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    System,
    User,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EcoAction {
    /// Masked at runtime by the eco boot generator; never started in eco mode.
    BlockedAtEcoBoot,
    /// Allowed but started only when requested (socket/D-Bus activation).
    OnDemand,
    /// Must stay running.
    Keep,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct PolicyEntry {
    pub unit: &'static str,
    pub scope: Scope,
    pub category: &'static str,
    pub eco: EcoAction,
    /// Check that must pass before the dashboard may stop it.
    #[serde(skip)]
    pub guard: Guard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Guard {
    None,
    NoRunningContainers,
    NoClientConnections(u16),
    Never,
}

use EcoAction::*;
use Scope::*;

macro_rules! entry {
    ($u:expr, $s:expr, $c:expr, $e:expr) => {
        PolicyEntry { unit: $u, scope: $s, category: $c, eco: $e, guard: Guard::None }
    };
    ($u:expr, $s:expr, $c:expr, $e:expr, $g:expr) => {
        PolicyEntry { unit: $u, scope: $s, category: $c, eco: $e, guard: $g }
    };
}

/// Single source of truth for the service matrix. The eco boot generator
/// (system/generator) masks the BlockedAtEcoBoot entries; keep them in sync
/// via `adg-powerd policy --generator-list`.
pub const POLICY: &[PolicyEntry] = &[
    entry!("NetworkManager.service", System, "network", Keep, Guard::Never),
    entry!("wpa_supplicant.service", System, "network", Keep, Guard::Never),
    entry!("systemd-logind.service", System, "core", Keep, Guard::Never),
    entry!("power-profiles-daemon.service", System, "power", Keep, Guard::Never),
    entry!("upower.service", System, "power", Keep, Guard::Never),
    entry!("bluetooth.service", System, "bluetooth", OnDemand, Guard::Never),
    entry!("pipewire.socket", User, "audio", OnDemand, Guard::Never),
    entry!("wireplumber.service", User, "audio", OnDemand, Guard::Never),
    entry!("docker.service", System, "containers", BlockedAtEcoBoot, Guard::NoRunningContainers),
    entry!("docker.socket", System, "containers", BlockedAtEcoBoot, Guard::NoRunningContainers),
    entry!("containerd.service", System, "containers", BlockedAtEcoBoot, Guard::NoRunningContainers),
    entry!("k3s.service", System, "kubernetes", BlockedAtEcoBoot, Guard::NoRunningContainers),
    entry!("k3s-agent.service", System, "kubernetes", BlockedAtEcoBoot, Guard::NoRunningContainers),
    entry!("snap.microk8s.daemon-kubelite.service", System, "kubernetes", BlockedAtEcoBoot, Guard::NoRunningContainers),
    entry!("postgresql.service", System, "database", BlockedAtEcoBoot, Guard::NoClientConnections(5432)),
    // Debian/Ubuntu run each cluster as postgresql@<ver>-<name>; masking the
    // template covers every instance.
    entry!("postgresql@.service", System, "database", BlockedAtEcoBoot, Guard::NoClientConnections(5432)),
    entry!("mysql.service", System, "database", BlockedAtEcoBoot, Guard::NoClientConnections(3306)),
    entry!("mariadb.service", System, "database", BlockedAtEcoBoot, Guard::NoClientConnections(3306)),
    entry!("redis-server.service", System, "database", BlockedAtEcoBoot, Guard::NoClientConnections(6379)),
    entry!("mongod.service", System, "database", BlockedAtEcoBoot, Guard::NoClientConnections(27017)),
    entry!("libvirtd.service", System, "virtualization", BlockedAtEcoBoot, Guard::Never),
    entry!("libvirtd.socket", System, "virtualization", BlockedAtEcoBoot, Guard::Never),
    entry!("virtqemud.socket", System, "virtualization", BlockedAtEcoBoot, Guard::Never),
    entry!("snap.lxd.daemon.service", System, "virtualization", BlockedAtEcoBoot, Guard::Never),
    entry!("ollama.service", System, "gpu", BlockedAtEcoBoot),
    // Both keep the RTX open, so it never reaches runtime suspend (D3cold).
    entry!("nvidia-persistenced.service", System, "gpu", BlockedAtEcoBoot),
    entry!("nvidia-powerd.service", System, "gpu", BlockedAtEcoBoot),
    entry!("unattended-upgrades.service", System, "updates", BlockedAtEcoBoot),
    entry!("apt-daily.timer", System, "updates", BlockedAtEcoBoot),
    entry!("apt-daily-upgrade.timer", System, "updates", BlockedAtEcoBoot),
    entry!("packagekit.service", System, "updates", BlockedAtEcoBoot),
    entry!("fwupd-refresh.timer", System, "updates", BlockedAtEcoBoot),
    entry!("snapd.snap-repair.timer", System, "updates", BlockedAtEcoBoot),
    entry!("update-notifier-download.timer", System, "updates", BlockedAtEcoBoot),
    entry!("motd-news.timer", System, "updates", BlockedAtEcoBoot),
    entry!("man-db.timer", System, "indexer", BlockedAtEcoBoot),
    entry!("plocate-updatedb.timer", System, "indexer", BlockedAtEcoBoot),
    entry!("snap.cups.cupsd.service", System, "printing", BlockedAtEcoBoot),
    entry!("snap.cups.cups-browsed.service", System, "printing", BlockedAtEcoBoot),
    entry!("cups-browsed.service", System, "printing", BlockedAtEcoBoot),
    entry!("localsearch-3.service", User, "indexer", BlockedAtEcoBoot),
    entry!("tracker-miner-fs-3.service", User, "indexer", BlockedAtEcoBoot),
    entry!("evolution-data-server.service", User, "indexer", BlockedAtEcoBoot),
];

pub fn lookup(unit: &str) -> Option<&'static PolicyEntry> {
    POLICY.iter().find(|e| e.unit == unit)
}

pub fn generator_list(scope: Scope) -> Vec<&'static str> {
    POLICY.iter().filter(|e| e.scope == scope && e.eco == BlockedAtEcoBoot).map(|e| e.unit).collect()
}

/// Count containerd shims; each running container has one.
pub fn running_containers(proc_root: &Path) -> usize {
    fs::read_dir(proc_root)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().chars().all(|c| c.is_ascii_digit()))
        .filter(|e| {
            fs::read_to_string(e.path().join("comm"))
                .is_ok_and(|c| c.trim_end().starts_with("containerd-shim"))
        })
        .count()
}

/// Count ESTABLISHED TCP connections whose local port is `port`,
/// from the text of /proc/net/tcp or tcp6.
pub fn established_on_port(table: &str, port: u16) -> usize {
    table
        .lines()
        .skip(1)
        .filter(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            f.len() > 3
                && f[3] == "01"
                && f[1].rsplit(':').next().and_then(|p| u16::from_str_radix(p, 16).ok()) == Some(port)
        })
        .count()
}

pub fn client_connections(port: u16) -> usize {
    ["/proc/net/tcp", "/proc/net/tcp6"]
        .iter()
        .filter_map(|p| fs::read_to_string(p).ok())
        .map(|t| established_on_port(&t, port))
        .sum()
}

/// Ok(()) if stopping `unit` cannot interrupt active work.
pub fn may_stop(entry: &PolicyEntry) -> Result<(), String> {
    match entry.guard {
        Guard::None => Ok(()),
        Guard::Never => Err(match entry.eco {
            EcoAction::OnDemand => format!("{} is on demand; use its power toggle instead of stopping the daemon", entry.unit),
            _ => format!("{} is required and is never stopped from the dashboard", entry.unit),
        }),
        Guard::NoRunningContainers => match running_containers(Path::new("/proc")) {
            0 => Ok(()),
            n => Err(format!("{n} container(s) are running; stop them yourself first")),
        },
        Guard::NoClientConnections(port) => match client_connections(port) {
            0 => Ok(()),
            n => Err(format!("{n} client connection(s) open on port {port}; close them first")),
        },
    }
}

// ---------------------------------------------------------------- eco profile

/// Values captured before the eco profile is applied, so it can be reverted.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct RevertState {
    pub applied_at: u64,
    pub power_profile: Option<String>,
    pub backlight_device: Option<String>,
    pub backlight_raw: Option<u32>,
    pub keyboard_backlight: Option<i32>,
    pub bluetooth_powered: Option<bool>,
}

/// Eco brightness as a raw value: `percent` of max, never below 1 so the
/// panel is not blanked by accident.
pub fn eco_brightness(max: u64, percent: f64) -> u32 {
    let v = (max as f64 * percent.clamp(1.0, 100.0) / 100.0).round() as u64;
    v.clamp(1, max.max(1)) as u32
}

pub fn state_dir() -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("adg-power")
}

pub fn save_revert(dir: &Path, s: &RevertState) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    let tmp = dir.join("revert.json.tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(s)?)?;
    fs::rename(tmp, dir.join("revert.json"))
}

pub fn load_revert(dir: &Path) -> Option<RevertState> {
    serde_json::from_slice(&fs::read(dir.join("revert.json")).ok()?).ok()
}

pub fn clear_revert(dir: &Path) {
    let _ = fs::remove_file(dir.join("revert.json"));
}

pub fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Append one JSON line to the audit log. Callers must not pass secrets.
pub fn audit(dir: &Path, action: &str, detail: &str, ok: bool) {
    let _ = fs::create_dir_all(dir);
    let line = serde_json::json!({ "t": unix_now(), "action": action, "detail": detail, "ok": ok });
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(dir.join("audit.log")) {
        let _ = writeln!(f, "{line}");
    }
}

pub fn read_audit(dir: &Path, last: usize) -> Vec<serde_json::Value> {
    let text = fs::read_to_string(dir.join("audit.log")).unwrap_or_default();
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(last)..]
        .iter()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_manager_is_never_blocked() {
        let e = lookup("NetworkManager.service").unwrap();
        assert_eq!(e.eco, Keep);
        assert!(may_stop(e).is_err());
        assert!(!generator_list(System).contains(&"NetworkManager.service"));
    }

    #[test]
    fn docker_and_databases_blocked_at_eco_boot() {
        let sys = generator_list(System);
        for u in ["docker.service", "docker.socket", "containerd.service", "k3s.service", "postgresql.service", "postgresql@.service", "nvidia-persistenced.service", "ollama.service"] {
            assert!(sys.contains(&u), "{u} missing");
        }
    }

    #[test]
    fn tcp_table_parsing() {
        let t = "  sl  local_address rem_address   st tx_queue rx_queue
   0: 0100007F:1538 00000000:0000 0A 00000000:00000000 00:00000000 00000000
   1: 0100007F:1538 0100007F:D2A4 01 00000000:00000000 00:00000000 00000000
   2: 0100007F:D2A4 0100007F:1538 01 00000000:00000000 00:00000000 00000000
";
        // Only the server side (local port 5432 = 0x1538) in ESTABLISHED counts.
        assert_eq!(established_on_port(t, 5432), 1);
        assert_eq!(established_on_port(t, 3306), 0);
    }

    #[test]
    fn eco_brightness_never_blanks() {
        assert_eq!(eco_brightness(96000, 20.0), 19200);
        assert_eq!(eco_brightness(10, 0.0), 1);
        assert_eq!(eco_brightness(0, 20.0), 1);
    }

    #[test]
    fn revert_round_trip_and_audit() {
        let d = tempfile::tempdir().unwrap();
        let s = RevertState { applied_at: 5, power_profile: Some("balanced".into()), ..Default::default() };
        save_revert(d.path(), &s).unwrap();
        assert_eq!(load_revert(d.path()), Some(s));
        clear_revert(d.path());
        assert_eq!(load_revert(d.path()), None);
        audit(d.path(), "profile", "eco", true);
        audit(d.path(), "profile", "revert", true);
        let a = read_audit(d.path(), 1);
        assert_eq!(a.len(), 1);
        assert_eq!(a[0]["detail"], "revert");
    }
}
