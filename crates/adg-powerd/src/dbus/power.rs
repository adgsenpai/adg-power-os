//! Power profile, display brightness and keyboard backlight.
//!
//! All three go through system services that authorize the active session
//! themselves: power-profiles-daemon, systemd-logind and UPower.

use super::err_text;
use serde::Serialize;
use zbus::blocking::{Connection, Proxy};

const PROFILE_BUSES: [(&str, &str, &str); 2] = [
    ("org.freedesktop.UPower.PowerProfiles", "/org/freedesktop/UPower/PowerProfiles", "org.freedesktop.UPower.PowerProfiles"),
    ("net.hadess.PowerProfiles", "/net/hadess/PowerProfiles", "net.hadess.PowerProfiles"),
];

pub const PROFILES: [&str; 3] = ["power-saver", "balanced", "performance"];

#[derive(Debug, Clone, Serialize, Default)]
pub struct ProfileStatus {
    pub available: bool,
    pub active: Option<String>,
    pub choices: Vec<String>,
    pub degraded: Option<String>,
}

pub struct Power {
    conn: Connection,
}

impl Power {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    fn profiles_proxy(&self) -> Option<Proxy<'static>> {
        PROFILE_BUSES.iter().find_map(|(bus, path, iface)| {
            let p = Proxy::new(&self.conn, *bus, *path, *iface).ok()?;
            p.get_property::<String>("ActiveProfile").ok().map(|_| p)
        })
    }

    pub fn profile(&self) -> ProfileStatus {
        let Some(p) = self.profiles_proxy() else { return ProfileStatus::default() };
        let choices: Vec<std::collections::HashMap<String, zbus::zvariant::OwnedValue>> =
            p.get_property("Profiles").unwrap_or_default();
        ProfileStatus {
            available: true,
            active: p.get_property("ActiveProfile").ok(),
            choices: choices.iter().filter_map(|c| super::prop_str(c, "Profile")).collect(),
            degraded: p.get_property::<String>("PerformanceDegraded").ok().filter(|s| !s.is_empty()),
        }
    }

    pub fn set_profile(&self, name: &str) -> Result<(), String> {
        if !PROFILES.contains(&name) {
            return Err("Unknown profile".into());
        }
        let p = self.profiles_proxy().ok_or("power-profiles-daemon is not running")?;
        p.set_property("ActiveProfile", name).map_err(|e| err_text(e.into()))
    }

    /// Set backlight through logind so no root or udev rule is needed.
    pub fn set_brightness(&self, device: &str, value: u32) -> Result<(), String> {
        if !device.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
            return Err("Invalid backlight device".into());
        }
        let s = Proxy::new(
            &self.conn,
            "org.freedesktop.login1",
            "/org/freedesktop/login1/session/auto",
            "org.freedesktop.login1.Session",
        )
        .map_err(err_text)?;
        s.call::<_, _, ()>("SetBrightness", &("backlight", device, value)).map_err(err_text)
    }

    fn kbd(&self) -> Result<Proxy<'static>, String> {
        Proxy::new(
            &self.conn,
            "org.freedesktop.UPower",
            "/org/freedesktop/UPower/KbdBacklight",
            "org.freedesktop.UPower.KbdBacklight",
        )
        .map_err(err_text)
    }

    /// (current, max) or None when the laptop exposes no keyboard backlight.
    pub fn keyboard_backlight(&self) -> Option<(i32, i32)> {
        let k = self.kbd().ok()?;
        let max: i32 = k.call("GetMaxBrightness", &()).ok()?;
        let cur: i32 = k.call("GetBrightness", &()).ok()?;
        (max > 0).then_some((cur, max))
    }

    pub fn set_keyboard_backlight(&self, value: i32) -> Result<(), String> {
        let (_, max) = self.keyboard_backlight().ok_or("No keyboard backlight exposed")?;
        self.kbd()?.call::<_, _, ()>("SetBrightness", &(value.clamp(0, max),)).map_err(err_text)
    }
}
