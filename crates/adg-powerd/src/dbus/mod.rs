//! D-Bus adapters. Every privileged operation goes through the service's own
//! polkit check (NetworkManager, BlueZ, systemd, power-profiles-daemon); this
//! daemon never runs as root and never spawns a shell.

pub mod audio;
pub mod bluez;
pub mod network;
pub mod power;
pub mod systemd;

use zbus::zvariant::{OwnedValue, Value};

pub type Props = std::collections::HashMap<String, OwnedValue>;

pub fn prop_str(p: &Props, k: &str) -> Option<String> {
    p.get(k).and_then(|v| v.downcast_ref::<&str>().ok().map(str::to_owned))
}

pub fn prop_bool(p: &Props, k: &str) -> Option<bool> {
    p.get(k).and_then(|v| v.downcast_ref::<bool>().ok())
}

pub fn prop_i16(p: &Props, k: &str) -> Option<i16> {
    p.get(k).and_then(|v| v.downcast_ref::<i16>().ok())
}

pub fn prop_u32(p: &Props, k: &str) -> Option<u32> {
    p.get(k).and_then(|v| v.downcast_ref::<u32>().ok())
}

pub fn owned<'a, T: Into<Value<'a>>>(v: T) -> OwnedValue {
    v.into().try_to_owned().expect("plain values are always ownable")
}

/// Human-readable error for the UI, without leaking request bodies.
pub fn err_text(e: zbus::Error) -> String {
    match e {
        zbus::Error::MethodError(name, msg, _) => {
            let short = name.as_str().rsplit('.').next().unwrap_or("Error").to_string();
            match msg {
                Some(m) => format!("{short}: {m}"),
                None => short,
            }
        }
        zbus::Error::FDO(fdo) => fdo.to_string(),
        other => other.to_string(),
    }
}
