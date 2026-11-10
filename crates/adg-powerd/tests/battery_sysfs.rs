//! Acceptance tests E and F: battery discovery and unit conversion against
//! synthetic sysfs trees, including BAT1-only machines and missing fields.

use adg_power::battery::*;
use adg_power::history::History;
use std::fs;
use std::path::Path;

fn write(dir: &Path, files: &[(&str, &str)]) {
    fs::create_dir_all(dir).unwrap();
    for (k, v) in files {
        fs::write(dir.join(k), format!("{v}\n")).unwrap();
    }
}

/// The real MSI Cyborg 15 layout: BAT1 with charge_* and current_now,
/// ADP1 adapter, and a Logitech mouse battery that must be ignored.
fn cyborg(root: &Path, status: &str, current_ua: &str, ac: &str) {
    write(
        &root.join("BAT1"),
        &[
            ("type", "Battery"),
            ("status", status),
            ("capacity", "87"),
            ("charge_full", "3392000"),
            ("charge_full_design", "3500000"),
            ("charge_now", "2951040"),
            ("current_now", current_ua),
            ("voltage_now", "17000000"),
            ("voltage_min_design", "15480000"),
            ("cycle_count", "0"),
            ("manufacturer", "MSI"),
        ],
    );
    write(&root.join("ADP1"), &[("type", "Mains"), ("online", ac)]);
    write(
        &root.join("hidpp_battery_5"),
        &[("type", "Battery"), ("scope", "Device"), ("status", "Discharging"), ("capacity", "40")],
    );
}

