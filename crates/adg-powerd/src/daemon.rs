//! Daemon state: the sampler thread and every action the API exposes.

use crate::battery::{self, ChargeState, PowerSnapshot};
use crate::dbus::{audio::Audio, bluez::Bluetooth, network::Network, power::Power, systemd::Systemd};
use crate::history::{History, WindowStats};
use crate::policy::{self, RevertState, Scope};
use crate::system::{self, Backlight, CpuTimes, GpuState, Memory};
use serde::Serialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use zbus::blocking::Connection;

/// No D-Bus call may hang a request (and through it, the dashboard).
pub const DBUS_TIMEOUT: Duration = Duration::from_secs(20);
pub const REAP_INTERVAL: Duration = Duration::from_secs(10);

fn connect(system: bool) -> zbus::Result<Connection> {
    let b = if system {
        zbus::blocking::connection::Builder::system()?
    } else {
        zbus::blocking::connection::Builder::session()?
    };
    b.method_timeout(DBUS_TIMEOUT).build()
}

pub const SAMPLE_INTERVAL: Duration = Duration::from_secs(3);
/// 3 s × 1200 = one hour of history.
pub const HISTORY_CAP: usize = 1200;
pub const ECO_BRIGHTNESS_PERCENT: f64 = 20.0;
pub const KERNEL_MARKER: &str = "adgpower.eco=1";

#[derive(Debug, Clone, Serialize)]
pub struct PowerView {
    #[serde(flatten)]
    pub snap: PowerSnapshot,
    pub smooth_w: Option<f64>,
    pub runtime_hours: Option<f64>,
    pub time_to_full_hours: Option<f64>,
    pub health_percent: Option<f64>,
    pub window_5m: WindowStats,
}

#[derive(Debug, Clone, Serialize)]
pub struct Status {
    pub time: u64,
    pub hostname: String,
    pub power: PowerView,
    pub cpu_percent: Option<f64>,
    pub memory: Option<Memory>,
    pub gpus: Vec<GpuState>,
    pub backlight: Option<Backlight>,
    pub keyboard_backlight: Option<(i32, i32)>,
    pub profile: crate::dbus::power::ProfileStatus,
    pub eco_applied: bool,
    pub eco_boot: bool,
    pub sway: bool,
}

struct Sampler {
    history: History,
    last_cpu: Option<CpuTimes>,
    cpu_percent: Option<f64>,
    last: Option<PowerSnapshot>,
}

pub struct Daemon {
    pub sysfs_power: PathBuf,
    pub state_dir: PathBuf,
    sampler: Mutex<Sampler>,
    system_bus: Option<Connection>,
    pub bluetooth: Option<Bluetooth>,
    audio: Audio,
    session_bus: Option<Connection>,
    kbd_available: OnceLock<bool>,
    /// Brightness before an idle dim, restored on input.
    dimmed_from: Mutex<Option<(String, u32)>>,
    reaper: Mutex<crate::apps::Reaper>,
}

fn hostname() -> String {
    std::fs::read_to_string("/proc/sys/kernel/hostname").map(|s| s.trim().to_string()).unwrap_or_default()
}

pub fn eco_boot_active(cmdline: &str) -> bool {
    cmdline.split_whitespace().any(|a| a == KERNEL_MARKER)
}

type ApiResult = Result<Value, String>;

impl Daemon {
    pub fn new(sysfs_power: PathBuf) -> Self {
        let system_bus = connect(true).map_err(|e| eprintln!("system bus unavailable: {e}")).ok();
        let session_bus = connect(false).ok();
        let bluetooth = system_bus.clone().map(Bluetooth::new);
        if let Some(bt) = &bluetooth {
            if let Err(e) = bt.register_agent() {
                eprintln!("bluetooth agent not registered: {e}");
            }
        }
        Self {
            sysfs_power,
            state_dir: policy::state_dir(),
            sampler: Mutex::new(Sampler {
                history: History::new(HISTORY_CAP, 0.25),
                last_cpu: system::read_cpu_times(),
                cpu_percent: None,
                last: None,
            }),
            audio: Audio::new(session_bus.clone()),
            session_bus,
            system_bus,
            bluetooth,
            kbd_available: OnceLock::new(),
            dimmed_from: Mutex::new(None),
            reaper: Mutex::new(crate::apps::Reaper::default()),
        }
    }

