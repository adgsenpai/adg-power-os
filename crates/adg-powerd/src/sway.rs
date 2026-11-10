//! Prefer a 60 Hz mode on the internal panel under Sway, with rollback.

use serde::Deserialize;
use std::process::Command;

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct Mode {
    pub width: u32,
    pub height: u32,
    /// millihertz
    pub refresh: u32,
}

#[derive(Debug, Deserialize)]
pub struct Output {
    pub name: String,
    #[serde(default)]
    pub active: bool,
    pub current_mode: Option<Mode>,
    #[serde(default)]
    pub modes: Vec<Mode>,
}

fn valid_output(n: &str) -> bool {
    !n.is_empty() && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// The ~60 Hz mode at the current resolution, if lower than the current rate.
pub fn sixty_hz_for(o: &Output) -> Option<Mode> {
    let cur = o.current_mode.as_ref()?;
    if cur.refresh <= 61_000 {
        return None;
    }
    o.modes
        .iter()
        .filter(|m| m.width == cur.width && m.height == cur.height)
        .filter(|m| (59_000..=61_000).contains(&m.refresh))
        .max_by_key(|m| m.refresh)
        .cloned()
}

fn mode_arg(m: &Mode) -> String {
    format!("{}x{}@{:.3}Hz", m.width, m.height, m.refresh as f64 / 1000.0)
}

fn outputs() -> Result<Vec<Output>, String> {
    let o = Command::new("swaymsg").args(["-r", "-t", "get_outputs"]).output().map_err(|e| e.to_string())?;
    serde_json::from_slice(&o.stdout).map_err(|e| e.to_string())
}

fn set_mode(name: &str, m: &Mode) -> Result<(), String> {
    if !valid_output(name) {
        return Err("Invalid output name".into());
    }
    let s = Command::new("swaymsg").args(["output", name, "mode", &mode_arg(m)]).status().map_err(|e| e.to_string())?;
    s.success().then_some(()).ok_or_else(|| "swaymsg failed".into())
}

fn saved_path() -> std::path::PathBuf {
    crate::policy::state_dir().join("display-modes.json")
}

pub fn prefer_60hz() -> Result<(), String> {
    let mut saved: Vec<(String, Mode)> = Vec::new();
    for o in outputs()?.into_iter().filter(|o| o.active) {
        if let (Some(m), Some(cur)) = (sixty_hz_for(&o), o.current_mode.clone()) {
            set_mode(&o.name, &m)?;
            saved.push((o.name, cur));
        }
    }
    if saved.is_empty() {
        return Ok(());
    }
    let json: Vec<_> = saved
        .iter()
        .map(|(n, m)| serde_json::json!({"name": n, "width": m.width, "height": m.height, "refresh": m.refresh}))
        .collect();
    std::fs::create_dir_all(crate::policy::state_dir()).map_err(|e| e.to_string())?;
    std::fs::write(saved_path(), serde_json::to_vec(&json).unwrap()).map_err(|e| e.to_string())
}

pub fn restore_mode() -> Result<(), String> {
    let Ok(bytes) = std::fs::read(saved_path()) else { return Ok(()) };
    #[derive(Deserialize)]
    struct Saved {
        name: String,
        #[serde(flatten)]
        mode: Mode,
    }
    let list: Vec<Saved> = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    for s in list {
        set_mode(&s.name, &s.mode)?;
    }
    let _ = std::fs::remove_file(saved_path());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(w: u32, h: u32, r: u32) -> Mode {
        Mode { width: w, height: h, refresh: r }
    }

    #[test]
    fn picks_60hz_at_same_resolution() {
        let o = Output {
            name: "eDP-1".into(),
            active: true,
            current_mode: Some(m(1920, 1080, 144_000)),
            modes: vec![m(1920, 1080, 144_000), m(1920, 1080, 60_010), m(1280, 720, 60_000)],
        };
        assert_eq!(sixty_hz_for(&o), Some(m(1920, 1080, 60_010)));
        assert_eq!(mode_arg(&m(1920, 1080, 60_010)), "1920x1080@60.010Hz");
    }

    #[test]
    fn leaves_60hz_alone() {
        let o = Output { name: "eDP-1".into(), active: true, current_mode: Some(m(1920, 1080, 60_000)), modes: vec![] };
        assert_eq!(sixty_hz_for(&o), None);
        assert!(!valid_output("eDP-1; exit"));
    }
}
