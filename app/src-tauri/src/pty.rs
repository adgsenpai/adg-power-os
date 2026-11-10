//! Embedded terminal: a real PTY running the user's login shell, streamed to
//! xterm.js in the dashboard. Sessions are killed when their tab closes or
//! the window closes, so nothing is left running in the background.

use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, State};

struct Session {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
}

#[derive(Default)]
pub struct Ptys {
    next: AtomicU32,
    sessions: Mutex<HashMap<u32, Session>>,
}

impl Ptys {
    pub fn kill_all(&self) {
        for (_, mut s) in self.sessions.lock().unwrap().drain() {
            let _ = s.child.kill();
            let _ = s.child.wait();
        }
    }
}

#[derive(Clone, Serialize)]
struct Output {
    id: u32,
    data: String,
}

/// Split off a trailing incomplete UTF-8 sequence so multi-byte characters
/// split across reads are not mangled.
pub fn take_valid_utf8(buf: &mut Vec<u8>) -> String {
    let valid = match std::str::from_utf8(buf) {
        Ok(_) => buf.len(),
        Err(e) if e.error_len().is_none() => e.valid_up_to(),
        Err(_) => buf.len(), // genuinely invalid bytes: let lossy replace them
    };
    let rest = buf.split_off(valid);
    let s = String::from_utf8_lossy(buf).into_owned();
    *buf = rest;
    s
}

fn size(cols: u16, rows: u16) -> PtySize {
    PtySize { rows: rows.clamp(2, 500), cols: cols.clamp(2, 1000), pixel_width: 0, pixel_height: 0 }
}

#[tauri::command]
pub fn pty_open(app: AppHandle, ptys: State<'_, Ptys>, cols: u16, rows: u16, command: Option<String>) -> Result<u32, String> {
    let pair = native_pty_system().openpty(size(cols, rows)).map_err(|e| e.to_string())?;
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".into());
    let mut cmd = CommandBuilder::new(&shell);
    // Only one preset command is accepted from the UI.
    match command.as_deref() {
        None => cmd.arg("-l"),
        Some("claude") => {
            cmd.arg("-lic");
            cmd.arg(format!("claude; exec {shell} -l"));
        }
        Some(_) => return Err("command not allowed".into()),
    }
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    if let Some(home) = std::env::var_os("HOME") {
        cmd.cwd(home);
    }
    let child = pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?;
    drop(pair.slave);
    let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
    let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
    let id = ptys.next.fetch_add(1, Ordering::SeqCst) + 1;
    ptys.sessions.lock().unwrap().insert(id, Session { master: pair.master, writer, child });

    std::thread::spawn(move || {
        let mut buf = [0u8; 16 * 1024];
        let mut pending: Vec<u8> = Vec::new();
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    pending.extend_from_slice(&buf[..n]);
                    let data = take_valid_utf8(&mut pending);
                    if !data.is_empty() {
                        let _ = app.emit("pty-output", Output { id, data });
                    }
                }
            }
        }
        let _ = app.emit("pty-exit", id);
    });
    Ok(id)
}

#[tauri::command]
pub fn pty_write(ptys: State<'_, Ptys>, id: u32, data: String) -> Result<(), String> {
    let mut map = ptys.sessions.lock().unwrap();
    let s = map.get_mut(&id).ok_or("no such terminal")?;
    s.writer.write_all(data.as_bytes()).map_err(|e| e.to_string())?;
    s.writer.flush().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn pty_resize(ptys: State<'_, Ptys>, id: u32, cols: u16, rows: u16) -> Result<(), String> {
    let map = ptys.sessions.lock().unwrap();
    let s = map.get(&id).ok_or("no such terminal")?;
    s.master.resize(size(cols, rows)).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn pty_close(ptys: State<'_, Ptys>, id: u32) -> Result<(), String> {
    if let Some(mut s) = ptys.sessions.lock().unwrap().remove(&id) {
        let _ = s.child.kill();
        let _ = s.child.wait();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::take_valid_utf8;

    #[test]
    fn keeps_split_multibyte_chars() {
        let euro = "€".as_bytes(); // 3 bytes
        let mut b = vec![b'a', euro[0], euro[1]];
        assert_eq!(take_valid_utf8(&mut b), "a");
        assert_eq!(b.len(), 2);
        b.push(euro[2]);
        assert_eq!(take_valid_utf8(&mut b), "€");
        assert!(b.is_empty());
    }
}

#[cfg(test)]
mod pty_smoke {
    use portable_pty::{native_pty_system, CommandBuilder, PtySize};
    use std::io::Read;

    #[test]
    fn shell_round_trip() {
        let pair = native_pty_system().openpty(PtySize { rows: 24, cols: 80, pixel_width: 0, pixel_height: 0 }).unwrap();
        let mut cmd = CommandBuilder::new("/bin/sh");
        cmd.args(["-c", "echo adg-pty-ok"]);
        let mut child = pair.slave.spawn_command(cmd).unwrap();
        drop(pair.slave);
        let mut r = pair.master.try_clone_reader().unwrap();
        child.wait().unwrap();
        drop(pair.master);
        let mut out = String::new();
        let _ = r.read_to_string(&mut out);
        assert!(out.contains("adg-pty-ok"), "got {out:?}");
    }
}
