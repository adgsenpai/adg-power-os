//! Tauri shell for the ADG Power dashboard.
//!
//! The webview never touches the system directly. Its single command,
//! `daemon`, forwards an allowlisted request to adg-powerd over the user's
//! Unix socket. Closing this window does not stop the daemon or networking.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde_json::Value;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

const ALLOWED_PREFIXES: &[&str] = &[
    "/api/health", "/api/status", "/api/history", "/api/profile", "/api/brightness", "/api/keyboard",
    "/api/eco/", "/api/helper", "/api/wifi", "/api/bluetooth", "/api/audio", "/api/services",
    "/api/launch", "/api/audit",
];

fn allowed(method: &str, path: &str) -> bool {
    matches!(method, "GET" | "POST")
        && path.is_ascii()
        && !path.contains("..")
        && !path.contains(char::is_whitespace)
        && ALLOWED_PREFIXES.iter().any(|p| path.starts_with(p))
}

#[tauri::command]
async fn daemon(method: String, path: String, body: Option<Value>) -> Result<Value, String> {
    if !allowed(&method, &path) {
        return Err("request not allowed".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let sock = adg_power::server::default_socket();
        let mut s = UnixStream::connect(&sock).map_err(|_| "adg-powerd is not running".to_string())?;
        s.set_read_timeout(Some(Duration::from_secs(30))).ok();
        let body = body.map(|b| b.to_string()).unwrap_or_default();
        write!(
            s,
            "{method} {path} HTTP/1.1\r\nHost: adg-powerd\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .map_err(|e| e.to_string())?;
        let mut out = String::new();
        s.read_to_string(&mut out).map_err(|e| e.to_string())?;
        let json = out.split_once("\r\n\r\n").map(|(_, b)| b).unwrap_or("");
        serde_json::from_str(json).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![daemon])
        .run(tauri::generate_context!())
        .expect("failed to start ADG Power dashboard");
}

#[cfg(test)]
mod tests {
    #[test]
    fn allowlist() {
        assert!(super::allowed("GET", "/api/status"));
        assert!(super::allowed("POST", "/api/wifi/connect"));
        assert!(!super::allowed("DELETE", "/api/status"));
        assert!(!super::allowed("GET", "/etc/passwd"));
        assert!(!super::allowed("GET", "/api/status/../../x"));
    }
}
