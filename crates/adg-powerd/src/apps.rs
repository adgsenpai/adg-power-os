//! Running apps launched from the dashboard, closing them completely, and
//! reaping apps whose windows are all gone.
//!
//! Every launch runs in its own transient systemd user unit (app-adg-*), so
//! "Close" stops the whole cgroup: helper processes, renderers and
//! background workers all go, and their memory is freed.

use crate::dbus::err_text;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use zbus::blocking::{Connection, Proxy};
use zbus::proxy::MethodFlags;
use zbus::zvariant::OwnedObjectPath;

const SD: &str = "org.freedesktop.systemd1";
const PREFIX: &str = "app-adg-";

#[derive(Debug, Clone, Serialize)]
pub struct App {
    pub unit: String,
    pub app: String,
    pub memory_bytes: Option<u64>,
    pub cpu_seconds: Option<f64>,
    pub tasks: Option<u64>,
    pub started: Option<u64>,
    pub windows: Option<usize>,
}

/// "app-adg-chrome-1794274243.service" → "chrome"
pub fn app_id(unit: &str) -> Option<&str> {
    let s = unit.strip_prefix(PREFIX)?.strip_suffix(".service")?;
    let (id, ts) = s.rsplit_once('-')?;
    ts.chars().all(|c| c.is_ascii_digit()).then_some(id)
}

pub fn valid_app_unit(unit: &str) -> bool {
    app_id(unit).is_some_and(|id| crate::launcher::APPS.contains(&id))
}

type UnitRow = (String, String, String, String, String, String, OwnedObjectPath, u32, String, OwnedObjectPath);

fn u64_prop(p: &Proxy, k: &str) -> Option<u64> {
    p.get_property::<u64>(k).ok().filter(|v| *v != u64::MAX)
}

pub fn list(session: &Connection, window_pids: Option<&HashSet<u32>>) -> Result<Vec<App>, String> {
    let mgr = Proxy::new(session, SD, "/org/freedesktop/systemd1", "org.freedesktop.systemd1.Manager").map_err(err_text)?;
    let rows: Vec<UnitRow> = mgr
        .call("ListUnitsByPatterns", &(vec!["active", "activating"], vec![format!("{PREFIX}*")]))
        .map_err(err_text)?;
    let mut out = Vec::new();
    for r in rows {
        let unit = r.0;
        let Some(id) = app_id(&unit).map(str::to_owned) else { continue };
        let Ok(svc) = Proxy::new(session, SD, r.6.clone(), "org.freedesktop.systemd1.Service") else { continue };
        let unit_p = Proxy::new(session, SD, r.6, "org.freedesktop.systemd1.Unit").ok();
        let cgroup: Option<String> = svc.get_property("ControlGroup").ok();
        let windows = match (window_pids, cgroup.as_deref()) {
            (Some(w), Some(cg)) => Some(cgroup_pids(cg).iter().filter(|p| w.contains(p)).count()),
            _ => None,
        };
        out.push(App {
            memory_bytes: u64_prop(&svc, "MemoryCurrent"),
            cpu_seconds: u64_prop(&svc, "CPUUsageNSec").map(|n| n as f64 / 1e9),
            tasks: u64_prop(&svc, "TasksCurrent"),
            started: unit_p.and_then(|u| u64_prop(&u, "ActiveEnterTimestamp")).map(|us| us / 1_000_000),
            windows,
            app: id,
            unit,
        });
    }
    out.sort_by(|a, b| b.memory_bytes.cmp(&a.memory_bytes));
    Ok(out)
}

pub fn cgroup_pids(cgroup: &str) -> Vec<u32> {
    let path = Path::new("/sys/fs/cgroup").join(cgroup.trim_start_matches('/')).join("cgroup.procs");
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.trim().parse().ok())
        .collect()
}

/// Stop the whole unit: SIGTERM to every process, SIGKILL after systemd's timeout.
pub fn stop(session: &Connection, unit: &str) -> Result<(), String> {
    if !valid_app_unit(unit) {
        return Err("Not an app launched from the dashboard".into());
    }
    let mgr = Proxy::new(session, SD, "/org/freedesktop/systemd1", "org.freedesktop.systemd1.Manager").map_err(err_text)?;
    mgr.call_with_flags::<_, _, OwnedObjectPath>("StopUnit", MethodFlags::AllowInteractiveAuth.into(), &(unit, "replace"))
        .map(|_| ())
        .map_err(err_text)
}

/// Tracks how many consecutive passes each app has had no window.
#[derive(Default)]
pub struct Reaper {
    windowless: HashMap<String, u32>,
}

/// Grace before a windowless app is stopped: passes × reaper interval.
pub const REAP_AFTER_PASSES: u32 = 3;
/// Apps younger than this are never reaped (still opening their window).
pub const REAP_MIN_AGE_S: u64 = 30;

impl Reaper {
    /// Returns the units that should be stopped now.
    pub fn pass(&mut self, apps: &[App], now: u64) -> Vec<String> {
        let mut seen = HashSet::new();
        let mut doomed = Vec::new();
        for a in apps {
            seen.insert(a.unit.clone());
            let old_enough = a.started.is_some_and(|s| now.saturating_sub(s) >= REAP_MIN_AGE_S);
            match a.windows {
                Some(0) if old_enough => {
                    let n = self.windowless.entry(a.unit.clone()).or_insert(0);
                    *n += 1;
                    if *n >= REAP_AFTER_PASSES {
                        doomed.push(a.unit.clone());
                    }
                }
                _ => {
                    self.windowless.remove(&a.unit);
                }
            }
        }
        self.windowless.retain(|u, _| seen.contains(u));
        doomed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(unit: &str, windows: Option<usize>, started: u64) -> App {
        App { unit: unit.into(), app: "chrome".into(), memory_bytes: None, cpu_seconds: None, tasks: None, started: Some(started), windows }
    }

    #[test]
    fn unit_names() {
        assert_eq!(app_id("app-adg-chrome-1794274243.service"), Some("chrome"));
        assert_eq!(app_id("app-adg-network-editor-17.service"), Some("network-editor"));
        assert!(valid_app_unit("app-adg-chrome-1.service"));
        assert!(!valid_app_unit("docker.service"));
        assert!(!valid_app_unit("app-adg-bash-1.service"));
        assert!(!valid_app_unit("app-adg-chrome-x.service"));
    }

    #[test]
    fn reaper_waits_for_consecutive_windowless_passes() {
        let mut r = Reaper::default();
        let u = "app-adg-chrome-1.service";
        // Too young: never reaped.
        assert!(r.pass(&[app(u, Some(0), 100)], 110).is_empty());
        // Old enough, windowless: reaped on the third pass, not before.
        assert!(r.pass(&[app(u, Some(0), 0)], 100).is_empty());
        assert!(r.pass(&[app(u, Some(0), 0)], 105).is_empty());
        assert_eq!(r.pass(&[app(u, Some(0), 0)], 110), vec![u.to_string()]);
        // A window reappearing resets the count.
        let mut r = Reaper::default();
        r.pass(&[app(u, Some(0), 0)], 100);
        r.pass(&[app(u, Some(0), 0)], 105);
        r.pass(&[app(u, Some(1), 0)], 110);
        assert!(r.pass(&[app(u, Some(0), 0)], 115).is_empty());
        // Unknown window state (not under Sway) is never reaped.
        let mut r = Reaper::default();
        for t in 0..10 {
            assert!(r.pass(&[app(u, None, 0)], 100 + t).is_empty());
        }
    }
}
