//! adg-power-helper: the only code in ADG Power OS that runs as root.
//!
//! Invoked through pkexec (polkit action com.adgstudios.power.helper), it
//! performs exactly one allowlisted action and exits:
//!
//!   adg-power-helper gpu-runtime-pm <pci-address> auto|on
//!   adg-power-helper wifi-powersave <interface> on|off
//!
//! Arguments are validated against sysfs; nothing is passed to a shell.

use std::fs;
use std::path::Path;
use std::process::{exit, Command};

fn fail(msg: &str) -> ! {
    eprintln!("adg-power-helper: {msg}");
    exit(1)
}

fn valid_pci(a: &str) -> bool {
    // 0000:01:00.0
    let b = a.as_bytes();
    a.len() == 12
        && b[4] == b':'
        && b[7] == b':'
        && b[10] == b'.'
        && a.chars().enumerate().all(|(i, c)| matches!(i, 4 | 7 | 10) || c.is_ascii_hexdigit())
}

fn valid_iface(i: &str) -> bool {
    !i.is_empty() && i.len() <= 15 && i.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn gpu_runtime_pm(addr: &str, value: &str) {
    if !valid_pci(addr) {
        fail("invalid PCI address");
    }
    let dev = Path::new("/sys/bus/pci/devices").join(addr);
    let class = fs::read_to_string(dev.join("class")).unwrap_or_else(|_| fail("no such PCI device"));
    if !class.trim().starts_with("0x03") {
        fail("device is not a display controller");
    }
    // Runtime PM only: never unbind the driver or remove the device.
    fs::write(dev.join("power/control"), value).unwrap_or_else(|e| fail(&format!("write failed: {e}")));
}

fn wifi_powersave(iface: &str, value: &str) {
    if !valid_iface(iface) || !Path::new("/sys/class/net").join(iface).join("wireless").exists() {
        fail("not a wireless interface");
    }
    let iw = ["/usr/sbin/iw", "/sbin/iw", "/usr/bin/iw"].into_iter().find(|p| Path::new(p).exists());
    let iw = iw.unwrap_or_else(|| fail("iw is not installed (sudo apt install iw)"));
    let st = Command::new(iw)
        .args(["dev", iface, "set", "power_save", value])
        .env_clear()
        .status()
        .unwrap_or_else(|e| fail(&e.to_string()));
    if !st.success() {
        fail("iw reported an error");
    }
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    match (a.first().map(String::as_str), a.get(1), a.get(2).map(String::as_str), a.len()) {
        (Some("gpu-runtime-pm"), Some(addr), Some(v @ ("auto" | "on")), 3) => gpu_runtime_pm(addr, v),
        (Some("wifi-powersave"), Some(iface), Some(v @ ("on" | "off")), 3) => wifi_powersave(iface, v),
        _ => fail("usage: gpu-runtime-pm <pci> auto|on | wifi-powersave <iface> on|off"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation() {
        assert!(valid_pci("0000:01:00.0"));
        assert!(!valid_pci("0000:01:00.0/../../x"));
        assert!(!valid_pci("../../../etc"));
        assert!(valid_iface("wlo1"));
        assert!(!valid_iface("wlo1 && reboot"));
        assert!(!valid_iface("../x"));
    }
}