    fn bus(&self) -> Result<&Connection, String> {
        self.system_bus.as_ref().ok_or_else(|| "System D-Bus is unavailable".to_string())
    }

    fn network(&self) -> Result<Network, String> {
        Ok(Network::new(self.bus()?.clone()))
    }

    fn power(&self) -> Result<Power, String> {
        Ok(Power::new(self.bus()?.clone()))
    }

    fn bt(&self) -> Result<&Bluetooth, String> {
        self.bluetooth.as_ref().ok_or_else(|| "Bluetooth unavailable".to_string())
    }

    /// Take one sample. Called by the sampler thread.
    pub fn sample(&self) {
        let snap = battery::snapshot(&self.sysfs_power);
        let now = policy::unix_now();
        let mut s = self.sampler.lock().unwrap();
        if let Some(cur) = system::read_cpu_times() {
            if let Some(prev) = s.last_cpu {
                s.cpu_percent = system::cpu_percent(prev, cur);
            }
            s.last_cpu = Some(cur);
        }
        // Only discharge readings are meaningful as "draw"; while charging the
        // sensor reports charge current, which is still useful to show.
        let discharging = snap.state == ChargeState::Discharging;
        s.history.push(now, snap.power_w, snap.capacity_percent, discharging);
        s.last = Some(snap);
    }

    pub fn power_view(&self) -> PowerView {
        let s = self.sampler.lock().unwrap();
        let snap = s.last.clone().unwrap_or_else(|| battery::snapshot(&self.sysfs_power));
        let smooth = s.history.smoothed();
        PowerView {
            runtime_hours: battery::runtime_hours(snap.state, snap.energy_now_wh, smooth),
            time_to_full_hours: battery::time_to_full_hours(snap.state, snap.energy_now_wh, snap.energy_full_wh, smooth),
            health_percent: snap.batteries.first().and_then(|b| b.health_percent()),
            window_5m: s.history.stats_since(policy::unix_now().saturating_sub(300)),
            smooth_w: smooth,
            snap,
        }
    }

    pub fn status(&self) -> Status {
        let cpu = self.sampler.lock().unwrap().cpu_percent;
        let power = self.power().ok();
        Status {
            time: policy::unix_now(),
            hostname: hostname(),
            power: self.power_view(),
            cpu_percent: cpu,
            memory: system::read_memory(),
            gpus: system::read_gpus(),
            backlight: system::read_backlight(Path::new("/sys/class/backlight")),
            // Probe once: most laptops (this MSI included) never expose it.
            keyboard_backlight: power.as_ref().and_then(|p| {
                if *self.kbd_available.get_or_init(|| p.keyboard_backlight().is_some()) { p.keyboard_backlight() } else { None }
            }),
            profile: power.map(|p| p.profile()).unwrap_or_default(),
            eco_applied: policy::load_revert(&self.state_dir).is_some(),
            eco_boot: eco_boot_active(&std::fs::read_to_string("/proc/cmdline").unwrap_or_default()),
            sway: crate::sway::active(),
        }
    }

    pub fn history(&self, minutes: u64) -> Value {
        let since = policy::unix_now().saturating_sub(minutes * 60);
        let s = self.sampler.lock().unwrap();
        let pts: Vec<_> = s.history.samples().filter(|p| p.t >= since).collect();
        json!({ "interval_s": SAMPLE_INTERVAL.as_secs(), "samples": pts })
    }

    fn audit<T>(&self, action: &str, detail: &str, r: &Result<T, String>) {
        policy::audit(&self.state_dir, action, detail, r.is_ok());
    }

