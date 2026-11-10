//! Allowlisted application launcher. Apps run in their own transient systemd
//! user unit so closing the dashboard or restarting the daemon leaves them be.

use std::path::Path;
use std::process::{Command, Stdio};

pub const APPS: &[&str] = &["terminal", "claude", "chrome", "files", "music", "network-editor"];

fn which(bin: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(bin).is_file()))
        .unwrap_or(false)
        || Path::new(bin).is_file()
}

fn first<'a>(cands: &[&'a str]) -> Option<&'a str> {
    cands.iter().copied().find(|c| which(c))
}

fn terminal_with(cmd: Option<&str>) -> Option<Vec<String>> {
    let term = first(&["foot", "kitty", "gnome-terminal", "x-terminal-emulator"])?;
    let mut v = vec![term.to_string()];
    if let Some(c) = cmd {
        // gnome-terminal and x-terminal-emulator need a separator.
        if term == "gnome-terminal" {
            v.push("--".into());
        } else if term == "x-terminal-emulator" {
            v.push("-e".into());
        }
        v.push(c.into());
    }
    Some(v)
}

fn chrome(extra: &[&str]) -> Option<Vec<String>> {
    let c = first(&["google-chrome", "google-chrome-stable", "chromium", "chromium-browser"])?;
    let mut v = vec![c.to_string(), "--ozone-platform-hint=auto".to_string()];
    v.extend(extra.iter().map(|s| s.to_string()));
    Some(v)
}

/// Fixed argv for each allowlisted app id. No user input reaches argv.
pub fn command_for(app: &str) -> Option<Vec<String>> {
    match app {
        "terminal" => terminal_with(None),
        "claude" => terminal_with(Some("claude")),
        "chrome" => chrome(&[]),
        "music" => chrome(&["--app=https://music.youtube.com"]),
        "files" => first(&["nautilus", "thunar", "pcmanfm", "nemo"]).map(|f| vec![f.to_string()]),
        "network-editor" => first(&["nm-connection-editor"]).map(|f| vec![f.to_string()]),
        _ => None,
    }
}

pub fn spawn(app: &str, argv: &[String]) -> Result<(), String> {
    if !APPS.contains(&app) || argv.is_empty() {
        return Err("Unknown app".into());
    }
    let unit = format!("app-adg-{app}-{}", crate::policy::unix_now());
    let mut cmd = Command::new("systemd-run");
    cmd.args(["--user", "--collect", "--quiet", "--unit", &unit]);
    for var in ["WAYLAND_DISPLAY", "DISPLAY", "XDG_CURRENT_DESKTOP", "XDG_SESSION_TYPE", "SWAYSOCK", "XAUTHORITY"] {
        if std::env::var_os(var).is_some() {
            cmd.args(["-E", var]);
        }
    }
    cmd.arg("--").args(argv).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped());
    let out = cmd.output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn unknown_apps_are_rejected() {
        assert!(super::command_for("rm -rf /").is_none());
        assert!(super::spawn("bash", &["bash".into()]).is_err());
    }
}
