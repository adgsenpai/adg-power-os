//! Allowlisted application launcher. Apps run in their own transient systemd
//! user unit so closing the dashboard leaves them be, and closing the app
//! (or "Close" in Running apps) stops the whole unit and frees its memory.

use std::path::Path;
use std::process::{Command, Stdio};

pub const APPS: &[&str] = &["terminal", "claude", "chrome", "files", "music", "network-editor", "lock"];

fn which(bin: &str) -> Option<String> {
    let mut dirs: Vec<std::path::PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    // systemd user services get a minimal PATH; add the usual per-user bins.
    if let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) {
        for d in [".local/bin", ".cargo/bin", ".npm-global/bin", "bin"] {
            dirs.push(home.join(d));
        }
    }
    dirs.push("/usr/bin".into());
    dirs.push("/snap/bin".into());
    dirs.iter().map(|d| d.join(bin)).find(|p| p.is_file()).map(|p| p.to_string_lossy().into_owned())
}

fn first(cands: &[&str]) -> Option<String> {
    cands.iter().find_map(|c| which(c))
}

/// Interactive login shell so ~/.profile and ~/.bashrc set PATH (nvm,
/// ~/.local/bin…). `then` runs first and the shell stays open afterwards.
fn shell_cmd(then: Option<&str>) -> Vec<String> {
    let shell = std::env::var("SHELL").ok().filter(|s| Path::new(s).is_file()).unwrap_or_else(|| "/bin/bash".into());
    match then {
        // Fixed string, never user input.
        Some(cmd) => vec![shell.clone(), "-lic".into(), format!("{cmd}; exec {shell} -l")],
        None => vec![shell, "-l".into()],
    }
}

fn terminal_with(then: Option<&str>) -> Option<Vec<String>> {
    let term = first(&["foot", "kitty", "gnome-terminal", "x-terminal-emulator"])?;
    let mut v = vec![term.clone()];
    if term.ends_with("gnome-terminal") {
        v.push("--".into());
    } else if term.ends_with("x-terminal-emulator") {
        v.push("-e".into());
    } else if term.ends_with("foot") {
        v.extend(["--app-id".into(), "adg-terminal".into()]);
    }
    v.extend(shell_cmd(then));
    Some(v)
}

/// Chrome tuned for battery: Intel VA-API video decode (YouTube on the iGPU's
/// media engine instead of the CPU), no background mode after the last window.
fn chrome(extra: &[&str]) -> Option<Vec<String>> {
    let c = first(&["google-chrome", "google-chrome-stable", "chromium", "chromium-browser"])?;
    let mut v = vec![
        c,
        "--ozone-platform-hint=auto".to_string(),
        "--enable-features=AcceleratedVideoDecodeLinuxGL,AcceleratedVideoDecodeLinuxZeroCopyGL,VaapiIgnoreDriverChecks".to_string(),
        "--disable-background-mode".to_string(),
    ];
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
        "files" => first(&["nautilus", "thunar", "pcmanfm", "nemo"]).map(|f| vec![f]),
        "network-editor" => first(&["nm-connection-editor"]).map(|f| vec![f]),
        "lock" => first(&["swaylock"]).map(|f| vec![f, "-f".into(), "-c".into(), "0b101b".into()]),
        _ => None,
    }
}

/// Environment that keeps launched apps on the Intel iGPU so the RTX stays in
/// runtime suspend. Only set for files that exist on this machine.
pub fn igpu_env() -> Vec<(String, String)> {
    let mut env = Vec::new();
    let egl = "/usr/share/glvnd/egl_vendor.d/50_mesa.json";
    if Path::new(egl).exists() {
        env.push(("__EGL_VENDOR_LIBRARY_FILENAMES".into(), egl.into()));
        env.push(("__GLX_VENDOR_LIBRARY_NAME".into(), "mesa".into()));
    }
    for icd in ["/usr/share/vulkan/icd.d/intel_icd.json", "/usr/share/vulkan/icd.d/intel_icd.x86_64.json"] {
        if Path::new(icd).exists() {
            env.push(("VK_DRIVER_FILES".into(), icd.into()));
            env.push(("VK_ICD_FILENAMES".into(), icd.into()));
            break;
        }
    }
    env.push(("LIBVA_DRIVER_NAME".into(), "iHD".into()));
    env
}

pub fn spawn(app: &str, argv: &[String]) -> Result<(), String> {
    if !APPS.contains(&app) || argv.is_empty() {
        return Err("Unknown app".into());
    }
    let unit = format!("app-adg-{app}-{}", crate::policy::unix_now());
    let mut cmd = Command::new("systemd-run");
    cmd.args(["--user", "--collect", "--quiet", "--unit", &unit]);
    // Close kills the whole cgroup; give apps a moment to save state first.
    cmd.args(["-p", "KillMode=control-group", "-p", "TimeoutStopSec=5"]);
    for var in ["WAYLAND_DISPLAY", "DISPLAY", "XDG_CURRENT_DESKTOP", "XDG_SESSION_TYPE", "XAUTHORITY", "HOME", "SHELL"] {
        if std::env::var_os(var).is_some() {
            cmd.args(["-E", var]);
        }
    }
    if let Some(sock) = crate::sway::socket() {
        cmd.arg("-E").arg(format!("SWAYSOCK={}", sock.display()));
    }
    for (k, v) in igpu_env() {
        cmd.arg("-E").arg(format!("{k}={v}"));
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

    #[test]
    fn claude_runs_in_a_login_shell_that_stays_open() {
        let v = super::shell_cmd(Some("claude"));
        assert!(v[1] == "-lic");
        assert!(v[2].starts_with("claude; exec "));
    }
}
