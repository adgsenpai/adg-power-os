//! Minimal HTTP/1.1 JSON API on a per-user Unix socket.
//!
//! The socket lives in $XDG_RUNTIME_DIR/adg-power (mode 0700), so only the
//! owning user can connect, and browsers cannot reach it (no CSRF surface).

use crate::daemon::Daemon;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

const MAX_HEADER: usize = 16 * 1024;
const MAX_BODY: usize = 64 * 1024;

pub fn default_socket() -> PathBuf {
    if let Some(p) = std::env::var_os("ADG_POWERD_SOCKET") {
        return PathBuf::from(p);
    }
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("/run/user/{}", unsafe_uid())));
    base.join("adg-power").join("powerd.sock")
}

fn unsafe_uid() -> u32 {
    // Read our uid without libc: /proc/self/status "Uid:\treal\t..."
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| s.lines().find(|l| l.starts_with("Uid:")).and_then(|l| l.split_whitespace().nth(1)?.parse().ok()))
        .unwrap_or(0)
}

#[derive(Debug, PartialEq)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub query: String,
    pub body: Vec<u8>,
}

pub fn read_request<R: Read>(r: R) -> Result<Request, String> {
    let mut r = BufReader::new(r);
    let mut line = String::new();
    let mut total = 0;
    r.read_line(&mut line).map_err(|e| e.to_string())?;
    total += line.len();
    let mut parts = line.split_whitespace();
    let method = parts.next().ok_or("bad request")?.to_string();
    let target = parts.next().ok_or("bad request")?.to_string();
    let mut content_length = 0usize;
    loop {
        let mut h = String::new();
        let n = r.read_line(&mut h).map_err(|e| e.to_string())?;
        total += n;
        if total > MAX_HEADER {
            return Err("headers too large".into());
        }
        if n == 0 || h == "\r\n" || h == "\n" {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            if k.trim().eq_ignore_ascii_case("content-length") {
                content_length = v.trim().parse().map_err(|_| "bad content-length")?;
            }
        }
    }
    if content_length > MAX_BODY {
        return Err("body too large".into());
    }
    let mut body = vec![0; content_length];
    r.read_exact(&mut body).map_err(|e| e.to_string())?;
    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p.to_string(), q.to_string()),
        None => (target, String::new()),
    };
    Ok(Request { method, path, query, body })
}

fn query_param<'a>(q: &'a str, key: &str) -> Option<&'a str> {
    q.split('&').filter_map(|kv| kv.split_once('=')).find(|(k, _)| *k == key).map(|(_, v)| v)
}

fn s<'a>(b: &'a Value, k: &str) -> Option<&'a str> {
    b.get(k).and_then(Value::as_str)
}

