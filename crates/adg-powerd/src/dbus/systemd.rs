//! systemd unit state and allowlisted start/stop through org.freedesktop.systemd1.
//!
//! Stopping a system unit triggers systemd's own polkit check
//! (org.freedesktop.systemd1.manage-units), so the user is prompted by their
//! polkit agent. Only units in the policy allowlist can be touched.

use super::err_text;
use serde::Serialize;
use zbus::blocking::{Connection, Proxy};
use zbus::proxy::MethodFlags;
use zbus::zvariant::OwnedObjectPath;

const SD: &str = "org.freedesktop.systemd1";
const SD_PATH: &str = "/org/freedesktop/systemd1";
const SD_MGR: &str = "org.freedesktop.systemd1.Manager";
const SD_UNIT: &str = "org.freedesktop.systemd1.Unit";

#[derive(Debug, Clone, Serialize)]
pub struct UnitState {
    pub name: String,
    pub load_state: String,
    pub active_state: String,
    pub sub_state: String,
    pub unit_file_state: String,
    pub description: String,
}

impl UnitState {
    pub fn installed(&self) -> bool {
        self.load_state == "loaded" || self.load_state == "masked"
    }
}

pub struct Systemd {
    conn: Connection,
}

/// Unit names are limited to systemd's own character set.
pub fn valid_unit_name(n: &str) -> bool {
    !n.is_empty()
        && n.len() <= 255
        && n.contains('.')
        && n.chars().all(|c| c.is_ascii_alphanumeric() || ":-_.\\@".contains(c))
}

impl Systemd {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    pub fn unit(&self, name: &str) -> Result<UnitState, String> {
        if !valid_unit_name(name) {
            return Err("Invalid unit name".into());
        }
        let mgr = Proxy::new(&self.conn, SD, SD_PATH, SD_MGR).map_err(err_text)?;
        let path: OwnedObjectPath = mgr.call("LoadUnit", &(name,)).map_err(err_text)?;
        let u = Proxy::new(&self.conn, SD, path, SD_UNIT).map_err(err_text)?;
        let s = |k: &str| u.get_property::<String>(k).unwrap_or_default();
        Ok(UnitState {
            name: name.to_string(),
            load_state: s("LoadState"),
            active_state: s("ActiveState"),
            sub_state: s("SubState"),
            unit_file_state: s("UnitFileState"),
            description: s("Description"),
        })
    }

    fn job(&self, method: &str, name: &str) -> Result<(), String> {
        if !valid_unit_name(name) {
            return Err("Invalid unit name".into());
        }
        let mgr = Proxy::new(&self.conn, SD, SD_PATH, SD_MGR).map_err(err_text)?;
        mgr.call_with_flags::<_, _, OwnedObjectPath>(
            method,
            MethodFlags::AllowInteractiveAuth.into(),
            &(name, "replace"),
        )
        .map(|_| ())
        .map_err(err_text)
    }

    pub fn start(&self, name: &str) -> Result<(), String> {
        self.job("StartUnit", name)
    }

    pub fn stop(&self, name: &str) -> Result<(), String> {
        self.job("StopUnit", name)
    }
}

#[cfg(test)]
mod tests {
    use super::valid_unit_name;

    #[test]
    fn unit_names() {
        assert!(valid_unit_name("docker.service"));
        assert!(valid_unit_name("postgresql@16-main.service"));
        assert!(!valid_unit_name("docker"));
        assert!(!valid_unit_name("docker.service; reboot"));
        assert!(!valid_unit_name("../etc/passwd.service x"));
    }
}
