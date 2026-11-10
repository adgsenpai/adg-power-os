//! BlueZ over D-Bus: power, discovery, pair, trust, connect.
//!
//! A pairing agent with NoInputNoOutput capability is registered so headphones
//! can pair without a desktop agent. It only accepts requests while the user
//! has an explicit pairing window open from the dashboard.

use super::{err_text, prop_bool, prop_i16, prop_str, Props};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{ObjectPath, OwnedObjectPath};

const BLUEZ: &str = "org.bluez";
const ADAPTER: &str = "org.bluez.Adapter1";
const DEVICE: &str = "org.bluez.Device1";
const AGENT_PATH: &str = "/com/adgstudios/power/agent";
const PAIRING_WINDOW_SECS: u64 = 60;

#[derive(Debug, Clone, Serialize)]
pub struct BtDevice {
    pub address: String,
    pub name: String,
    pub icon: Option<String>,
    pub paired: bool,
    pub trusted: bool,
    pub connected: bool,
    pub rssi: Option<i16>,
    pub battery_percent: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct BtStatus {
    pub available: bool,
    pub powered: bool,
    pub discovering: bool,
    pub adapter: Option<String>,
    pub devices: Vec<BtDevice>,
    pub error: Option<String>,
}

type Managed = HashMap<OwnedObjectPath, HashMap<String, Props>>;

/// Valid Bluetooth MAC: six colon-separated hex pairs.
pub fn valid_address(a: &str) -> bool {
    let parts: Vec<&str> = a.split(':').collect();
    parts.len() == 6 && parts.iter().all(|p| p.len() == 2 && p.chars().all(|c| c.is_ascii_hexdigit()))
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Shared "pairing allowed until" timestamp.
#[derive(Clone, Default)]
pub struct PairingWindow(Arc<AtomicU64>);

impl PairingWindow {
    pub fn open(&self) {
        self.0.store(now() + PAIRING_WINDOW_SECS, Ordering::SeqCst);
    }
    pub fn close(&self) {
        self.0.store(0, Ordering::SeqCst);
    }
    pub fn is_open(&self) -> bool {
        now() < self.0.load(Ordering::SeqCst)
    }
}

struct Agent {
    window: PairingWindow,
}

fn rejected() -> zbus::fdo::Error {
    zbus::fdo::Error::Failed("org.bluez.Error.Rejected".into())
}

#[zbus::interface(name = "org.bluez.Agent1")]
impl Agent {
    fn release(&self) {}
    fn request_pin_code(&self, _device: ObjectPath<'_>) -> zbus::fdo::Result<String> {
        if self.window.is_open() { Ok("0000".into()) } else { Err(rejected()) }
    }
    fn display_pin_code(&self, _device: ObjectPath<'_>, _pincode: String) {}
    fn request_passkey(&self, _device: ObjectPath<'_>) -> zbus::fdo::Result<u32> {
        if self.window.is_open() { Ok(0) } else { Err(rejected()) }
    }
    fn display_passkey(&self, _device: ObjectPath<'_>, _passkey: u32, _entered: u16) {}
    fn request_confirmation(&self, _device: ObjectPath<'_>, _passkey: u32) -> zbus::fdo::Result<()> {
        if self.window.is_open() { Ok(()) } else { Err(rejected()) }
    }
    fn request_authorization(&self, _device: ObjectPath<'_>) -> zbus::fdo::Result<()> {
        if self.window.is_open() { Ok(()) } else { Err(rejected()) }
    }
    fn authorize_service(&self, _device: ObjectPath<'_>, _uuid: String) -> zbus::fdo::Result<()> {
        // Services on already-paired devices (A2DP reconnect) are allowed by
        // BlueZ itself for trusted devices; we only see untrusted ones here.
        if self.window.is_open() { Ok(()) } else { Err(rejected()) }
    }
    fn cancel(&self) {}
}

pub struct Bluetooth {
    conn: Connection,
    pub window: PairingWindow,
}

impl Bluetooth {
    pub fn new(conn: Connection) -> Self {
        Self { conn, window: PairingWindow::default() }
    }

    /// Register our agent. Failure is non-fatal (another agent may own pairing).
    pub fn register_agent(&self) -> Result<(), String> {
        self.conn
            .object_server()
            .at(AGENT_PATH, Agent { window: self.window.clone() })
            .map_err(err_text)?;
        let mgr = Proxy::new(&self.conn, BLUEZ, "/org/bluez", "org.bluez.AgentManager1").map_err(err_text)?;
        let path = ObjectPath::try_from(AGENT_PATH).map_err(|e| e.to_string())?;
        mgr.call::<_, _, ()>("RegisterAgent", &(&path, "NoInputNoOutput")).map_err(err_text)?;
        // Become default only if nobody else is; ignore failure.
        let _ = mgr.call::<_, _, ()>("RequestDefaultAgent", &(&path,));
        Ok(())
    }

    fn managed(&self) -> zbus::Result<Managed> {
        let om = Proxy::new(&self.conn, BLUEZ, "/", "org.freedesktop.DBus.ObjectManager")?;
        om.call("GetManagedObjects", &())
    }

    fn adapter_path(m: &Managed) -> Option<OwnedObjectPath> {
        let mut v: Vec<&OwnedObjectPath> = m.iter().filter(|(_, i)| i.contains_key(ADAPTER)).map(|(p, _)| p).collect();
        v.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        v.first().map(|p| (*p).clone())
    }

    pub fn status(&self) -> BtStatus {
        match self.status_inner() {
            Ok(s) => s,
            Err(e) => BtStatus { error: Some(err_text(e)), ..Default::default() },
        }
    }

    fn status_inner(&self) -> zbus::Result<BtStatus> {
        let m = self.managed()?;
        let Some(ap) = Self::adapter_path(&m) else {
            return Ok(BtStatus::default());
        };
        let a = &m[&ap][ADAPTER];
        let mut devices: Vec<BtDevice> = m
            .iter()
            .filter(|(p, _)| p.as_str().starts_with(ap.as_str()))
            .filter_map(|(_, ifaces)| {
                let d = ifaces.get(DEVICE)?;
                let address = prop_str(d, "Address")?;
                let battery = ifaces
                    .get("org.bluez.Battery1")
                    .and_then(|b| b.get("Percentage"))
                    .and_then(|v| v.downcast_ref::<u8>().ok());
                Some(BtDevice {
                    name: prop_str(d, "Alias").or_else(|| prop_str(d, "Name")).unwrap_or_else(|| address.clone()),
                    icon: prop_str(d, "Icon"),
                    paired: prop_bool(d, "Paired").unwrap_or(false),
                    trusted: prop_bool(d, "Trusted").unwrap_or(false),
                    connected: prop_bool(d, "Connected").unwrap_or(false),
                    rssi: prop_i16(d, "RSSI"),
                    battery_percent: battery,
                    address,
                })
            })
            // Unnamed, unpaired beacons clutter the list.
            .filter(|d| d.paired || d.name != d.address)
            .collect();
        devices.sort_by(|a, b| {
            b.connected.cmp(&a.connected).then(b.paired.cmp(&a.paired)).then(b.rssi.cmp(&a.rssi))
        });
        Ok(BtStatus {
            available: true,
            powered: prop_bool(a, "Powered").unwrap_or(false),
            discovering: prop_bool(a, "Discovering").unwrap_or(false),
            adapter: prop_str(a, "Alias"),
            devices,
            error: None,
        })
    }

    fn adapter(&self) -> Result<Proxy<'static>, String> {
        let m = self.managed().map_err(err_text)?;
        let p = Self::adapter_path(&m).ok_or("No Bluetooth adapter")?;
        Proxy::new(&self.conn, BLUEZ, p, ADAPTER).map_err(err_text)
    }

    fn device(&self, address: &str) -> Result<Proxy<'static>, String> {
        if !valid_address(address) {
            return Err("Invalid device address".into());
        }
        let m = self.managed().map_err(err_text)?;
        let path = m
            .iter()
            .find(|(_, i)| i.get(DEVICE).and_then(|d| prop_str(d, "Address")).as_deref() == Some(address))
            .map(|(p, _)| p.clone())
            .ok_or("Device not found")?;
        Proxy::new(&self.conn, BLUEZ, path, DEVICE).map_err(err_text)
    }

    pub fn set_powered(&self, on: bool) -> Result<(), String> {
        self.adapter()?.set_property("Powered", on).map_err(|e| err_text(e.into()))
    }

    pub fn discover(&self, on: bool) -> Result<(), String> {
        let a = self.adapter()?;
        if on {
            self.window.open();
            a.call::<_, _, ()>("StartDiscovery", &()).map_err(err_text)
        } else {
            a.call::<_, _, ()>("StopDiscovery", &()).map_err(err_text)
        }
    }

    pub fn pair(&self, address: &str) -> Result<(), String> {
        let d = self.device(address)?;
        self.window.open();
        let r = d.call::<_, _, ()>("Pair", &()).map_err(err_text);
        self.window.close();
        match r {
            Ok(()) => {}
            Err(e) if e.contains("AlreadyExists") => {}
            Err(e) => return Err(e),
        }
        d.set_property("Trusted", true).map_err(|e| err_text(e.into()))?;
        d.call::<_, _, ()>("Connect", &()).map_err(err_text)
    }

    pub fn connect(&self, address: &str) -> Result<(), String> {
        self.device(address)?.call::<_, _, ()>("Connect", &()).map_err(err_text)
    }

    pub fn disconnect(&self, address: &str) -> Result<(), String> {
        self.device(address)?.call::<_, _, ()>("Disconnect", &()).map_err(err_text)
    }

    pub fn set_trusted(&self, address: &str, on: bool) -> Result<(), String> {
        self.device(address)?.set_property("Trusted", on).map_err(|e| err_text(e.into()))
    }

    pub fn forget(&self, address: &str) -> Result<(), String> {
        let dev = self.device(address)?;
        let path: OwnedObjectPath = dev.path().to_owned().into();
        self.adapter()?.call::<_, _, ()>("RemoveDevice", &(&path,)).map_err(err_text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses() {
        assert!(valid_address("AA:BB:CC:00:11:22"));
        assert!(!valid_address("AA:BB:CC:00:11"));
        assert!(!valid_address("AA:BB:CC:00:11:2Z"));
        assert!(!valid_address("AA:BB:CC:00:11:22; rm -rf /"));
    }

    #[test]
    fn pairing_window() {
        let w = PairingWindow::default();
        assert!(!w.is_open());
        w.open();
        assert!(w.is_open());
        w.close();
        assert!(!w.is_open());
    }
}