    // ------------------------------------------------------------ profile

    pub fn set_profile(&self, name: &str) -> ApiResult {
        let r = self.power()?.set_profile(name);
        self.audit("power_profile", name, &r);
        r.map(|_| json!({}))
    }

    pub fn set_brightness_percent(&self, pct: f64) -> ApiResult {
        let bl = system::read_backlight(Path::new("/sys/class/backlight")).ok_or("No backlight")?;
        let raw = policy::eco_brightness(bl.max_brightness, pct);
        self.power()?.set_brightness(&bl.device, raw)?;
        Ok(json!({ "raw": raw }))
    }

    pub fn dim(&self, on: bool) -> ApiResult {
        let power = self.power()?;
        let mut saved = self.dimmed_from.lock().unwrap();
        if on {
            let bl = system::read_backlight(Path::new("/sys/class/backlight")).ok_or("No backlight")?;
            if saved.is_none() {
                *saved = Some((bl.device.clone(), bl.brightness as u32));
            }
            power.set_brightness(&bl.device, policy::eco_brightness(bl.max_brightness, 5.0))?;
        } else if let Some((dev, raw)) = saved.take() {
            power.set_brightness(&dev, raw)?;
        }
        Ok(json!({}))
    }

    pub fn set_keyboard(&self, v: i32) -> ApiResult {
        let r = self.power()?.set_keyboard_backlight(v);
        self.audit("keyboard_backlight", &v.to_string(), &r);
        r.map(|_| json!({}))
    }

    /// Apply the eco profile, remembering prior values the first time.
    pub fn apply_eco(&self) -> ApiResult {
        let power = self.power()?;
        let mut notes: Vec<String> = Vec::new();
        if policy::load_revert(&self.state_dir).is_none() {
            let bl = system::read_backlight(Path::new("/sys/class/backlight"));
            let rs = RevertState {
                applied_at: policy::unix_now(),
                power_profile: power.profile().active,
                backlight_device: bl.as_ref().map(|b| b.device.clone()),
                backlight_raw: bl.as_ref().map(|b| b.brightness as u32),
                keyboard_backlight: power.keyboard_backlight().map(|k| k.0),
                bluetooth_powered: self.bluetooth.as_ref().map(|b| b.status().powered),
            };
            policy::save_revert(&self.state_dir, &rs).map_err(|e| e.to_string())?;
        }
        fn step(notes: &mut Vec<String>, name: &str, r: Result<(), String>) {
            notes.push(match r {
                Ok(()) => format!("{name}: ok"),
                Err(e) => format!("{name}: {e}"),
            });
        }
        step(&mut notes, "power-saver", power.set_profile("power-saver"));
        step(&mut notes, "brightness", self.set_brightness_percent(ECO_BRIGHTNESS_PERCENT).map(|_| ()));
        match power.keyboard_backlight() {
            Some(_) => step(&mut notes, "keyboard", power.set_keyboard_backlight(0)),
            None => notes.push("keyboard: not exposed by firmware".into()),
        }
        // Bluetooth is on demand: power it down only if nothing is connected.
        if let Some(bt) = &self.bluetooth {
            let st = bt.status();
            if st.powered && !st.devices.iter().any(|d| d.connected) {
                step(&mut notes, "bluetooth off", bt.set_powered(false));
            } else if st.powered {
                notes.push("bluetooth: left on (device connected)".into());
            }
        }
        if crate::sway::active() {
            step(&mut notes, "display 60Hz", crate::sway::prefer_60hz());
        }
        policy::audit(&self.state_dir, "eco_apply", &notes.join("; "), true);
        Ok(json!({ "steps": notes }))
    }

