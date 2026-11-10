//! Battery discovery and unit conversion from the Linux power_supply class.
//!
//! Sysfs units (see Documentation/ABI/testing/sysfs-class-power):
//!   power_now   µW       energy_now  µWh
//!   current_now µA       charge_now  µAh
//!   voltage_now µV
//!
//! Nothing here assumes a battery is called BAT0: every power_supply entry is
//! inspected and system batteries are selected by type and scope.

use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

pub const DEFAULT_SYSFS_ROOT: &str = "/sys/class/power_supply";

/// Below this draw the reading is treated as "no measurable discharge".
pub const MIN_MEANINGFUL_WATTS: f64 = 0.3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChargeState {
    /// On battery and draining.
    Discharging,
    /// On AC and the battery is taking charge.
    Charging,
    /// Battery reports full.
    Full,
    /// On AC, battery neither charging nor draining (charge limit, hold).
    AcIdle,
    /// Firmware gives no usable status.
    Unknown,
}

/// Where a figure came from, so the UI can mark derived values as estimates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// Read directly in watts / watt-hours (power_now, energy_now).
    Direct,
    /// Derived from current × voltage or charge × voltage.
    Derived,
    /// Not available from this battery.
    Missing,
}

#[derive(Debug, Clone, Serialize)]
pub struct BatteryReading {
    pub name: String,
    pub status_raw: String,
    pub capacity_percent: Option<f64>,
    /// Instantaneous power through the battery in watts (always ≥ 0).
    pub power_w: Option<f64>,
    pub power_source: Source,
    pub energy_now_wh: Option<f64>,
    pub energy_full_wh: Option<f64>,
    pub energy_full_design_wh: Option<f64>,
    pub energy_source: Source,
    pub voltage_v: Option<f64>,
    pub cycle_count: Option<u64>,
    pub manufacturer: Option<String>,
    pub model: Option<String>,
}