/// Dispatch one request. Returns (http status, json).
pub fn route(d: &Daemon, req: &Request) -> (u16, Value) {
    let body: Value = if req.body.is_empty() {
        json!({})
    } else {
        match serde_json::from_slice(&req.body) {
            Ok(v) => v,
            Err(_) => return (400, json!({"ok": false, "error": "invalid JSON"})),
        }
    };
    let b = &body;
    let on = b.get("on").and_then(Value::as_bool);
    let r: Result<Value, String> = match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/api/health") => Ok(json!({"version": env!("CARGO_PKG_VERSION")})),
        ("GET", "/api/status") => Ok(serde_json::to_value(d.status()).unwrap()),
        ("GET", "/api/history") => {
            let m = query_param(&req.query, "minutes").and_then(|m| m.parse().ok()).unwrap_or(30u64).min(60);
            Ok(d.history(m))
        }
        ("POST", "/api/profile") => s(b, "name").ok_or("name required".into()).and_then(|n| d.set_profile(n)),
        ("POST", "/api/brightness") => b
            .get("percent")
            .and_then(Value::as_f64)
            .ok_or("percent required".into())
            .and_then(|p| d.set_brightness_percent(p)),
        ("POST", "/api/brightness/dim") => d.dim(true),
        ("POST", "/api/brightness/undim") => d.dim(false),
        ("POST", "/api/keyboard") => b
            .get("value")
            .and_then(Value::as_i64)
            .ok_or("value required".into())
            .and_then(|v| d.set_keyboard(v as i32)),
        ("POST", "/api/eco/apply") => d.apply_eco(),
        ("POST", "/api/eco/revert") => d.revert_eco(),
        ("POST", "/api/helper") => match (s(b, "action"), s(b, "target"), s(b, "value")) {
            (Some(a), Some(t), Some(v)) => d.helper(a, t, v),
            _ => Err("action, target and value required".into()),
        },
        ("GET", "/api/wifi") => d.wifi(),
        ("GET", "/api/wifi/networks") => d.wifi_networks(),
        ("POST", "/api/wifi/scan") => d.wifi_scan(),
        ("POST", "/api/wifi/enabled") => d.wifi_enable(on.unwrap_or(true)),
        ("POST", "/api/wifi/connect") => match s(b, "ssid") {
            Some(ssid) => d.wifi_connect(ssid, s(b, "password")),
            None => Err("ssid required".into()),
        },
        ("POST", "/api/wifi/disconnect") => d.wifi_disconnect(),
        ("GET", "/api/bluetooth") => d.bluetooth(),
        ("POST", p) if p.starts_with("/api/bluetooth/") => {
            d.bt_action(&p["/api/bluetooth/".len()..], s(b, "address"), on)
        }
        ("GET", "/api/audio") => d.audio(),
        ("POST", "/api/audio/default") => match b.get("id").and_then(Value::as_u64) {
            Some(id) => d.audio_default(id as u32),
            None => Err("id required".into()),
        },
        ("POST", "/api/audio/volume") => match (b.get("id").and_then(Value::as_u64), b.get("volume").and_then(Value::as_f64)) {
            (Some(id), Some(v)) => d.audio_volume(id as u32, v),
            _ => Err("id and volume required".into()),
        },
        ("POST", "/api/audio/playpause") => d.audio_play_pause(),
        ("GET", "/api/services") => d.services(),
        ("POST", "/api/services/stop") => s(b, "unit").ok_or("unit required".into()).and_then(|u| d.service_action(u, false)),
        ("POST", "/api/services/start") => s(b, "unit").ok_or("unit required".into()).and_then(|u| d.service_action(u, true)),
        ("POST", "/api/launch") => s(b, "app").ok_or("app required".into()).and_then(|a| d.launch(a)),
        ("GET", "/api/audit") => d.audit_log(),
        ("GET", "/api/apps") => d.apps(),
        ("POST", "/api/apps/close") => s(b, "unit").ok_or("unit required".into()).and_then(|u| d.app_stop(u)),
        ("GET", "/api/hogs") => d.hogs(),
        ("POST", p) if p.starts_with("/api/session/") => d.session_action(&p["/api/session/".len()..]),
        _ => return (404, json!({"ok": false, "error": "not found"})),
    };
    match r {
        Ok(data) => (200, json!({"ok": true, "data": data})),
        Err(e) => (400, json!({"ok": false, "error": e})),
    }
}

fn respond(mut s: UnixStream, status: u16, v: &Value) {
    let body = v.to_string();
    let reason = match status {
        200 => "OK",
        404 => "Not Found",
        _ => "Bad Request",
    };
    let _ = write!(
        s,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
}

fn handle(d: &Daemon, stream: UnixStream) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let Ok(reader) = stream.try_clone() else { return };
    match read_request(reader) {
        Ok(req) => {
            let (st, v) = route(d, &req);
            respond(stream, st, &v);
        }
        Err(e) => respond(stream, 400, &json!({"ok": false, "error": e})),
    }
}

pub fn serve(d: Arc<Daemon>, socket: PathBuf) -> std::io::Result<()> {
    let dir = socket.parent().expect("socket has a parent");
    std::fs::create_dir_all(dir)?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    if socket.exists() {
        // Refuse to steal a live socket from another instance.
        if UnixStream::connect(&socket).is_ok() {
            return Err(std::io::Error::new(std::io::ErrorKind::AddrInUse, "adg-powerd is already running"));
        }
        std::fs::remove_file(&socket)?;
    }
    let listener = UnixListener::bind(&socket)?;
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))?;
    eprintln!("adg-powerd listening on {}", socket.display());
    for stream in listener.incoming().flatten() {
        let d = d.clone();
        std::thread::spawn(move || handle(&d, stream));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_post_with_body() {
        let raw = b"POST /api/wifi/connect?x=1 HTTP/1.1\r\nHost: x\r\nContent-Length: 15\r\n\r\n{\"ssid\":\"home\"}";
        let r = read_request(&raw[..]).unwrap();
        assert_eq!(r.method, "POST");
        assert_eq!(r.path, "/api/wifi/connect");
        assert_eq!(r.query, "x=1");
        assert_eq!(r.body, b"{\"ssid\":\"home\"}");
    }

    #[test]
    fn rejects_huge_body() {
        let raw = b"POST / HTTP/1.1\r\nContent-Length: 999999999\r\n\r\n";
        assert!(read_request(&raw[..]).is_err());
    }

    #[test]
    fn query_params() {
        assert_eq!(query_param("minutes=10&x=2", "minutes"), Some("10"));
        assert_eq!(query_param("", "minutes"), None);
    }
}