    pub fn revert_eco(&self) -> ApiResult {
        let rs = policy::load_revert(&self.state_dir).ok_or("Eco profile is not applied")?;
        let power = self.power()?;
        let mut notes = Vec::new();
        if let Some(p) = &rs.power_profile {
            notes.push(format!("profile: {:?}", power.set_profile(p)));
        }
        if let (Some(d), Some(v)) = (&rs.backlight_device, rs.backlight_raw) {
            notes.push(format!("brightness: {:?}", power.set_brightness(d, v)));
        }
        if let Some(k) = rs.keyboard_backlight {
            notes.push(format!("keyboard: {:?}", power.set_keyboard_backlight(k)));
        }
        if let (Some(true), Some(bt)) = (rs.bluetooth_powered, &self.bluetooth) {
            notes.push(format!("bluetooth: {:?}", bt.set_powered(true)));
        }
        if crate::sway::active() {
            notes.push(format!("display: {:?}", crate::sway::restore_mode()));
        }
        policy::clear_revert(&self.state_dir);
        policy::audit(&self.state_dir, "eco_revert", &notes.join("; "), true);
        Ok(json!({ "steps": notes }))
    }

    // ------------------------------------------------------------ privileged helper

    /// Ask the root helper (via pkexec + polkit) to run one allowlisted action.
    pub fn helper(&self, action: &str, target: &str, value: &str) -> ApiResult {
        let ok_action = matches!(action, "gpu-runtime-pm" | "wifi-powersave" | "eco-boot-once");
        let ok_value = matches!(value, "auto" | "on" | "off");
        let ok_target = !target.is_empty() && target.chars().all(|c| c.is_ascii_alphanumeric() || ":.-_".contains(c));
        if !(ok_action && ok_value && ok_target) {
            return Err("Action not allowed".into());
        }
        let helper = helper_path().ok_or("adg-power-helper is not installed (run scripts/install.sh)")?;
        let out = Command::new("pkexec").arg(helper).args([action, target, value]).output().map_err(|e| e.to_string())?;
        let r = if out.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
        };
        self.audit("helper", &format!("{action} {target} {value}"), &r);
        r.map(|_| json!({}))
    }

    // ------------------------------------------------------------ network

    pub fn wifi(&self) -> ApiResult {
        Ok(serde_json::to_value(self.network()?.status()).unwrap())
    }
    pub fn wifi_networks(&self) -> ApiResult {
        Ok(serde_json::to_value(self.network()?.access_points()?).unwrap())
    }
    pub fn wifi_scan(&self) -> ApiResult {
        self.network()?.request_scan().map(|_| json!({}))
    }
    pub fn wifi_enable(&self, on: bool) -> ApiResult {
        let r = self.network()?.set_enabled(on);
        self.audit("wifi_radio", if on { "on" } else { "off" }, &r);
        r.map(|_| json!({}))
    }
    pub fn wifi_connect(&self, ssid: &str, password: Option<&str>) -> ApiResult {
        if ssid.is_empty() || ssid.len() > 32 {
            return Err("Invalid SSID".into());
        }
        let r = self.network()?.connect(ssid, password.filter(|p| !p.is_empty()));
        // The SSID is logged; the password never is.
        self.audit("wifi_connect", ssid, &r);
        r.map(|_| json!({}))
    }
    pub fn wifi_disconnect(&self) -> ApiResult {
        let r = self.network()?.disconnect();
        self.audit("wifi_disconnect", "", &r);
        r.map(|_| json!({}))
    }

    // ------------------------------------------------------------ bluetooth

    pub fn bluetooth(&self) -> ApiResult {
        Ok(serde_json::to_value(self.bt()?.status()).unwrap())
    }
    pub fn bt_action(&self, action: &str, address: Option<&str>, on: Option<bool>) -> ApiResult {
        let bt = self.bt()?;
        let addr = || address.ok_or_else(|| "address required".to_string());
        let r = match action {
            "power" => bt.set_powered(on.unwrap_or(true)),
            "discover" => bt.discover(on.unwrap_or(true)),
            "pair" => bt.pair(addr()?),
            "connect" => bt.connect(addr()?),
            "disconnect" => bt.disconnect(addr()?),
            "trust" => bt.set_trusted(addr()?, on.unwrap_or(true)),
            "forget" => bt.forget(addr()?),
            _ => return Err("Unknown Bluetooth action".into()),
        };
        self.audit(&format!("bluetooth_{action}"), address.unwrap_or(""), &r);
        r.map(|_| json!({}))
    }

    // ------------------------------------------------------------ audio

    pub fn audio(&self) -> ApiResult {
        Ok(serde_json::to_value(self.audio.status()).unwrap())
    }
    pub fn audio_default(&self, id: u32) -> ApiResult {
        self.audio.set_default(id).map(|_| json!({}))
    }
    pub fn audio_volume(&self, id: u32, v: f64) -> ApiResult {
        self.audio.set_volume(id, v).map(|_| json!({}))
    }
    pub fn audio_play_pause(&self) -> ApiResult {
        self.audio.play_pause().map(|_| json!({}))
    }

    // ------------------------------------------------------------ services

    pub fn services(&self) -> ApiResult {
        let sys = self.system_bus.clone().map(Systemd::new);
        let user = self.session_bus.clone().map(Systemd::new);
        let rows: Vec<Value> = policy::POLICY
            .iter()
            .map(|e| {
                let sd = match e.scope {
                    Scope::System => sys.as_ref(),
                    Scope::User => user.as_ref(),
                };
                let st = sd.and_then(|s| s.unit(e.unit).ok());
                json!({
                    "unit": e.unit,
                    "scope": e.scope,
                    "category": e.category,
                    "eco": e.eco,
                    "installed": st.as_ref().is_some_and(|s| s.installed()),
                    "active_state": st.as_ref().map(|s| s.active_state.clone()),
                    "sub_state": st.as_ref().map(|s| s.sub_state.clone()),
                    "unit_file_state": st.as_ref().map(|s| s.unit_file_state.clone()),
                    "description": st.as_ref().map(|s| s.description.clone()),
                    "stoppable": policy::may_stop(e).is_ok(),
                    "stop_blocker": policy::may_stop(e).err(),
                })
            })
            .collect();
        Ok(json!({ "services": rows, "running_containers": policy::running_containers(Path::new("/proc")) }))
    }

    pub fn service_action(&self, unit: &str, start: bool) -> ApiResult {
        let entry = policy::lookup(unit).ok_or("Unit is not managed by ADG Power OS")?;
        if !start {
            policy::may_stop(entry)?;
        }
        let bus = match entry.scope {
            Scope::System => self.bus()?.clone(),
            Scope::User => self.session_bus.clone().ok_or("Session bus unavailable")?,
        };
        let sd = Systemd::new(bus);
        let r = if start { sd.start(unit) } else { sd.stop(unit) };
        self.audit(if start { "service_start" } else { "service_stop" }, unit, &r);
        r.map(|_| json!({}))
    }

    // ------------------------------------------------------------ launcher

    pub fn launch(&self, app: &str) -> ApiResult {
        let argv = crate::launcher::command_for(app).ok_or("Unknown app")?;
        crate::launcher::spawn(app, &argv)?;
        Ok(json!({ "launched": app }))
    }

    // ------------------------------------------------------------ running apps

    pub fn apps(&self) -> ApiResult {
        let bus = self.session_bus.as_ref().ok_or("Session bus unavailable")?;
        let pids = crate::sway::window_pids();
        let list = crate::apps::list(bus, pids.as_ref())?;
        Ok(json!({ "apps": list, "sway": pids.is_some() }))
    }

    pub fn app_stop(&self, unit: &str) -> ApiResult {
        let bus = self.session_bus.as_ref().ok_or("Session bus unavailable")?;
        let r = crate::apps::stop(bus, unit);
        self.audit("app_close", unit, &r);
        r.map(|_| json!({}))
    }

    /// Stop dashboard-launched apps whose windows have all been closed, so
    /// background processes (Chrome's especially) don't keep using power.
    pub fn reap(&self) {
        let Some(bus) = self.session_bus.as_ref() else { return };
        let Some(pids) = crate::sway::window_pids() else { return };
        let Ok(list) = crate::apps::list(bus, Some(&pids)) else { return };
        let doomed = self.reaper.lock().unwrap().pass(&list, policy::unix_now());
        for unit in doomed {
            let r = crate::apps::stop(bus, &unit);
            self.audit("app_reaped", &unit, &r);
        }
    }

    // ------------------------------------------------------------ power hogs

    pub fn hogs(&self) -> ApiResult {
        let top = crate::procs::top(8, Duration::from_millis(800));
        let sys = self.system_bus.clone().map(Systemd::new);
        let gpu_services: Vec<String> = crate::procs::GPU_SERVICES
            .iter()
            .filter(|u| sys.as_ref().and_then(|s| s.unit(u).ok()).is_some_and(|st| st.active_state == "active"))
            .map(|u| u.to_string())
            .collect();
        Ok(serde_json::to_value(crate::procs::PowerHogs {
            top,
            gpu_users: crate::procs::gpu_users(Path::new("/proc")),
            gpu_services,
        })
        .unwrap())
    }

    // ------------------------------------------------------------ session

    fn logind(&self, method: &str) -> Result<(), String> {
        let p = zbus::blocking::Proxy::new(self.bus()?, "org.freedesktop.login1", "/org/freedesktop/login1", "org.freedesktop.login1.Manager")
            .map_err(crate::dbus::err_text)?;
        p.call_with_flags::<_, _, ()>(method, zbus::proxy::MethodFlags::AllowInteractiveAuth.into(), &(true,))
            .map(|_| ())
            .map_err(crate::dbus::err_text)
    }

    pub fn session_action(&self, action: &str) -> ApiResult {
        let r = match action {
            "lock" => {
                if crate::sway::active() {
                    self.launch("lock").map(|_| ())
                } else {
                    zbus::blocking::Proxy::new(self.bus()?, "org.freedesktop.login1", "/org/freedesktop/login1/session/auto", "org.freedesktop.login1.Session")
                        .and_then(|p| p.call::<_, _, ()>("Lock", &()))
                        .map_err(crate::dbus::err_text)
                }
            }
            "logout" => {
                if crate::sway::active() {
                    crate::sway::exit()
                } else {
                    Command::new("gnome-session-quit").args(["--logout", "--no-prompt"]).status()
                        .map_err(|e| e.to_string())
                        .and_then(|s| s.success().then_some(()).ok_or_else(|| "gnome-session-quit failed".to_string()))
                }
            }
            "suspend" => self.logind("Suspend"),
            "reboot" => self.logind("Reboot"),
            "poweroff" => self.logind("PowerOff"),
            "eco-reboot" => self.helper("eco-boot-once", "adg-power-os", "on").and_then(|_| self.logind("Reboot")).map(|_| ()),
            _ => return Err("Unknown session action".into()),
        };
        self.audit("session", action, &r);
        r.map(|_| json!({}))
    }

    pub fn audit_log(&self) -> ApiResult {
        Ok(json!(policy::read_audit(&self.state_dir, 50)))
    }
}

fn helper_path() -> Option<PathBuf> {
    ["/usr/libexec/adg-power/adg-power-helper", "/usr/local/libexec/adg-power/adg-power-helper"]
        .iter()
        .map(PathBuf::from)
        .find(|p| p.exists())
}

#[cfg(test)]
mod tests {
    use super::eco_boot_active;

    #[test]
    fn kernel_marker() {
        assert!(eco_boot_active("BOOT_IMAGE=/vmlinuz ro quiet adgpower.eco=1 splash"));
        assert!(!eco_boot_active("BOOT_IMAGE=/vmlinuz ro quiet splash"));
        assert!(!eco_boot_active("adgpower.eco=10"));
    }
}
