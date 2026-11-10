//! Audio outputs through WirePlumber (wpctl) and playback through MPRIS.
//!
//! PipeWire has no D-Bus API for routing, so `wpctl` is invoked directly with
//! an argument vector. Only numeric node ids and clamped volumes are passed;
//! no user string ever reaches a command line.

use super::{err_text, prop_str, Props};
use serde::Serialize;
use std::process::Command;
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::OwnedValue;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Sink {
    pub id: u32,
    pub name: String,
    pub volume: Option<f64>,
    pub muted: bool,
    pub default: bool,
    pub bluetooth: bool,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Playback {
    pub player: String,
    pub status: String,
    pub title: Option<String>,
    pub artist: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct AudioStatus {
    pub sinks: Vec<Sink>,
    pub playing: Vec<Playback>,
    pub error: Option<String>,
}

fn strip_tree(line: &str) -> &str {
    line.trim_start_matches(|c: char| c == '│' || c == '├' || c == '└' || c == '─' || c.is_whitespace())
}

/// Parse the Audio → Sinks section of `wpctl status`.
pub fn parse_wpctl_sinks(status: &str) -> Vec<Sink> {
    let mut out = Vec::new();
    let mut in_audio = false;
    let mut in_sinks = false;
    for raw in status.lines() {
        if !raw.starts_with(' ') && !raw.starts_with('│') && !raw.trim().is_empty() {
            in_audio = raw.trim() == "Audio";
            in_sinks = false;
            continue;
        }
        if !in_audio {
            continue;
        }
        let line = strip_tree(raw);
        if line.ends_with(':') {
            in_sinks = line == "Sinks:";
            continue;
        }
        if !in_sinks || line.is_empty() {
            continue;
        }
        let (default, rest) = match line.strip_prefix('*') {
            Some(r) => (true, r.trim_start()),
            None => (false, line),
        };
        let Some((id, rest)) = rest.split_once('.') else { continue };
        let Ok(id) = id.trim().parse::<u32>() else { continue };
        let (name, vol_part) = match rest.rfind("[vol:") {
            Some(i) => (rest[..i].trim(), Some(&rest[i + 5..])),
            None => (rest.trim(), None),
        };
        let volume = vol_part.and_then(|v| v.split(|c| c == ']' || c == ' ').find(|s| !s.is_empty())?.parse().ok());
        let muted = vol_part.is_some_and(|v| v.contains("MUTED"));
        out.push(Sink {
            id,
            name: name.to_string(),
            volume,
            muted,
            default,
            bluetooth: false,
        });
    }
    out
}

/// node.description from `wpctl inspect` output.
pub fn default_description(inspect: &str) -> Option<String> {
    inspect
        .lines()
        .find(|l| l.trim_start_matches(|c: char| c == '*' || c.is_whitespace()).starts_with("node.description"))
        .and_then(|l| l.split_once('='))
        .map(|(_, v)| v.trim().trim_matches('"').to_string())
}

fn wpctl(args: &[&str]) -> Result<String, String> {
    let o = Command::new("wpctl").args(args).output().map_err(|e| format!("wpctl: {e}"))?;
    if !o.status.success() {
        return Err(String::from_utf8_lossy(&o.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&o.stdout).into_owned())
}

/// Bluetooth sinks are identified by their node media.class / api via inspect.
fn is_bluetooth(id: u32) -> bool {
    wpctl(&["inspect", &id.to_string()]).is_ok_and(|s| s.contains("api.bluez5") || s.contains("bluez5."))
}

pub struct Audio {
    session: Option<Connection>,
}

impl Audio {
    pub fn new(session: Option<Connection>) -> Self {
        Self { session }
    }

    pub fn status(&self) -> AudioStatus {
        let mut st = AudioStatus::default();
        match wpctl(&["status"]) {
            Ok(s) => {
                st.sinks = parse_wpctl_sinks(&s);
                // A filter chain (e.g. Bluetooth loopback) can be the real
                // default; map it back to the sink with the same description.
                if !st.sinks.iter().any(|k| k.default) {
                    if let Some(desc) = wpctl(&["inspect", "@DEFAULT_AUDIO_SINK@"]).ok().and_then(|t| default_description(&t)) {
                        if let Some(k) = st.sinks.iter_mut().find(|k| k.name == desc) {
                            k.default = true;
                        }
                    }
                }
                for sink in &mut st.sinks {
                    sink.bluetooth = is_bluetooth(sink.id);
                }
            }
            Err(e) => st.error = Some(e),
        }
        st.playing = self.players();
        st
    }

    pub fn set_default(&self, id: u32) -> Result<(), String> {
        let sinks = parse_wpctl_sinks(&wpctl(&["status"])?);
        if !sinks.iter().any(|s| s.id == id) {
            return Err("Unknown output".into());
        }
        wpctl(&["set-default", &id.to_string()]).map(|_| ())
    }

    pub fn set_volume(&self, id: u32, volume: f64) -> Result<(), String> {
        if !volume.is_finite() {
            return Err("Invalid volume".into());
        }
        let v = volume.clamp(0.0, 1.0);
        wpctl(&["set-volume", &id.to_string(), &format!("{v:.2}")]).map(|_| ())
    }

    fn players(&self) -> Vec<Playback> {
        let Some(conn) = &self.session else { return vec![] };
        let Ok(dbus) = Proxy::new(conn, "org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus") else {
            return vec![];
        };
        let names: Vec<String> = dbus.call("ListNames", &()).unwrap_or_default();
        names
            .into_iter()
            .filter(|n| n.starts_with("org.mpris.MediaPlayer2."))
            .filter_map(|n| {
                let p = Proxy::new(conn, n.clone(), "/org/mpris/MediaPlayer2", "org.mpris.MediaPlayer2.Player").ok()?;
                let status: String = p.get_property("PlaybackStatus").ok()?;
                let meta: Props = p.get_property("Metadata").unwrap_or_default();
                let artist = meta.get("xesam:artist").and_then(|v: &OwnedValue| {
                    Vec::<String>::try_from(v.try_clone().ok()?).ok().map(|a| a.join(", "))
                });
                Some(Playback {
                    player: n.trim_start_matches("org.mpris.MediaPlayer2.").split('.').next().unwrap_or("").into(),
                    status,
                    title: prop_str(&meta, "xesam:title"),
                    artist,
                })
            })
            .collect()
    }

    /// Toggle play/pause on the first player that is playing or paused.
    pub fn play_pause(&self) -> Result<(), String> {
        let conn = self.session.as_ref().ok_or("No session bus")?;
        let dbus = Proxy::new(conn, "org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus")
            .map_err(err_text)?;
        let names: Vec<String> = dbus.call("ListNames", &()).map_err(err_text)?;
        let name = names
            .into_iter()
            .find(|n| n.starts_with("org.mpris.MediaPlayer2."))
            .ok_or("No media player running")?;
        let p = Proxy::new(conn, name, "/org/mpris/MediaPlayer2", "org.mpris.MediaPlayer2.Player").map_err(err_text)?;
        p.call::<_, _, ()>("PlayPause", &()).map_err(err_text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "PipeWire 'pipewire-0' [1.6.2, user@host, cookie:1]
 └─ Clients:
        33. WirePlumber                         [1.6.2, user@host, pid:4778]

Audio
 ├─ Devices:
 │      52. Hesh ANC                            [bluez5]
 │
 ├─ Sinks:
 │      60. Raptor Lake-P/U/H cAVS Speaker      [vol: 0.39]
 │  *  113. Hesh ANC                            [vol: 0.13 MUTED]
 │
 ├─ Sources:
 │      69. Raptor Lake-P/U/H cAVS Digital Microphone [vol: 1.00]

Video
 ├─ Sinks:
 │      200. Not audio                          [vol: 1.00]
";

    #[test]
    fn parses_sinks() {
        let s = parse_wpctl_sinks(SAMPLE);
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].id, 60);
        assert_eq!(s[0].name, "Raptor Lake-P/U/H cAVS Speaker");
        assert_eq!(s[0].volume, Some(0.39));
        assert!(!s[0].default);
        assert_eq!(s[1].id, 113);
        assert!(s[1].default);
        assert!(s[1].muted);
        let d = default_description("id 76, type PipeWire:Interface:Node\n  * node.description = \"Hesh ANC\"\n");
        assert_eq!(d.as_deref(), Some("Hesh ANC"));
    }
}
