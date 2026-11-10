//! NetworkManager over D-Bus: scan, list, connect, disconnect.
//!
//! SSIDs and passphrases travel only as D-Bus values. They are never placed on
//! a command line and passphrases are never logged or echoed back.

use super::{err_text, owned, Props};
use serde::Serialize;
use std::collections::HashMap;
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

const NM: &str = "org.freedesktop.NetworkManager";
const NM_PATH: &str = "/org/freedesktop/NetworkManager";
const DEV_IFACE: &str = "org.freedesktop.NetworkManager.Device";
const WIFI_IFACE: &str = "org.freedesktop.NetworkManager.Device.Wireless";
const AP_IFACE: &str = "org.freedesktop.NetworkManager.AccessPoint";
const NM_DEVICE_TYPE_WIFI: u32 = 2;

// NM 802.11 AP flags.
const AP_FLAGS_PRIVACY: u32 = 0x1;
const SEC_KEY_MGMT_PSK: u32 = 0x100;
const SEC_KEY_MGMT_802_1X: u32 = 0x200;
const SEC_KEY_MGMT_SAE: u32 = 0x400;
const SEC_KEY_MGMT_OWE: u32 = 0x800;

#[derive(Debug, Clone, Serialize)]
pub struct AccessPoint {
    pub ssid: String,
    pub bssid: String,
    pub strength: u8,
    pub frequency_mhz: u32,
    pub security: String,
    pub active: bool,
    pub known: bool,
    #[serde(skip)]
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct WifiStatus {
    pub available: bool,
    pub enabled: bool,
    pub interface: Option<String>,
    pub ssid: Option<String>,
    pub strength: Option<u8>,
    pub ipv4: Option<String>,
    pub state: String,
    pub error: Option<String>,
}

/// Describe an AP's security from NM flags. Pure so it can be tested.
pub fn security_label(flags: u32, wpa: u32, rsn: u32) -> String {
    let all = wpa | rsn;
    if all & SEC_KEY_MGMT_802_1X != 0 {
        "Enterprise".into()
    } else if all & SEC_KEY_MGMT_SAE != 0 {
        if all & SEC_KEY_MGMT_PSK != 0 { "WPA2/WPA3".into() } else { "WPA3".into() }
    } else if rsn & SEC_KEY_MGMT_PSK != 0 {
        "WPA2".into()
    } else if wpa & SEC_KEY_MGMT_PSK != 0 {
        "WPA".into()
    } else if all & SEC_KEY_MGMT_OWE != 0 {
        "OWE".into()
    } else if flags & AP_FLAGS_PRIVACY != 0 {
        "WEP".into()
    } else {
        "Open".into()
    }
}

fn device_state_name(s: u32) -> &'static str {
    match s {
        10 => "unmanaged",
        20 => "unavailable",
        30 => "disconnected",
        40..=90 => "connecting",
        100 => "connected",
        110 => "deactivating",
        120 => "failed",
        _ => "unknown",
    }
}

pub struct Network {
    conn: Connection,
}