impl BatteryReading {
    /// Battery health: full capacity relative to design capacity.
    pub fn health_percent(&self) -> Option<f64> {
        match (self.energy_full_wh, self.energy_full_design_wh) {
            (Some(f), Some(d)) if d > 0.0 => Some(f / d * 100.0),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PowerSnapshot {
    pub batteries: Vec<BatteryReading>,
    pub ac_online: Option<bool>,
    pub state: ChargeState,
    pub capacity_percent: Option<f64>,
    pub power_w: Option<f64>,
    pub energy_now_wh: Option<f64>,
    pub energy_full_wh: Option<f64>,
    /// True when any figure was computed rather than read directly.
    pub estimated: bool,
}

fn read_trim(dir: &Path, file: &str) -> Option<String> {
    fs::read_to_string(dir.join(file))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn read_i64(dir: &Path, file: &str) -> Option<i64> {
    read_trim(dir, file)?.parse::<i64>().ok()
}

/// µW → W. Some firmware reports a negative value while discharging.
pub fn microwatts_to_watts(uw: i64) -> f64 {
    (uw as f64).abs() / 1e6
}

/// µWh → Wh.
pub fn microwatt_hours_to_wh(uwh: i64) -> f64 {
    (uwh as f64).abs() / 1e6
}

/// µA × µV → W.
pub fn current_voltage_to_watts(ua: i64, uv: i64) -> f64 {
    ((ua as f64) * (uv as f64)).abs() / 1e12
}

/// µAh × µV → Wh.
pub fn charge_voltage_to_wh(uah: i64, uv: i64) -> f64 {
    ((uah as f64) * (uv as f64)).abs() / 1e12
}

/// Is this power_supply entry a system battery (not a mouse or headset)?
pub fn is_system_battery(dir: &Path) -> bool {
    let ty = read_trim(dir, "type").unwrap_or_default();
    if ty != "Battery" {
        return false;
    }
    // Peripheral batteries (hid, bluetooth) report scope=Device.
    match read_trim(dir, "scope") {
        Some(scope) => scope != "Device",
        None => true,
    }
}

/// Find all system batteries under a power_supply root, sorted by name.
pub fn discover_batteries(root: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| is_system_battery(p))
        .collect();
    out.sort();
    out
}

/// Is any mains/USB adapter online? None if no adapter is exposed.
pub fn ac_online(root: &Path) -> Option<bool> {
    let mut seen = false;
    for entry in fs::read_dir(root).into_iter().flatten().flatten() {
        let p = entry.path();
        let ty = read_trim(&p, "type").unwrap_or_default();
        if matches!(ty.as_str(), "Mains" | "USB" | "USB_C" | "USB_PD") {
            if let Some(v) = read_i64(&p, "online") {
                seen = true;
                if v == 1 {
                    return Some(true);
                }
            }
        }
    }
    seen.then_some(false)
}

pub fn read_battery(dir: &Path) -> BatteryReading {
    let name = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let voltage = read_i64(dir, "voltage_now");

    let (power_w, power_source) = if let Some(p) = read_i64(dir, "power_now") {
        (Some(microwatts_to_watts(p)), Source::Direct)
    } else if let (Some(i), Some(v)) = (read_i64(dir, "current_now"), voltage) {
        (Some(current_voltage_to_watts(i, v)), Source::Derived)
    } else {
        (None, Source::Missing)
    };

    // Energy: prefer energy_* (µWh); else charge_* (µAh) × voltage_now.
    // The same voltage converts now/full/design so their ratios stay equal
    // to the firmware's charge ratios (otherwise "now" could exceed "full").
    let nominal_v = voltage.or_else(|| read_i64(dir, "voltage_min_design"));
    let (energy_now, energy_full, energy_design, energy_source) =
        if let Some(e) = read_i64(dir, "energy_now") {
            (
                Some(microwatt_hours_to_wh(e)),
                read_i64(dir, "energy_full").map(microwatt_hours_to_wh),
                read_i64(dir, "energy_full_design").map(microwatt_hours_to_wh),
                Source::Direct,
            )
        } else if let (Some(c), Some(v)) = (read_i64(dir, "charge_now"), nominal_v) {
            let conv = |f: &str| match (read_i64(dir, f), nominal_v) {
                (Some(c), Some(v)) => Some(charge_voltage_to_wh(c, v)),
                _ => None,
            };
            (
                Some(charge_voltage_to_wh(c, v)),
                conv("charge_full"),
                conv("charge_full_design"),
                Source::Derived,
            )
        } else {
            (None, None, None, Source::Missing)
        };

    // capacity is the firmware's own percentage; fall back to energy ratio.
    let capacity = read_i64(dir, "capacity").map(|c| c as f64).or_else(|| {
        match (energy_now, energy_full) {
            (Some(n), Some(f)) if f > 0.0 => Some((n / f * 100.0).clamp(0.0, 100.0)),
            _ => None,
        }
    });

    BatteryReading {
        name,
        status_raw: read_trim(dir, "status").unwrap_or_else(|| "Unknown".into()),
        capacity_percent: capacity,
        power_w,
        power_source,
        energy_now_wh: energy_now,
        energy_full_wh: energy_full,
        energy_full_design_wh: energy_design,
        energy_source,
        voltage_v: voltage.map(|v| v as f64 / 1e6),
        cycle_count: read_i64(dir, "cycle_count").and_then(|c| u64::try_from(c).ok()).filter(|c| *c > 0),
        manufacturer: read_trim(dir, "manufacturer"),
        model: read_trim(dir, "model_name"),
    }
}

/// Combine per-battery status strings and adapter state into one state.
pub fn classify(statuses: &[&str], ac: Option<bool>) -> ChargeState {
    if statuses.is_empty() {
        return ChargeState::Unknown;
    }
    if statuses.iter().any(|s| *s == "Discharging") {
        // Some firmware says Discharging briefly after plugging in.
        return if ac == Some(true) { ChargeState::AcIdle } else { ChargeState::Discharging };
    }
    if statuses.iter().any(|s| *s == "Charging") {
        return ChargeState::Charging;
    }
    if statuses.iter().all(|s| *s == "Full") {
        return ChargeState::Full;
    }
    if statuses.iter().any(|s| *s == "Not charging") {
        return ChargeState::AcIdle;
    }
    match ac {
        Some(true) => ChargeState::AcIdle,
        Some(false) => ChargeState::Discharging,
        None => ChargeState::Unknown,
    }
}

pub fn snapshot(root: &Path) -> PowerSnapshot {
    let batteries: Vec<BatteryReading> =
        discover_batteries(root).iter().map(|d| read_battery(d)).collect();
    let ac = ac_online(root);
    let statuses: Vec<&str> = batteries.iter().map(|b| b.status_raw.as_str()).collect();
    let state = classify(&statuses, ac);

    let sum = |f: fn(&BatteryReading) -> Option<f64>| -> Option<f64> {
        let vals: Vec<f64> = batteries.iter().filter_map(f).collect();
        (!vals.is_empty()).then(|| vals.iter().sum())
    };
    let energy_now = sum(|b| b.energy_now_wh);
    let energy_full = sum(|b| b.energy_full_wh);
    let capacity = match (energy_now, energy_full) {
        (Some(n), Some(f)) if f > 0.0 && batteries.len() > 1 => Some((n / f * 100.0).clamp(0.0, 100.0)),
        _ => batteries.first().and_then(|b| b.capacity_percent),
    };
    let estimated = batteries
        .iter()
        .any(|b| b.power_source == Source::Derived || b.energy_source == Source::Derived);

    PowerSnapshot {
        power_w: sum(|b| b.power_w),
        energy_now_wh: energy_now,
        energy_full_wh: energy_full,
        capacity_percent: capacity,
        state,
        ac_online: ac,
        estimated,
        batteries,
    }
}

/// Hours of runtime left, or None when an estimate would be misleading
/// (charging, full, on AC, or no measurable draw).
pub fn runtime_hours(state: ChargeState, energy_now_wh: Option<f64>, smoothed_w: Option<f64>) -> Option<f64> {
    if state != ChargeState::Discharging {
        return None;
    }
    match (energy_now_wh, smoothed_w) {
        (Some(e), Some(w)) if w >= MIN_MEANINGFUL_WATTS && e > 0.0 => Some(e / w),
        _ => None,
    }
}

/// Hours until full while charging, or None.
pub fn time_to_full_hours(
    state: ChargeState,
    energy_now_wh: Option<f64>,
    energy_full_wh: Option<f64>,
    smoothed_w: Option<f64>,
) -> Option<f64> {
    if state != ChargeState::Charging {
        return None;
    }
    match (energy_now_wh, energy_full_wh, smoothed_w) {
        (Some(n), Some(f), Some(w)) if w >= MIN_MEANINGFUL_WATTS && f > n => Some((f - n) / w),
        _ => None,
    }
}