fn approx(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

#[test]
fn unit_conversions() {
    assert!(approx(microwatts_to_watts(9_800_000), 9.8));
    assert!(approx(microwatts_to_watts(-9_800_000), 9.8));
    assert!(approx(microwatt_hours_to_wh(52_000_000), 52.0));
    // 0.5 A at 17 V = 8.5 W
    assert!(approx(current_voltage_to_watts(500_000, 17_000_000), 8.5));
    // 3.392 Ah at 15.48 V = 52.508 Wh
    assert!(approx(charge_voltage_to_wh(3_392_000, 15_480_000), 52.50816));
}

#[test]
fn discovers_bat1_without_bat0_and_skips_peripherals() {
    let d = tempfile::tempdir().unwrap();
    cyborg(d.path(), "Discharging", "576470", "0");
    let found = discover_batteries(d.path());
    assert_eq!(found.len(), 1);
    assert!(found[0].ends_with("BAT1"));
}

#[test]
fn derived_power_and_energy_on_bat1() {
    let d = tempfile::tempdir().unwrap();
    cyborg(d.path(), "Discharging", "576470", "0");
    let s = snapshot(d.path());
    assert_eq!(s.state, ChargeState::Discharging);
    assert_eq!(s.ac_online, Some(false));
    assert!(s.estimated);
    let b = &s.batteries[0];
    assert_eq!(b.power_source, Source::Derived);
    // 0.57647 A × 17 V ≈ 9.8 W
    assert!((b.power_w.unwrap() - 9.79999).abs() < 1e-3);
    // 2.95104 Ah × 17 V ≈ 50.17 Wh
    assert!((b.energy_now_wh.unwrap() - 50.16768).abs() < 1e-4);
    // Full uses the same voltage: 3.392 Ah × 17 V
    assert!((b.energy_full_wh.unwrap() - 57.664).abs() < 1e-4);
    assert!(b.energy_now_wh.unwrap() <= b.energy_full_wh.unwrap());
    assert!((b.health_percent().unwrap() - 96.914).abs() < 0.01);
    assert_eq!(b.cycle_count, None, "zero cycle count means unreported");
    let rt = runtime_hours(s.state, s.energy_now_wh, s.power_w).unwrap();
    assert!((rt - 5.119).abs() < 0.01);
}

#[test]
fn direct_power_now_and_energy_now() {
    let d = tempfile::tempdir().unwrap();
    write(
        &d.path().join("BAT0"),
        &[
            ("type", "Battery"),
            ("status", "Discharging"),
            ("power_now", "7500000"),
            ("energy_now", "30000000"),
            ("energy_full", "60000000"),
            ("energy_full_design", "70000000"),
            ("voltage_now", "11400000"),
        ],
    );
    let s = snapshot(d.path());
    let b = &s.batteries[0];
    assert_eq!(b.power_source, Source::Direct);
    assert_eq!(b.energy_source, Source::Direct);
    assert!(!s.estimated);
    assert!(approx(b.power_w.unwrap(), 7.5));
    assert!(approx(b.energy_now_wh.unwrap(), 30.0));
    // capacity file missing: falls back to energy ratio
    assert!(approx(b.capacity_percent.unwrap(), 50.0));
    assert!(approx(runtime_hours(s.state, s.energy_now_wh, Some(7.5)).unwrap(), 4.0));
}

#[test]
fn missing_sensor_fields_do_not_panic() {
    let d = tempfile::tempdir().unwrap();
    // No current, no voltage, no charge: only type and status.
    write(&d.path().join("BAT2"), &[("type", "Battery"), ("status", "Unknown")]);
    let s = snapshot(d.path());
    let b = &s.batteries[0];
    assert_eq!(b.power_w, None);
    assert_eq!(b.power_source, Source::Missing);
    assert_eq!(b.energy_now_wh, None);
    assert_eq!(b.capacity_percent, None);
    assert_eq!(s.state, ChargeState::Unknown);
    assert_eq!(runtime_hours(s.state, s.energy_now_wh, s.power_w), None);
}

#[test]
fn current_without_voltage_is_missing_not_zero() {
    let d = tempfile::tempdir().unwrap();
    write(&d.path().join("BAT1"), &[("type", "Battery"), ("status", "Discharging"), ("current_now", "500000")]);
    let b = &snapshot(d.path()).batteries[0];
    assert_eq!(b.power_w, None);
}

#[test]
fn no_battery_at_all() {
    let d = tempfile::tempdir().unwrap();
    write(&d.path().join("AC"), &[("type", "Mains"), ("online", "1")]);
    let s = snapshot(d.path());
    assert!(s.batteries.is_empty());
    assert_eq!(s.state, ChargeState::Unknown);
    assert_eq!(s.power_w, None);
}

#[test]
fn runtime_hidden_while_charging_full_or_zero_draw() {
    let d = tempfile::tempdir().unwrap();

    cyborg(d.path(), "Charging", "1500000", "1");
    let s = snapshot(d.path());
    assert_eq!(s.state, ChargeState::Charging);
    assert_eq!(runtime_hours(s.state, s.energy_now_wh, s.power_w), None);
    assert!(time_to_full_hours(s.state, s.energy_now_wh, s.energy_full_wh, s.power_w).is_some());

    cyborg(d.path(), "Full", "0", "1");
    let s = snapshot(d.path());
    assert_eq!(s.state, ChargeState::Full);
    assert_eq!(runtime_hours(s.state, s.energy_now_wh, s.power_w), None);

    // Discharging but the sensor reads zero: no fake "infinite" runtime.
    cyborg(d.path(), "Discharging", "0", "0");
    let s = snapshot(d.path());
    assert_eq!(s.state, ChargeState::Discharging);
    assert_eq!(runtime_hours(s.state, s.energy_now_wh, s.power_w), None);

    // On AC with a charge limit hold.
    cyborg(d.path(), "Not charging", "0", "1");
    assert_eq!(snapshot(d.path()).state, ChargeState::AcIdle);
}

#[test]
fn classify_edge_cases() {
    assert_eq!(classify(&["Discharging"], Some(true)), ChargeState::AcIdle);
    assert_eq!(classify(&["Unknown"], Some(false)), ChargeState::Discharging);
    assert_eq!(classify(&["Unknown"], None), ChargeState::Unknown);
    assert_eq!(classify(&["Full", "Charging"], Some(true)), ChargeState::Charging);
    assert_eq!(classify(&[], Some(true)), ChargeState::Unknown);
}

#[test]
fn two_batteries_are_summed() {
    let d = tempfile::tempdir().unwrap();
    for (n, e) in [("BAT0", "20000000"), ("BAT1", "10000000")] {
        write(
            &d.path().join(n),
            &[
                ("type", "Battery"),
                ("status", "Discharging"),
                ("power_now", "3000000"),
                ("energy_now", e),
                ("energy_full", "40000000"),
            ],
        );
    }
    let s = snapshot(d.path());
    assert_eq!(s.batteries.len(), 2);
    assert!(approx(s.power_w.unwrap(), 6.0));
    assert!(approx(s.energy_now_wh.unwrap(), 30.0));
    assert!(approx(s.capacity_percent.unwrap(), 37.5));
}

#[test]
fn smoothing_and_window_stats() {
    let mut h = History::new(4, 0.5);
    h.push(1, Some(10.0), None, true);
    h.push(2, Some(20.0), None, true);
    assert!(approx(h.smoothed().unwrap(), 15.0));
    h.push(3, None, None, true); // missing reading keeps the previous estimate
    assert!(approx(h.smoothed().unwrap(), 15.0));
    h.push(4, Some(8.0), None, true);
    h.push(5, Some(12.0), None, true); // evicts t=1
    assert_eq!(h.len(), 4);
    let w = h.stats_since(0);
    assert_eq!(w.samples, 3);
    assert!(approx(w.min_w.unwrap(), 8.0));
    assert!(approx(w.max_w.unwrap(), 20.0));
    // Plugging in resets smoothing, and AC readings stay out of the stats.
    h.push(6, Some(30.0), None, false);
    assert!(approx(h.smoothed().unwrap(), 30.0));
    assert!(approx(h.stats_since(0).max_w.unwrap(), 12.0));
}