impl Network {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    fn proxy(&self, path: &str, iface: &'static str) -> zbus::Result<Proxy<'static>> {
        Proxy::new(&self.conn, NM, path.to_owned(), iface)
    }

    fn wifi_device(&self) -> zbus::Result<Option<OwnedObjectPath>> {
        let nm = self.proxy(NM_PATH, NM)?;
        let devices: Vec<OwnedObjectPath> = nm.call("GetDevices", &())?;
        for d in devices {
            let dev = self.proxy(d.as_str(), DEV_IFACE)?;
            if dev.get_property::<u32>("DeviceType")? == NM_DEVICE_TYPE_WIFI {
                return Ok(Some(d));
            }
        }
        Ok(None)
    }

    pub fn status(&self) -> WifiStatus {
        self.status_inner().unwrap_or_else(|e| WifiStatus {
            error: Some(err_text(e)),
            state: "unknown".into(),
            ..Default::default()
        })
    }

    fn status_inner(&self) -> zbus::Result<WifiStatus> {
        let nm = self.proxy(NM_PATH, NM)?;
        let enabled: bool = nm.get_property("WirelessEnabled")?;
        let Some(dev_path) = self.wifi_device()? else {
            return Ok(WifiStatus { enabled, state: "no-device".into(), ..Default::default() });
        };
        let dev = self.proxy(dev_path.as_str(), DEV_IFACE)?;
        let wifi = self.proxy(dev_path.as_str(), WIFI_IFACE)?;
        let state: u32 = dev.get_property("State")?;
        let ap: OwnedObjectPath = wifi.get_property("ActiveAccessPoint")?;
        let (ssid, strength) = if ap.as_str() != "/" {
            let ap = self.proxy(ap.as_str(), AP_IFACE)?;
            let raw: Vec<u8> = ap.get_property("Ssid")?;
            (Some(String::from_utf8_lossy(&raw).into_owned()), Some(ap.get_property::<u8>("Strength")?))
        } else {
            (None, None)
        };
        Ok(WifiStatus {
            available: true,
            enabled,
            interface: dev.get_property::<String>("Interface").ok(),
            ssid,
            strength,
            ipv4: self.ipv4(&dev).ok().flatten(),
            state: device_state_name(state).into(),
            error: None,
        })
    }

    fn ipv4(&self, dev: &Proxy) -> zbus::Result<Option<String>> {
        let cfg: OwnedObjectPath = dev.get_property("Ip4Config")?;
        if cfg.as_str() == "/" {
            return Ok(None);
        }
        let ip = self.proxy(cfg.as_str(), "org.freedesktop.NetworkManager.IP4Config")?;
        let data: Vec<Props> = ip.get_property("AddressData")?;
        Ok(data.first().and_then(|d| {
            let a = super::prop_str(d, "address")?;
            let p = super::prop_u32(d, "prefix")?;
            Some(format!("{a}/{p}"))
        }))
    }

    pub fn set_enabled(&self, on: bool) -> Result<(), String> {
        let nm = self.proxy(NM_PATH, NM).map_err(err_text)?;
        nm.set_property("WirelessEnabled", on).map_err(|e| err_text(e.into()))
    }

    pub fn request_scan(&self) -> Result<(), String> {
        let dev = self.wifi_device().map_err(err_text)?.ok_or("No Wi-Fi device")?;
        let wifi = self.proxy(dev.as_str(), WIFI_IFACE).map_err(err_text)?;
        let opts: HashMap<String, OwnedValue> = HashMap::new();
        match wifi.call::<_, _, ()>("RequestScan", &(opts,)) {
            Ok(()) => Ok(()),
            // NM rate-limits scans; an existing recent scan is fine.
            Err(e) => {
                let t = err_text(e);
                if t.contains("NotAllowed") || t.contains("rate") { Ok(()) } else { Err(t) }
            }
        }
    }

    fn known_ssids(&self) -> zbus::Result<HashMap<Vec<u8>, OwnedObjectPath>> {
        let settings = self.proxy("/org/freedesktop/NetworkManager/Settings", "org.freedesktop.NetworkManager.Settings")?;
        let conns: Vec<OwnedObjectPath> = settings.call("ListConnections", &())?;
        let mut out = HashMap::new();
        for c in conns {
            let p = self.proxy(c.as_str(), "org.freedesktop.NetworkManager.Settings.Connection")?;
            let s: HashMap<String, Props> = match p.call("GetSettings", &()) {
                Ok(s) => s,
                Err(_) => continue,
            };
            if let Some(ssid) = s
                .get("802-11-wireless")
                .and_then(|w| w.get("ssid"))
                .and_then(|v| Vec::<u8>::try_from(v.try_clone().ok()?).ok())
            {
                out.insert(ssid, c);
            }
        }
        Ok(out)
    }

    pub fn access_points(&self) -> Result<Vec<AccessPoint>, String> {
        let dev = self.wifi_device().map_err(err_text)?.ok_or("No Wi-Fi device")?;
        let wifi = self.proxy(dev.as_str(), WIFI_IFACE).map_err(err_text)?;
        let active: OwnedObjectPath = wifi.get_property("ActiveAccessPoint").map_err(|e| err_text(e.into()))?;
        let aps: Vec<OwnedObjectPath> = wifi.call("GetAllAccessPoints", &()).map_err(err_text)?;
        let known = self.known_ssids().unwrap_or_default();
        let mut best: HashMap<String, AccessPoint> = HashMap::new();
        for path in aps {
            let Ok(ap) = self.proxy(path.as_str(), AP_IFACE) else { continue };
            let Ok(raw) = ap.get_property::<Vec<u8>>("Ssid") else { continue };
            if raw.is_empty() {
                continue; // hidden network
            }
            let ssid = String::from_utf8_lossy(&raw).into_owned();
            let g = |k: &str| ap.get_property::<u32>(k).unwrap_or(0);
            let item = AccessPoint {
                strength: ap.get_property::<u8>("Strength").unwrap_or(0),
                bssid: ap.get_property::<String>("HwAddress").unwrap_or_default(),
                frequency_mhz: g("Frequency"),
                security: security_label(g("Flags"), g("WpaFlags"), g("RsnFlags")),
                active: path == active,
                known: known.contains_key(&raw),
                path: path.to_string(),
                ssid: ssid.clone(),
            };
            // Keep the strongest BSSID per SSID, but never hide the active one.
            match best.get(&ssid) {
                Some(cur) if cur.active || (cur.strength >= item.strength && !item.active) => {}
                _ => {
                    best.insert(ssid, item);
                }
            }
        }
        let mut v: Vec<AccessPoint> = best.into_values().collect();
        v.sort_by(|a, b| b.active.cmp(&a.active).then(b.strength.cmp(&a.strength)));
        Ok(v)
    }

    /// Connect to an SSID. Uses a saved profile when one exists; otherwise
    /// creates one with the given passphrase (stored by NetworkManager).
    pub fn connect(&self, ssid: &str, passphrase: Option<&str>) -> Result<(), String> {
        let dev = self.wifi_device().map_err(err_text)?.ok_or("No Wi-Fi device")?;
        let aps = self.access_points()?;
        let ap = aps.iter().find(|a| a.ssid == ssid).ok_or("Network not in range; scan again")?;
        let ap_path = OwnedObjectPath::try_from(ap.path.clone()).map_err(|e| e.to_string())?;
        let nm = self.proxy(NM_PATH, NM).map_err(err_text)?;

        let known = self.known_ssids().unwrap_or_default();
        if let (Some(conn), None) = (known.get(ssid.as_bytes()), passphrase) {
            nm.call::<_, _, OwnedObjectPath>("ActivateConnection", &(conn, &dev, &ap_path))
                .map_err(err_text)?;
            return Ok(());
        }

        let mut settings: HashMap<&str, HashMap<&str, OwnedValue>> = HashMap::new();
        if ap.security != "Open" {
            let psk = passphrase.ok_or("This network needs a password")?;
            if ap.security == "Enterprise" {
                return Err("Enterprise networks need nm-connection-editor".into());
            }
            let key_mgmt = if ap.security == "WPA3" { "sae" } else { "wpa-psk" };
            let mut sec = HashMap::new();
            sec.insert("key-mgmt", owned(key_mgmt));
            sec.insert("psk", owned(psk));
            settings.insert("802-11-wireless-security", sec);
        }
        nm.call::<_, _, (OwnedObjectPath, OwnedObjectPath)>(
            "AddAndActivateConnection",
            &(settings, &dev, &ap_path),
        )
        .map_err(err_text)?;
        Ok(())
    }

    pub fn disconnect(&self) -> Result<(), String> {
        let dev = self.wifi_device().map_err(err_text)?.ok_or("No Wi-Fi device")?;
        let d = self.proxy(dev.as_str(), DEV_IFACE).map_err(err_text)?;
        d.call::<_, _, ()>("Disconnect", &()).map_err(err_text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn security_labels() {
        assert_eq!(security_label(0, 0, 0), "Open");
        assert_eq!(security_label(1, 0, 0), "WEP");
        assert_eq!(security_label(1, 0, SEC_KEY_MGMT_PSK), "WPA2");
        assert_eq!(security_label(1, SEC_KEY_MGMT_PSK, 0), "WPA");
        assert_eq!(security_label(1, 0, SEC_KEY_MGMT_SAE), "WPA3");
        assert_eq!(security_label(1, 0, SEC_KEY_MGMT_SAE | SEC_KEY_MGMT_PSK), "WPA2/WPA3");
        assert_eq!(security_label(1, 0, SEC_KEY_MGMT_802_1X), "Enterprise");
    }
}
